[CmdletBinding()]
param(
    [string]$Destination
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fixtureRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($Destination)) {
    $Destination = Join-Path $fixtureRoot '.tools\nsis-amd64'
}

$releaseTag = 'v3.13.7497.330'
$assetName = 'nsis-3.13.7497.330-negrutiu-amd64.exe'
$expectedSha256 = '7cfa6bda44cf415c0557708858aa2048593c469292b5d4d54a5852d829625e87'
$downloadUrl = "https://github.com/negrutiu/nsis/releases/download/$releaseTag/$assetName"
$makeNsisPath = Join-Path $Destination 'makensis.exe'

if (Test-Path -LiteralPath $makeNsisPath) {
    Write-Output $makeNsisPath
    exit 0
}

$resolvedDestination = [IO.Path]::GetFullPath($Destination)
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $fixtureRoot '.tools'))
if (-not $resolvedDestination.StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Destination must remain under $allowedRoot"
}

[IO.Directory]::CreateDirectory($resolvedDestination) | Out-Null
$downloadPath = Join-Path ([IO.Path]::GetTempPath()) $assetName

try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $downloadPath -UseBasicParsing
    $actualSha256 = (Get-FileHash -LiteralPath $downloadPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualSha256 -ne $expectedSha256) {
        throw "NSIS compiler SHA-256 mismatch. Expected $expectedSha256, got $actualSha256"
    }

    $process = Start-Process -FilePath $downloadPath -ArgumentList @('/S', "/D=$resolvedDestination") -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "NSIS compiler installer failed with exit code $($process.ExitCode)"
    }
} finally {
    Remove-Item -LiteralPath $downloadPath -Force -ErrorAction SilentlyContinue
}

if (-not (Test-Path -LiteralPath $makeNsisPath)) {
    throw "NSIS compiler installation did not produce $makeNsisPath"
}

Write-Output $makeNsisPath
