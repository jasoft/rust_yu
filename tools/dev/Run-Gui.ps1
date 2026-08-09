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

& $bootstrapScript -SkipFrontend -SkipCheck
if (-not $?) {
    throw "Worktree initialization failed"
}

Push-Location (Join-Path $repoRoot "src-tauri")
try {
    npx tauri dev
} finally {
    Pop-Location
}
