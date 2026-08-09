# Rust Yu 便携版打包脚本。兼容 Windows PowerShell 5.1。
[CmdletBinding()]
param(
    [string]$BinaryPath = (Join-Path $PSScriptRoot "..\..\src-tauri\target\release\rust-yu-tauri.exe"),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot "..\..\dist\portable"),
    [string]$PackageName = "rust-yu-portable",
    [switch]$SkipArchive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$binary = (Resolve-Path -LiteralPath $BinaryPath -ErrorAction Stop).Path
if ([System.IO.Path]::GetExtension($binary) -ine ".exe") {
    throw "便携版输入必须是 Windows EXE: $binary"
}

$outputRoot = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$packageRoot = Join-Path $outputRoot $PackageName
if (Test-Path -LiteralPath $packageRoot) {
    throw "便携版目录已存在，为避免覆盖用户文件而停止: $packageRoot"
}

New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $packageRoot ([System.IO.Path]::GetFileName($binary)))
New-Item -ItemType File -Path (Join-Path $packageRoot "portable.flag") -Force | Out-Null
$readme = @(
    "Rust Yu Portable Edition",
    "",
    "Run the EXE directly. The first run creates data\ next to the EXE for cache, logs, backups, monitor sessions, and reports.",
    "Portable mode does not register a Rust Yu administrator task. Uninstalling a target may still request UAC.",
    "Keep this directory writable. Do not run from inside an archive or place data\ in a shared writable directory."
) -join [Environment]::NewLine
[System.IO.File]::WriteAllText(
    (Join-Path $packageRoot "README.txt"),
    $readme,
    (New-Object System.Text.UTF8Encoding($false))
)

$archive = Join-Path $outputRoot ("$PackageName.zip")
if (-not $SkipArchive) {
    if (Test-Path -LiteralPath $archive) {
        throw "便携版压缩包已存在，为避免覆盖用户文件而停止: $archive"
    }
    Compress-Archive -Path (Join-Path $packageRoot "*") -DestinationPath $archive -CompressionLevel Optimal
}

Write-Host "便携版目录: $packageRoot" -ForegroundColor Green
if (-not $SkipArchive) { Write-Host "便携版压缩包: $archive" -ForegroundColor Green }
