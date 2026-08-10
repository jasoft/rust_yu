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

function Use-X64RustupShim {
    $rustupBin = Join-Path $env:USERPROFILE ".cargo\bin"
    $rustupExecutable = Join-Path $rustupBin "rustup.exe"
    if (-not (Test-Path -LiteralPath $rustupExecutable -PathType Leaf)) {
        throw "未找到 Rustup X64 toolchain。请安装 stable-x86_64-pc-windows-msvc，并确保 $rustupExecutable 存在。"
    }
    # 提权后的 PowerShell 可能继承旧的 ARM GNU Rust PATH；将 Rustup shim 放在最前面，
    # 让仓库根目录的 rust-toolchain.toml 强制选择 X64 MSVC 编译器。
    $env:Path = "$rustupBin;$env:Path"
}

function Import-VsX64Environment {
    $vcVarsAll = Get-ChildItem "C:\Program Files (x86)\Microsoft Visual Studio" -Filter "vcvarsall.bat" -Recurse -File -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
    if ([string]::IsNullOrWhiteSpace($vcVarsAll)) {
        throw "未找到 Visual Studio vcvarsall.bat。请安装 Visual Studio Build Tools 的 Desktop development with C++ 工作负载。"
    }
    $environmentLines = cmd.exe /d /s /c "`"$vcVarsAll`" x64 >nul && set"
    foreach ($line in $environmentLines) {
        $separator = $line.IndexOf("=")
        if ($separator -gt 0) {
            [Environment]::SetEnvironmentVariable($line.Substring(0, $separator), $line.Substring($separator + 1), "Process")
        }
    }
}

if ($InitSubmodules) {
    git -C $repoRoot submodule update --init --recursive
    if ($LASTEXITCODE -ne 0) { throw "初始化 Git submodule 失败" }
}

$target = "x86_64-pc-windows-msvc"
Use-X64RustupShim
Import-VsX64Environment
$linker = Get-Command link.exe -ErrorAction SilentlyContinue
if ($null -eq $linker) {
    throw "未找到 x64 MSVC linker (link.exe)。请安装 Visual Studio Build Tools 的 Desktop development with C++ 工作负载，然后重新运行此脚本。"
}
$rustVersion = rustc -vV | Out-String
if ($rustVersion -notmatch "host:\s+x86_64-pc-windows-msvc") {
    throw "当前 Rust 编译器不是 x64 MSVC toolchain。请安装并启用 stable-x86_64-pc-windows-msvc，然后重新运行此脚本。`n$rustVersion"
}
Write-Host "已选择 x64 MSVC 工具链: $target"
Write-Host "linker: $($linker.Source)"

if (-not $SkipFrontend) {
    $tauriTools = Join-Path $repoRoot "src-tauri"
    npm --prefix $tauriTools ci
    if ($LASTEXITCODE -ne 0) { throw "Tauri CLI npm ci 失败" }
    $frontend = Join-Path $repoRoot "src-tauri\src-frontends\webui"
    npm --prefix $frontend ci
    if ($LASTEXITCODE -ne 0) { throw "WebUI npm ci 失败" }
}

if (-not $SkipCheck) {
    cargo check --workspace --target $target
    if ($LASTEXITCODE -ne 0) { throw "cargo check --workspace 失败" }
}

Write-Host "Rust Yu worktree 初始化完成。" -ForegroundColor Green
