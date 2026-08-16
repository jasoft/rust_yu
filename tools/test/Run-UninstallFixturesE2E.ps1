#requires -Version 7.0

[CmdletBinding()]
param(
    [ValidateRange(30, 120)]
    [int]$StartupTimeoutSeconds = 120,
    [switch]$CleanupOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$DebugPort = 9223
$scriptPath = [IO.Path]::GetFullPath($PSCommandPath)
$initializeScript = Join-Path $repoRoot "tools\dev\Initialize-Worktree.ps1"
$runnerPath = Join-Path $repoRoot "tools\test\uninstall-fixtures.e2e.mjs"
$artifactRoot = Join-Path $repoRoot ("target\test-logs\uninstall-fixtures-e2e\" + (Get-Date -Format "yyyyMMdd-HHmmss"))
$tauriLog = Join-Path $artifactRoot "tauri-dev.log"
$tauriErrorLog = Join-Path $artifactRoot "tauri-dev.error.log"
$hostProcess = $null
$previousBrowserArguments = [Environment]::GetEnvironmentVariable("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "Process")

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
    $runGuiScript = Join-Path $repoRoot "tools\dev\Run-Gui.ps1"
    $runGuiProcesses = @(Get-CimInstance Win32_Process -Filter "Name = 'pwsh.exe'" | Where-Object {
        $_.CommandLine -and $_.CommandLine.Contains($runGuiScript, [StringComparison]::OrdinalIgnoreCase)
    })
    foreach ($runGuiProcess in $runGuiProcesses) {
        Write-Warning "检测到当前 checkout 的旧 Run-Gui 进程树，正在按两分钟止损规则终止。"
        Stop-TrackedProcessTree $runGuiProcess.ProcessId
    }

    $expectedExecutable = Join-Path $repoRoot "target\x86_64-pc-windows-msvc\debug\rust-yu-tauri.exe"
    $matchingApps = @(Get-CimInstance Win32_Process -Filter "Name = 'rust-yu-tauri.exe'" | Where-Object {
        $_.ExecutablePath -and [IO.Path]::GetFullPath($_.ExecutablePath) -eq [IO.Path]::GetFullPath($expectedExecutable)
    })
    foreach ($app in $matchingApps) {
        Write-Warning "检测到当前 checkout 的旧 Rust Yu 开发窗口，正在关闭以避免锁定 E2E 产物。"
        Stop-Process -Id $app.ProcessId -Force -ErrorAction Stop
    }

    $listeners = @(Get-NetTCPConnection -LocalPort 11000 -State Listen -ErrorAction SilentlyContinue)
    foreach ($listener in $listeners) {
        $owner = Get-CimInstance Win32_Process -Filter "ProcessId = $($listener.OwningProcess)" -ErrorAction Stop
        $commandLine = [string]$owner.CommandLine
        if ($commandLine -notlike "*$repoRoot*" -and $commandLine -notmatch "vite(?:\\|/)bin(?:\\|/)vite\.js") {
            throw "WebUI 端口 11000 被非当前 checkout 的进程占用（PID $($listener.OwningProcess)），拒绝结束该进程。"
        }
        Write-Warning "检测到当前 checkout 的旧 Vite 服务，正在关闭以便 E2E 使用固定端口。"
        Stop-Process -Id $listener.OwningProcess -Force -ErrorAction Stop
    }

    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ((Get-NetTCPConnection -LocalPort 11000 -State Listen -ErrorAction SilentlyContinue) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 200
    }
    if (Get-NetTCPConnection -LocalPort 11000 -State Listen -ErrorAction SilentlyContinue) {
        throw "当前 checkout 的旧开发会话未能释放 WebUI 端口 11000。"
    }
}

function Remove-RemainingTestFixtures {
    $xplorerProductCode = "{93CA8C7C-F9B0-4FA3-B0EE-7DFDE701112A}"
    $xplorerKey = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$xplorerProductCode"
    $xplorerWowKey = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\$xplorerProductCode"
    if ((Test-Path -LiteralPath $xplorerKey) -or (Test-Path -LiteralPath $xplorerWowKey)) {
        Write-Warning "E2E 未清除 Xplorer，正在调用其精确 MSI ProductCode 做收尾。"
        $cleanup = Start-Process -FilePath "msiexec.exe" -ArgumentList @("/x", $xplorerProductCode, "/qn", "/norestart") -Wait -PassThru -WindowStyle Hidden
        if ($cleanup.ExitCode -notin @(0, 1605, 1614, 1641, 3010)) {
            Write-Warning "Xplorer 收尾卸载退出代码：$($cleanup.ExitCode)"
        }
    }

    $legacyKey = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_legacy_test_app_is1"
    $legacyWowKey = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_legacy_test_app_is1"
    if ((Test-Path -LiteralPath $legacyKey) -or (Test-Path -LiteralPath $legacyWowKey)) {
        $legacyUninstaller = Join-Path $env:ProgramFiles "RustYu Legacy Test App\unins000.exe"
        if (Test-Path -LiteralPath $legacyUninstaller -PathType Leaf) {
            Write-Warning "E2E 未清除 Legacy fixture，正在调用其精确卸载器做收尾。"
            $cleanup = Start-Process -FilePath $legacyUninstaller -ArgumentList @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART") -Wait -PassThru -WindowStyle Hidden
            if ($cleanup.ExitCode -notin @(0, 1641, 3010)) {
                Write-Warning "Legacy fixture 收尾卸载退出代码：$($cleanup.ExitCode)"
            }
        }
    }

    $knownResidueDirectories = @(
        [IO.Path]::Combine($env:ProgramFiles, "RustYu Legacy Test App"),
        [IO.Path]::Combine($env:LOCALAPPDATA, "RustYuLegacyTest")
    )
    foreach ($directory in $knownResidueDirectories) {
        $fullPath = [IO.Path]::GetFullPath($directory)
        if (-not [string]::Equals($fullPath, $directory, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Fixture 收尾目录不是预期的绝对路径：$directory"
        }
        if (-not (Test-Path -LiteralPath $fullPath -PathType Container)) { continue }
        $item = Get-Item -LiteralPath $fullPath -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Fixture 收尾目录是重解析点，拒绝递归删除：$fullPath"
        }
        Write-Warning "正在删除已记录证据后的精确 fixture 残留目录：$fullPath"
        Remove-Item -LiteralPath $fullPath -Recurse -Force
    }
}

if (-not (Test-IsAdministrator)) {
    Write-Host "卸载 E2E 会安装并删除测试夹具，正在请求管理员权限..." -ForegroundColor Yellow
    $pwsh = Get-Command pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1 -ExpandProperty Source
    $elevatedArguments = @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ('"' + $scriptPath + '"'),
        "-StartupTimeoutSeconds", $StartupTimeoutSeconds
    )
    if ($CleanupOnly) { $elevatedArguments += "-CleanupOnly" }
    $elevated = Start-Process -FilePath $pwsh -ArgumentList $elevatedArguments -WorkingDirectory $repoRoot -Verb RunAs -Wait -PassThru
    exit $elevated.ExitCode
}

if ($CleanupOnly) {
    Stop-ExistingCheckoutDevSession
    Remove-RemainingTestFixtures
    Write-Host "卸载 fixtures 测试环境收尾完成。" -ForegroundColor Green
    exit 0
}

if (Get-NetTCPConnection -LocalPort $DebugPort -State Listen -ErrorAction SilentlyContinue) {
    throw "调试端口 $DebugPort 已被占用。"
}

New-Item -ItemType Directory -Path $artifactRoot -Force | Out-Null

try {
    Stop-ExistingCheckoutDevSession
    $tauriX64Binding = Join-Path $repoRoot "src-tauri\node_modules\@tauri-apps\cli-win32-x64-msvc\cli.win32-x64-msvc.node"
    $webuiX64Binding = Join-Path $repoRoot "src-tauri\src-frontends\webui\node_modules\@rolldown\binding-win32-x64-msvc\rolldown-binding.win32-x64-msvc.node"
    if ((Test-Path -LiteralPath $tauriX64Binding -PathType Leaf) -and (Test-Path -LiteralPath $webuiX64Binding -PathType Leaf)) {
        & $initializeScript -SkipFrontend -SkipCheck
    } else {
        & $initializeScript -SkipCheck
    }
    if (-not $?) { throw "X64 worktree initialization failed" }

    [Environment]::SetEnvironmentVariable("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--remote-debugging-port=$DebugPort", "Process")
    $npx = Get-Command npx.cmd -CommandType Application -ErrorAction Stop | Select-Object -First 1 -ExpandProperty Source
    $hostProcess = Start-Process -FilePath $npx -ArgumentList @("tauri", "dev", "--config", "tauri.e2e.conf.json") `
        -WorkingDirectory (Join-Path $repoRoot "src-tauri") `
        -RedirectStandardOutput $tauriLog -RedirectStandardError $tauriErrorLog `
        -WindowStyle Hidden -PassThru

    & node $runnerPath --port $DebugPort --artifacts $artifactRoot --startup-timeout-ms ($StartupTimeoutSeconds * 1000)
    if ($LASTEXITCODE -ne 0) { throw "卸载 fixtures E2E 失败，Node 退出代码：$LASTEXITCODE" }

    Write-Host "卸载 fixtures E2E 已通过。证据目录：$artifactRoot" -ForegroundColor Green
} finally {
    if ($null -ne $hostProcess -and -not $hostProcess.HasExited) {
        Stop-TrackedProcessTree $hostProcess.Id
    }
    Remove-RemainingTestFixtures
    [Environment]::SetEnvironmentVariable("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", $previousBrowserArguments, "Process")
}
