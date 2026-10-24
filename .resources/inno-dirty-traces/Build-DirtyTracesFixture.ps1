param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# 强制 x64 MSVC 工具链（AGENTS.md §2A）：本机 PATH 可能优先命中 ARM Rust，
# 一律走 rustup 的 x64 代理并核对 host。
$rustcPath = Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe'
if (-not (Test-Path $rustcPath)) {
    throw "Missing x64 rustc: $rustcPath"
}
$hostLine = (& $rustcPath -vV | Select-String '^host:').Line
if ($hostLine -notmatch 'x86_64-pc-windows-msvc') {
    throw "rustc host must be x86_64-pc-windows-msvc, got: $hostLine"
}

$fixtureRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$isccPath = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
if (-not (Test-Path $isccPath)) {
    throw "Missing ISCC.exe: $isccPath"
}

function Get-PeMachine {
    param([Parameter(Mandatory = $true)][string]$Path)
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 0x40 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
        throw "Not a PE file: $Path"
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
    return [BitConverter]::ToUInt16($bytes, $peOffset + 4)
}

function Test-Amd64Pe {
    param([Parameter(Mandatory = $true)][string]$Path)
    $machine = Get-PeMachine -Path $Path
    if ($machine -ne 0x8664) {
        throw ("PE machine is not AMD64 (0x8664): {0} (0x{1:X4})" -f $Path, $machine)
    }
}

$targets = @(
    @{ Source = Join-Path $fixtureRoot 'payload\tools\RustYuDirtyDaemon.rs';  Output = Join-Path $fixtureRoot 'payload\app\RustYuDirtyDaemon.exe' },
    @{ Source = Join-Path $fixtureRoot 'payload\tools\RustYuDirtyService.rs'; Output = Join-Path $fixtureRoot 'payload\app\RustYuDirtyService.exe' }
)
foreach ($target in $targets) {
    if (-not (Test-Path $target.Source)) {
        throw "Missing helper source: $($target.Source)"
    }
    & $rustcPath --edition 2021 $target.Source -C opt-level=z -o $target.Output
    if ($LASTEXITCODE -ne 0) {
        throw "rustc failed for $($target.Source)"
    }
    Test-Amd64Pe -Path $target.Output
}

& $isccPath (Join-Path $fixtureRoot 'DirtyTracesFixture.iss')
if ($LASTEXITCODE -ne 0) {
    throw 'ISCC.exe failed'
}

# 安装包壳是 Inno Setup 6 的标准 win32 引导桩（0x014C），只做引导；
# 真正进入系统的 payload（上面的两个 exe）已强制校验为 AMD64。这里只确认
# 壳是合法 PE，防止产物被意外替换成其他架构的安装器。
$setupMachine = Get-PeMachine -Path (Join-Path $fixtureRoot 'output\RustYuDirtyTracesSetup.exe')
if ($setupMachine -ne 0x8664 -and $setupMachine -ne 0x014C) {
    throw ("Unexpected setup stub PE machine: 0x{0:X4}" -f $setupMachine)
}

Write-Output 'fixture-built'
