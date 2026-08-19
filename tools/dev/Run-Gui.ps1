#requires -Version 7.0

[CmdletBinding()]
param(
    [ValidateSet("local", "lan")]
    [string]$ApiMode,
    [ValidateRange(1, 65535)]
    [int]$ApiPort = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$bootstrapScript = Join-Path $repoRoot "tools\dev\Initialize-Worktree.ps1"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-IsAdministrator)) {
    Write-Host "Run-Gui 需要管理员权限，正在请求 UAC..." -ForegroundColor Yellow
    $powershellExecutable = Get-Command pwsh.exe -CommandType Application -ErrorAction Stop |
        Select-Object -First 1 -ExpandProperty Source
    $scriptPath = [IO.Path]::GetFullPath($PSCommandPath)
    $quotedScriptPath = '"' + $scriptPath.Replace('"', '\"') + '"'
    $elevationArguments = @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $quotedScriptPath
    )
    if (-not [string]::IsNullOrWhiteSpace($ApiMode)) {
        $elevationArguments += @("-ApiMode", $ApiMode)
    }
    if ($ApiPort -gt 0) {
        $elevationArguments += @(
            "-ApiPort",
            $ApiPort.ToString([Globalization.CultureInfo]::InvariantCulture)
        )
    }

    try {
        $elevatedProcess = Start-Process `
            -FilePath $powershellExecutable `
            -ArgumentList $elevationArguments `
            -WorkingDirectory $repoRoot `
            -Verb RunAs `
            -Wait `
            -PassThru
        exit $elevatedProcess.ExitCode
    } catch {
        throw "未能以管理员权限启动 Run-Gui。请在 UAC 对话框中允许此次操作。原始错误：$($_.Exception.Message)"
    }
}

$tauriX64Binding = Join-Path $repoRoot "src-tauri\node_modules\@tauri-apps\cli-win32-x64-msvc\cli.win32-x64-msvc.node"
$webuiX64Binding = Join-Path $repoRoot "src-tauri\src-frontends\webui\node_modules\@rolldown\binding-win32-x64-msvc\rolldown-binding.win32-x64-msvc.node"
$frontendReady = (Test-Path -LiteralPath $tauriX64Binding -PathType Leaf) -and
    (Test-Path -LiteralPath $webuiX64Binding -PathType Leaf)

if ($frontendReady) {
    & $bootstrapScript -SkipFrontend -SkipCheck
} else {
    Write-Host "检测到前端原生依赖不是完整的 X64 版本，正在重新安装..." -ForegroundColor Yellow
    & $bootstrapScript -SkipCheck
}
if (-not $?) {
    throw "Worktree initialization failed"
}

if (-not [string]::IsNullOrWhiteSpace($ApiMode)) {
    $env:RUST_YU_ENABLE_API = "1"
    $env:RUST_YU_API_MODE = $ApiMode
    if ($ApiPort -gt 0) {
        $env:RUST_YU_API_PORT = $ApiPort.ToString([Globalization.CultureInfo]::InvariantCulture)
    }
    Write-Host "HTTP API 模式：$ApiMode，端口：$(if ($ApiPort -gt 0) { $ApiPort } else { 8080 })" -ForegroundColor Cyan
}

Push-Location (Join-Path $repoRoot "src-tauri")
try {
    npx tauri dev
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri GUI 启动失败，npx tauri dev 退出码：$LASTEXITCODE"
    }
} finally {
    Pop-Location
}
