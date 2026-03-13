param(
    [ValidateSet('interactive', 'quiet')]
    [string]$Mode = 'interactive'
)

$ErrorActionPreference = 'Stop'

Set-StrictMode -Version Latest

$uninstaller = Join-Path $PSScriptRoot 'unins000.exe'
if (-not (Test-Path $uninstaller)) {
    throw "Missing real uninstaller: $uninstaller"
}

Start-Sleep -Seconds 2

$arguments = switch ($Mode) {
    'quiet' { @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') }
    default { @('/NORESTART') }
}

$process = Start-Process -FilePath $uninstaller -ArgumentList $arguments -Wait -PassThru
exit $process.ExitCode
