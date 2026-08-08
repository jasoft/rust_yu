$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$scriptPath = Join-Path $repoRoot "tools\release\publish-release.ps1"
$parseErrors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($scriptPath, [ref]$null, [ref]$parseErrors)
if ($parseErrors.Count -gt 0) { throw "publish-release.ps1 解析失败: $($parseErrors | Out-String)" }

. $scriptPath

$versionState = Get-ReleaseVersionState -RepoRoot $repoRoot
if (-not $versionState.IsConsistent) { throw "当前仓库的 GUI 版本不一致" }
$versionPropertyNames = @($versionState.PSObject.Properties | ForEach-Object { $_.Name })
if ($versionPropertyNames -contains "CliVersion") { throw "发布状态不应再包含 CliVersion" }
if ($versionState.Version -ne $versionState.RootCargoVersion) { throw "Version alias mismatch" }

$nextPatch = Get-NextPatchVersion -Version "0.1.0"
if ($nextPatch -ne "0.1.1") { throw "next patch version mismatch: $nextPatch" }
$nextAvailable = Get-NextAvailableReleaseVersion -Version "0.1.0" -VersionExistsScript {
    param($candidateVersion)
    return $candidateVersion -in @("0.1.0", "0.1.1")
}
if ($nextAvailable -ne "0.1.2") { throw "next available version mismatch: $nextAvailable" }

$assetName = Get-ReleaseAssetFileName -Version $versionState.Version
if ($assetName -ne ("rust-yu-" + $versionState.Version + "-windows-x64-setup.exe")) { throw "unexpected GUI asset name: $assetName" }
if ($assetName -match "yu\.exe|\.zip$") { throw "CLI or zip asset leaked into release name" }

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rust-yu-release-test-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path (Join-Path $tempRoot "src-tauri") -Force | Out-Null
try {
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
    Set-Content -LiteralPath (Join-Path $tempRoot "Cargo.lock") -Value @'
[[package]]
name = "rust-yu"
version = "0.1.0"

[[package]]
name = "rust-yu-tauri"
version = "0.1.0"
'@
    Set-ProjectVersion -RepoRoot $tempRoot -Version "0.1.1"
    $updated = Get-ReleaseVersionState -RepoRoot $tempRoot
    if (-not $updated.IsConsistent -or $updated.Version -ne "0.1.1") { throw "version bump did not preserve GUI consistency" }
    $lock = Get-Content -LiteralPath (Join-Path $tempRoot "Cargo.lock") -Raw
    if ($lock -notmatch 'name = "rust-yu"\s+version = "0.1.1"') { throw "root Cargo.lock version was not updated" }
    if ($lock -notmatch 'name = "rust-yu-tauri"\s+version = "0.1.1"') { throw "Tauri Cargo.lock version was not updated" }
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if ((Get-Content -LiteralPath $scriptPath -Raw) -match "cargo[^\r\n]*--bin\s+yu|target[^\r\n]*yu\.exe|CliVersion|src/main\.rs|update-manifest|Scoop") {
    throw "发布脚本仍包含 CLI/Scoop 发布路径"
}

Write-Host "Publish release script tests passed" -ForegroundColor Green
