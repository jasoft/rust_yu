# Rust Yu Windows 分发说明

Rust Yu 提供两种发布形态：NSIS 安装版和便携 ZIP。两者共用同一个 Tauri GUI 和 Rust 工作流；区别只在启动器如何管理应用自身的状态和升级入口。

## 安装版

安装版由 Tauri NSIS 目标生成，默认安装到受保护目录，并沿用管理员 GUI 启动流程。安装版的缓存、备份、安装监控和报告存放在当前用户的 Rust Yu 应用数据目录。卸载 Rust Yu 时，NSIS hook 会先要求 GUI 清理受保护的启动任务，失败就中止卸载。

```powershell
Set-Location .\src-tauri
npx tauri build --bundles nsis
```

## 便携版

先得到目标架构的 `rust-yu-tauri.exe`（工作区默认输出在 `target\release`），再从仓库根目录运行：

```powershell
.\tools\release\package-portable.ps1 `
  -BinaryPath .\target\release\rust-yu-tauri.exe
```

脚本生成 `dist\portable\rust-yu-portable\` 和同名 ZIP。目录中包含 EXE、`portable.flag` 和说明文件；第一次启动会在 EXE 同级创建 `data\`。便携数据根目录统一供以下内容使用：

- SQLite 程序缓存和图标缓存；
- 日志、删除前备份、安装监控快照和卸载报告/导出文件；
- 启动过程中不创建 Rust Yu 的管理员计划任务，也不依赖安装目录注册表配置。

便携版必须放在用户可写的本地目录。启动器会拒绝把 `data\` 解析为符号链接或普通文件，并用独立的实例互斥体命名空间隔离同一用户下的安装版。图标缓存同时受 Tauri `$RESOURCE/data/icon-cache/**/*` 资产范围保护。

## 架构和兼容性

- 目标平台：Windows 10/11 x64 和 ARM64；安装器与 ZIP 必须按目标架构分别构建。
- Rust Yu 的卸载、注册表和系统集成功能仍需要管理员令牌；便携不等于绕过 UAC。
- 在发布前运行 `tools\release\Test-Package-Portable.ps1` 和 `tools\release\Test-Publish-Release.ps1`。
- ARM64 GNU 目标使用仓库内 `vendor\tauri-winres` 补丁：`tauri-winres` 原有的 `windres` 路径不接受 `pe-aarch64-little`，补丁会改用 `llvm-rc` 和 `llvm-cvtres /MACHINE:ARM64` 生成正确的 COFF 资源。请确保这两个 LLVM 工具在 `PATH` 中；也可以分别用 `RUST_YU_LLVM_RC` 和 `RUST_YU_LLVM_CVTRES` 指定完整路径。
- 在 Parallels Windows ARM 开发机上，先运行 `cargo build -p rust-yu-tauri` 验证 ARM64 可执行文件，再运行 Tauri 的 NSIS 打包命令。已验证生成的 EXE 为 ARM64，且包含资源表；不能只把 `cargo check` 当成完整桌面构建证据。

## 数据恢复和迁移

安装版与便携版故意不自动互相复制数据，避免把旧机器的备份或路径快照静默带入新环境。用户需要迁移时，应在应用内导出报告；备份会话应通过恢复中心在同一数据根目录中使用。
