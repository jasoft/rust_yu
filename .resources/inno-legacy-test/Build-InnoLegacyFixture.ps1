param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fixtureRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$helperSource = Join-Path $fixtureRoot 'tools\SpawnUninstallHelper.rs'
$helperOutput = Join-Path $fixtureRoot 'payload\app\SpawnUninstallHelper.exe'
$isccPath = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
$issPath = Join-Path $fixtureRoot 'LegacyUninstallTest.iss'

if (-not (Test-Path $helperSource)) {
    throw "Missing helper source: $helperSource"
}

if (-not (Test-Path $isccPath)) {
    throw "Missing ISCC.exe: $isccPath"
}

& rustc $helperSource -C opt-level=z -o $helperOutput
if ($LASTEXITCODE -ne 0) {
    throw 'rustc failed'
}

& $isccPath $issPath
if ($LASTEXITCODE -ne 0) {
    throw 'ISCC.exe failed'
}

Write-Output 'fixture-built'
