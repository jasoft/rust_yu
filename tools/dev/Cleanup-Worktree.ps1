[CmdletBinding()]
param(
    [switch]$PruneStaleWorktrees
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Set-Location $repoRoot

$currentWorktree = (git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($currentWorktree)) {
    throw "当前目录不是 Git worktree，无法执行清理。"
}

$currentWorktree = [IO.Path]::GetFullPath($currentWorktree).TrimEnd('\')
$mainWorktree = $null
$worktreeLines = @(git worktree list --porcelain)
for ($index = 0; $index -lt $worktreeLines.Count; $index += 1) {
    if ($worktreeLines[$index] -like "worktree *") {
        $candidate = $worktreeLines[$index].Substring(9)
        if ($null -eq $mainWorktree) {
            $mainWorktree = [IO.Path]::GetFullPath($candidate).TrimEnd('\')
        }
    }
}

if ($currentWorktree -eq $mainWorktree) {
    throw "拒绝清理主 worktree：$currentWorktree。请在 Codex 创建的临时 worktree 中运行。"
}

Write-Host "正在停止当前 worktree 启动的开发进程：$currentWorktree"
$processes = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {
    $commandLine = [string]$_.CommandLine
    $matchesWorktree = $commandLine.IndexOf($currentWorktree, [StringComparison]::OrdinalIgnoreCase) -ge 0
    $isDevelopmentProcess = $_.Name -in @("rust-yu-tauri.exe", "vite.exe") -or
        ($_.Name -eq "node.exe" -and $commandLine -match "vite")
    $matchesWorktree -and $isDevelopmentProcess -and $_.ProcessId -ne $PID
})

foreach ($process in $processes) {
    Write-Host "停止 $($process.Name) (PID $($process.ProcessId))"
    Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
}

if ($PruneStaleWorktrees) {
    Write-Host "清理已失效的 Git worktree 元数据。"
    git worktree prune --verbose
    if ($LASTEXITCODE -ne 0) {
        throw "git worktree prune 失败。"
    }
}

Write-Host "Worktree 清理完成。未删除源代码或主 worktree。" -ForegroundColor Green
