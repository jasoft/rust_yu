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
    throw "Missing installer output: $outputPath"
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
    $yuExe = Join-Path $repoRoot 'target\debug\yu.exe'
    $yuStdoutLog = Join-Path $repoRoot 'tools\test\yu-uninstall-stdout.log'
    $yuStderrLog = Join-Path $repoRoot 'tools\test\yu-uninstall-stderr.log'

    Start-Process -FilePath $outputPath -ArgumentList '/VERYSILENT', '/NORESTART' -Wait

    if (-not (Test-Path $registryPath)) {
        throw "Missing uninstall registry key after install: $registryPath"
    }

    $registryValue = Get-ItemProperty -Path $registryPath
    if ($registryValue.UninstallString -notmatch 'SpawnUninstallHelper\.exe') {
        throw "UninstallString does not point to SpawnUninstallHelper.exe: $($registryValue.UninstallString)"
    }

    if ($registryValue.QuietUninstallString -notmatch 'SpawnUninstallHelper\.exe') {
        throw "QuietUninstallString does not point to SpawnUninstallHelper.exe: $($registryValue.QuietUninstallString)"
    }

    if (-not (Test-Path $yuExe)) {
        throw "Missing yu executable: $yuExe"
    }

    Remove-Item -Force $yuStdoutLog, $yuStderrLog -ErrorAction SilentlyContinue
    $yuProcess = Start-Process -FilePath $yuExe `
        -ArgumentList 'uninstall', '"RustYu Legacy Test App"', '--timeout', '15' `
        -Wait `
        -PassThru `
        -RedirectStandardOutput $yuStdoutLog `
        -RedirectStandardError $yuStderrLog
    $yuText = ''
    if (Test-Path $yuStdoutLog) {
        $yuText += Get-Content -Raw $yuStdoutLog
    }
    if (Test-Path $yuStderrLog) {
        $yuText += Get-Content -Raw $yuStderrLog
    }

    $expectedLeftoverFailure = $yuText -match 'install_dir_exists=true'
    if ($yuProcess.ExitCode -ne 0 -and -not $expectedLeftoverFailure) {
        throw "yu uninstall failed:`n$yuText"
    }

    if ($yuText -notmatch 'Job Object') {
        throw "yu output did not hit the waitforjobs path:`n$yuText"
    }

    if (-not (Test-Path $leftoverFile)) {
        throw "Missing install-dir leftover file: $leftoverFile"
    }

    if (-not (Test-Path $appDataFile)) {
        throw "Missing AppData leftover file: $appDataFile"
    }
}

Write-Output "fixture-ready"
