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

    if ($CaptureOutput) {
        $output = & $FilePath @Arguments 2>&1
        $exitCode = $LASTEXITCODE
        if (-not $IgnoreExitCode -and $exitCode -ne 0) {
            $joined = ($output | Out-String).Trim()
            throw "命令执行失败($exitCode): $displayCommand`n$joined"
        }
        return ($output | Out-String).Trim()
    }

    & $FilePath @Arguments
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

function Get-ReleaseVersionState {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    $cargoVersion = Get-CargoPackageVersion -CargoTomlPath (Join-Path $RepoRoot "Cargo.toml")
    $tauriVersion = Get-TauriConfigVersion -TauriConfigPath (Join-Path $RepoRoot "src-tauri\tauri.conf.json")
    $isConsistent = $cargoVersion -eq $tauriVersion

    [pscustomobject]@{
        CargoVersion = $cargoVersion
        TauriVersion = $tauriVersion
        Version      = $cargoVersion
        IsConsistent = $isConsistent
    }
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

function Commit-ManifestIfNeeded {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $manifestPath = "bucket/yu.json"
    $changed = Invoke-ExternalCommand -FilePath git -Arguments @(
        "-C", $RepoRoot, "status", "--short", "--", $manifestPath
    ) -CaptureOutput

    if ([string]::IsNullOrWhiteSpace($changed)) {
        return $false
    }

    Write-Step "提交 manifest 更新"
    Invoke-ExternalCommand -FilePath git -Arguments @("-C", $RepoRoot, "add", "--", $manifestPath)
    Invoke-ExternalCommand -FilePath git -Arguments @(
        "-C", $RepoRoot, "commit", "-m", "Release v$Version"
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
        "-C", $RepoRoot, "rev-list", "-n", "1", $TagName
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
        throw "版本不一致：Cargo.toml=$($versionState.CargoVersion), tauri.conf.json=$($versionState.TauriVersion)"
    }

    $releaseVersion = if ([string]::IsNullOrWhiteSpace($RequestedVersion)) {
        $versionState.Version
    } else {
        $RequestedVersion
    }

    if ($releaseVersion -ne $versionState.Version) {
        throw "请求发布版本 $releaseVersion 与仓库版本 $($versionState.Version) 不一致"
    }

    $tagName = Get-ReleaseTagName -Version $releaseVersion
    $assetPath = New-ReleaseAsset -RepoRoot $repoRoot -Version $releaseVersion
    Update-ScoopManifest -RepoRoot $repoRoot -Version $releaseVersion -AssetPath $assetPath -Repo $Repo
    $null = Commit-ManifestIfNeeded -RepoRoot $repoRoot -Version $releaseVersion
    Ensure-TagAtHead -RepoRoot $repoRoot -TagName $tagName
    Push-ReleaseRefs -RepoRoot $repoRoot -Branch $Branch -TagName $tagName
    Publish-GitHubRelease -Repo $Repo -TagName $tagName -AssetPath $assetPath -NotesFile $NotesFile

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
