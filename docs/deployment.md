# Rust Yu Windows 分发说明

Rust Yu 提供两种发布形态：NSIS 安装版和便携 ZIP。两者共用同一个 Tauri GUI 和 Rust 工作流；区别只在启动器如何管理应用自身的状态和升级入口。

## 安装版

安装版由 Tauri NSIS 目标生成，默认安装到受保护目录，并沿用管理员 GUI 启动流程。安装版的缓存、备份、安装监控和报告存放在当前用户的 Rust Yu 应用数据目录。卸载 Rust Yu 时，NSIS hook 会先要求 GUI 清理受保护的启动任务，失败就中止卸载。

```powershell
Set-Location .\src-tauri
npx tauri build --target x86_64-pc-windows-msvc --bundles nsis
```

## 便携版

先得到 x64 目标的 `rust-yu-tauri.exe`（工作区默认输出在 `src-tauri\target\x86_64-pc-windows-msvc\release`），再从仓库根目录运行：

```powershell
.\tools\release\package-portable.ps1 `
  -BinaryPath .\src-tauri\target\x86_64-pc-windows-msvc\release\rust-yu-tauri.exe
```

脚本生成 `dist\portable\rust-yu-portable\` 和同名 ZIP。目录中包含 EXE、`portable.flag` 和说明文件；第一次启动会在 EXE 同级创建 `data\`。便携数据根目录统一供以下内容使用：

- SQLite 程序缓存和图标缓存；
- 日志、删除前备份、安装监控快照和卸载报告/导出文件；
- 启动过程中不创建 Rust Yu 的管理员计划任务，也不依赖安装目录注册表配置。

便携版必须放在用户可写的本地目录。启动器会拒绝把 `data\` 解析为符号链接或普通文件，并用独立的实例互斥体命名空间隔离同一用户下的安装版。图标缓存同时受 Tauri `$RESOURCE/data/icon-cache/**/*` 资产范围保护。

## 架构和兼容性

- 发布目标：Windows 10/11 x64（`x86_64-pc-windows-msvc`）。ARM64 开发机只作为运行和测试环境，不作为发布目标。
- Rust Yu 的卸载、注册表和系统集成功能仍需要管理员令牌；便携不等于绕过 UAC。
- 在发布前运行 `tools\release\Test-Package-Portable.ps1` 和 `tools\release\Test-Publish-Release.ps1`。
- 在 Parallels Windows ARM 开发机上，先确保 x64 MSVC linker (`link.exe`) 在 `PATH` 中，再运行 `cargo build -p rust-yu-tauri --release`；随后运行 Tauri 的 NSIS 打包命令。仓库配置会自动选择 `x86_64-pc-windows-msvc`，不能依赖当前主机架构推断发布架构。

## 数据恢复和迁移

安装版与便携版故意不自动互相复制数据，避免把旧机器的备份或路径快照静默带入新环境。用户需要迁移时，应在应用内导出报告；备份会话应通过恢复中心在同一数据根目录中使用。
