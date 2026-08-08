# 验证 Rust Yu 正式安装器的管理员 GUI 与计划任务生命周期。
# 该脚本只操作本产品固定任务，不使用用户可写目录作为高权限动作目标。
[CmdletBinding()]
param(
    [string]$InstallerPath = "",
    [string]$InstallRoot = (Join-Path $env:ProgramFiles "Rust Yu"),
    [string]$ExecutablePath = "",
    [string]$TaskPath = "\Rust Yu\ElevatedGui",
    [switch]$SkipInstall,
    [switch]$KeepInstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Get-CurrentUserSid {
    return ([Security.Principal.WindowsIdentity]::GetCurrent()).User.Value
}

function Get-TaskXml {
    param([string]$Path)
    $result = & schtasks.exe /Query /TN $Path /XML 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "无法读取计划任务 $Path：$($result -join ' ')"
    }
    return ($result -join "`n")
}

function Assert-TaskDefinition {
    param([string]$Path, [string]$ExpectedExecutable, [string]$ExpectedSid)
    $xml = Get-TaskXml -Path $Path
    Assert-Condition ($xml -match [regex]::Escape($ExpectedExecutable)) "计划任务动作不是正式安装目录 EXE"
    Assert-Condition ($xml -match "--elevated-entry") "计划任务缺少固定管理员入口参数"
    Assert-Condition ($xml -match "InteractiveToken") "计划任务不是 InteractiveToken 登录类型"
    Assert-Condition ($xml -match "HighestAvailable") "计划任务不是最高可用运行级别"
    Assert-Condition ($xml -match [regex]::Escape($ExpectedSid)) "计划任务 principal 不是当前用户 SID"
    Assert-Condition ($xml -match "IgnoreNew") "计划任务没有 IgnoreNew 并发策略"
    Assert-Condition ($xml -notmatch "target\\|AppData|Temp|Users\\Public") "计划任务动作疑似指向用户可写路径"
    return $xml
}

function Invoke-InstalledGui {
    param([string]$Path, [string]$LogPath)
    $process = Start-Process -FilePath $Path -PassThru -WindowStyle Hidden
    try {
        if (-not $process.WaitForExit(15000)) {
            throw "GUI 启动超过 15 秒未退出（任务启动可能未完成）"
        }
        Add-Content -LiteralPath $LogPath -Value "normal launch exit=$($process.ExitCode)"
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    }
}

Assert-Condition ($env:OS -eq "Windows_NT") "该验证脚本只能在 Windows 上运行"
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
Assert-Condition $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) "请在管理员 PowerShell 中运行"
Assert-Condition ([System.IO.Path]::GetFullPath($InstallRoot).StartsWith([System.IO.Path]::GetFullPath($env:ProgramFiles), [StringComparison]::OrdinalIgnoreCase)) "安装目录必须位于 Program Files"

$logRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rust-yu-elevated-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $logRoot -Force | Out-Null
$logPath = Join-Path $logRoot "lifecycle.log"
try {
    Add-Content -LiteralPath $logPath -Value "started $(Get-Date -Format o)"

    if (-not $SkipInstall) {
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($InstallerPath)) "未指定 InstallerPath"
        Assert-Condition (Test-Path -LiteralPath $InstallerPath -PathType Leaf) "安装器不存在: $InstallerPath"
        $install = Start-Process -FilePath (Resolve-Path $InstallerPath) -ArgumentList @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART") -Wait -PassThru
        Assert-Condition ($install.ExitCode -eq 0) "安装器失败，退出码 $($install.ExitCode)"
    }

    Assert-Condition (Test-Path -LiteralPath $InstallRoot -PathType Container) "安装目录不存在: $InstallRoot"
    if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
        $candidate = Get-ChildItem -LiteralPath $InstallRoot -Filter "*.exe" -File | Where-Object { $_.Name -notmatch "unins|uninstall|setup" } | Select-Object -First 1
        Assert-Condition ($null -ne $candidate) "安装目录中找不到 GUI EXE"
        $ExecutablePath = $candidate.FullName
    }
    $ExecutablePath = (Resolve-Path -LiteralPath $ExecutablePath).Path
    Assert-Condition ($ExecutablePath.StartsWith([System.IO.Path]::GetFullPath($env:ProgramFiles), [StringComparison]::OrdinalIgnoreCase)) "GUI EXE 不在 Program Files"

    $sid = Get-CurrentUserSid
    $null = Assert-TaskDefinition -Path $TaskPath -ExpectedExecutable $ExecutablePath -ExpectedSid $sid
    Invoke-InstalledGui -Path $ExecutablePath -LogPath $logPath
    $firstXml = Assert-TaskDefinition -Path $TaskPath -ExpectedExecutable $ExecutablePath -ExpectedSid $sid
    Invoke-InstalledGui -Path $ExecutablePath -LogPath $logPath
    $secondXml = Assert-TaskDefinition -Path $TaskPath -ExpectedExecutable $ExecutablePath -ExpectedSid $sid
    Assert-Condition ($firstXml -eq $secondXml) "第二次启动改变了计划任务定义"

    Write-Host "Elevated GUI lifecycle passed. Log: $logPath" -ForegroundColor Green
} catch {
    Add-Content -LiteralPath $logPath -Value ("FAILED: " + $_.Exception.ToString())
    Write-Host "失败诊断日志: $logPath" -ForegroundColor Red
    throw
} finally {
    if (-not $KeepInstall -and -not $SkipInstall -and (Test-Path -LiteralPath $InstallRoot)) {
        Write-Warning "验证成功后请通过正式卸载器执行任务清理；脚本不直接删除 Program Files。"
    }
}
