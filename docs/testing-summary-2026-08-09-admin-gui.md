# 管理员 GUI 实施验收记录（2026-08-09）

## 已提交实现

本轮实施按计划拆分为独立提交：

- `38c3d5a`–`0065f9a`：应用卸载工作流、Tauri coordinator、模态残留审查、Task Scheduler 提升启动、per-machine NSIS、CLI 退役。
- `5372b13`：清理剩余 Scoop updater。
- `1ed6d2e`：收紧 WebView CSP/capabilities，移除 shell 插件，补齐 destructive command 管理员校验。
- `c006eb6`：增加提升启动与安装生命周期验证脚本。
- `ce1f89b`：发布脚本只构建和发布 GUI NSIS 安装器。

## 已通过的验证

```text
cargo fmt --all -- --check
cargo test -p rust-yu --lib -- --test-threads=1       116 passed
Set-Location src-tauri/src-frontends/webui
npm run test                                           3 passed
npm run lint                                           passed
npm run build                                          passed
Set-Location ../../..
powershell -File tools/release/Test-Publish-Release.ps1 passed
powershell -File tools/release/publish-release.ps1 -DryRun -SkipBuild -SkipPush -SkipRelease passed
```

前端构建仍会报告仓库既有的 `INEFFECTIVE_DYNAMIC_IMPORT` 信息性警告，不是失败。

## 当前环境限制

本机是 Windows 11 ARM checkout，Tauri 原生 bundle/check 在 `tauri-winres` 阶段被环境工具链阻断：

```text
windres: Can't detect target endianness and architecture
```

因此本记录不会虚构 NSIS 安装器或管理员 Task Scheduler 实机通过结果。获得可用的 ARM GNU/LLVM `windres` 或 x64 Windows 构建环境后，应运行：

```powershell
cargo check --workspace
Set-Location src-tauri
npx tauri build --bundles nsis
Set-Location ..
.\tools\test\Verify-ElevatedGuiLifecycle.ps1 -InstallerPath <NSIS installer>
.\tools\test\Verify-InnoLegacyFixture.ps1 -RunLifecycle
```

## 安全门禁

- 正式 bundle 仅允许 NSIS `perMachine`，安装目录必须在 Program Files。
- 不再构建、测试、打包或发布 `yu.exe`。
- 计划任务 action、参数和任务路径是后端固定值；debug/worktree 不注册持久任务。
- destructive commands 在后端重新检查管理员权限；前端不能通过任意路径或命令执行。
- 残留清理只接受扫描快照中的 trace ID、明确确认和执行前复核。
- 生产 CSP 不含 `unsafe-inline`/`unsafe-eval`，capability 不再包含 shell 权限。
