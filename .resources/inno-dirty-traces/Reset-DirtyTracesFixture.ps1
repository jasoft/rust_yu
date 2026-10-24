param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# 脏卸载夹具的全量清理：结束常驻进程、停止/删除服务、删除计划任务与防火墙规则、
# 清掉全部注册表残留和文件。只针对夹具自有的固定名称，绝不触碰共享路径。
# 需要管理员 PowerShell（服务 / HKLM / 公共桌面）。

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Reset-DirtyTracesFixture.ps1 requires an elevated PowerShell session'
}

$serviceName = 'RustYuDirtyTraces'
$taskName = 'RustYuDirtyTracesStartTask'
$firewallRuleName = 'RustYu Dirty Traces Daemon'
$arpKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1'
$defaultInstallDir = Join-Path $env:ProgramFiles 'RustYu Dirty Traces Test App'

# 安装目录优先从 ARP 键解析（用户可能改过安装路径），键不存在时退回默认值
$installDir = $defaultInstallDir
if (Test-Path -LiteralPath $arpKey) {
    $arp = Get-ItemProperty -LiteralPath $arpKey
    if ($arp.InstallLocation) {
        $installDir = $arp.InstallLocation
    }
}

# ---- 1) 常驻进程 ----
Get-Process -Name 'RustYuDirtyDaemon' -ErrorAction SilentlyContinue | Stop-Process -Force

# ---- 2) 服务：先停后删（原生命令允许失败，目标可能已不存在） ----
$ErrorActionPreference = 'Continue'
& sc.exe stop $serviceName 2>$null | Out-Null
& sc.exe delete $serviceName 2>$null | Out-Null
& schtasks.exe /Delete /TN $taskName /F 2>$null | Out-Null
& netsh.exe advfirewall firewall delete rule name="$firewallRuleName" 2>$null | Out-Null
$ErrorActionPreference = 'Stop'

# ---- 3) 注册表残留（只用固定键名/值名，避免通配） ----
foreach ($key in @(
    'HKLM:\SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces',
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe',
    $arpKey
)) {
    if (Test-Path -LiteralPath $key) {
        Remove-Item -LiteralPath $key -Recurse -Force
    }
}
foreach ($value in @(
    @{ Key = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'; Name = 'RustYuDirtyTraces' },
    @{ Key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'; Name = 'RUSTYU_DIRTY_TRACES_HOME' }
)) {
    if (Test-Path -LiteralPath $value.Key) {
        Remove-ItemProperty -LiteralPath $value.Key -Name $value.Name -ErrorAction SilentlyContinue
    }
}

# ---- 4) 快捷方式残留 ----
$desktopLink = Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'RustYu Dirty Traces Daemon.lnk'
if (Test-Path -LiteralPath $desktopLink) {
    Remove-Item -LiteralPath $desktopLink -Force
}
$startMenuDir = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'RustYu Fixture Labs'
if (Test-Path -LiteralPath $startMenuDir) {
    Remove-Item -LiteralPath $startMenuDir -Recurse -Force
}

# ---- 5) 文件残留（安装目录 + ProgramData + LocalAppData） ----
foreach ($dir in @(
    $installDir,
    (Join-Path $env:ProgramData 'RustYu Dirty Traces'),
    (Join-Path $env:LOCALAPPDATA 'RustYu Dirty Traces')
)) {
    if (Test-Path -LiteralPath $dir) {
        Remove-Item -LiteralPath $dir -Recurse -Force
    }
}

Write-Output 'dirty-traces-reset'
