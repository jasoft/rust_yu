[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$bootstrapScript = Join-Path $repoRoot "tools\dev\Initialize-Worktree.ps1"

& $bootstrapScript -SkipFrontend -SkipCheck
if (-not $?) {
    throw "Worktree initialization failed"
}

Push-Location (Join-Path $repoRoot "src-tauri")
try {
    npx tauri dev
} finally {
    Pop-Location
}
