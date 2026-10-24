# 脏卸载夹具的"额外痕迹"注册器：安装结束时由 DirtyTracesFixture.iss 的 [Code] 段
# 以管理员身份调用。它注册的服务、计划任务、防火墙规则、快捷方式和常驻进程
# 都不会被夹具自带的卸载器清理——这正是本夹具要模拟的"脏卸载"行为。
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallRoot
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$serviceName = 'RustYuDirtyTraces'
$taskName = 'RustYuDirtyTracesStartTask'
$firewallRuleName = 'RustYu Dirty Traces Daemon'
$daemonPath = Join-Path $InstallRoot 'RustYuDirtyDaemon.exe'
$servicePath = Join-Path $InstallRoot 'RustYuDirtyService.exe'

# 分步日志：写到 ProgramData 痕迹目录，任何一步失败都能事后定位到具体步骤与错误
$script:TraceDir = Join-Path $env:ProgramData 'RustYu Dirty Traces'
New-Item -ItemType Directory -Path $script:TraceDir -Force | Out-Null
$script:LogFile = Join-Path $script:TraceDir 'register-extra.log'
Set-Content -LiteralPath $script:LogFile -Value ("{0} register start installRoot={1}" -f (Get-Date -Format s), $InstallRoot)

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][scriptblock]$Body
    )
    Add-Content -LiteralPath $script:LogFile -Value ("{0} step={1} start" -f (Get-Date -Format s), $Name)
    try {
        & $Body
        Add-Content -LiteralPath $script:LogFile -Value ("{0} step={1} ok" -f (Get-Date -Format s), $Name)
    }
    catch {
        Add-Content -LiteralPath $script:LogFile -Value ("{0} step={1} FAILED: {2}" -f (Get-Date -Format s), $Name, $_)
        throw
    }
}

# ---- 幂等预清理：重装时先移除旧痕迹载体，保证每次安装后的状态确定 ----
# 这一段允许失败（目标本来就可能不存在），临时放宽 EAP 避免 WinPS 把
# 原生命令的 stderr 当作终止错误（NativeCommandError）。
$ErrorActionPreference = 'Continue'
& sc.exe stop $serviceName 2>$null | Out-Null
& sc.exe delete $serviceName 2>$null | Out-Null
& schtasks.exe /Delete /TN $taskName /F 2>$null | Out-Null
& netsh.exe advfirewall firewall delete rule name="$firewallRuleName" 2>$null | Out-Null
$ErrorActionPreference = 'Stop'
Get-Process -Name 'RustYuDirtyDaemon' -ErrorAction SilentlyContinue | Stop-Process -Force

# ---- 1) 注册并启动常驻 Windows 服务（卸载器不 sc delete → 服务残留） ----
Invoke-Step 'create-service' {
    & sc.exe create $serviceName type= own start= demand obj= LocalSystem binPath= "`"$servicePath`""
    if ($LASTEXITCODE -ne 0) { throw "sc.exe create failed with exit code $LASTEXITCODE" }
}
Invoke-Step 'start-service' {
    & sc.exe start $serviceName
    if ($LASTEXITCODE -ne 0) { throw "sc.exe start failed with exit code $LASTEXITCODE" }
}

# ---- 2) 启动常驻守护进程（卸载后仍在运行 → 进程残留 + 文件锁定） ----
Invoke-Step 'start-daemon' {
    Start-Process -FilePath $daemonPath -WindowStyle Hidden
}

# ---- 3) 计划任务残留（ONSTART / SYSTEM，不启用触发就不会真实执行） ----
Invoke-Step 'create-task' {
    & schtasks.exe /Create /F /TN $taskName /TR "`"$daemonPath`"" /SC ONSTART /RU SYSTEM /RL HIGHEST
    if ($LASTEXITCODE -ne 0) { throw "schtasks.exe /Create failed with exit code $LASTEXITCODE" }
}

# ---- 4) 防火墙入站规则残留 ----
Invoke-Step 'firewall-rule' {
    & netsh.exe advfirewall firewall add rule name="$firewallRuleName" dir=in action=allow program="$daemonPath" enable=yes
    if ($LASTEXITCODE -ne 0) { throw "netsh.exe add rule failed with exit code $LASTEXITCODE" }
}

# ---- 5) 注册表残留：Inno [Registry] 段创建的值会被卸载器默认删除，所以全部
#          在这里写——卸载器感知不到的写入才是真正的注册表残留 ----
Invoke-Step 'registry-publisher' {
    $publisherKey = 'HKLM:\SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces'
    New-Item -Path $publisherKey -Force | Out-Null
    New-ItemProperty -LiteralPath $publisherKey -Name 'InstallRoot' -Value $InstallRoot -PropertyType String -Force | Out-Null
    $updatesKey = Join-Path $publisherKey 'Updates'
    New-Item -Path $updatesKey -Force | Out-Null
    New-ItemProperty -LiteralPath $updatesKey -Name 'Channel' -Value 'stable' -PropertyType String -Force | Out-Null
    New-ItemProperty -LiteralPath $updatesKey -Name 'AutoUpdate' -Value 1 -PropertyType DWord -Force | Out-Null
}
Invoke-Step 'registry-run' {
    $runKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'
    New-ItemProperty -LiteralPath $runKey -Name 'RustYuDirtyTraces' -Value "`"$daemonPath`" /startup" -PropertyType String -Force | Out-Null
}
Invoke-Step 'registry-apppaths' {
    $appPathsKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe'
    New-Item -Path $appPathsKey -Force | Out-Null
    # 默认值（未命名值）：Get-Item 返回的 RegistryKey 是只读的，必须用
    # OpenSubKey($true) 拿可写句柄，否则 SetValue 抛"无法写入到注册表键"
    $writableKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
        'SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe', $true)
    if ($null -eq $writableKey) { throw 'failed to open writable App Paths key' }
    try { $writableKey.SetValue('', $daemonPath) }
    finally { $writableKey.Close() }
    New-ItemProperty -LiteralPath $appPathsKey -Name 'Path' -Value $InstallRoot -PropertyType String -Force | Out-Null
}
Invoke-Step 'registry-environment' {
    # 应用专属环境变量（REG_EXPAND_SZ；刻意不动 Path，避免破坏系统 PATH）
    $environmentKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
    New-ItemProperty -LiteralPath $environmentKey -Name 'RUSTYU_DIRTY_TRACES_HOME' -Value $InstallRoot -PropertyType ExpandString -Force | Out-Null
}

# ---- 6) Inno [Icons] 感知不到的快捷方式（卸载器无从删除 → 快捷方式残留） ----
Invoke-Step 'shortcuts' {
    $shell = New-Object -ComObject WScript.Shell
    $desktopLink = Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'RustYu Dirty Traces Daemon.lnk'
    $desktopShortcut = $shell.CreateShortcut($desktopLink)
    $desktopShortcut.TargetPath = $daemonPath
    $desktopShortcut.Description = 'RustYu dirty uninstall fixture daemon'
    $desktopShortcut.Save()

    $startMenuDir = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'RustYu Fixture Labs'
    New-Item -ItemType Directory -Path $startMenuDir -Force | Out-Null
    $startMenuLink = Join-Path $startMenuDir 'RustYu Dirty Traces Daemon.lnk'
    $startMenuShortcut = $shell.CreateShortcut($startMenuLink)
    $startMenuShortcut.TargetPath = $daemonPath
    $startMenuShortcut.Description = 'RustYu dirty uninstall fixture daemon'
    $startMenuShortcut.Save()
}

# ---- 7) 痕迹清单：Reset/Verify 脚本以它为准，避免清理范围漂移 ----
Invoke-Step 'manifest' {
    $manifest = [ordered]@{
        fixture      = 'inno-dirty-traces'
        service      = $serviceName
        scheduledTask = $taskName
        firewallRule = $firewallRuleName
        daemonProcess = 'RustYuDirtyDaemon'
        daemonPath   = $daemonPath
        servicePath  = $servicePath
        desktopShortcut = 'RustYu Dirty Traces Daemon.lnk'
        startMenuShortcut = 'RustYu Fixture Labs\RustYu Dirty Traces Daemon.lnk'
        programDataDir = $script:TraceDir
        localAppDataDir = (Join-Path $env:LOCALAPPDATA 'RustYu Dirty Traces')
    }
    $manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $script:TraceDir 'traces-manifest.json') -Encoding UTF8
}

Write-Output 'dirty-traces-registered'
