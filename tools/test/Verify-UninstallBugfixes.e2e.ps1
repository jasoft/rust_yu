#requires -Version 7.0

# 针对“失效安装不可卸载”与“卸载错误残留列表”两个缺陷修复的 GUI 回归验证。
# 通过 tauri.e2e.conf.json 打开 WebView2 CDP 端口，用 Playwright 连接真实窗口；
# plan_uninstall 由 runner 注入 test double，不会执行任何真实卸载。
[CmdletBinding()]
param(
    [int]$StartupTimeoutSeconds = 120,
    [switch]$CleanupOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$initializeScript = Join-Path $repoRoot "tools\dev\Initialize-Worktree.ps1"
$runnerPath = Join-Path $repoRoot "tools\test\uninstall-bugfixes.e2e.mjs"
$DebugPort = 9223
$artifactRoot = Join-Path $repoRoot ("target\test-logs\uninstall-bugfixes-e2e\" + (Get-Date -Format "yyyyMMdd-HHmmss"))
$tauriLog = Join-Path $artifactRoot "tauri-dev.log"
$tauriErrorLog = Join-Path $artifactRoot "tauri-dev.error.log"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Stop-TrackedProcessTree([int]$RootProcessId) {
    $processes = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
    $children = @{}
    foreach ($process in $processes) {
        $parentId = [int]$process.ParentProcessId
        if (-not $children.ContainsKey($parentId)) {
            $children[$parentId] = [Collections.Generic.List[int]]::new()
        }
        $children[$parentId].Add([int]$process.ProcessId)
    }

    $ordered = [Collections.Generic.List[int]]::new()
    function Add-Descendants([int]$ProcessId) {
        if ($children.ContainsKey($ProcessId)) {
            foreach ($childId in $children[$ProcessId]) {
                Add-Descendants $childId
            }
        }
        $ordered.Add($ProcessId)
    }
    Add-Descendants $RootProcessId
    foreach ($processId in $ordered) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
}

function Stop-ExistingCheckoutDevSession {
    $expectedExecutable = Join-Path $repoRoot "target\x86_64-pc-windows-msvc\debug\rust-yu-tauri.exe"
    $matchingApps = @(Get-CimInstance Win32_Process -Filter "Name = 'rust-yu-tauri.exe'" | Where-Object {
        $_.ExecutablePath -and [IO.Path]::GetFullPath($_.ExecutablePath) -eq [IO.Path]::GetFullPath($expectedExecutable)
    })
    foreach ($app in $matchingApps) {
        Write-Warning "关闭当前 checkout 的旧 Rust Yu 开发窗口，避免锁定构建产物。"
        Stop-Process -Id $app.ProcessId -Force -ErrorAction SilentlyContinue
    }

    $listeners = @(Get-NetTCPConnection -LocalPort 11000 -State Listen -ErrorAction SilentlyContinue)
    foreach ($listener in $listeners) {
        $owner = Get-CimInstance Win32_Process -Filter "ProcessId = $($listener.OwningProcess)" -ErrorAction Stop
        $commandLine = [string]$owner.CommandLine
        if ($commandLine -notlike "*$repoRoot*" -and $commandLine -notmatch "vite(?:\\|/)bin(?:\\|/)vite\.js") {
            throw "WebUI 端口 11000 被非当前 checkout 的进程占用（PID $($listener.OwningProcess)），拒绝结束该进程。"
        }
        Stop-Process -Id $listener.OwningProcess -Force -ErrorAction SilentlyContinue
    }
}

if (-not (Test-IsAdministrator)) {
    Write-Host "验证需要管理员权限的 GUI 会话，正在请求 UAC..." -ForegroundColor Yellow
    $pwsh = Get-Command pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1 -ExpandProperty Source
    $scriptPath = [IO.Path]::GetFullPath($PSCommandPath)
    $quotedScriptPath = '"' + $scriptPath.Replace('"', '\"') + '"'
    $elevatedProcess = Start-Process `
        -FilePath $pwsh `
        -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $quotedScriptPath, "-StartupTimeoutSeconds", "$StartupTimeoutSeconds") `
        -WorkingDirectory $repoRoot `
        -Verb RunAs `
        -Wait `
        -PassThru
    exit $elevatedProcess.ExitCode
}

if ($CleanupOnly) {
    Stop-ExistingCheckoutDevSession
    Write-Host "验证环境收尾完成。" -ForegroundColor Green
    exit 0
}

if (Get-NetTCPConnection -LocalPort $DebugPort -State Listen -ErrorAction SilentlyContinue) {
    throw "调试端口 $DebugPort 已被占用。"
}

New-Item -ItemType Directory -Path $artifactRoot -Force | Out-Null

$previousBrowserArguments = [Environment]::GetEnvironmentVariable("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "Process")
$hostProcess = $null
try {
    Stop-ExistingCheckoutDevSession

    & $initializeScript -SkipFrontend -SkipCheck
    if (-not $?) { throw "X64 worktree initialization failed" }

    # tauri.e2e.conf.json 已通过 additionalBrowserArgs 打开 CDP；环境变量作为兜底。
    [Environment]::SetEnvironmentVariable("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--remote-debugging-port=$DebugPort", "Process")
    $npx = Get-Command npx.cmd -CommandType Application -ErrorAction Stop | Select-Object -First 1 -ExpandProperty Source
    $hostProcess = Start-Process -FilePath $npx -ArgumentList @("tauri", "dev", "--config", "tauri.e2e.conf.json") `
        -WorkingDirectory (Join-Path $repoRoot "src-tauri") `
        -RedirectStandardOutput $tauriLog -RedirectStandardError $tauriErrorLog `
        -WindowStyle Hidden -PassThru

    & node $runnerPath --port $DebugPort --artifacts $artifactRoot --startup-timeout-ms ($StartupTimeoutSeconds * 1000)
    if ($LASTEXITCODE -ne 0) { throw "卸载缺陷修复 E2E 验证失败，Node 退出代码：$LASTEXITCODE" }

    Write-Host "卸载缺陷修复 E2E 验证已通过。证据目录：$artifactRoot" -ForegroundColor Green
} finally {
    if ($null -ne $hostProcess -and -not $hostProcess.HasExited) {
        Stop-TrackedProcessTree $hostProcess.Id
    }
    [Environment]::SetEnvironmentVariable("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", $previousBrowserArguments, "Process")
}
