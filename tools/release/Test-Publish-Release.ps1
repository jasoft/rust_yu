Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
$scriptPath = Join-Path $repoRoot "tools\release\publish-release.ps1"
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rust-yu-release-test-" + [System.Guid]::NewGuid().ToString("N"))

. $scriptPath

$parseOutput = powershell -NoProfile -ExecutionPolicy Bypass -Command "& { . '$scriptPath'; 'parse-ok' }" 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "expected Windows PowerShell to parse publish-release.ps1 successfully, got: $parseOutput"
}

if (($parseOutput | Out-String).Trim() -ne "parse-ok") {
    throw "unexpected parse probe output: $parseOutput"
}

$missingTagOutput = Invoke-ExternalCommand -FilePath git -Arguments @(
    "-C", $repoRoot, "rev-parse", "-q", "--verify", "refs/tags/definitely-missing-release-tag"
) -CaptureOutput -IgnoreExitCode
if (-not [string]::IsNullOrWhiteSpace($missingTagOutput)) {
    throw "expected missing tag lookup to return empty output, got: $missingTagOutput"
}

if (Test-ReleaseTagExists -RepoRoot $repoRoot -TagName "definitely-missing-release-tag") {
    throw "expected Test-ReleaseTagExists to return false for a missing tag"
}

$versionState = Get-ReleaseVersionState -RepoRoot $repoRoot
if (-not $versionState.IsConsistent) {
    throw "expected Cargo.toml and tauri.conf.json versions to match"
}

if ($versionState.RootCargoVersion -ne "0.1.0") {
    throw "expected root cargo version 0.1.0, got $($versionState.RootCargoVersion)"
}

if ($versionState.TauriCargoVersion -ne "0.1.0") {
    throw "expected tauri cargo version 0.1.0, got $($versionState.TauriCargoVersion)"
}

if ($versionState.TauriConfigVersion -ne "0.1.0") {
    throw "expected tauri config version 0.1.0, got $($versionState.TauriConfigVersion)"
}

if ($versionState.CliVersion -ne "0.1.0") {
    throw "expected cli version 0.1.0, got $($versionState.CliVersion)"
}

if ($versionState.Version -ne "0.1.0") {
    throw "expected version 0.1.0, got $($versionState.Version)"
}

$nextPatchVersion = Get-NextPatchVersion -Version "0.1.0"
if ($nextPatchVersion -ne "0.1.1") {
    throw "expected next patch version 0.1.1, got $nextPatchVersion"
}

$nextAvailableVersion = Get-NextAvailableReleaseVersion -Version "0.1.0" -VersionExistsScript {
    param($candidateVersion)

    return $candidateVersion -in @("0.1.0", "0.1.1")
}
if ($nextAvailableVersion -ne "0.1.2") {
    throw "expected next available version 0.1.2, got $nextAvailableVersion"
}

New-Item -ItemType Directory -Path $tempRoot | Out-Null
New-Item -ItemType Directory -Path (Join-Path $tempRoot "src-tauri") | Out-Null
New-Item -ItemType Directory -Path (Join-Path $tempRoot "src") | Out-Null

Set-Content -LiteralPath (Join-Path $tempRoot "Cargo.toml") -Value @'
[package]
version = "0.1.0"
'@
Set-Content -LiteralPath (Join-Path $tempRoot "src-tauri\Cargo.toml") -Value @'
[package]
version = "0.1.0"
'@
Set-Content -LiteralPath (Join-Path $tempRoot "src-tauri\tauri.conf.json") -Value @'
{
  "version": "0.1.0"
}
'@
Set-Content -LiteralPath (Join-Path $tempRoot "src\main.rs") -Value @'
#[command(version = "0.1.0")]
'@

Set-ProjectVersion -RepoRoot $tempRoot -Version "0.1.1"

$updatedVersionState = Get-ReleaseVersionState -RepoRoot $tempRoot
if (-not $updatedVersionState.IsConsistent) {
    throw "expected temp repo versions to stay consistent after bump"
}

if ($updatedVersionState.Version -ne "0.1.1") {
    throw "expected temp repo version 0.1.1, got $($updatedVersionState.Version)"
}

$artifactName = Get-ReleaseAssetFileName -Version $versionState.Version
if ($artifactName -ne "yu-0.1.0-windows-amd64.zip") {
    throw "unexpected asset file name: $artifactName"
}

$tagName = Get-ReleaseTagName -Version $versionState.Version
if ($tagName -ne "v0.1.0") {
    throw "unexpected tag name: $tagName"
}

Remove-Item -LiteralPath $tempRoot -Recurse -Force

Write-Host "Publish release script tests passed"
