#requires -Version 7.0

[CmdletBinding()]
param(
    [switch]$InstallUserFixtures,
    [switch]$InstallMachineFixture,
    [switch]$RunAnalysis
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fixtureRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $fixtureRoot '..\..')).Path
$manifestPath = Join-Path $fixtureRoot 'expected-traces.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -Depth 20

if ($manifest.schema_version -ne 1 -or $manifest.fixtures.Count -lt 3) {
    throw 'Truth manifest must contain at least three schema-v1 fixtures'
}
$fixtureIds = @($manifest.fixtures | ForEach-Object id)
if (($fixtureIds | Sort-Object -Unique).Count -ne $fixtureIds.Count) {
    throw 'Truth manifest fixture ids must be unique'
}
foreach ($fixture in $manifest.fixtures) {
    if ($fixture.expected.Count -lt 8) {
        throw "Fixture $($fixture.id) has insufficient truth coverage"
    }
    $expectedIds = @($fixture.expected | ForEach-Object id)
    if (($expectedIds | Sort-Object -Unique).Count -ne $expectedIds.Count) {
        throw "Fixture $($fixture.id) contains duplicate truth ids"
    }
}

function Get-PeMachine([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "Not a PE file: $Path"
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    return [BitConverter]::ToUInt16($bytes, $peOffset + 4)
}

$payload = Join-Path $fixtureRoot 'payload\common\FixtureApp.exe'
$outputs = @($manifest.fixtures | ForEach-Object { Join-Path $fixtureRoot $_.installer })
foreach ($path in @($payload) + $outputs) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing built fixture: $path"
    }
    if ((Get-PeMachine $path) -ne 0x8664) {
        throw "Fixture is not x64 PE: $path"
    }
}

$selected = [System.Collections.Generic.List[string]]::new()
if ($InstallUserFixtures) {
    foreach ($fixtureId in @('deep-nsis-user', 'deep-nsis-sparse')) {
        $fixture = $manifest.fixtures | Where-Object id -EQ $fixtureId
        $installer = Join-Path $fixtureRoot $fixture.installer
        $process = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden
        if ($process.ExitCode -ne 0) {
            throw "Fixture installer failed: $fixtureId ($($process.ExitCode))"
        }
        $selected.Add($fixtureId)
    }
    if (-not (Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\RustYuDeepScanUser')) {
        throw 'Current-user rich fixture ARP key is missing after installation'
    }
    if (-not (Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\RustYuDeepScanSparse')) {
        throw 'Sparse fixture ARP key is missing after installation'
    }
}

if ($InstallMachineFixture) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'InstallMachineFixture requires an elevated PowerShell session'
    }
    $fixture = $manifest.fixtures | Where-Object id -EQ 'deep-nsis-machine'
    $installer = Join-Path $fixtureRoot $fixture.installer
    $process = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0) {
        throw "Machine fixture installer failed: $($process.ExitCode)"
    }
    if (-not (Test-Path 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\RustYuDeepScanMachine')) {
        throw 'Machine fixture ARP key is missing after installation'
    }
    $selected.Add('deep-nsis-machine')
}

if ($RunAnalysis) {
    if ($selected.Count -eq 0) {
        throw 'RunAnalysis requires fixtures installed by this invocation'
    }
    & (Join-Path $repoRoot 'tools\dev\Initialize-Worktree.ps1') -SkipFrontend
    if (-not $?) { throw 'x64 worktree initialization failed' }
    $env:RUST_YU_DEEP_SCAN_FIXTURES = $selected -join ','
    Push-Location $repoRoot
    try {
        cargo test --test deep_analysis_fixtures -- --ignored --nocapture --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'read-only fixture analysis failed' }
    } finally {
        Pop-Location
        Remove-Item Env:\RUST_YU_DEEP_SCAN_FIXTURES -ErrorAction SilentlyContinue
    }
}

Write-Output "deep-analysis-fixtures-verified; installed=$($selected -join ',')"
