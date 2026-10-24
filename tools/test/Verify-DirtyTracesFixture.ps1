param(
    # 执行完整生命周期验证（需要管理员 PowerShell）：重置 → 静默安装 →
    # 断言全部痕迹存在 → 执行夹具的脏卸载器 → 断言只有卸载键被删、其余残留保留。
    [switch]$RunLifecycle,
    # 生命周期结束后立即执行 Reset 清理（默认保留残留现场供手工/ GUI 验证）
    [switch]$CleanAfterLifecycle
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$fixtureRoot = Join-Path $repoRoot '.resources\inno-dirty-traces'
$buildScriptPath = Join-Path $fixtureRoot 'Build-DirtyTracesFixture.ps1'
$resetScriptPath = Join-Path $fixtureRoot 'Reset-DirtyTracesFixture.ps1'
$issPath = Join-Path $fixtureRoot 'DirtyTracesFixture.iss'
$outputPath = Join-Path $fixtureRoot 'output\RustYuDirtyTracesSetup.exe'

# ---------- 结构校验 ----------

$requiredPaths = @(
    $fixtureRoot,
    $buildScriptPath,
    $resetScriptPath,
    $issPath,
    (Join-Path $fixtureRoot 'payload\tools\RustYuDirtyDaemon.rs'),
    (Join-Path $fixtureRoot 'payload\tools\RustYuDirtyService.rs'),
    (Join-Path $fixtureRoot 'payload\app\Register-ExtraTraces.ps1'),
    (Join-Path $fixtureRoot 'payload\app\README.txt'),
    (Join-Path $fixtureRoot 'payload\app\config\default.json'),
    (Join-Path $fixtureRoot 'payload\app\logs\service-heartbeat.log'),
    (Join-Path $fixtureRoot 'payload\programdata\telemetry\pending.json'),
    (Join-Path $fixtureRoot 'payload\programdata\cache\index.bin'),
    (Join-Path $fixtureRoot 'payload\appdata\user-state.json')
)
$missingPaths = @($requiredPaths | Where-Object { -not (Test-Path $_) })
if ($missingPaths.Count -gt 0) {
    throw "Missing fixture paths: $($missingPaths -join ', ')"
}

if (-not (Test-Path $outputPath)) {
    throw "Missing prebuilt installer fixture: $outputPath. Run Build-DirtyTracesFixture.ps1 first."
}

# PowerShell 脚本语法冒烟检查：解析失败直接失败，避免把语法错误留到生命周期里
foreach ($script in @($buildScriptPath, $resetScriptPath, (Join-Path $fixtureRoot 'payload\app\Register-ExtraTraces.ps1'))) {
    $null = [scriptblock]::Create((Get-Content -LiteralPath $script -Raw))
}

function Assert-True {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Message
    )
    if (-not $Condition) {
        throw "ASSERT FAILED: $Message"
    }
}

# ---------- 生命周期验证 ----------

if ($RunLifecycle) {
    if (-not [Environment]::Is64BitProcess) {
        throw 'RunLifecycle requires 64-bit PowerShell (32-bit session sees the wrong registry view)'
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'RunLifecycle requires an elevated PowerShell session'
    }

    $serviceName = 'RustYuDirtyTraces'
    $taskName = 'RustYuDirtyTracesStartTask'
    $firewallRuleName = 'RustYu Dirty Traces Daemon'
    $arpKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1'
    $installDir = Join-Path $env:ProgramFiles 'RustYu Dirty Traces Test App'

    # 1) 干净起点
    & $resetScriptPath
    Assert-True (-not (Test-Path $arpKey)) 'reset should remove the ARP key'

    # 2) 静默安装
    & $outputPath /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- | Out-Null
    Assert-True ($LASTEXITCODE -eq 0) "silent install should succeed (exit=$LASTEXITCODE)"

    # 3) 安装态断言：全部痕迹就位
    Assert-True (Test-Path $arpKey) 'ARP uninstall key should exist after install'
    $arp = Get-ItemProperty -LiteralPath $arpKey
    Assert-True ($arp.DisplayName -eq 'RustYu Dirty Traces Test App') 'ARP DisplayName mismatch'
    Assert-True ($null -ne $arp.QuietUninstallString -and $arp.QuietUninstallString -like '*unins000.exe*') 'QuietUninstallString should be populated'

    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    Assert-True ($null -ne $service) 'service should be registered'
    Assert-True ($service.Status -eq 'Running') "service should be running (got $($service.Status))"

    Assert-True ($null -ne (Get-Process -Name 'RustYuDirtyDaemon' -ErrorAction SilentlyContinue)) 'daemon process should be running'

    & schtasks.exe /Query /TN $taskName | Out-Null
    Assert-True ($LASTEXITCODE -eq 0) 'scheduled task should exist'

    $firewallOutput = (& netsh.exe advfirewall firewall show rule name="$firewallRuleName") -join "`n"
    Assert-True ($firewallOutput -like "*$firewallRuleName*") 'firewall rule should exist'

    Assert-True ($null -ne (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run' -Name 'RustYuDirtyTraces' -ErrorAction SilentlyContinue)) 'HKLM Run value should exist'
    Assert-True ($null -ne (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' -Name 'RUSTYU_DIRTY_TRACES_HOME' -ErrorAction SilentlyContinue)) 'machine environment variable should exist'
    Assert-True (Test-Path 'HKLM:\SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces') 'HKLM publisher key should exist'
    Assert-True (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe') 'App Paths key should exist'

    foreach ($file in @(
        (Join-Path $installDir 'RustYuDirtyDaemon.exe'),
        (Join-Path $installDir 'RustYuDirtyService.exe'),
        (Join-Path $installDir 'logs\service-heartbeat.log'),
        (Join-Path $installDir 'README.txt'),
        (Join-Path $env:ProgramData 'RustYu Dirty Traces\telemetry\pending.json'),
        (Join-Path $env:ProgramData 'RustYu Dirty Traces\cache\index.bin'),
        (Join-Path $env:LOCALAPPDATA 'RustYu Dirty Traces\leftover-user-state.json'),
        (Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'RustYu Dirty Traces Daemon.lnk')
    )) {
        Assert-True (Test-Path -LiteralPath $file) "installed file should exist: $file"
    }

    # 4) 执行夹具自带的"脏卸载器"（QuietUninstallString）
    $quietCommand = $arp.QuietUninstallString
    if ($quietCommand -match '^"([^"]+)"\s*(.*)$') {
        $uninstallerExe = $Matches[1]
        $uninstallerArgs = $Matches[2]
    }
    else {
        $parts = $quietCommand -split ' ', 2
        $uninstallerExe = $parts[0]
        $uninstallerArgs = if ($parts.Count -gt 1) { $parts[1] } else { '' }
    }
    Start-Process -FilePath $uninstallerExe -ArgumentList $uninstallerArgs -Wait -WindowStyle Hidden

    # unins000 会自拷贝到临时目录再执行，轮询等待 ARP 键真正消失
    $arpGone = $false
    foreach ($attempt in 1..30) {
        if (-not (Test-Path $arpKey)) { $arpGone = $true; break }
        Start-Sleep -Seconds 2
    }
    Assert-True $arpGone 'dirty uninstaller should remove its own ARP key'

    # 5) 脏卸载后断言：只有卸载键消失，其余残留原样保留
    Assert-True (-not (Test-Path (Join-Path $installDir 'README.txt'))) 'README.txt should be deleted by the uninstaller (proof it ran)'
    Assert-True (Test-Path (Join-Path $installDir 'RustYuDirtyDaemon.exe')) 'daemon exe should survive the uninstall'

    $serviceAfter = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    Assert-True ($null -ne $serviceAfter) 'service should be LEFT BEHIND after uninstall'
    Assert-True ($serviceAfter.Status -eq 'Running') "leftover service should still be running (got $($serviceAfter.Status))"

    Assert-True ($null -ne (Get-Process -Name 'RustYuDirtyDaemon' -ErrorAction SilentlyContinue)) 'daemon process should be LEFT BEHIND after uninstall'

    & schtasks.exe /Query /TN $taskName | Out-Null
    Assert-True ($LASTEXITCODE -eq 0) 'scheduled task should be LEFT BEHIND after uninstall'

    $firewallAfter = (& netsh.exe advfirewall firewall show rule name="$firewallRuleName") -join "`n"
    Assert-True ($firewallAfter -like "*$firewallRuleName*") 'firewall rule should be LEFT BEHIND after uninstall'

    Assert-True ($null -ne (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run' -Name 'RustYuDirtyTraces' -ErrorAction SilentlyContinue)) 'HKLM Run value should be LEFT BEHIND after uninstall'
    Assert-True (Test-Path 'HKLM:\SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces') 'publisher key should be LEFT BEHIND after uninstall'
    Assert-True (Test-Path (Join-Path $env:ProgramData 'RustYu Dirty Traces\cache\index.bin')) 'ProgramData leftovers should survive the uninstall'
    Assert-True (Test-Path (Join-Path $env:LOCALAPPDATA 'RustYu Dirty Traces\leftover-user-state.json')) 'LocalAppData leftovers should survive the uninstall'
    Assert-True (Test-Path (Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'RustYu Dirty Traces Daemon.lnk')) 'desktop shortcut should be LEFT BEHIND after uninstall'

    # 6) 可选清理
    if ($CleanAfterLifecycle) {
        & $resetScriptPath
        Assert-True ($null -eq (Get-Service -Name $serviceName -ErrorAction SilentlyContinue)) 'reset should delete the service'
        Assert-True ($null -eq (Get-Process -Name 'RustYuDirtyDaemon' -ErrorAction SilentlyContinue)) 'reset should kill the daemon'
        Assert-True (-not (Test-Path $installDir)) 'reset should remove the install directory'
        Write-Output 'dirty-traces-lifecycle-verified-and-cleaned'
    }
    else {
        Write-Output 'dirty-traces-lifecycle-verified (leftovers kept; run Reset-DirtyTracesFixture.ps1 to clean)'
    }
}
else {
    Write-Output 'dirty-traces-structure-verified'
}
