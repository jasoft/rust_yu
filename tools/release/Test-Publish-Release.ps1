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

if ($versionState.RootCargoVersion -notmatch '^\d+\.\d+\.\d+$') {
    throw "expected root cargo version to use major.minor.patch, got $($versionState.RootCargoVersion)"
}

if ($versionState.TauriCargoVersion -ne $versionState.RootCargoVersion) {
    throw "expected tauri cargo version to match root cargo version"
}

if ($versionState.TauriConfigVersion -ne $versionState.RootCargoVersion) {
    throw "expected tauri config version to match root cargo version"
}

if ($versionState.CliVersion -ne $versionState.RootCargoVersion) {
    throw "expected cli version to match root cargo version"
}

if ($versionState.Version -ne $versionState.RootCargoVersion) {
    throw "expected version alias to match root cargo version"
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
Set-Content -LiteralPath (Join-Path $tempRoot "Cargo.lock") -Value @'
[[package]]
name = "rust-yu"
version = "0.1.0"

[[package]]
name = "rust-yu-tauri"
version = "0.1.0"
'@

Set-ProjectVersion -RepoRoot $tempRoot -Version "0.1.1"

$utf8Strict = New-Object System.Text.UTF8Encoding($false, $true)
foreach ($path in @(
    (Join-Path $tempRoot "Cargo.toml"),
    (Join-Path $tempRoot "src-tauri\Cargo.toml"),
    (Join-Path $tempRoot "src-tauri\tauri.conf.json"),
    (Join-Path $tempRoot "src\main.rs")
)) {
    $fileBytes = [System.IO.File]::ReadAllBytes($path)
    try {
        [void]$utf8Strict.GetString($fileBytes)
    } catch {
        throw "expected version-updated file to remain valid UTF-8: $path"
    }
}

$updatedVersionState = Get-ReleaseVersionState -RepoRoot $tempRoot
if (-not $updatedVersionState.IsConsistent) {
    throw "expected temp repo versions to stay consistent after bump"
}

if ($updatedVersionState.Version -ne "0.1.1") {
    throw "expected temp repo version 0.1.1, got $($updatedVersionState.Version)"
}

$updatedCargoLock = Get-Content -LiteralPath (Join-Path $tempRoot "Cargo.lock") -Raw
if ($updatedCargoLock -notmatch 'name = "rust-yu"\s+version = "0.1.1"') {
    throw "expected Cargo.lock to update rust-yu package version"
}

if ($updatedCargoLock -notmatch 'name = "rust-yu-tauri"\s+version = "0.1.1"') {
    throw "expected Cargo.lock to update rust-yu-tauri package version"
}

$artifactName = Get-ReleaseAssetFileName -Version $versionState.Version
if ($artifactName -ne ("yu-" + $versionState.Version + "-windows-amd64.zip")) {
    throw "unexpected asset file name: $artifactName"
}

$tagName = Get-ReleaseTagName -Version $versionState.Version
if ($tagName -ne ("v" + $versionState.Version)) {
    throw "unexpected tag name: $tagName"
}

Remove-Item -LiteralPath $tempRoot -Recurse -Force

Write-Host "Publish release script tests passed"
