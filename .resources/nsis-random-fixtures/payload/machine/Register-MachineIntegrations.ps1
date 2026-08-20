#requires -Version 5.1

param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath
)

$ErrorActionPreference = 'Stop'
$serviceName = 'RustYuDeepScanMachine'
$taskName = 'RustYu Deep Scan Machine Fixture'
$command = $ExecutablePath
$logPath = Join-Path $env:ProgramData 'RustYuDeepScanMachine\integration-registration.log'

try {
    New-Item -ItemType Directory -Path (Split-Path -Parent $logPath) -Force | Out-Null

    & sc.exe query $serviceName *> $null
    if ($LASTEXITCODE -eq 0) {
        & sc.exe config $serviceName binPath= $command start= demand DisplayName= 'RustYu Deep Scan Machine Fixture Service' *> $null
    } else {
        & sc.exe create $serviceName binPath= $command start= demand DisplayName= 'RustYu Deep Scan Machine Fixture Service' *> $null
    }
    if ($LASTEXITCODE -ne 0) { throw "sc.exe failed with exit code $LASTEXITCODE" }

    $action = New-ScheduledTaskAction -Execute $ExecutablePath -Argument '--scheduled'
    $trigger = New-ScheduledTaskTrigger -AtLogOn
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -RunLevel Limited -Force | Out-Null
    Disable-ScheduledTask -TaskName $taskName | Out-Null
    "registered service=$serviceName task=$taskName" | Set-Content -LiteralPath $logPath -Encoding utf8
} catch {
    "failed: $($_.Exception.Message)" | Set-Content -LiteralPath $logPath -Encoding utf8
    throw
}
