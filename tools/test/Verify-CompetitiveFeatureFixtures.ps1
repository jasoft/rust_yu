param([switch]$RunLifecycle)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$fixtureRoot = Join-Path $repoRoot '.resources\competitive-regression'
$scenarioPath = Join-Path $fixtureRoot 'scenarios.json'
$workerPath = Join-Path $fixtureRoot 'FixtureWorker.ps1'
foreach ($path in @($scenarioPath, $workerPath)) {
    if (-not (Test-Path -LiteralPath $path)) { throw "Missing competitive fixture asset: $path" }
}
$spec = Get-Content -LiteralPath $scenarioPath -Raw | ConvertFrom-Json
$required = @('install-monitor', 'service-task', 'update-overwrite', 'abnormal-exit', 'restore-consistency', 'report-consistency')
$actual = @($spec.scenarios.id)
foreach ($id in $required) {
    if ($id -notin $actual) { throw "Missing scenario: $id" }
}
foreach ($metric in @('detection_rate', 'false_association_count', 'wait_correctness', 'restore_success_rate')) {
    if ($metric -notin @($spec.metrics)) { throw "Missing metric: $metric" }
}

if ($RunLifecycle) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'RunLifecycle requires an elevated PowerShell session'
    }
    $root = Join-Path $env:ProgramData ("RustYuCompetitiveFixture-" + [guid]::NewGuid().ToString('N'))
    try {
        & $workerPath -Action Install -Root $root
        if (-not (Test-Path -LiteralPath (Join-Path $root 'install.complete'))) { throw 'Install evidence marker missing' }
        $task = schtasks.exe /Query /TN RustYuCompetitiveFixtureTask /FO LIST 2>$null
        if ($LASTEXITCODE -ne 0 -or $task -notmatch 'RustYuCompetitiveFixtureTask') { throw 'Owned task was not created' }
        $service = Get-CimInstance Win32_Service -Filter "Name='RustYuCompetitiveFixtureService'"
        if (-not $service -or $service.PathName -notlike "$root*") { throw 'Owned service path association failed' }
        & $workerPath -Action Update -Root $root
        $before = Get-FileHash -LiteralPath (Join-Path $root 'data\config.json.before-update') -Algorithm SHA256
        $after = Get-FileHash -LiteralPath (Join-Path $root 'data\config.json') -Algorithm SHA256
        if ($before.Hash -eq $after.Hash) { throw 'Update fixture did not overwrite the tracked file' }
        $abnormal = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $workerPath, '-Action', 'AbnormalExit', '-Root', $root) -Wait -PassThru -WindowStyle Hidden
        if ($abnormal.ExitCode -ne 23) { throw "Expected abnormal exit 23, got $($abnormal.ExitCode)" }
        if (-not (Test-Path -LiteralPath (Join-Path $root 'data\partial-evidence.marker'))) { throw 'Partial evidence was not retained after abnormal exit' }
    } finally {
        & $workerPath -Action Cleanup -Root $root
    }
}

Write-Output 'competitive-fixtures-ready'
