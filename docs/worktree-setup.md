# Worktree 初始化与 x64 构建

Git worktree 只检出 Git 中受版本控制的文件。它不会复制被 `.gitignore` 忽略的构建缓存或当前终端环境，因此新 worktree 第一次使用需要初始化：

- `target/` 不复制，每个 worktree 都会重新建立 Rust 构建缓存；
- `node_modules/`、前端 `dist/` 不复制，需要重新执行 `npm ci`；
- `src-tauri/node_modules/` 也不复制，Tauri CLI 必须在该目录单独执行 `npm ci`；
- x64 MSVC linker 的 `PATH` 等 PowerShell 环境变量不复制；
- `legacy-delphi` 是 Git submodule，需要显式初始化；
- `.env` 等本地文件如果未被 Git 跟踪，也不会出现在新 worktree。

本项目即使运行在 Windows 11 ARM 环境，也固定使用 `x86_64-pc-windows-msvc` 目标。需要先安装 Visual Studio Build Tools 的 **Desktop development with C++** 工作负载，并在当前终端的 `PATH` 中提供 x64 MSVC `link.exe`。Node.js 也必须使用 Windows x64 版本，否则 Vite/Tauri CLI 会尝试加载 ARM64 原生绑定并启动失败。

初始化脚本会自动寻找 x64 `node.exe` 并确保 `npm`、`npx` 与它来自同一目录。如果 x64 Node 不在常规安装位置，可以显式配置：

```powershell
$env:RUST_YU_NODE_X64 = "C:\Tools\node-x64"
```

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
node.exe=<Windows x64 Node.js>\node.exe
```

然后使用离线缓存优先模式执行两个 `npm ci`，并用 `cargo metadata` 快速校验 workspace 清单。初始化不会默认编译整个 Rust 依赖图，避免每个全新 worktree 都等待数分钟；第一次真正编译由 `Run-Gui.ps1`、`cargo build` 或显式完整检查触发。

需要在初始化时执行完整 x64 `cargo check --workspace` 时使用：

```powershell
.\tools\dev\Initialize-Worktree.ps1 -RunCheck
```

仓库根目录的 `.cargo\config.toml` 已经固定默认目标；如果要在当前 PowerShell 手工执行 Cargo：

```powershell
cargo check --workspace --target x86_64-pc-windows-msvc
```

如果不需要安装前端依赖，或者只想初始化子模块：

```powershell
.\tools\dev\Initialize-Worktree.ps1 -SkipFrontend
.\tools\dev\Initialize-Worktree.ps1 -SkipFrontend -InitSubmodules
```

旧的 `-SkipCheck` 参数仍保留用于兼容已有脚本，但快速初始化现在已经是默认行为。

不要把 `target/` 或 `node_modules/` 强行复制到其他 worktree；它们包含路径相关的缓存，可能让 Cargo/Vite 使用错误 checkout 的产物。创建 worktree 后应以当前 worktree 的绝对路径重新执行初始化。若脚本提示缺少 `link.exe`，先从 Visual Studio Installer 安装 x64 MSVC 工具链；若提示缺少 X64 Node.js，则安装 Windows x64 版 Node 或设置 `RUST_YU_NODE_X64`。不要退回 ARM64 GNU/LLVM-MinGW 或 ARM64 Node。
