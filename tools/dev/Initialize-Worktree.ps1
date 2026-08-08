[CmdletBinding()]
param(
    [switch]$SkipFrontend,
    [switch]$SkipCheck,
    [switch]$InitSubmodules
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Set-Location $repoRoot

function Find-LlvmMingwBin {
    $candidates = [System.Collections.Generic.List[string]]::new()
    $configuredRoot = [Environment]::GetEnvironmentVariable("LLVM_MINGW_HOME")
    if (-not [string]::IsNullOrWhiteSpace($configuredRoot)) {
        $candidates.Add((Join-Path $configuredRoot "bin"))
    }
    $wingetRoot = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages"
    if (Test-Path -LiteralPath $wingetRoot) {
        Get-ChildItem -LiteralPath $wingetRoot -Directory -Filter "MartinStorsjo.LLVM-MinGW*" |
            ForEach-Object { $candidates.Add($_.FullName) }
    }
    foreach ($candidate in $candidates | Select-Object -Unique) {
        if (-not (Test-Path -LiteralPath $candidate -PathType Container)) {
            continue
        }
        $windres = Get-ChildItem -LiteralPath $candidate -Recurse -Filter "windres.exe" -File -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($null -ne $windres) {
            return $windres.Directory.FullName
        }
    }
    throw "未找到支持 ARM64 GNU 资源编译的 LLVM-MinGW。请安装 LLVM-MinGW，或设置 LLVM_MINGW_HOME。"
}

if ($InitSubmodules) {
    git -C $repoRoot submodule update --init --recursive
    if ($LASTEXITCODE -ne 0) { throw "初始化 Git submodule 失败" }
}

$llvmBin = Find-LlvmMingwBin
$env:MINGW_CHOST = "aarch64-w64-mingw32"
$env:Path = "$llvmBin;$env:Path"
Write-Host "已设置 ARM64 资源编译器: $llvmBin\windres.exe"
Write-Host "MINGW_CHOST=$env:MINGW_CHOST"

if (-not $SkipFrontend) {
    $frontend = Join-Path $repoRoot "src-tauri\src-frontends\webui"
    npm --prefix $frontend ci
    if ($LASTEXITCODE -ne 0) { throw "WebUI npm ci 失败" }
}

if (-not $SkipCheck) {
    cargo check --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo check --workspace 失败" }
}

Write-Host "Rust Yu worktree 初始化完成。" -ForegroundColor Green
