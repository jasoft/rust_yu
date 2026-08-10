param(
    [ValidateSet('Install', 'Update', 'AbnormalExit', 'Cleanup')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [string]$Root
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$rootPath = [IO.Path]::GetFullPath($Root)
$programPath = Join-Path $rootPath 'app'
$dataPath = Join-Path $rootPath 'data'
$taskName = 'RustYuCompetitiveFixtureTask'
$serviceName = 'RustYuCompetitiveFixtureService'

switch ($Action) {
    'Install' {
        New-Item -ItemType Directory -Path $programPath, $dataPath -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $programPath 'launcher.cmd') -Value '@echo RustYu fixture' -Encoding ascii
        Set-Content -LiteralPath (Join-Path $dataPath 'config.json') -Value '{"version":1,"preserve":"original"}' -Encoding utf8
        schtasks.exe /Create /TN $taskName /TR "`"$(Join-Path $programPath 'launcher.cmd')`"" /SC ONLOGON /F | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Failed to create fixture scheduled task' }
        sc.exe create $serviceName binPath= "`"$(Join-Path $programPath 'launcher.cmd')`"" start= demand DisplayName= "RustYu Competitive Fixture Service" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Failed to create fixture service' }
        Set-Content -LiteralPath (Join-Path $rootPath 'install.complete') -Value (Get-Date).ToString('o') -Encoding ascii
    }
    'Update' {
        $config = Join-Path $dataPath 'config.json'
        if (-not (Test-Path -LiteralPath $config)) { throw 'Install phase must run before update' }
        Copy-Item -LiteralPath $config -Destination "$config.before-update" -Force
        Set-Content -LiteralPath $config -Value '{"version":2,"preserve":"updated"}' -Encoding utf8
        Set-Content -LiteralPath (Join-Path $programPath 'update.marker') -Value (Get-Date).ToString('o') -Encoding ascii
    }
    'AbnormalExit' {
        New-Item -ItemType Directory -Path $dataPath -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $dataPath 'partial-evidence.marker') -Value 'written-before-exit' -Encoding ascii
        exit 23
    }
    'Cleanup' {
        schtasks.exe /Delete /TN $taskName /F 2>$null | Out-Null
        sc.exe delete $serviceName 2>$null | Out-Null
        if (Test-Path -LiteralPath $rootPath) { Remove-Item -LiteralPath $rootPath -Recurse -Force }
    }
}
