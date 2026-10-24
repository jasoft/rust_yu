# 留痕夹具（leftover-trace fixtures）安装/清理脚本。
#
# 背景：developer.rs 的开发者夹具目录注册了 leftover-machine / leftover-user /
# leftover-sparse 三个"卸载器故意只删卸载键"的夹具，本脚本是它们的唯一实现。
#
# 每个夹具安装后都会创建一个完整的软件足迹（文件 + 注册表 + 快捷方式），
# 其 UninstallString 是一个"脏卸载器"：执行后只删除自身的 ARP 卸载键，
# 其余全部留下，用来验证 Rust Yu 的残留扫描（deep analysis / leftovers）。
#
# 用法：
#   Install-LeftoverFixtures.ps1 -FixtureIds leftover-machine        # 安装（machine 需要管理员）
#   Install-LeftoverFixtures.ps1 -FixtureIds leftover-user,leftover-sparse
#   Install-LeftoverFixtures.ps1 -FixtureIds leftover-machine -Cleanup  # 清理该夹具全部痕迹
param(
    # 要安装或清理的夹具 id，必须与 developer.rs 的 allowlist 一致
    [string[]]$FixtureIds,
    # 清理（而非安装）指定夹具创建的全部痕迹
    [switch]$Cleanup
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:UninstallRoot = 'Software\Microsoft\Windows\CurrentVersion\Uninstall'
$script:Publisher = 'RustYu Fixture Labs'
$script:InstallDate = '20261024'

# 与 developer.rs FIXTURES allowlist 保持同步
$script:KnownFixtures = @('leftover-machine', 'leftover-user', 'leftover-sparse')

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-ValidFixtureIds {
    foreach ($fixtureId in $script:ResolvedIds) {
        if ($script:KnownFixtures -notcontains $fixtureId) {
            throw "Unknown fixture id '$fixtureId'. Known ids: $($script:KnownFixtures -join ', ')"
        }
    }
    if ($script:ResolvedIds.Count -eq 0) {
        throw 'FixtureIds is required'
    }
}

function New-PlaceholderFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )
    # 只在夹具自有的目录树里创建文件；父目录不存在时逐级创建
    $parent = Split-Path -Parent $Path
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    Set-Content -LiteralPath $Path -Value $Content -Encoding UTF8
}

function New-RegistryTree {
    param(
        [Parameter(Mandatory = $true)][ValidateSet('HKLM', 'HKCU')][string]$Hive,
        [Parameter(Mandatory = $true)][string]$KeyPath,
        [Parameter(Mandatory = $true)][hashtable]$Values
    )
    $drive = "$($Hive):\$KeyPath"
    if (-not (Test-Path -LiteralPath $drive)) {
        New-Item -Path $drive -Force | Out-Null
    }
    foreach ($name in $Values.Keys) {
        $value = $Values[$name]
        if ($value -is [int]) {
            New-ItemProperty -LiteralPath $drive -Name $name -Value $value -PropertyType DWord -Force | Out-Null
        }
        else {
            New-ItemProperty -LiteralPath $drive -Name $name -Value ([string]$value) -PropertyType String -Force | Out-Null
        }
    }
}

function Remove-RegistryTree {
    param(
        [Parameter(Mandatory = $true)][ValidateSet('HKLM', 'HKCU')][string]$Hive,
        [Parameter(Mandatory = $true)][string]$KeyPath
    )
    $drive = "$($Hive):\$KeyPath"
    if (Test-Path -LiteralPath $drive) {
        Remove-Item -LiteralPath $drive -Recurse -Force
    }
}

function Remove-RegistryValue {
    param(
        [Parameter(Mandatory = $true)][ValidateSet('HKLM', 'HKCU')][string]$Hive,
        [Parameter(Mandatory = $true)][string]$KeyPath,
        [Parameter(Mandatory = $true)][string]$ValueName
    )
    $drive = "$($Hive):\$KeyPath"
    if (Test-Path -LiteralPath $drive) {
        Remove-ItemProperty -LiteralPath $drive -Name $ValueName -ErrorAction SilentlyContinue
    }
}

function Get-DirtyUninstallCommand {
    # "脏卸载器"：执行时只删除自身的 ARP 卸载键，其余痕迹原样保留。
    # 显式使用 System32 的 Windows PowerShell，避免被 MSIX 版 pwsh 的
    # 注册表虚拟化影响（与 developer.rs 的 Script 通道保持一致）。
    # 注意不要加 -WindowStyle Hidden：与 -Command 组合在部分环境下直接失败
    # （实测 exit 127）；真实卸载器也不会带窗口样式参数，调用方负责隐藏窗口。
    param(
        [Parameter(Mandatory = $true)][ValidateSet('HKLM', 'HKCU')][string]$Hive,
        [Parameter(Mandatory = $true)][string]$ArpKeyName
    )
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $key = "$($Hive):\$($script:UninstallRoot)\$ArpKeyName"
    return "$powershell -NoProfile -ExecutionPolicy Bypass -Command `"Remove-Item -LiteralPath '$key' -Recurse -Force`""
}

function New-Shortcut {
    param(
        [Parameter(Mandatory = $true)][string]$LinkPath,
        [Parameter(Mandatory = $true)][string]$TargetPath,
        [string]$Arguments = ''
    )
    $parent = Split-Path -Parent $LinkPath
    if (-not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($LinkPath)
    $shortcut.TargetPath = $TargetPath
    if ($Arguments) { $shortcut.Arguments = $Arguments }
    $shortcut.Save()
}

function Remove-TreeIfExists {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}

# ---------- leftover-machine：机器级（HKLM + Program Files + ProgramData） ----------

function Get-MachinePaths {
    return [ordered]@{
        InstallDir  = Join-Path $env:ProgramFiles 'RustYuLeftoverMachine'
        ProgramData = Join-Path $env:ProgramData 'RustYuLeftoverMachine'
        PublisherKey = "Software\$($script:Publisher)\RustYuLeftoverMachine"
        ArpKeyName  = 'RustYuLeftoverMachine'
        RunKey      = 'Software\Microsoft\Windows\CurrentVersion\Run'
        RunValue    = 'RustYuLeftoverMachine'
    }
}

function Install-LeftoverMachine {
    if (-not (Test-Administrator)) {
        throw 'leftover-machine writes HKLM and Program Files; an elevated PowerShell is required'
    }
    Cleanup-LeftoverMachine
    $paths = Get-MachinePaths
    $installDir = $paths.InstallDir

    # 安装目录足迹（占位文件：残留扫描只关心存在性与路径，不会执行它们）
    New-PlaceholderFile -Path (Join-Path $installDir 'RustYuLeftoverMachine.exe') -Content "placeholder executable for $installDir"
    New-PlaceholderFile -Path (Join-Path $installDir 'uninstall-helper.dll') -Content 'placeholder dll'
    New-PlaceholderFile -Path (Join-Path $installDir 'config\settings.json') -Content '{ "fixture": "leftover-machine", "scope": "machine" }'
    New-PlaceholderFile -Path (Join-Path $installDir 'logs\install-trace.log') -Content "installed $((Get-Date).ToString('s'))"

    # ProgramData 足迹
    New-PlaceholderFile -Path (Join-Path $paths.ProgramData 'cache\index.bin') -Content 'machine fixture cache index'
    New-PlaceholderFile -Path (Join-Path $paths.ProgramData 'telemetry\pending.json') -Content '{ "pending": 1 }'

    # HKLM 厂商键（卸载器故意不删的注册表残留）
    New-RegistryTree -Hive HKLM -KeyPath $paths.PublisherKey -Values @{
        'InstallRoot' = $installDir
        'DataRoot'    = $paths.ProgramData
        'Version'     = '1.0.0'
    }
    New-RegistryTree -Hive HKLM -KeyPath "$($paths.PublisherKey)\Components" -Values @{
        'Core'         = '1.0.0'
        'ServiceLayer' = '1.0.0'
    }

    # HKLM Run 启动项残留
    New-RegistryTree -Hive HKLM -KeyPath $paths.RunKey -Values @{}
    New-ItemProperty -LiteralPath "HKLM:\$($paths.RunKey)" -Name $paths.RunValue -Value "`"$installDir\RustYuLeftoverMachine.exe`" /background" -PropertyType String -Force | Out-Null

    # ARP 卸载键：UninstallString 是"只删卸载键"的脏卸载器
    New-RegistryTree -Hive HKLM -KeyPath "$($script:UninstallRoot)\$($paths.ArpKeyName)" -Values @{
        'DisplayName'         = 'RustYu Leftover Machine Fixture'
        'DisplayVersion'     = '1.0.0'
        'Publisher'          = $script:Publisher
        'InstallLocation'    = $installDir
        'DisplayIcon'        = "$installDir\RustYuLeftoverMachine.exe,0"
        'UninstallString'    = (Get-DirtyUninstallCommand -Hive HKLM -ArpKeyName $paths.ArpKeyName)
        'QuietUninstallString' = (Get-DirtyUninstallCommand -Hive HKLM -ArpKeyName $paths.ArpKeyName)
        'EstimatedSize'      = 1024
        'NoModify'           = 1
        'NoRepair'           = 1
        'InstallDate'        = $script:InstallDate
        'Comments'           = 'RustYu leftover-trace fixture (machine scope)'
    }
}

function Cleanup-LeftoverMachine {
    if (-not (Test-Administrator)) {
        throw 'leftover-machine cleanup touches HKLM and Program Files; an elevated PowerShell is required'
    }
    $paths = Get-MachinePaths
    Remove-TreeIfExists -Path $paths.InstallDir
    Remove-TreeIfExists -Path $paths.ProgramData
    Remove-RegistryTree -Hive HKLM -KeyPath $paths.PublisherKey
    Remove-RegistryValue -Hive HKLM -KeyPath $paths.RunKey -ValueName $paths.RunValue
    Remove-RegistryTree -Hive HKLM -KeyPath "$($script:UninstallRoot)\$($paths.ArpKeyName)"
}

# ---------- leftover-user：当前用户（HKCU + 用户目录 + 个人快捷方式） ----------

function Get-UserPaths {
    return [ordered]@{
        InstallDir  = Join-Path $env:LOCALAPPDATA 'Programs\RustYuLeftoverUser'
        RoamingDir  = Join-Path $env:APPDATA 'RustYuLeftoverUser'
        LocalDir    = Join-Path $env:LOCALAPPDATA 'RustYuLeftoverUser'
        PublisherKey = "Software\$($script:Publisher)\RustYuLeftoverUser"
        ArpKeyName  = 'RustYuLeftoverUser'
        RunKey      = 'Software\Microsoft\Windows\CurrentVersion\Run'
        RunValue    = 'RustYuLeftoverUser'
        DesktopLink = Join-Path ([Environment]::GetFolderPath('Desktop')) 'RustYu Leftover User.lnk'
        StartMenuDir = Join-Path ([Environment]::GetFolderPath('Programs')) 'RustYuLeftoverUser'
    }
}

function Install-LeftoverUser {
    Cleanup-LeftoverUser
    $paths = Get-UserPaths
    $installDir = $paths.InstallDir

    # 用户级安装目录足迹
    New-PlaceholderFile -Path (Join-Path $installDir 'RustYuLeftoverUser.exe') -Content "placeholder executable for $installDir"
    New-PlaceholderFile -Path (Join-Path $installDir 'helper.dll') -Content 'placeholder dll'
    New-PlaceholderFile -Path (Join-Path $installDir 'config\app.json') -Content '{ "fixture": "leftover-user", "scope": "user" }'
    New-PlaceholderFile -Path (Join-Path $installDir 'logs\session.log') -Content "installed $((Get-Date).ToString('s'))"

    # Roaming / Local AppData 足迹
    New-PlaceholderFile -Path (Join-Path $paths.RoamingDir 'config\profile.json') -Content '{ "theme": "light" }'
    New-PlaceholderFile -Path (Join-Path $paths.LocalDir 'Cache\state.db') -Content 'user fixture cache'

    # HKCU 厂商键残留
    New-RegistryTree -Hive HKCU -KeyPath $paths.PublisherKey -Values @{
        'InstallRoot' = $installDir
        'DataRoot'    = $paths.RoamingDir
        'Version'     = '1.0.0'
    }

    # HKCU Run 启动项残留
    New-RegistryTree -Hive HKCU -KeyPath $paths.RunKey -Values @{}
    New-ItemProperty -LiteralPath "HKCU:\$($paths.RunKey)" -Name $paths.RunValue -Value "`"$installDir\RustYuLeftoverUser.exe`" /autostart" -PropertyType String -Force | Out-Null

    # 个人快捷方式（桌面 + 开始菜单），卸载器故意不删
    New-Shortcut -LinkPath $paths.DesktopLink -TargetPath (Join-Path $installDir 'RustYuLeftoverUser.exe')
    New-Shortcut -LinkPath (Join-Path $paths.StartMenuDir 'RustYu Leftover User.lnk') -TargetPath (Join-Path $installDir 'RustYuLeftoverUser.exe')

    # ARP 卸载键（HKCU），脏卸载器同样只删这个键
    New-RegistryTree -Hive HKCU -KeyPath "$($script:UninstallRoot)\$($paths.ArpKeyName)" -Values @{
        'DisplayName'           = 'RustYu Leftover User Fixture'
        'DisplayVersion'       = '1.0.0'
        'Publisher'            = $script:Publisher
        'InstallLocation'      = $installDir
        'DisplayIcon'          = "$installDir\RustYuLeftoverUser.exe,0"
        'UninstallString'      = (Get-DirtyUninstallCommand -Hive HKCU -ArpKeyName $paths.ArpKeyName)
        'QuietUninstallString' = (Get-DirtyUninstallCommand -Hive HKCU -ArpKeyName $paths.ArpKeyName)
        'EstimatedSize'        = 768
        'InstallDate'          = $script:InstallDate
        'Comments'             = 'RustYu leftover-trace fixture (current user)'
    }
}

function Cleanup-LeftoverUser {
    $paths = Get-UserPaths
    Remove-TreeIfExists -Path $paths.InstallDir
    Remove-TreeIfExists -Path $paths.RoamingDir
    Remove-TreeIfExists -Path $paths.LocalDir
    Remove-RegistryTree -Hive HKCU -KeyPath $paths.PublisherKey
    Remove-RegistryValue -Hive HKCU -KeyPath $paths.RunKey -ValueName $paths.RunValue
    Remove-RegistryTree -Hive HKCU -KeyPath "$($script:UninstallRoot)\$($paths.ArpKeyName)"
    if (Test-Path -LiteralPath $paths.DesktopLink) {
        Remove-Item -LiteralPath $paths.DesktopLink -Force
    }
    Remove-TreeIfExists -Path $paths.StartMenuDir
}

# ---------- leftover-sparse：泛型命名 + 非标准位置（保守发现验证） ----------

function Get-SparsePaths {
    return [ordered]@{
        InstallDir   = Join-Path $env:LOCALAPPDATA 'RustYuSharedTools\Helper'
        VendorKey    = 'Software\GenericToolsCo\SharedHelper'
        ArpKeyName   = 'RustYuLeftoverSparse'
        ArpName      = 'RustYu Shared Helper'
    }
}

function Install-LeftoverSparse {
    Cleanup-LeftoverSparse
    $paths = Get-SparsePaths
    $installDir = $paths.InstallDir

    # 非标准安装位置（目录名与 ARP 名称几乎无交集，验证别名/路径匹配的保守性）
    New-PlaceholderFile -Path (Join-Path $installDir 'SharedHelper.exe') -Content "placeholder executable for $installDir"
    New-PlaceholderFile -Path (Join-Path $installDir 'helper.dat') -Content 'sparse fixture data'

    # 稀疏厂商键：只有最少的值
    New-RegistryTree -Hive HKCU -KeyPath $paths.VendorKey -Values @{
        'Home' = $installDir
    }

    # ARP 键故意缺 Publisher / DisplayIcon / EstimatedSize 等，模拟信息稀疏的遗留安装
    New-RegistryTree -Hive HKCU -KeyPath "$($script:UninstallRoot)\$($paths.ArpKeyName)" -Values @{
        'DisplayName'           = $paths.ArpName
        'DisplayVersion'       = '2.4.1'
        'InstallLocation'      = $installDir
        'UninstallString'      = (Get-DirtyUninstallCommand -Hive HKCU -ArpKeyName $paths.ArpKeyName)
        'QuietUninstallString' = (Get-DirtyUninstallCommand -Hive HKCU -ArpKeyName $paths.ArpKeyName)
        'Comments'             = 'RustYu leftover-trace fixture (sparse naming)'
    }
}

function Cleanup-LeftoverSparse {
    $paths = Get-SparsePaths
    Remove-TreeIfExists -Path $paths.InstallDir
    Remove-RegistryTree -Hive HKCU -KeyPath $paths.VendorKey
    Remove-RegistryTree -Hive HKCU -KeyPath "$($script:UninstallRoot)\$($paths.ArpKeyName)"
}

# ---------- 分发 ----------

# 兼容 "leftover-user,leftover-sparse" 形式的逗号分隔输入；developer.rs 传单个 id
$script:ResolvedIds = @($FixtureIds | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })

Assert-ValidFixtureIds

foreach ($fixtureId in $script:ResolvedIds) {
    $scope = $fixtureId -replace '^leftover-', ''
    $installFn = "Install-Leftover$(($scope[0].ToString().ToUpper()) + $scope.Substring(1))"
    $cleanupFn = "Cleanup-Leftover$(($scope[0].ToString().ToUpper()) + $scope.Substring(1))"
    if ($Cleanup) {
        & $cleanupFn
        Write-Output "cleaned: $fixtureId"
    }
    else {
        & $installFn
        Write-Output "installed: $fixtureId"
    }
}
