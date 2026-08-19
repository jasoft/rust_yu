#requires -Version 7.0

[CmdletBinding()]
param(
    [ValidateRange(1, 65535)]
    [int]$Port = 8080
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path

Write-Host "Rust Yu HTTP API 局域网模式：监听 0.0.0.0:$Port" -ForegroundColor Green
Write-Host "其他机器请访问：http://<本机局域网IP>:$Port/api/programs" -ForegroundColor Cyan

& (Join-Path $repoRoot "tools\dev\Run-Gui.ps1") -ApiMode lan -ApiPort $Port
exit $LASTEXITCODE
