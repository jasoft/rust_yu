#requires -Version 7.0

[CmdletBinding()]
param(
    [switch]$VerifyWorkspace,
    [string]$LogPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not [string]::IsNullOrWhiteSpace($LogPath)) {
    Start-Transcript -LiteralPath $LogPath -Force | Out-Null
}

function Get-VsX64Environment {
    $vsWhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
    $vcVarsAll = $null
    if (Test-Path -LiteralPath $vsWhere -PathType Leaf) {
        $installationPath = & $vsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($installationPath)) {
            $candidate = Join-Path $installationPath.Trim() "VC\Auxiliary\Build\vcvarsall.bat"
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                $vcVarsAll = $candidate
            }
        }
    }
    if ([string]::IsNullOrWhiteSpace($vcVarsAll)) {
        $vcVarsAll = Get-ChildItem "C:\Program Files (x86)\Microsoft Visual Studio" -Filter "vcvarsall.bat" -Recurse -File -ErrorAction SilentlyContinue |
            Select-Object -First 1 -ExpandProperty FullName
    }
    if ([string]::IsNullOrWhiteSpace($vcVarsAll)) {
        throw "未找到 Visual Studio x64 MSVC 工具链。"
    }

    $vcVarsCommand = '"' + $vcVarsAll + '" x64 >nul && set'
    $lines = cmd.exe /d /s /c $vcVarsCommand
    $environment = @{}
    foreach ($line in $lines) {
        $separator = $line.IndexOf("=")
        if ($separator -gt 0) {
            $environment[$line.Substring(0, $separator)] = $line.Substring($separator + 1)
        }
    }
    return $environment
}

function Add-UniquePathEntry {
    param(
        [System.Collections.Generic.List[string]]$Entries,
        [string]$Value
    )
    if ([string]::IsNullOrWhiteSpace($Value)) { return }
    if (-not ($Entries | Where-Object { $_.TrimEnd('\').Equals($Value.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) })) {
        $Entries.Add($Value)
    }
}

function Set-UserEnvironmentValue {
    param([string]$Name, [string]$Value)
    [Environment]::SetEnvironmentVariable($Name, $Value, "User")
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "请在管理员 PowerShell 7（pwsh.exe）中运行此脚本，以修改机器 PATH。"
}

$rustup = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (-not (Test-Path -LiteralPath $rustup -PathType Leaf)) {
    throw "未找到 Rustup：$rustup"
}

$x64Toolchain = "stable-x86_64-pc-windows-msvc"
$toolchains = & $rustup toolchain list
if (-not ($toolchains -match [regex]::Escape($x64Toolchain))) {
    & $rustup toolchain install $x64Toolchain --profile minimal --force-non-host
    if ($LASTEXITCODE -ne 0) { throw "安装 x64 MSVC Rust toolchain 失败。" }
}
& $rustup set default-host x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw "设置 Rustup 默认 host 失败。" }
& $rustup default $x64Toolchain --force-non-host
if ($LASTEXITCODE -ne 0) { throw "设置 x64 MSVC Rust 默认 toolchain 失败。" }

$nodeCandidates = @(
    $env:RUST_YU_NODE_X64,
    (Join-Path $env:USERPROFILE ".codex\runtimes\node-v25.9.0-win-x64\node.exe"),
    (Join-Path $env:ProgramFiles "nodejs\node.exe"),
    (Join-Path $env:LOCALAPPDATA "Programs\nodejs\node.exe")
)
$x64Node = $null
foreach ($candidate in ($nodeCandidates | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)) {
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) { continue }
    $architecture = & $candidate -p "process.arch" 2>$null
    if ($LASTEXITCODE -eq 0 -and $architecture -eq "x64") {
        $x64Node = (Resolve-Path -LiteralPath $candidate).Path
        break
    }
}
if ([string]::IsNullOrWhiteSpace($x64Node)) {
    throw "未找到 x64 Node.js。请安装 Windows x64 Node.js，或设置 RUST_YU_NODE_X64。"
}
$nodeDir = Split-Path -Parent $x64Node
if (-not (Test-Path -LiteralPath (Join-Path $nodeDir "npm.cmd") -PathType Leaf)) {
    throw "x64 Node.js 目录缺少 npm.cmd：$nodeDir"
}

$vsEnvironment = Get-VsX64Environment
$linker = Join-Path $vsEnvironment.VCToolsInstallDir "bin\HostX64\x64"
if (-not (Test-Path -LiteralPath (Join-Path $linker "link.exe") -PathType Leaf)) {
    throw "x64 MSVC linker 不存在：$linker\link.exe"
}

$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine") -split ";" |
    Where-Object {
        $_ -and
        $_ -notmatch "Rust stable LLVM" -and
        $_ -notmatch "llvm-mingw.*aarch64" -and
        $_.TrimEnd('\') -ne "C:\Program Files\nodejs"
    }
[Environment]::SetEnvironmentVariable("Path", ($machinePath -join ";"), "Machine")

$userEntries = [System.Collections.Generic.List[string]]::new()
foreach ($entry in ([Environment]::GetEnvironmentVariable("Path", "User") -split ";")) {
    if ([string]::IsNullOrWhiteSpace($entry)) { continue }
    if ($entry -match "Rust stable LLVM|llvm-mingw.*aarch64|nodejs") { continue }
    Add-UniquePathEntry -Entries $userEntries -Value $entry
}
$preferredEntries = @($nodeDir, $cargoBin, $linker)
$combinedEntries = [System.Collections.Generic.List[string]]::new()
foreach ($entry in $preferredEntries) { Add-UniquePathEntry -Entries $combinedEntries -Value $entry }
foreach ($entry in $userEntries) { Add-UniquePathEntry -Entries $combinedEntries -Value $entry }
Set-UserEnvironmentValue -Name "Path" -Value ($combinedEntries -join ";")
Set-UserEnvironmentValue -Name "RUSTUP_TOOLCHAIN" -Value $x64Toolchain
Set-UserEnvironmentValue -Name "RUST_YU_NODE_X64" -Value $x64Node

# 将 x64 MSVC/Windows SDK 的必要变量持久化，确保新 Terminal 直接运行 cargo 时能找到 link.exe、头文件和库。
foreach ($name in @("INCLUDE", "LIB", "LIBPATH", "VCToolsInstallDir", "VCToolsVersion", "VCINSTALLDIR", "WindowsSdkDir", "WindowsSDKVersion", "UniversalCRTSdkDir", "UCRTVersion", "VSCMD_ARG_TGT_ARCH", "PreferredToolArchitecture")) {
    if ($vsEnvironment.ContainsKey($name)) {
        Set-UserEnvironmentValue -Name $name -Value $vsEnvironment[$name]
    }
}

# 当前管理员进程也立即切换，便于脚本结束前验证，而不必等待新 Terminal。
$env:Path = "$nodeDir;$cargoBin;$linker;" + ($vsEnvironment.Path -replace [regex]::Escape($nodeDir), "")
$env:RUSTUP_TOOLCHAIN = $x64Toolchain
foreach ($name in @("INCLUDE", "LIB", "LIBPATH", "VCToolsInstallDir", "VCToolsVersion", "VCINSTALLDIR", "WindowsSdkDir", "WindowsSDKVersion", "UniversalCRTSdkDir", "UCRTVersion", "VSCMD_ARG_TGT_ARCH", "PreferredToolArchitecture")) {
    if ($vsEnvironment.ContainsKey($name)) { [Environment]::SetEnvironmentVariable($name, $vsEnvironment[$name], "Process") }
}

Write-Host "已将当前用户和机器编译环境固定为 X64。" -ForegroundColor Green
Write-Host "Rust: $(& $rustup show active-toolchain)"
Write-Host "Node: $x64Node ($(& $x64Node -p 'process.arch'))"
Write-Host "Linker: $linker\link.exe"

if ($VerifyWorkspace) {
    $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    Push-Location $repoRoot
    try {
        cargo check --workspace
        if ($LASTEXITCODE -ne 0) { throw "cargo check --workspace 失败。" }
    } finally {
        Pop-Location
    }
}
