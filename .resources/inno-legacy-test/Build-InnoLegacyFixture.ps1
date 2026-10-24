param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fixtureRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$helperSource = Join-Path $fixtureRoot 'tools\SpawnUninstallHelper.rs'
$helperOutput = Join-Path $fixtureRoot 'payload\app\SpawnUninstallHelper.exe'
$isccPath = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
$issPath = Join-Path $fixtureRoot 'LegacyUninstallTest.iss'

# 强制 x64 MSVC 工具链（AGENTS.md §2A）：本机 PATH 可能优先命中 ARM Rust
$rustcPath = Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe'
if (-not (Test-Path $rustcPath)) {
    throw "Missing x64 rustc: $rustcPath"
}
$hostLine = (& $rustcPath -vV | Select-String '^host:').Line
if ($hostLine -notmatch 'x86_64-pc-windows-msvc') {
    throw "rustc host must be x86_64-pc-windows-msvc, got: $hostLine"
}

if (-not (Test-Path $helperSource)) {
    throw "Missing helper source: $helperSource"
}

if (-not (Test-Path $isccPath)) {
    throw "Missing ISCC.exe: $isccPath"
}

& $rustcPath --edition 2021 $helperSource -C opt-level=z -o $helperOutput
if ($LASTEXITCODE -ne 0) {
    throw 'rustc failed'
}

& $isccPath $issPath
if ($LASTEXITCODE -ne 0) {
    throw 'ISCC.exe failed'
}

Write-Output 'fixture-built'
