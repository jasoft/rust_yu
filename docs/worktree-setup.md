# Worktree 初始化与 ARM64 构建

Git worktree 只检出 Git 中受版本控制的文件。它不会复制被 `.gitignore` 忽略的构建缓存或当前终端环境，因此新 worktree 第一次使用需要初始化：

- `target/` 不复制，每个 worktree 都会重新建立 Rust 构建缓存；
- `node_modules/`、前端 `dist/` 不复制，需要重新执行 `npm ci`；
- `src-tauri/node_modules/` 也不复制，Tauri CLI 必须在该目录单独执行 `npm ci`；
- `MINGW_CHOST`、`PATH` 等 PowerShell 环境变量不复制；
- `legacy-delphi` 是 Git submodule，需要显式初始化；
- `.env` 等本地文件如果未被 Git 跟踪，也不会出现在新 worktree。

在本项目的 Windows 11 ARM 环境，资源编译必须使用 LLVM-MinGW 的 ARM64 `windres`。默认 PATH 中 Scoop GCC 的 `windres` 不支持 `pe-aarch64-little`，会产生：

```text
windres: Can't detect target endianness and architecture
```

新 worktree 推荐直接运行：

```powershell
.\tools\dev\Initialize-Worktree.ps1 -InitSubmodules
```

脚本会定位 LLVM-MinGW、在当前脚本进程中设置：

```text
MINGW_CHOST=aarch64-w64-mingw32
PATH=<llvm-mingw>\bin;...
```

然后执行 `npm ci` 和 `cargo check --workspace`。这些环境变量只对脚本及其子进程有效；如果要在当前 PowerShell 后续手工执行 Cargo，应先使用相同变量：

```powershell
$env:MINGW_CHOST = "aarch64-w64-mingw32"
$llvmBin = "C:\Users\weiwang\AppData\Local\Microsoft\WinGet\Packages\MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\llvm-mingw-20260616-ucrt-aarch64\bin"
$env:Path = "$llvmBin;$env:Path"
cargo check --workspace
```

如果只想安装前端依赖或只想初始化子模块：

```powershell
.\tools\dev\Initialize-Worktree.ps1 -SkipCheck
.\tools\dev\Initialize-Worktree.ps1 -SkipFrontend -SkipCheck -InitSubmodules
```

不要把 `target/` 或 `node_modules/` 强行复制到其他 worktree；它们包含路径相关的缓存，可能让 Cargo/Vite 使用错误 checkout 的产物。创建 worktree 后应以当前 worktree 的绝对路径重新执行初始化。
