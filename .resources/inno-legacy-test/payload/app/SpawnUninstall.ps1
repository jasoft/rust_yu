param(
    [ValidateSet('interactive', 'quiet')]
    [string]$Mode = 'interactive'
)

$ErrorActionPreference = 'Stop'

Set-StrictMode -Version Latest

$workerScript = Join-Path $PSScriptRoot 'UninstallWorker.ps1'
if (-not (Test-Path $workerScript)) {
    throw "Missing uninstall worker script: $workerScript"
}

$powershellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$arguments = @(
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-File', $workerScript,
    '-Mode', $Mode
)

Start-Process -FilePath $powershellExe -ArgumentList $arguments -WindowStyle Hidden | Out-Null
exit 0
