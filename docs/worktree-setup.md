# Worktree 初始化与 x64 构建

Git worktree 只检出 Git 中受版本控制的文件。它不会复制被 `.gitignore` 忽略的构建缓存或当前终端环境，因此新 worktree 第一次使用需要初始化：

- `target/` 不复制，每个 worktree 都会重新建立 Rust 构建缓存；
- `node_modules/`、前端 `dist/` 不复制，需要重新执行 `npm ci`；
- `src-tauri/node_modules/` 也不复制，Tauri CLI 必须在该目录单独执行 `npm ci`；
- x64 MSVC linker 的 `PATH` 等 PowerShell 环境变量不复制；
- `legacy-delphi` 是 Git submodule，需要显式初始化；
- `.env` 等本地文件如果未被 Git 跟踪，也不会出现在新 worktree。

本项目即使运行在 Windows 11 ARM 环境，也固定使用 `x86_64-pc-windows-msvc` 目标。需要先安装 Visual Studio Build Tools 的 **Desktop development with C++** 工作负载，并在当前终端的 `PATH` 中提供 x64 MSVC `link.exe`。

仓库根目录的 `rust-toolchain.toml` 还会强制使用 `stable-x86_64-pc-windows-msvc` Rust 编译器。ARM Windows 上首次安装时需要允许 Rustup 安装非本机 toolchain：

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal --force-non-host
```

```text
link.exe: fatal error LNK1104
```

新 worktree 推荐直接运行：

```powershell
.\tools\dev\Initialize-Worktree.ps1 -InitSubmodules
```

脚本会检查 x64 MSVC linker，并在当前脚本进程中使用：

```text
x86_64-pc-windows-msvc
link.exe=<Visual Studio Build Tools>\VC\Tools\MSVC\...\bin\Hostx64\x64\link.exe
```

然后执行 `npm ci` 和 x64 `cargo check --workspace`。仓库根目录的 `.cargo\config.toml` 已经固定默认目标；如果要在当前 PowerShell 手工执行 Cargo：

```powershell
cargo check --workspace --target x86_64-pc-windows-msvc
```

如果只想安装前端依赖或只想初始化子模块：

```powershell
.\tools\dev\Initialize-Worktree.ps1 -SkipCheck
.\tools\dev\Initialize-Worktree.ps1 -SkipFrontend -SkipCheck -InitSubmodules
```

不要把 `target/` 或 `node_modules/` 强行复制到其他 worktree；它们包含路径相关的缓存，可能让 Cargo/Vite 使用错误 checkout 的产物。创建 worktree 后应以当前 worktree 的绝对路径重新执行初始化。若脚本提示缺少 `link.exe`，先从 Visual Studio Installer 安装 x64 MSVC 工具链，不要退回 ARM64 GNU/LLVM-MinGW。
