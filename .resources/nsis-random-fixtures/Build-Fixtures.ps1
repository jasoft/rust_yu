#requires -Version 7.0

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fixtureRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $fixtureRoot '..\..')).Path
$bootstrap = Join-Path $repoRoot 'tools\dev\Initialize-Worktree.ps1'
& $bootstrap -SkipFrontend
if (-not $?) { throw 'x64 worktree initialization failed' }

$rustHost = (& rustc -vV | Select-String '^host:' | ForEach-Object { $_.Line.Split(':', 2)[1].Trim() })
if ($rustHost -ne 'x86_64-pc-windows-msvc') {
    throw "Fixture payload requires x86_64-pc-windows-msvc; found $rustHost"
}

$source = Join-Path $fixtureRoot 'payload\common\FixtureApp.rs'
$payload = Join-Path $fixtureRoot 'payload\common\FixtureApp.exe'
& rustc $source --target x86_64-pc-windows-msvc -C opt-level=z -C strip=symbols -o $payload
if ($LASTEXITCODE -ne 0) { throw 'fixture payload rustc build failed' }

$makensis = Join-Path $fixtureRoot '.tools\nsis-amd64\makensis.exe'
if (-not (Test-Path -LiteralPath $makensis -PathType Leaf)) {
    throw "Missing bundled x64 NSIS compiler: $makensis"
}

$scripts = @(
    'MachineRichFixture.nsi',
    'UserRichFixture.nsi',
    'SparseLegacyFixture.nsi'
)
Push-Location $fixtureRoot
try {
    foreach ($script in $scripts) {
        & $makensis /INPUTCHARSET UTF8 /V3 $script
        if ($LASTEXITCODE -ne 0) { throw "NSIS build failed: $script" }
    }
} finally {
    Pop-Location
}

function Get-PeMachine([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "Not a PE file: $Path"
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($peOffset -lt 0 -or $peOffset + 6 -gt $bytes.Length) {
        throw "Invalid PE header: $Path"
    }
    return [BitConverter]::ToUInt16($bytes, $peOffset + 4)
}

$outputs = @(
    $payload,
    (Join-Path $fixtureRoot 'output\RustYuDeepScanMachineSetup.exe'),
    (Join-Path $fixtureRoot 'output\RustYuDeepScanUserSetup.exe'),
    (Join-Path $fixtureRoot 'output\RustYuDeepScanSparseSetup.exe')
)
foreach ($output in $outputs) {
    if ((Get-PeMachine $output) -ne 0x8664) {
        throw "Fixture is not x64 PE: $output"
    }
}

Write-Output 'deep-analysis-fixtures-built-x64'
