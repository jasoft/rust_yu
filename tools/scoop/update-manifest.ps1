param(
    [Parameter(Mandatory = $true)]
    [string]$Version,

    [string]$Repo = "jasoft/rust_yu",

    [string]$ManifestPath = "bucket/yu.json",

    [string]$AssetPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($AssetPath)) {
    $AssetPath = "dist/scoop/yu-$Version-windows-amd64.zip"
}

if (-not (Test-Path -LiteralPath $AssetPath)) {
    throw "未找到发布资产: $AssetPath"
}

$hash = (Get-FileHash -LiteralPath $AssetPath -Algorithm SHA256).Hash.ToLowerInvariant()
$downloadUrl = "https://github.com/$Repo/releases/download/v$Version/yu-$Version-windows-amd64.zip"

$manifest = [ordered]@{
    version      = $Version
    description  = "Windows uninstaller CLI for listing installed programs, preparing uninstall actions, scanning leftovers, and generating reports."
    homepage     = "https://github.com/$Repo"
    license      = "MIT"
    architecture = [ordered]@{
        "64bit" = [ordered]@{
            url  = $downloadUrl
            hash = $hash
        }
    }
    bin          = "yu.exe"
    checkver     = [ordered]@{
        github = "https://github.com/$Repo"
    }
    autoupdate   = [ordered]@{
        architecture = [ordered]@{
            "64bit" = [ordered]@{
                url = "https://github.com/$Repo/releases/download/v`$version/yu-`$version-windows-amd64.zip"
            }
        }
    }
}

$manifestJson = $manifest | ConvertTo-Json -Depth 8
$manifestJson | Set-Content -LiteralPath $ManifestPath -Encoding ascii

Write-Host "Updated $ManifestPath"
Write-Host "Version: $Version"
Write-Host "Asset: $AssetPath"
Write-Host "SHA256: $hash"
