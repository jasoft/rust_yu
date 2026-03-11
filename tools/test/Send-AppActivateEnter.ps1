param(
    [Parameter(Mandatory = $true)]
    [string]$Title,
    [int]$Attempts = 30,
    [int]$Presses = 2,
    [int]$DelaySeconds = 1,
    [int]$PauseMilliseconds = 700
)

Add-Type -AssemblyName System.Windows.Forms

$shell = New-Object -ComObject WScript.Shell

for ($attempt = 0; $attempt -lt $Attempts; $attempt++) {
    Start-Sleep -Seconds $DelaySeconds

    if ($shell.AppActivate($Title)) {
        for ($press = 0; $press -lt $Presses; $press++) {
            [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
            Start-Sleep -Milliseconds $PauseMilliseconds
        }

        Write-Output "sent"
        exit 0
    }
}

Write-Error "window-not-found: $Title"
exit 1
