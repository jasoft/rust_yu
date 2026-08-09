$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$scriptPath = Join-Path $repoRoot "tools\release\package-portable.ps1"
$tokens = $null
$parseErrors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($scriptPath, [ref]$tokens, [ref]$parseErrors)
if (@($parseErrors).Count -gt 0) { throw "package-portable.ps1 解析失败: $($parseErrors | Out-String)" }

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rust-yu-portable-test-" + [Guid]::NewGuid().ToString("N"))
$fakeBinary = Join-Path $tempRoot "rust-yu-tauri.exe"
$output = Join-Path $tempRoot "out"
try {
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
    [System.IO.File]::WriteAllBytes($fakeBinary, [byte[]](0x4D, 0x5A, 0x90, 0x00))

    & $scriptPath -BinaryPath $fakeBinary -OutputDirectory $output

    $packageRoot = Join-Path $output "rust-yu-portable"
    $archive = Join-Path $output "rust-yu-portable.zip"
    foreach ($required in @(
        (Join-Path $packageRoot "rust-yu-tauri.exe"),
        (Join-Path $packageRoot "portable.flag"),
        (Join-Path $packageRoot "README.txt"),
        $archive
    )) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "便携版缺少输出: $required" }
    }

    $archiveEntries = Get-ChildItem -LiteralPath $archive -ErrorAction SilentlyContinue
    if ($null -eq $archiveEntries) { throw "无法读取便携版压缩包: $archive" }
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Portable package script tests passed" -ForegroundColor Green
