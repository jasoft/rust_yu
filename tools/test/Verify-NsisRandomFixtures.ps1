[CmdletBinding()]
param(
    [string]$FixtureSetPath,
    [string[]]$FixtureIndex = @(),
    [switch]$RunLifecycle
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-PeMachine {
    param([Parameter(Mandatory)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Not a PE file: $Path" }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) { throw "Invalid PE signature: $Path" }
        return $reader.ReadUInt16()
    } finally {
        $reader.Dispose()
        $stream.Dispose()
    }
}

function Wait-UntilRemoved {
    param(
        [Parameter(Mandatory)][string]$InstallDirectory,
        [Parameter(Mandatory)][string]$RegistryPath
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        if (-not (Test-Path -LiteralPath $InstallDirectory) -and -not (Test-Path -LiteralPath $RegistryPath)) {
            return
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "Uninstall did not finish within 30 seconds: $InstallDirectory"
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$fixtureRoot = Join-Path $repoRoot '.resources\nsis-random-fixtures'
if ([string]::IsNullOrWhiteSpace($FixtureSetPath)) {
    $latestManifest = Get-ChildItem -LiteralPath (Join-Path $fixtureRoot 'output') -Filter 'fixture-set.json' -Recurse -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($null -eq $latestManifest) {
        throw 'No fixture set found. Run Build-NsisRandomFixtures.ps1 first.'
    }
    $FixtureSetPath = $latestManifest.FullName
}

$manifestPath = [IO.Path]::GetFullPath($FixtureSetPath)
$allowedOutputRoot = [IO.Path]::GetFullPath((Join-Path $fixtureRoot 'output'))
if (-not $manifestPath.StartsWith(($allowedOutputRoot + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)) {
    throw "Fixture manifest must be under $allowedOutputRoot"
}
if (-not (Test-Path -LiteralPath $manifestPath)) {
    throw "Missing fixture manifest: $manifestPath"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.schemaVersion -ne 1 -or $manifest.target -ne 'amd64-unicode' -or $manifest.peMachine -ne 'AMD64') {
    throw 'Fixture manifest does not declare the required amd64-unicode target'
}
if ($manifest.count -ne $manifest.fixtures.Count) {
    throw "Fixture count mismatch: manifest says $($manifest.count), found $($manifest.fixtures.Count)"
}

$setRoot = Split-Path -Parent $manifestPath
$selectedFixtures = @($manifest.fixtures)
$requestedFixtureIndexes = @(
    foreach ($rawIndex in $FixtureIndex) {
        foreach ($indexText in $rawIndex.Split(',', [StringSplitOptions]::RemoveEmptyEntries)) {
            $parsedIndex = 0
            if (-not [int]::TryParse($indexText.Trim(), [ref]$parsedIndex) -or $parsedIndex -lt 0) {
                throw "Invalid fixture index: $indexText"
            }
            $parsedIndex
        }
    }
)
if ($requestedFixtureIndexes.Count -gt 0) {
    $selectedFixtures = @($selectedFixtures | Where-Object { $requestedFixtureIndexes -contains $_.index })
    if ($selectedFixtures.Count -ne $requestedFixtureIndexes.Count) {
        throw 'One or more requested fixture indexes do not exist'
    }
}

$names = @($manifest.fixtures.productName | Sort-Object -Unique)
$packages = @($manifest.fixtures.packageName | Sort-Object -Unique)
$codes = @($manifest.fixtures.productCode | Sort-Object -Unique)
if ($names.Count -ne $manifest.count -or $packages.Count -ne $manifest.count -or $codes.Count -ne $manifest.count) {
    throw 'Fixture product names, package names, and product codes must all be unique'
}

foreach ($fixture in $selectedFixtures) {
    $installerPath = Join-Path $setRoot $fixture.installer
    if (-not (Test-Path -LiteralPath $installerPath)) {
        throw "Missing installer: $installerPath"
    }
    if ((Get-PeMachine -Path $installerPath) -ne 0x8664) {
        throw "Installer is not an AMD64 PE: $installerPath"
    }
    $installerHash = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($installerHash -ne $fixture.installerSha256) {
        throw "Installer SHA-256 mismatch: $installerPath"
    }

    if (-not $RunLifecycle) { continue }

    $expectedInstallRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs\RustYuFixtures'))
    $installDirectory = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA ([string]$fixture.installDirectoryRelativeToLocalAppData)))
    if (-not $installDirectory.StartsWith(($expectedInstallRoot + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)) {
        throw "Unsafe fixture install directory: $installDirectory"
    }
    $registryPath = [string]$fixture.uninstallRegistryKey

    if (Test-Path -LiteralPath $installDirectory) {
        throw "Fixture is already installed; uninstall it before retrying: $installDirectory"
    }

    $installProcess = Start-Process -FilePath $installerPath -ArgumentList '/S' -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) {
        throw "Fixture installer failed with exit code $($installProcess.ExitCode): $installerPath"
    }

    try {
        if (-not (Test-Path -LiteralPath $registryPath)) {
            throw "Missing 64-bit uninstall registry entry: $registryPath"
        }
        foreach ($file in $fixture.files) {
            $installedFile = Join-Path $installDirectory $file.path
            if (-not (Test-Path -LiteralPath $installedFile)) {
                throw "Missing installed fixture file: $installedFile"
            }
            $installedHash = (Get-FileHash -LiteralPath $installedFile -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($installedHash -ne $file.sha256) {
                throw "Installed file SHA-256 mismatch: $installedFile"
            }
        }

        $uninstallerPath = Join-Path $installDirectory 'uninstall.exe'
        if ((Get-PeMachine -Path $uninstallerPath) -ne 0x8664) {
            throw "Uninstaller is not an AMD64 PE: $uninstallerPath"
        }
        $uninstallProcess = Start-Process -FilePath $uninstallerPath -ArgumentList '/S' -PassThru
        $uninstallProcess.WaitForExit(30000) | Out-Null
        Wait-UntilRemoved -InstallDirectory $installDirectory -RegistryPath $registryPath
    } finally {
        if (Test-Path -LiteralPath $registryPath) {
            Remove-Item -LiteralPath $registryPath -Recurse -Force
        }
        if (Test-Path -LiteralPath $installDirectory) {
            # 只清理经过上方前缀校验的测试夹具目录，确保失败重试不会污染机器。
            Remove-Item -LiteralPath $installDirectory -Recurse -Force
        }
    }
}

Write-Output ("nsis-fixtures-ready count={0} lifecycle={1}" -f $selectedFixtures.Count, $RunLifecycle.IsPresent)
