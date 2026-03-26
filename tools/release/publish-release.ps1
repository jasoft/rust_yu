# Keep this script UTF-8 with BOM so Windows PowerShell 5.1 can parse Chinese strings.

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

function Write-Step {
    param([string]$Message)

    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Format-ArgumentForDisplay {
    param([string]$Value)

    if ($Value -match '\s') {
        return '"' + $Value.Replace('"', '\"') + '"'
    }

    return $Value
}

function Invoke-ExternalCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [string[]]$Arguments = @(),

        [switch]$CaptureOutput,

        [switch]$IgnoreExitCode
    )

    $displayArgs = ($Arguments | ForEach-Object { Format-ArgumentForDisplay $_ }) -join " "
    $displayCommand = if ([string]::IsNullOrWhiteSpace($displayArgs)) {
        $FilePath
    } else {
        "$FilePath $displayArgs"
    }

    if ($DryRun -and -not $CaptureOutput) {
        Write-Host "[dry-run] $displayCommand"
        return
    }

    $previousErrorActionPreference = $ErrorActionPreference
    if ($CaptureOutput) {
        try {
            $ErrorActionPreference = "Continue"
            $output = & $FilePath @Arguments 2>&1
        } finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        $exitCode = $LASTEXITCODE
        if (-not $IgnoreExitCode -and $exitCode -ne 0) {
            $joined = ($output | Out-String).Trim()
            throw "命令执行失败($exitCode): $displayCommand`n$joined"
        }
        return ($output | Out-String).Trim()
    }

    try {
        $ErrorActionPreference = "Continue"
        & $FilePath @Arguments
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $exitCode = $LASTEXITCODE
    if (-not $IgnoreExitCode -and $exitCode -ne 0) {
        throw "命令执行失败($exitCode): $displayCommand"
    }
}

function Ensure-CommandAvailable {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "未找到命令: $Name"
    }
}

function Get-RepoRoot {
    if ($PSScriptRoot) {
        return Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
    }

    return (Get-Location).Path
}

function Get-CargoPackageVersion {
    param([Parameter(Mandatory = $true)][string]$CargoTomlPath)

    $lines = Get-Content -LiteralPath $CargoTomlPath
    $insidePackage = $false
    foreach ($line in $lines) {
        if ($line -match '^\s*\[package\]\s*$') {
            $insidePackage = $true
            continue
        }
        if ($insidePackage -and $line -match '^\s*\[') {
            break
        }
        if ($insidePackage -and $line -match '^\s*version\s*=\s*"([^"]+)"\s*$') {
            return $matches[1]
        }
    }

    throw "无法从 Cargo.toml 读取 package.version: $CargoTomlPath"
}

function Get-TauriConfigVersion {
    param([Parameter(Mandatory = $true)][string]$TauriConfigPath)

    $config = Get-Content -LiteralPath $TauriConfigPath -Raw | ConvertFrom-Json
    if (-not $config.version) {
        throw "无法从 tauri.conf.json 读取 version: $TauriConfigPath"
    }

    return [string]$config.version
}

function Get-CliCommandVersion {
    param([Parameter(Mandatory = $true)][string]$CliMainPath)

    $content = Get-Content -LiteralPath $CliMainPath -Raw
    $match = [regex]::Match($content, '#\[command\(version\s*=\s*"([^"]+)"\)\]')
    if (-not $match.Success) {
        throw "无法从 main.rs 读取 clap version: $CliMainPath"
    }

    return $match.Groups[1].Value
}

function Get-ReleaseVersionFiles {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    return [pscustomobject]@{
        RootCargoToml  = Join-Path $RepoRoot "Cargo.toml"
        TauriCargoToml = Join-Path $RepoRoot "src-tauri\Cargo.toml"
        TauriConfig    = Join-Path $RepoRoot "src-tauri\tauri.conf.json"
        CliMainRs      = Join-Path $RepoRoot "src\main.rs"
        TrackedPaths   = @(
            "Cargo.toml",
            "src-tauri/Cargo.toml",
            "src-tauri/tauri.conf.json",
            "src/main.rs"
        )
    }
}

function Get-ReleaseVersionState {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    $files = Get-ReleaseVersionFiles -RepoRoot $RepoRoot
    $rootCargoVersion = Get-CargoPackageVersion -CargoTomlPath $files.RootCargoToml
    $tauriCargoVersion = Get-CargoPackageVersion -CargoTomlPath $files.TauriCargoToml
    $tauriConfigVersion = Get-TauriConfigVersion -TauriConfigPath $files.TauriConfig
    $cliVersion = Get-CliCommandVersion -CliMainPath $files.CliMainRs
    $isConsistent = @(
        $rootCargoVersion,
        $tauriCargoVersion,
        $tauriConfigVersion,
        $cliVersion
    ) | Select-Object -Unique | Measure-Object | Select-Object -ExpandProperty Count
    $isConsistent = $isConsistent -eq 1

    [pscustomobject]@{
        RootCargoVersion  = $rootCargoVersion
        TauriCargoVersion = $tauriCargoVersion
        TauriConfigVersion = $tauriConfigVersion
        CliVersion        = $cliVersion
        Version           = $rootCargoVersion
        IsConsistent      = $isConsistent
    }
}

function Get-NextPatchVersion {
    param([Parameter(Mandatory = $true)][string]$Version)

    if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
        throw "仅支持 major.minor.patch 形式的版本号: $Version"
    }

    $major = [int]$matches[1]
    $minor = [int]$matches[2]
    $patch = [int]$matches[3] + 1

    return "$major.$minor.$patch"
}

function Get-NextAvailableReleaseVersion {
    param(
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][scriptblock]$VersionExistsScript
    )

    $candidateVersion = $Version
    while (& $VersionExistsScript $candidateVersion) {
        $candidateVersion = Get-NextPatchVersion -Version $candidateVersion
    }

    return $candidateVersion
}

function Get-ReleaseAssetFileName {
    param([Parameter(Mandatory = $true)][string]$Version)

    return "yu-$Version-windows-amd64.zip"
}

function Get-ReleaseTagName {
    param([Parameter(Mandatory = $true)][string]$Version)

    return "v$Version"
}

function Get-ReleaseAssetPath {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Version
    )

    return Join-Path $RepoRoot ("dist\scoop\" + (Get-ReleaseAssetFileName -Version $Version))
}

function Set-CargoPackageVersion {
    param(
        [Parameter(Mandatory = $true)][string]$CargoTomlPath,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $lines = Get-Content -LiteralPath $CargoTomlPath
    $insidePackage = $false
    $updated = $false
    for ($index = 0; $index -lt $lines.Count; $index++) {
        $line = $lines[$index]
        if ($line -match '^\s*\[package\]\s*$') {
            $insidePackage = $true
            continue
        }
        if ($insidePackage -and $line -match '^\s*\[') {
            break
        }
        if ($insidePackage -and $line -match '^\s*version\s*=\s*"([^"]+)"\s*$') {
            $lines[$index] = $line -replace '"([^"]+)"', ('"' + $Version + '"')
            $updated = $true
            break
        }
    }

    if (-not $updated) {
        throw "无法更新 Cargo.toml 中的 package.version: $CargoTomlPath"
    }

    Set-Content -LiteralPath $CargoTomlPath -Value $lines
}

function Set-TauriConfigVersion {
    param(
        [Parameter(Mandatory = $true)][string]$TauriConfigPath,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $content = Get-Content -LiteralPath $TauriConfigPath -Raw
    $regex = [regex]'(?m)^(\s*"version"\s*:\s*")([^"]+)(")'
    $updatedContent = $regex.Replace(
        $content,
        {
            param($match)

            return $match.Groups[1].Value + $Version + $match.Groups[3].Value
        },
        1
    )

    if ($updatedContent -eq $content) {
        throw "无法更新 tauri.conf.json 中的 version: $TauriConfigPath"
    }

    Set-Content -LiteralPath $TauriConfigPath -Value $updatedContent
}

function Set-CliCommandVersion {
    param(
        [Parameter(Mandatory = $true)][string]$CliMainPath,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $content = Get-Content -LiteralPath $CliMainPath -Raw
    $updatedContent = [regex]::Replace(
        $content,
        '#\[command\(version\s*=\s*"[^"]+"\)\]',
        '#[command(version = "' + $Version + '")]',
        1
    )

    if ($updatedContent -eq $content) {
        throw "无法更新 main.rs 中的 clap version: $CliMainPath"
    }

    Set-Content -LiteralPath $CliMainPath -Value $updatedContent
}

function Set-ProjectVersion {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $files = Get-ReleaseVersionFiles -RepoRoot $RepoRoot
    Set-CargoPackageVersion -CargoTomlPath $files.RootCargoToml -Version $Version
    Set-CargoPackageVersion -CargoTomlPath $files.TauriCargoToml -Version $Version
    Set-TauriConfigVersion -TauriConfigPath $files.TauriConfig -Version $Version
    Set-CliCommandVersion -CliMainPath $files.CliMainRs -Version $Version
}

function Test-ReleaseTagExists {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$TagName
    )

    $localTag = Invoke-ExternalCommand -FilePath git -Arguments @(
        "-C", $RepoRoot, "rev-parse", "-q", "--verify", "refs/tags/$TagName"
    ) -CaptureOutput -IgnoreExitCode
    if (-not [string]::IsNullOrWhiteSpace($localTag)) {
        return $true
    }

    $remoteTag = Invoke-ExternalCommand -FilePath git -Arguments @(
        "-C", $RepoRoot, "ls-remote", "--tags", "origin", "refs/tags/$TagName"
    ) -CaptureOutput -IgnoreExitCode
    return -not [string]::IsNullOrWhiteSpace($remoteTag)
}

function Assert-BranchIsReleasable {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$ExpectedBranch
    )

    $branch = Invoke-ExternalCommand -FilePath git -Arguments @("-C", $RepoRoot, "branch", "--show-current") -CaptureOutput
    if ($branch -ne $ExpectedBranch) {
        throw "当前分支为 '$branch'，发布脚本只允许在 '$ExpectedBranch' 上执行"
    }
}

function Assert-WorkingTreeClean {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    $status = Invoke-ExternalCommand -FilePath git -Arguments @("-C", $RepoRoot, "status", "--short") -CaptureOutput
    if (-not [string]::IsNullOrWhiteSpace($status)) {
        throw "工作区不干净，停止发布：`n$status"
    }
}

function Assert-GhAuthenticated {
    Invoke-ExternalCommand -FilePath gh -Arguments @("auth", "status") | Out-Null
}

function New-ReleaseAsset {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $assetPath = Get-ReleaseAssetPath -RepoRoot $RepoRoot -Version $Version
    $assetDir = Split-Path -Path $assetPath -Parent

    if (-not $DryRun) {
        New-Item -ItemType Directory -Force -Path $assetDir | Out-Null
    } else {
        Write-Host "[dry-run] ensure directory $assetDir"
    }

    if (-not $SkipBuild) {
        Write-Step "构建 yu.exe"
        Invoke-ExternalCommand -FilePath cargo -Arguments @("build", "--release", "--bin", "yu")
    }

    $binaryPath = Join-Path $RepoRoot "target\release\yu.exe"
    if (-not $DryRun -and -not (Test-Path -LiteralPath $binaryPath)) {
        throw "未找到构建产物: $binaryPath"
    }

    if (-not $DryRun -and (Test-Path -LiteralPath $assetPath)) {
        Remove-Item -LiteralPath $assetPath -Force
    }

    Write-Step "打包发布资产"
    Invoke-ExternalCommand -FilePath powershell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        "Compress-Archive -Path '$binaryPath' -DestinationPath '$assetPath' -Force"
    )

    if (-not $DryRun -and -not (Test-Path -LiteralPath $assetPath)) {
        throw "未生成发布资产: $assetPath"
    }

    return $assetPath
}

function Update-ScoopManifest {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$AssetPath,
        [Parameter(Mandatory = $true)][string]$Repo
    )

    Write-Step "更新 Scoop manifest"
    Invoke-ExternalCommand -FilePath powershell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        (Join-Path $RepoRoot "tools\scoop\update-manifest.ps1"),
        "-Version",
        $Version,
        "-Repo",
        $Repo,
        "-AssetPath",
        $AssetPath
    )
}

function Commit-ReleaseChangesIfNeeded {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $trackedPaths = @("bucket/yu.json") + (Get-ReleaseVersionFiles -RepoRoot $RepoRoot).TrackedPaths
    $gitStatusArgs = @("-C", $RepoRoot, "status", "--short", "--") + $trackedPaths
    $changed = Invoke-ExternalCommand -FilePath git -Arguments $gitStatusArgs -CaptureOutput

    if ([string]::IsNullOrWhiteSpace($changed)) {
        return $false
    }

    Write-Step "提交 release 变更"
    $gitAddArgs = @("-C", $RepoRoot, "add", "--") + $trackedPaths
    Invoke-ExternalCommand -FilePath git -Arguments $gitAddArgs

    Invoke-ExternalCommand -FilePath git -Arguments @(
        "-C", $RepoRoot, "commit", "-m", "Release v$Version"
    )
    return $true
}

function Commit-VersionBumpIfNeeded {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $trackedPaths = (Get-ReleaseVersionFiles -RepoRoot $RepoRoot).TrackedPaths
    $gitStatusArgs = @("-C", $RepoRoot, "status", "--short", "--") + $trackedPaths
    $changed = Invoke-ExternalCommand -FilePath git -Arguments $gitStatusArgs -CaptureOutput

    if ([string]::IsNullOrWhiteSpace($changed)) {
        return $false
    }

    Write-Step "提交版本推进"
    $gitAddArgs = @("-C", $RepoRoot, "add", "--") + $trackedPaths
    Invoke-ExternalCommand -FilePath git -Arguments $gitAddArgs
    Invoke-ExternalCommand -FilePath git -Arguments @(
        "-C", $RepoRoot, "commit", "-m", "Bump version to $Version"
    )
    return $true
}

function Ensure-TagAtHead {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$TagName
    )

    $headSha = Invoke-ExternalCommand -FilePath git -Arguments @("-C", $RepoRoot, "rev-parse", "HEAD") -CaptureOutput
    $tagSha = Invoke-ExternalCommand -FilePath git -Arguments @(
        "-C", $RepoRoot, "rev-parse", "-q", "--verify", "refs/tags/$TagName^{commit}"
    ) -CaptureOutput -IgnoreExitCode

    if ([string]::IsNullOrWhiteSpace($tagSha)) {
        Write-Step "创建 tag $TagName"
        Invoke-ExternalCommand -FilePath git -Arguments @(
            "-C", $RepoRoot, "tag", "-a", $TagName, "-m", "Release $TagName"
        )
        return
    }

    if ($tagSha -ne $headSha) {
        throw "tag $TagName 已存在，但不指向当前 HEAD($headSha)"
    }
}

function Push-ReleaseRefs {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Branch,
        [Parameter(Mandatory = $true)][string]$TagName
    )

    if ($SkipPush) {
        Write-Host "[skip] 跳过 git push"
        return
    }

    Write-Step "推送分支与 tag"
    Invoke-ExternalCommand -FilePath git -Arguments @("-C", $RepoRoot, "push", "origin", $Branch)
    Invoke-ExternalCommand -FilePath git -Arguments @("-C", $RepoRoot, "push", "origin", $TagName)
}

function Publish-GitHubRelease {
    param(
        [Parameter(Mandatory = $true)][string]$Repo,
        [Parameter(Mandatory = $true)][string]$TagName,
        [Parameter(Mandatory = $true)][string]$AssetPath,
        [string]$NotesFile = ""
    )

    if ($SkipRelease) {
        Write-Host "[skip] 跳过 GitHub Release 发布"
        return
    }

    $releaseExists = $true
    $viewOutput = Invoke-ExternalCommand -FilePath gh -Arguments @(
        "release", "view", $TagName, "--repo", $Repo
    ) -CaptureOutput -IgnoreExitCode
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($viewOutput)) {
        $releaseExists = $false
    }

    if (-not $releaseExists) {
        Write-Step "创建 GitHub Release"
        $createArgs = @("release", "create", $TagName, $AssetPath, "--repo", $Repo, "--title", $TagName)
        if ([string]::IsNullOrWhiteSpace($NotesFile)) {
            $createArgs += "--generate-notes"
        } else {
            if (-not (Test-Path -LiteralPath $NotesFile)) {
                throw "未找到 release notes 文件: $NotesFile"
            }
            $createArgs += @("--notes-file", $NotesFile)
        }
        Invoke-ExternalCommand -FilePath gh -Arguments $createArgs
        return
    }

    Write-Step "更新 GitHub Release 资产"
    if (-not [string]::IsNullOrWhiteSpace($NotesFile)) {
        if (-not (Test-Path -LiteralPath $NotesFile)) {
            throw "未找到 release notes 文件: $NotesFile"
        }
        Invoke-ExternalCommand -FilePath gh -Arguments @(
            "release", "edit", $TagName, "--repo", $Repo, "--title", $TagName, "--notes-file", $NotesFile
        )
    }

    Invoke-ExternalCommand -FilePath gh -Arguments @(
        "release", "upload", $TagName, $AssetPath, "--repo", $Repo, "--clobber"
    )
}

function Invoke-PublishRelease {
    param(
        [string]$RequestedVersion = "",
        [string]$Repo = "jasoft/rust_yu",
        [string]$Branch = "main",
        [string]$NotesFile = "",
        [switch]$SkipBuild,
        [switch]$SkipPush,
        [switch]$SkipRelease,
        [switch]$DryRun,
        [switch]$AllowDirty
    )

    $repoRoot = Get-RepoRoot

    Write-Step "检查发布环境"
    Ensure-CommandAvailable -Name git
    Ensure-CommandAvailable -Name cargo
    Ensure-CommandAvailable -Name gh

    if (-not $AllowDirty) {
        Assert-WorkingTreeClean -RepoRoot $repoRoot
    }
    Assert-BranchIsReleasable -RepoRoot $repoRoot -ExpectedBranch $Branch
    Assert-GhAuthenticated

    $versionState = Get-ReleaseVersionState -RepoRoot $repoRoot
    if (-not $versionState.IsConsistent) {
        throw "版本不一致：Cargo.toml=$($versionState.RootCargoVersion), src-tauri/Cargo.toml=$($versionState.TauriCargoVersion), tauri.conf.json=$($versionState.TauriConfigVersion), src/main.rs=$($versionState.CliVersion)"
    }

    $releaseVersion = if ([string]::IsNullOrWhiteSpace($RequestedVersion)) {
        Get-NextAvailableReleaseVersion -Version $versionState.Version -VersionExistsScript {
            param($candidateVersion)

            $candidateTagName = Get-ReleaseTagName -Version $candidateVersion
            return Test-ReleaseTagExists -RepoRoot $repoRoot -TagName $candidateTagName
        }
    } else {
        $RequestedVersion
    }

    if (-not [string]::IsNullOrWhiteSpace($RequestedVersion) -and $releaseVersion -ne $versionState.Version) {
        throw "请求发布版本 $releaseVersion 与仓库版本 $($versionState.Version) 不一致"
    }

    if ($releaseVersion -ne $versionState.Version) {
        Write-Step "自动递增发布版本到 $releaseVersion"
        if ($DryRun) {
            Write-Host "[dry-run] update version files to $releaseVersion"
        } else {
            Set-ProjectVersion -RepoRoot $repoRoot -Version $releaseVersion
        }
    }

    $tagName = Get-ReleaseTagName -Version $releaseVersion
    $assetPath = New-ReleaseAsset -RepoRoot $repoRoot -Version $releaseVersion
    Update-ScoopManifest -RepoRoot $repoRoot -Version $releaseVersion -AssetPath $assetPath -Repo $Repo
    $null = Commit-ReleaseChangesIfNeeded -RepoRoot $repoRoot -Version $releaseVersion
    Ensure-TagAtHead -RepoRoot $repoRoot -TagName $tagName
    Push-ReleaseRefs -RepoRoot $repoRoot -Branch $Branch -TagName $tagName
    Publish-GitHubRelease -Repo $Repo -TagName $tagName -AssetPath $assetPath -NotesFile $NotesFile

    if (-not $DryRun -and -not $SkipRelease) {
        $nextVersion = Get-NextPatchVersion -Version $releaseVersion
        Write-Step "发布后自动推进版本到 $nextVersion"
        Set-ProjectVersion -RepoRoot $repoRoot -Version $nextVersion
        $versionCommitted = Commit-VersionBumpIfNeeded -RepoRoot $repoRoot -Version $nextVersion
        if ($versionCommitted -and -not $SkipPush) {
            Write-Step "推送版本推进提交"
            Invoke-ExternalCommand -FilePath git -Arguments @("-C", $repoRoot, "push", "origin", $Branch)
        }
    }

    Write-Step "发布完成"
    Write-Host "Version : $releaseVersion"
    Write-Host "Tag     : $tagName"
    Write-Host "Asset   : $assetPath"
    Write-Host "Repo    : $Repo"
}

if ($MyInvocation.InvocationName -ne ".") {
    Invoke-PublishRelease `
        -RequestedVersion $Version `
        -Repo $Repo `
        -Branch $Branch `
        -NotesFile $NotesFile `
        -SkipBuild:$SkipBuild `
        -SkipPush:$SkipPush `
        -SkipRelease:$SkipRelease `
        -DryRun:$DryRun `
        -AllowDirty:$AllowDirty
}
