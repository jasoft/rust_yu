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

## 工作树构建诊断

本机是 Windows 11 ARM checkout。新 worktree 默认使用 PATH 中 Scoop GCC 的
`windres`，而 `tauri-winres` 为 `aarch64-pc-windows-gnullvm` 传入的 ARM64
资源目标无法被该工具识别，因此会报：

```text
windres: Can't detect target endianness and architecture
```

这个错误在当前功能 checkout 和干净的 `main` checkout 中均可复现，说明它不是
worktree 文件缺失造成的。使用 LLVM-MinGW 的 `windres`，并设置：

```powershell
$env:MINGW_CHOST = "aarch64-w64-mingw32"
$env:Path = "<llvm-mingw>\bin;$env:Path"
```

即可通过资源编译阶段。之后当前 checkout 暴露出的真实源码问题（`thiserror`
依赖、`Win32_System_Variant` feature、提升启动错误类型和两个 Rust 安全/借用
错误）已在 `3627d58` 修复。

已验证：

```text
cargo check --workspace                         passed (LLVM-MinGW 环境)
cargo test -p rust-yu --lib -- --test-threads=1 116 passed
cargo test -p rust-yu-tauri --lib -- --test-threads=1 18 passed
cargo fmt --all -- --check                      passed
```

前端 `npm ci`、`npm run test`、`npm run lint` 和 `npm run build` 也已通过。首次
执行 Tauri 打包前还必须在 `src-tauri` 单独执行 `npm ci`，否则会出现
`npm error could not determine executable to run`，因为 Tauri CLI 位于该目录的
被忽略 `node_modules` 中。

本机随后已用 LLVM-MinGW 启动 `npx tauri build --bundles nsis`；Rust release
编译越过资源和源码检查后，在 ARM64 LTO 阶段超过五分钟仍未生成最终 exe，已
停止这组当前 worktree 的构建进程。因此本记录不虚构 NSIS 安装器或管理员
Task Scheduler 实机通过结果。获得更快的 ARM64 构建环境或 x64 Windows 构建
环境后，应运行：

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
