[CmdletBinding()]
param(
    [ValidateRange(1, 50)]
    [int]$Count = 6,

    [ValidateRange(1, 2147483647)]
    [int]$Seed = (Get-Random -Minimum 1 -Maximum 2147483647),

    [string]$MakeNsisPath,

    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fixtureRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$templatePath = Join-Path $fixtureRoot 'FixtureTemplate.nsi'
$cleanupScriptPath = Join-Path $fixtureRoot 'UninstallCleanup.cmd'
$setRoot = Join-Path $fixtureRoot ("output\set-{0}" -f $Seed)
$payloadRoot = Join-Path $setRoot 'payload'
$installerRoot = Join-Path $setRoot 'installers'
$scriptRoot = Join-Path $setRoot 'scripts'
$manifestPath = Join-Path $setRoot 'fixture-set.json'

if (-not (Test-Path -LiteralPath $templatePath)) {
    throw "Missing NSIS template: $templatePath"
}
if (-not (Test-Path -LiteralPath $cleanupScriptPath)) {
    throw "Missing uninstall cleanup script: $cleanupScriptPath"
}

if (Test-Path -LiteralPath $setRoot) {
    if (-not $Force) {
        throw "Fixture set already exists: $setRoot. Choose another seed or pass -Force."
    }

    $resolvedOutputRoot = [IO.Path]::GetFullPath((Join-Path $fixtureRoot 'output'))
    $resolvedSetRoot = [IO.Path]::GetFullPath($setRoot)
    if (-not $resolvedSetRoot.StartsWith(($resolvedOutputRoot + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to replace path outside fixture output root: $resolvedSetRoot"
    }
    Remove-Item -LiteralPath $resolvedSetRoot -Recurse -Force
}

if ([string]::IsNullOrWhiteSpace($MakeNsisPath)) {
    $MakeNsisPath = Join-Path $fixtureRoot '.tools\nsis-amd64\makensis.exe'
}
if (-not (Test-Path -LiteralPath $MakeNsisPath)) {
    $bootstrapScript = Join-Path $fixtureRoot 'Install-NsisX64Compiler.ps1'
    $bootstrapOutput = @(& $bootstrapScript)
    $MakeNsisPath = $bootstrapOutput[-1]
    if (-not (Test-Path -LiteralPath $MakeNsisPath)) {
        throw "Unable to bootstrap the x64 NSIS compiler: $MakeNsisPath"
    }
}

[IO.Directory]::CreateDirectory($payloadRoot) | Out-Null
[IO.Directory]::CreateDirectory($installerRoot) | Out-Null
[IO.Directory]::CreateDirectory($scriptRoot) | Out-Null

$random = [Random]::new($Seed)
$adjectives = @('Amber', 'Brisk', 'Cobalt', 'Daring', 'Ember', 'Frost', 'Golden', 'Hidden', 'Indigo', 'Jolly', 'Keen', 'Lunar', 'Misty', 'Nimble', 'Opal', 'Quiet', 'Rapid', 'Solar', 'Velvet', 'Wild')
$nouns = @('Badger', 'Comet', 'Drift', 'Engine', 'Falcon', 'Garden', 'Harbor', 'Jigsaw', 'Lantern', 'Meadow', 'Nimbus', 'Orbit', 'Panda', 'Quartz', 'Rocket', 'Signal', 'Timber', 'Voyager', 'Willow', 'Zephyr')
$extensions = @('txt', 'json', 'cfg', 'dat', 'bin', 'cache', 'xml', 'ini')
$folders = @('app', 'config', 'data', 'plugins', 'assets', 'cache', 'profiles', 'locale')
$fixtures = [Collections.Generic.List[object]]::new()

for ($index = 0; $index -lt $Count; $index++) {
    $suffix = $random.Next(1000, 9999)
    $productName = '{0} {1} {2}' -f $adjectives[$random.Next($adjectives.Count)], $nouns[$random.Next($nouns.Count)], $suffix
    $productSlug = ($productName -replace '[^A-Za-z0-9]', '-')
    $packageName = '{0}-setup-x64.exe' -f $productSlug.ToLowerInvariant()
    $guidBytes = [byte[]]::new(16)
    $random.NextBytes($guidBytes)
    $productCode = ([Guid]::new($guidBytes)).ToString('D')
    $uninstallKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustYuFixture_$productCode"
    $fixturePayloadRoot = Join-Path $payloadRoot $productSlug
    [IO.Directory]::CreateDirectory($fixturePayloadRoot) | Out-Null

    $fileCount = $random.Next(5, 13)
    $files = [Collections.Generic.List[object]]::new()
    for ($fileIndex = 0; $fileIndex -lt $fileCount; $fileIndex++) {
        $folder = $folders[($fileIndex + $index) % $folders.Count]
        $extension = $extensions[$random.Next($extensions.Count)]
        $relativePath = '{0}\module-{1:D2}-{2}.{3}' -f $folder, $fileIndex, $random.Next(10000, 99999), $extension
        $filePath = Join-Path $fixturePayloadRoot $relativePath
        [IO.Directory]::CreateDirectory((Split-Path -Parent $filePath)) | Out-Null

        $length = $random.Next(128, 4097)
        $content = [byte[]]::new($length)
        $random.NextBytes($content)
        [IO.File]::WriteAllBytes($filePath, $content)
        $files.Add([ordered]@{
            path = $relativePath
            size = $length
            sha256 = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
        })
    }

    $identityPath = Join-Path $fixturePayloadRoot 'fixture-identity.json'
    $identity = [ordered]@{
        schemaVersion = 1
        productName = $productName
        productCode = $productCode
        seed = $Seed
        index = $index
    }
    [IO.File]::WriteAllText($identityPath, ($identity | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    $files.Add([ordered]@{
        path = 'fixture-identity.json'
        size = (Get-Item -LiteralPath $identityPath).Length
        sha256 = (Get-FileHash -LiteralPath $identityPath -Algorithm SHA256).Hash.ToLowerInvariant()
    })

    $installerPath = Join-Path $installerRoot $packageName
    $generatedScriptPath = Join-Path $scriptRoot ("$productSlug.nsi")
    $generatedCleanupPath = Join-Path $scriptRoot ("Cleanup-$productSlug.cmd")
    [IO.File]::Copy($templatePath, $generatedScriptPath, $true)
    $cleanupTemplate = [IO.File]::ReadAllText($cleanupScriptPath)
    $cleanupContent = $cleanupTemplate.Replace('__PRODUCT_SLUG__', $productSlug)
    [IO.File]::WriteAllText($generatedCleanupPath, $cleanupContent, [Text.Encoding]::ASCII)
    $payloadBytes = 0
    foreach ($file in $files) {
        $payloadBytes += [int64]$file.size
    }
    $estimatedSizeKb = [Math]::Max(1, [Math]::Ceiling($payloadBytes / 1KB))
    $buildNumber = ($index + 1)
    $arguments = @(
        '/V2',
        '/WX',
        '/INPUTCHARSET',
        'UTF8',
        "/DPRODUCT_NAME=$productName",
        "/DPRODUCT_SLUG=$productSlug",
        "/DUNINSTALL_KEY=$uninstallKey",
        "/DPAYLOAD_DIR=$fixturePayloadRoot",
        "/DOUTPUT_PATH=$installerPath",
        "/DCLEANUP_SCRIPT=$generatedCleanupPath",
        "/DESTIMATED_SIZE_KB=$estimatedSizeKb",
        "/DBUILD_NUMBER=$buildNumber",
        $generatedScriptPath
    )

    & $MakeNsisPath @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "makensis failed for $productName with exit code $LASTEXITCODE"
    }
    if (-not (Test-Path -LiteralPath $installerPath)) {
        throw "makensis did not produce expected installer: $installerPath"
    }

    $fixtures.Add([ordered]@{
        index = $index
        productName = $productName
        productCode = $productCode
        packageName = $packageName
        installer = "installers\$packageName"
        installerSha256 = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
        installDirectoryRelativeToLocalAppData = "Programs\RustYuFixtures\$productSlug"
        uninstallRegistryKey = "HKCU:\$uninstallKey"
        files = $files
    })
}

$manifest = [ordered]@{
    schemaVersion = 1
    architecture = 'amd64'
    peMachine = 'AMD64'
    target = 'amd64-unicode'
    seed = $Seed
    count = $Count
    compiler = [ordered]@{
        distribution = 'negrutiu-nsis-amd64'
        version = '3.13.7497.330'
    }
    fixtures = $fixtures
}
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))

Write-Output $manifestPath
