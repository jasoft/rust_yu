param(
    [string]$ConfigPath = "$PSScriptRoot\..\..\src-tauri\tauri.conf.json",
    [string]$HookPath = "$PSScriptRoot\..\..\src-tauri\windows\hooks.nsh",
    [string]$CapabilityPath = "$PSScriptRoot\..\..\src-tauri\capabilities\default.json"
)

$config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
if (@($config.bundle.targets) -join ',' -ne 'nsis') { throw 'bundle.targets must contain only nsis' }
if ($config.bundle.windows.nsis.installMode -ne 'perMachine') { throw 'NSIS must be perMachine' }
if ($config.bundle.windows.nsis.installerHooks -ne 'windows/hooks.nsh') { throw 'NSIS hook path mismatch' }
if (-not (Test-Path -LiteralPath $HookPath)) { throw 'NSIS hook file is missing' }
$hook = Get-Content -LiteralPath $HookPath -Raw
if ($hook -notmatch '--remove-launch-tasks') { throw 'pre-uninstall maintenance mode is missing' }
if ($hook -match 'yu\.exe') { throw 'retired yu.exe must not be packaged' }
$capability = Get-Content -LiteralPath $CapabilityPath -Raw | ConvertFrom-Json
if (@($capability.windows) -join ',' -ne 'main') { throw 'default capability must only target main' }
if (@($capability.permissions) -match 'shell:|unsafe-inline|unsafe-eval') { throw 'unsafe shell/CSP permission leaked into capability' }
$requiredCommands = @(
    'scan-browser-data', 'clean-browser-data', 'list-programs', 'warmup-program-metadata',
    'search-programs', 'scan-traces', 'clean-traces', 'list-cleaner-entries',
    'scan-cleaner-entries', 'clean-cleaner-entries', 'plan-uninstall', 'execute-uninstall',
    'clean-uninstall-residues', 'finish-uninstall', 'get-uninstall-job', 'get-reports',
    'delete-report', 'list-startup-items', 'get-startup-item', 'list-startup-sources',
    'plan-startup-action', 'apply-startup-action', 'rollback-startup-action',
    'plan-add-startup-item', 'add-startup-item'
)
foreach ($command in $requiredCommands) {
    if ($capability.permissions -notcontains "allow-$command") { throw "missing command permission: $command" }
}
Write-Output 'Tauri installer configuration is valid.'
