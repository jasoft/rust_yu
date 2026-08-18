#requires -Version 7.0

[CmdletBinding()]
param(
    [switch]$Test,
    [switch]$Lint,
    [switch]$Build
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$webUiRoot = Join-Path $repoRoot "src-tauri\src-frontends\webui"

$nodeCommand = Get-Command node.exe -CommandType Application -ErrorAction SilentlyContinue |
    Select-Object -First 1
if ($null -eq $nodeCommand) {
    throw "未找到 Node.js。请先安装依赖并配置可用的 Windows x64 Node.js。"
}

$nodeArchitecture = & $nodeCommand.Source -p "process.arch"
if ($LASTEXITCODE -ne 0 -or $nodeArchitecture -ne "x64") {
    throw "WebUI 检查必须使用 x64 Node.js，当前架构为：$nodeArchitecture"
}
Write-Host "WebUI 使用 x64 Node.js: $($nodeCommand.Source)" -ForegroundColor DarkGray

$npmPath = Join-Path (Split-Path -Parent $nodeCommand.Source) "npm.cmd"
if (-not (Test-Path -LiteralPath $npmPath -PathType Leaf)) {
    throw "当前 x64 Node.js 目录缺少 npm.cmd：$(Split-Path -Parent $nodeCommand.Source)"
}

$runTest = $Test.IsPresent
$runLint = $Lint.IsPresent
$runBuild = $Build.IsPresent
if (-not ($runTest -or $runLint -or $runBuild)) {
    # 默认只运行适合日常开发的快速检查；完整产物构建由 -Build 明确触发。
    $runTest = $true
    $runLint = $true
}

$steps = [System.Collections.Generic.List[object]]::new()
if ($runTest) { $steps.Add(@("run", "test")) }
if ($runLint) { $steps.Add(@("run", "lint")) }
if ($runBuild) { $steps.Add(@("run", "build")) }

Push-Location $webUiRoot
try {
    foreach ($step in $steps) {
        Write-Host ("WebUI: npm " + ($step -join " ")) -ForegroundColor Cyan
        & $npmPath @step
        if ($LASTEXITCODE -ne 0) {
            throw "WebUI 命令失败：npm $($step -join ' ')"
        }
    }
} finally {
    Pop-Location
}
