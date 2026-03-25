Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
$scriptPath = Join-Path $repoRoot "tools\release\publish-release.ps1"

. $scriptPath

$parseOutput = powershell -NoProfile -ExecutionPolicy Bypass -Command "& { . '$scriptPath'; 'parse-ok' }" 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "expected Windows PowerShell to parse publish-release.ps1 successfully, got: $parseOutput"
}

if (($parseOutput | Out-String).Trim() -ne "parse-ok") {
    throw "unexpected parse probe output: $parseOutput"
}

$versionState = Get-ReleaseVersionState -RepoRoot $repoRoot
if (-not $versionState.IsConsistent) {
    throw "expected Cargo.toml and tauri.conf.json versions to match"
}

if ($versionState.Version -ne "0.1.0") {
    throw "expected version 0.1.0, got $($versionState.Version)"
}

$artifactName = Get-ReleaseAssetFileName -Version $versionState.Version
if ($artifactName -ne "yu-0.1.0-windows-amd64.zip") {
    throw "unexpected asset file name: $artifactName"
}

$tagName = Get-ReleaseTagName -Version $versionState.Version
if ($tagName -ne "v0.1.0") {
    throw "unexpected tag name: $tagName"
}

Write-Host "Publish release script tests passed"
