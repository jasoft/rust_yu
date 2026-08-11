[CmdletBinding()]
param()

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
    $powershellExecutable = (Get-Process -Id $PID -ErrorAction Stop).Path
    $scriptPath = [IO.Path]::GetFullPath($PSCommandPath)
    $quotedScriptPath = '"' + $scriptPath.Replace('"', '\"') + '"'

    try {
        $elevatedProcess = Start-Process `
            -FilePath $powershellExecutable `
            -ArgumentList @(
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                $quotedScriptPath
            ) `
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

Push-Location (Join-Path $repoRoot "src-tauri")
try {
    npx tauri dev
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri GUI 启动失败，npx tauri dev 退出码：$LASTEXITCODE"
    }
} finally {
    Pop-Location
}
