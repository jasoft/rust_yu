# Rust Yu GUI 发布脚本。保持 UTF-8 with BOM，兼容 Windows PowerShell 5.1。
[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$Repo = "jasoft/rust_yu",
    [string]$Branch = "main",
    [string]$NotesFile = "",
    [switch]$SkipBuild,
    [switch]$SkipPush,
    [switch]$SkipRelease,
    [switch]$DryRun,
    [switch]$AllowDirty
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:DryRunMode = [bool]$DryRun
$script:SkipBuildMode = [bool]$SkipBuild

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Format-ArgumentForDisplay {
    param([string]$Value)
    if ($Value -match '\s') { return '"' + $Value.Replace('"', '\"') + '"' }
    return $Value
}

function Invoke-ExternalCommand {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$Arguments = @(),
        [switch]$CaptureOutput,
        [switch]$IgnoreExitCode
    )
    $displayArgs = ($Arguments | ForEach-Object { Format-ArgumentForDisplay $_ }) -join " "
    $displayCommand = if ([string]::IsNullOrWhiteSpace($displayArgs)) { $FilePath } else { "$FilePath $displayArgs" }
    if ($script:DryRunMode -and -not $CaptureOutput) {
        Write-Host "[dry-run] $displayCommand"
        return
    }
    if ($CaptureOutput) {
        $output = & $FilePath @Arguments 2>&1
        $exitCode = $LASTEXITCODE
        if (-not $IgnoreExitCode -and $exitCode -ne 0) {
            throw "命令执行失败($exitCode): $displayCommand`n$(($output | Out-String).Trim())"
        }
        return ($output | Out-String).Trim()
    }
    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE
    if (-not $IgnoreExitCode -and $exitCode -ne 0) { throw "命令执行失败($exitCode): $displayCommand" }
}

function Ensure-CommandAvailable {
    param([string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) { throw "未找到命令: $Name" }
}

function Get-RepoRoot {
    if ($PSScriptRoot) { return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path }
    return (Get-Location).Path
}

function Read-TextFileUtf8 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $encoding = New-Object System.Text.UTF8Encoding($false, $true)
    return [System.IO.File]::ReadAllText($Path, $encoding)
}

function Write-TextFileUtf8 {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Content)
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Get-CargoPackageVersion {
    param([Parameter(Mandatory = $true)][string]$CargoTomlPath)
    $insidePackage = $false
    foreach ($line in (Read-TextFileUtf8 $CargoTomlPath) -split "`r?`n") {
        if ($line -match '^\s*\[package\]\s*$') { $insidePackage = $true; continue }
        if ($insidePackage -and $line -match '^\s*\[') { break }
        if ($insidePackage -and $line -match '^\s*version\s*=\s*"([^"]+)"\s*$') { return $matches[1] }
    }
    throw "无法从 Cargo.toml 读取 package.version: $CargoTomlPath"
}

function Get-TauriConfigVersion {
    param([Parameter(Mandatory = $true)][string]$TauriConfigPath)
    $config = Read-TextFileUtf8 $TauriConfigPath | ConvertFrom-Json
    if (-not $config.version) { throw "无法从 tauri.conf.json 读取 version: $TauriConfigPath" }
    return [string]$config.version
}

function Get-ReleaseVersionFiles {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    return [pscustomobject]@{
        RootCargoToml = Join-Path $RepoRoot "Cargo.toml"
        TauriCargoToml = Join-Path $RepoRoot "src-tauri\Cargo.toml"
        TauriConfig = Join-Path $RepoRoot "src-tauri\tauri.conf.json"
        CargoLock = Join-Path $RepoRoot "Cargo.lock"
        TrackedPaths = @("Cargo.toml", "src-tauri/Cargo.toml", "src-tauri/tauri.conf.json", "Cargo.lock")
    }
}

function Get-ReleaseVersionState {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $files = Get-ReleaseVersionFiles $RepoRoot
    $root = Get-CargoPackageVersion $files.RootCargoToml
    $tauri = Get-CargoPackageVersion $files.TauriCargoToml
    $config = Get-TauriConfigVersion $files.TauriConfig
    $isConsistent = @($root, $tauri, $config) | Select-Object -Unique | Measure-Object | Select-Object -ExpandProperty Count
    return [pscustomobject]@{
        RootCargoVersion = $root
        TauriCargoVersion = $tauri
        TauriConfigVersion = $config
        Version = $root
        IsConsistent = ($isConsistent -eq 1)
    }
}

function Get-NextPatchVersion {
    param([Parameter(Mandatory = $true)][string]$Version)
    if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') { throw "仅支持 major.minor.patch 形式的版本号: $Version" }
    return "$([int]$matches[1]).$([int]$matches[2]).$([int]$matches[3] + 1)"
}

function Get-NextAvailableReleaseVersion {
    param([Parameter(Mandatory = $true)][string]$Version, [Parameter(Mandatory = $true)][scriptblock]$VersionExistsScript)
    $candidate = $Version
    while (& $VersionExistsScript $candidate) { $candidate = Get-NextPatchVersion $candidate }
    return $candidate
}

function Get-ReleaseAssetFileName {
    param([Parameter(Mandatory = $true)][string]$Version)
    return "rust-yu-$Version-windows-x64-setup.exe"
}

function Get-ReleaseTagName {
    param([Parameter(Mandatory = $true)][string]$Version)
    return "v$Version"
}

function Get-ReleaseAssetPath {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$Version)
    return Join-Path $RepoRoot ("dist\release\" + (Get-ReleaseAssetFileName $Version))
}

function Set-CargoPackageVersion {
    param([Parameter(Mandatory = $true)][string]$CargoTomlPath, [Parameter(Mandatory = $true)][string]$Version)
    $content = Read-TextFileUtf8 $CargoTomlPath
    $lines = $content -split "`r?`n"
    $inside = $false; $updated = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match '^\s*\[package\]\s*$') { $inside = $true; continue }
        if ($inside -and $lines[$i] -match '^\s*\[') { break }
        if ($inside -and $lines[$i] -match '^\s*version\s*=\s*"[^"]+"\s*$') {
            $lines[$i] = $lines[$i] -replace '"[^"]+"', ('"' + $Version + '"'); $updated = $true; break
        }
    }
    if (-not $updated) { throw "无法更新 Cargo.toml 中的 package.version: $CargoTomlPath" }
    Write-TextFileUtf8 $CargoTomlPath (($lines -join "`r`n") + $(if ($content.EndsWith("`n")) { "`r`n" } else { "" }))
}

function Set-TauriConfigVersion {
    param([Parameter(Mandatory = $true)][string]$TauriConfigPath, [Parameter(Mandatory = $true)][string]$Version)
    $content = Read-TextFileUtf8 $TauriConfigPath
    $updated = [regex]::Replace($content, '(?m)^(\s*"version"\s*:\s*")([^"]+)(")', { param($m) $m.Groups[1].Value + $Version + $m.Groups[3].Value }, 1)
    if ($updated -eq $content) { throw "无法更新 tauri.conf.json 中的 version: $TauriConfigPath" }
    Write-TextFileUtf8 $TauriConfigPath $updated
}

function Set-CargoLockPackageVersions {
    param([Parameter(Mandatory = $true)][string]$CargoLockPath, [Parameter(Mandatory = $true)][string]$Version)
    if (-not (Test-Path -LiteralPath $CargoLockPath)) { return }
    $content = Read-TextFileUtf8 $CargoLockPath
    $updated = [regex]::Replace($content, '(?ms)(\[\[package\]\]\r?\nname = "(?:rust-yu|rust-yu-tauri)"\r?\nversion = ")([^"]+)(")', { param($m) $m.Groups[1].Value + $Version + $m.Groups[3].Value })
    if ($updated -eq $content) { throw "无法更新 Cargo.lock 中的本地包版本: $CargoLockPath" }
    Write-TextFileUtf8 $CargoLockPath $updated
}

function Set-ProjectVersion {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$Version)
    $files = Get-ReleaseVersionFiles $RepoRoot
    Set-CargoPackageVersion $files.RootCargoToml $Version
    Set-CargoPackageVersion $files.TauriCargoToml $Version
    Set-TauriConfigVersion $files.TauriConfig $Version
    Set-CargoLockPackageVersions $files.CargoLock $Version
}

function Test-ReleaseTagExists {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$TagName)
    $local = Invoke-ExternalCommand git @("-C", $RepoRoot, "rev-parse", "-q", "--verify", "refs/tags/$TagName") -CaptureOutput -IgnoreExitCode
    if (-not [string]::IsNullOrWhiteSpace($local)) { return $true }
    $remote = Invoke-ExternalCommand git @("-C", $RepoRoot, "ls-remote", "--tags", "origin", "refs/tags/$TagName") -CaptureOutput -IgnoreExitCode
    return -not [string]::IsNullOrWhiteSpace($remote)
}

function Assert-BranchIsReleasable {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$ExpectedBranch)
    $branch = Invoke-ExternalCommand git @("-C", $RepoRoot, "branch", "--show-current") -CaptureOutput
    if ($branch -ne $ExpectedBranch) { throw "当前分支为 '$branch'，发布脚本只允许在 '$ExpectedBranch' 上执行" }
}

function Assert-WorkingTreeClean {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $status = Invoke-ExternalCommand git @("-C", $RepoRoot, "status", "--short") -CaptureOutput
    if (-not [string]::IsNullOrWhiteSpace($status)) { throw "工作区不干净，停止发布：`n$status" }
}

function Assert-GhAuthenticated { Invoke-ExternalCommand gh @("auth", "status") | Out-Null }

function Find-NsisInstaller {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $bundle = Join-Path $RepoRoot "src-tauri\target\release\bundle\nsis"
    $files = @(Get-ChildItem -LiteralPath $bundle -Filter "*.exe" -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -notmatch "uninstall" })
    if ($files.Count -ne 1) { throw "NSIS bundle 中应有且只有一个安装器，实际为 $($files.Count)：$bundle" }
    return $files[0].FullName
}

function New-ReleaseAsset {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$Version)
    $assetPath = Get-ReleaseAssetPath $RepoRoot $Version
    $assetDir = Split-Path $assetPath -Parent
    if ($script:DryRunMode) { Write-Host "[dry-run] ensure directory $assetDir" } else { New-Item -ItemType Directory -Force -Path $assetDir | Out-Null }
    if (-not $script:SkipBuildMode) {
        Write-Step "构建 Tauri NSIS GUI 安装器"
        Push-Location (Join-Path $RepoRoot "src-tauri")
        try { Invoke-ExternalCommand npx @("tauri", "build", "--bundles", "nsis") } finally { Pop-Location }
    }
    if ($script:DryRunMode) { Write-Host "[dry-run] copy NSIS installer to $assetPath"; return $assetPath }
    $installer = Find-NsisInstaller $RepoRoot
    Copy-Item -LiteralPath $installer -Destination $assetPath -Force
    if (-not (Test-Path -LiteralPath $assetPath -PathType Leaf)) { throw "未生成发布资产: $assetPath" }
    if ([System.IO.Path]::GetFileName($installer) -match 'yu\.exe') { throw "发布资产错误地指向已退役的 yu.exe" }
    return $assetPath
}

function Commit-VersionChangesIfNeeded {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$Version, [string]$Message = "Release v$Version")
    $paths = (Get-ReleaseVersionFiles $RepoRoot).TrackedPaths
    $changed = Invoke-ExternalCommand git (@("-C", $RepoRoot, "status", "--short", "--") + $paths) -CaptureOutput
    if ([string]::IsNullOrWhiteSpace($changed)) { return $false }
    Invoke-ExternalCommand git (@("-C", $RepoRoot, "add", "--") + $paths)
    Invoke-ExternalCommand git @("-C", $RepoRoot, "commit", "-m", $Message)
    return $true
}

function Ensure-TagAtHead {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$TagName)
    $head = Invoke-ExternalCommand git @("-C", $RepoRoot, "rev-parse", "HEAD") -CaptureOutput
    $tag = Invoke-ExternalCommand git @("-C", $RepoRoot, "rev-parse", "-q", "--verify", "refs/tags/$TagName^{commit}") -CaptureOutput -IgnoreExitCode
    if ([string]::IsNullOrWhiteSpace($tag)) { Invoke-ExternalCommand git @("-C", $RepoRoot, "tag", "-a", $TagName, "-m", "Release $TagName"); return }
    if ($tag -ne $head) { throw "tag $TagName 已存在，但不指向当前 HEAD($head)" }
}

function Push-ReleaseRefs {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [Parameter(Mandatory = $true)][string]$Branch, [Parameter(Mandatory = $true)][string]$TagName)
    if ($SkipPush) { Write-Host "[skip] 跳过 git push"; return }
    Invoke-ExternalCommand git @("-C", $RepoRoot, "push", "origin", $Branch)
    Invoke-ExternalCommand git @("-C", $RepoRoot, "push", "origin", $TagName)
}

function Publish-GitHubRelease {
    param([Parameter(Mandatory = $true)][string]$Repo, [Parameter(Mandatory = $true)][string]$TagName, [Parameter(Mandatory = $true)][string]$AssetPath, [string]$NotesFile = "")
    if ($SkipRelease) { Write-Host "[skip] 跳过 GitHub Release 发布"; return }
    $view = Invoke-ExternalCommand gh @("release", "view", $TagName, "--repo", $Repo) -CaptureOutput -IgnoreExitCode
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($view)) {
        $args = @("release", "create", $TagName, $AssetPath, "--repo", $Repo, "--title", $TagName)
        if ([string]::IsNullOrWhiteSpace($NotesFile)) { $args += "--generate-notes" } else { $args += @("--notes-file", $NotesFile) }
        Invoke-ExternalCommand gh $args
        return
    }
    if (-not [string]::IsNullOrWhiteSpace($NotesFile)) { Invoke-ExternalCommand gh @("release", "edit", $TagName, "--repo", $Repo, "--title", $TagName, "--notes-file", $NotesFile) }
    Invoke-ExternalCommand gh @("release", "upload", $TagName, $AssetPath, "--repo", $Repo, "--clobber")
}

function Invoke-PublishRelease {
    param([string]$RequestedVersion = "", [string]$Repo = "jasoft/rust_yu", [string]$Branch = "main", [string]$NotesFile = "", [switch]$SkipBuild, [switch]$SkipPush, [switch]$SkipRelease, [switch]$DryRun, [switch]$AllowDirty)
    $script:DryRunMode = [bool]$DryRun; $script:SkipBuildMode = [bool]$SkipBuild
    $repoRoot = Get-RepoRoot
    Ensure-CommandAvailable git; Ensure-CommandAvailable cargo; Ensure-CommandAvailable npx; Ensure-CommandAvailable gh
    if (-not $AllowDirty) { Assert-WorkingTreeClean $repoRoot }
    Assert-BranchIsReleasable $repoRoot $Branch
    if (-not $DryRun) { Assert-GhAuthenticated }
    $state = Get-ReleaseVersionState $repoRoot
    if (-not $state.IsConsistent) { throw "版本不一致：Cargo.toml=$($state.RootCargoVersion), src-tauri/Cargo.toml=$($state.TauriCargoVersion), tauri.conf.json=$($state.TauriConfigVersion)" }
    $releaseVersion = if ([string]::IsNullOrWhiteSpace($RequestedVersion)) { Get-NextAvailableReleaseVersion $state.Version { param($candidate) Test-ReleaseTagExists $repoRoot (Get-ReleaseTagName $candidate) } } else { $RequestedVersion }
    if (-not [string]::IsNullOrWhiteSpace($RequestedVersion) -and $releaseVersion -ne $state.Version) { throw "请求发布版本 $releaseVersion 与仓库版本 $($state.Version) 不一致" }
    if ($releaseVersion -ne $state.Version) { if ($DryRun) { Write-Host "[dry-run] update version files to $releaseVersion" } else { Set-ProjectVersion $repoRoot $releaseVersion } }
    $tag = Get-ReleaseTagName $releaseVersion
    $asset = New-ReleaseAsset $repoRoot $releaseVersion
    if (-not $DryRun) { $null = Commit-VersionChangesIfNeeded $repoRoot $releaseVersion }
    Ensure-TagAtHead $repoRoot $tag
    Push-ReleaseRefs $repoRoot $Branch $tag
    Publish-GitHubRelease $Repo $tag $asset $NotesFile
    if (-not $DryRun -and -not $SkipRelease) {
        $next = Get-NextPatchVersion $releaseVersion
        Set-ProjectVersion $repoRoot $next
        $committed = Commit-VersionChangesIfNeeded $repoRoot $next "Bump version to $next"
        if ($committed -and -not $SkipPush) { Invoke-ExternalCommand git @("-C", $repoRoot, "push", "origin", $Branch) }
    }
    Write-Host "发布完成：$releaseVersion / $asset" -ForegroundColor Green
}

if ($MyInvocation.InvocationName -ne ".") {
    Invoke-PublishRelease -RequestedVersion $Version -Repo $Repo -Branch $Branch -NotesFile $NotesFile -SkipBuild:$SkipBuild -SkipPush:$SkipPush -SkipRelease:$SkipRelease -DryRun:$DryRun -AllowDirty:$AllowDirty
}
