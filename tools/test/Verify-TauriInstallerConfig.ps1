param(
    [string]$ConfigPath = "$PSScriptRoot\..\..\src-tauri\tauri.conf.json",
    [string]$HookPath = "$PSScriptRoot\..\..\src-tauri\windows\hooks.nsh"
)

$config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
if (@($config.bundle.targets) -join ',' -ne 'nsis') { throw 'bundle.targets must contain only nsis' }
if ($config.bundle.windows.nsis.installMode -ne 'perMachine') { throw 'NSIS must be perMachine' }
if ($config.bundle.windows.nsis.installerHooks -ne 'windows/hooks.nsh') { throw 'NSIS hook path mismatch' }
if (-not (Test-Path -LiteralPath $HookPath)) { throw 'NSIS hook file is missing' }
$hook = Get-Content -LiteralPath $HookPath -Raw
if ($hook -notmatch '--remove-launch-tasks') { throw 'pre-uninstall maintenance mode is missing' }
if ($hook -match 'yu\.exe') { throw 'retired yu.exe must not be packaged' }
Write-Output 'Tauri installer configuration is valid.'
