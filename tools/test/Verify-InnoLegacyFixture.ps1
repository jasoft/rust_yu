param(
    [switch]$RunLifecycle
)

$ErrorActionPreference = 'Stop'

Set-StrictMode -Version Latest

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$fixtureRoot = Join-Path $repoRoot '.resources\inno-legacy-test'
$buildScriptPath = Join-Path $fixtureRoot 'Build-InnoLegacyFixture.ps1'
$scriptPath = Join-Path $fixtureRoot 'LegacyUninstallTest.iss'
$readmePath = Join-Path $fixtureRoot 'README.md'
$outputPath = Join-Path $fixtureRoot 'output\RustYuLegacyTestSetup.exe'
$spawnScriptPath = Join-Path $fixtureRoot 'payload\app\SpawnUninstallHelper.exe'
$workerScriptPath = Join-Path $fixtureRoot 'payload\app\UninstallWorker.ps1'

$requiredPaths = @(
    $fixtureRoot,
    $buildScriptPath,
    $scriptPath,
    $readmePath,
    $spawnScriptPath,
    $workerScriptPath
)

$missingPaths = @($requiredPaths | Where-Object { -not (Test-Path $_) })

if ($missingPaths.Count -gt 0) {
    $missingList = $missingPaths -join ', '
    throw "Missing fixture paths: $missingList"
}

if (-not (Test-Path $outputPath)) {
    throw "Missing prebuilt installer fixture: $outputPath. Run Build-InnoLegacyFixture.ps1 only when the fixture must be refreshed."
}

if ($RunLifecycle) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    $isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $isAdmin) {
        throw 'RunLifecycle requires an elevated PowerShell session'
    }

    $registryPath = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_legacy_test_app_is1'
    $installPath = 'C:\Program Files\RustYu Legacy Test App'
    $leftoverFile = Join-Path $installPath 'logs\leftover.log'
    $appDataFile = Join-Path $env:LOCALAPPDATA 'RustYuLegacyTest\Data\leftover-user-profile.json'
    Push-Location $repoRoot
    try {
        cargo test --test windows_lifecycle -- --ignored --nocapture
        if ($LASTEXITCODE -ne 0) { throw 'application workflow lifecycle test failed' }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path $leftoverFile)) {
        throw "Missing install-dir leftover file: $leftoverFile"
    }

    if (-not (Test-Path $appDataFile)) {
        throw "Missing AppData leftover file: $appDataFile"
    }
}

Write-Output "fixture-ready"
