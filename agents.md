# Project Context: Modern Windows Uninstaller (Tauri v2 + Rust)

## 1. Project Overview & Role

你是一个精通 Rust 系统编程和 Windows 内部机制的专家开发者。
当前项目是一个 **Windows 专用反安装工具**（类似 Your Uninstaller 的现代化重写版）。
目标是利用 **Tauri v2** 构建一个高性能、轻量级、且界面现代化的桌面应用。

## 1.5 Repository Layout

- `src-tauri/` — Rust workspace 根（crate 名 `rust-yu`），Tauri 后端命令、扫描/卸载引擎都在 `src-tauri/src/`。
- `src-tauri/src-frontends/webui/` — 唯一前端（React + TS + Vite + Tailwind 4），i18n 语言文件在 `src/i18n/locales/`，lint 用 `oxlint`，测试用 `vitest`。
- `tools/dev/` — 环境初始化与 GUI 启动脚本（`Initialize-Worktree.ps1`、`Run-Gui.ps1`、`Run-WebUiChecks.ps1`、`Configure-X64Environment.ps1`）；`tools/test/` — 卸载夹具与 E2E 脚本。
- `docs/` — 设计文档与测试记录；`openspec/` — 变更提案；`legacy-delphi/` — 旧版参考代码，勿改动；`vendor/` — patched crates（如 `tauri-winres`）。
- `.resources/` — 测试夹具（MSI、Inno Setup legacy fixture）；`resources/` — 应用资源，二者不是同一个目录。

## 2. Tech Stack Constraints

- **Core Framework:** Tauri v2.0+ (Must use `tauri-plugin` ecosystem).
- **Backend:** Rust (Edition 2021).
- **Frontend:** React + TypeScript + Vite.
- **Styling:** Tailwind CSS + 项目现有 Fluent Light 语义样式与 UI primitives（追求 Clean, Modern Windows 11 风格）。新增页面必须复用现有设计令牌和语义类，禁止重新引入一套不一致的深色主题；shadcn/ui 仅在与现有设计系统兼容时使用。
- **State Management:** Zustand (Frontend) / Tauri State (Backend).
- **Windows API:** `windows-rs` (用于底层 API), `winreg` (用于注册表操作).

### A. 强制 x64 编译链（不可回退）

- **唯一允许的 Rust 工具链：** `stable-x86_64-pc-windows-msvc`。Rust host、Cargo target、Clippy、rustfmt、Tauri 原生构建、测试和发布产物必须全部使用 AMD64/x64 MSVC。
- **禁止 ARM/GNU 回退：** 即使开发机是 Windows 11 ARM，也禁止使用 `aarch64-pc-windows-*`、`arm64`、`*-gnullvm` 或系统 PATH 中的 ARM Rust 来绕过缺失组件、编译错误或测试失败。x64 工具链不可用时必须停止并修复 x64 环境。
- 仓库根目录的 `rust-toolchain.toml` 和 `.cargo/config.toml` 是强制边界，不得改成 ARM；不得在命令中传入 `--target aarch64-pc-windows-gnullvm`。
- 这台机器的系统 PATH 可能优先命中 ARM GNU Rust。所有自动化必须通过 rustup 的 x64 代理调用 Rust 工具：

  ```powershell
  $X64Cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
  $X64Rustc = Join-Path $env:USERPROFILE ".cargo\bin\rustc.exe"
  $X64Rustfmt = Join-Path $env:USERPROFILE ".cargo\bin\rustfmt.exe"
  & $X64Rustc -vV
  & $X64Cargo +stable-x86_64-pc-windows-msvc check --workspace
  & $X64Cargo +stable-x86_64-pc-windows-msvc test --workspace
  & $X64Cargo +stable-x86_64-pc-windows-msvc clippy --workspace --all-targets
  & $X64Rustfmt --edition 2021 --check <files...>
  ```

- 每次编译或测试前都必须检查 `rustc -vV` 的 `host`，只有 `host: x86_64-pc-windows-msvc` 才能继续；同时检查最终 PE/安装包为 AMD64，而不是仅依赖命令名称判断架构。
- 安装 Rust 组件时必须显式指定 x64 工具链，例如：

  ```powershell
  & "$env:USERPROFILE\.cargo\bin\rustup.exe" component add clippy rustfmt --toolchain stable-x86_64-pc-windows-msvc
  ```

- 调用会间接执行 `rustc`/`cargo` 的 PowerShell、Tauri、NSIS 或测试脚本前，必须把 `%USERPROFILE%\.cargo\bin` 放到该进程 PATH 最前面，并再次验证 x64 host；禁止让脚本继承并误用系统 ARM Rust。

## 3. Critical Backend Rules (Rust)

### A. Safety & Error Handling (最重要的约束)

作为一个系统清理工具，**安全性是第一位的**。

- **No `unwrap()` / `expect()`:** 在涉及文件删除、注册表修改的代码路径中，严禁使用 `unwrap`。必须使用 `Result<T, E>` 并通过 `anyhow` 或 `thiserror` 传播错误。
- **Dry Run Mode:** 设计删除逻辑时，必须支持“模拟执行”模式（只列出要删除的项目，不实际执行），以便在 UI 中向用户确认。
- **Privilege Checks:** 在执行卸载命令前，必须检查当前进程是否拥有 **Administrator** 权限。如果不足，应返回明确错误提示前端请求提权。

### B. Windows Registry & System Interaction

- **Registry Traversal:** 使用 `winreg` crate 遍历 `HKLM` 和 `HKCU` 的 `Software\Microsoft\Windows\CurrentVersion\Uninstall` 键值。
- **Encoding:** Windows 边界必须使用 `windows-rs`、`winreg`、`OsString`/UTF-16 安全转换，禁止把 Windows 路径或注册表字符串按窄字节编码处理，防止中文软件名称乱码；不强制依赖某一个具体 UTF-16 包装类型。
- **Performance:** 扫描已安装软件时属于 I/O 密集型操作，**必须**在 `tokio::spawn` 或 `tauri::async_runtime` 中运行，绝对禁止阻塞主线程。

### C. Command Pattern

- 所有前端调用的 Rust 函数必须用 `#[tauri::command]` 宏装饰。
- 所有的 Command 必须返回 `Result<T, CommandError>`，其中 `CommandError` 需要实现 `serde::Serialize` 以便前端捕获。

## 4. Frontend Rules (React/TS)

### A. UI/UX for System Tools

- **Native Feel:** 优先复用项目现有 Fluent Light 设计系统。Mica/Acrylic 只有在实现了 Windows 版本兼容、不可用时回退和真实桌面验证后才能启用，不得把未实现的视觉效果写成已完成能力。
- **Optimistic UI:** 卸载过程可能需要时间，必须展示准确的 Loading 状态或进度条，不能让界面假死。
- **Log Streaming:** 使用 Tauri 的 Event System (`emit/listen`) 将后端的卸载日志实时传输到前端展示（例如：`Deleting registry key: HKEY_...`）。

### B. Architecture

- **Separation of Concerns:** UI 组件以渲染和交互编排为主，业务逻辑通过 Zustand store、service 或 Custom Hook 调用 Tauri Commands；禁止在大型页面中散落重复的 `invoke`、错误转换和状态机逻辑。
- **Type Safety:** 所有的 Tauri Command 参数和返回值必须在 TypeScript 中定义对应的 Interface/Type，最好使用工具自动生成类型定义。

### C. 国际化（强制）

- **禁止硬编码界面文字：** 修改界面或增加任何用户可见文字时，严禁把文案直接写在 HTML、JSX、TSX 或组件属性中。按钮、标题、说明、占位符、无障碍标签、确认提示、错误、状态和日志等文案都必须通过国际化函数读取。
- **每种语言独立文件：** 所有静态界面文案必须保存在 `src-tauri/src-frontends/webui/src/i18n/locales/` 下对应的独立语言文件中，并通过 `t("translation.key")` 使用。
- **语言文件必须同步：** 新增或修改翻译键时，必须同步更新所有已支持语言，且各语言的键和插值占位符必须完全一致；不得只更新某一种语言或依赖运行时回退掩盖缺失翻译。
- **动态文案使用插值：** 包含程序名、数量、路径等动态内容的文案必须使用翻译占位符传参，不能通过在组件中拼接硬编码句子生成。
- **非翻译标识也要集中管理：** 品牌名、文件格式、协议名等即使各语言内容相同，只要会显示在界面中，也应放入语言文件统一管理。
- **验收要求：** 界面文案变更后必须运行 WebUI 的 `npm test`、`npm run build` 和 `npm run lint`；其中 `test:i18n` 必须通过，以验证语言键、占位符、引用和硬编码约束。

## 5. Specific Implementation Guidelines (The "Uninstaller" Logic)

### A. Scanning Logic (The "Leftovers" Finder)

当用户选择卸载一个软件时，除了运行默认的 `UninstallString`，还需要智能扫描残留：

1. **Registry Scan:** 扫描 `Software\[Publisher]\[AppName]`。
2. **File System Scan:** 扫描 `Program Files`, `AppData\Local`, `AppData\Roaming`。
3. **Constraint:** 扫描算法必须保守。如果不确定某个文件是否属于该软件，**默认不删除**，或者标记为“低置信度”让用户人工确认。

### B. Process Management

- 在卸载前，使用 `sysinfo` crate 检查目标软件是否正在运行。如果是，提示用户或尝试自动结束进程（`Kill Process`）。

## 6. Code Style & Linting

- **Rust:** 遵循 `clippy::pedantic` 建议。
- **Naming:**
    - Rust: `snake_case` (functions, variables), `PascalCase` (structs, enums).
    - TS: `camelCase` (functions), `PascalCase` (components).
- **Comments:** 关键的系统操作逻辑（特别是删除文件的逻辑）必须写中文注释，解释为什么这样做。

## 7. 测试资源

- winget install 7zip.7zip --silent 可以安装一个测试用的普通app(非msi) 7-zip, 支持静默卸载和gui界面卸载
- .resources\Xplorer_0.3.1_x64.msi 可以用msi命令行静默安装一个msi程序供测试卸载.
- winget install --id JRSoftware.InnoSetup --silent --accept-package-agreements --accept-source-agreements 可以安装 Inno Setup 6 编译器
- .resources\inno-legacy-test\Build-InnoLegacyFixture.ps1 会先用 rustc 编译 `tools\SpawnUninstallHelper.rs` 为 `payload\app\SpawnUninstallHelper.exe`，再调用 Inno Setup 编译安装包
- .resources\inno-legacy-test\output\RustYuLegacyTestSetup.exe 是编译产物，支持 GUI 安装和 `/VERYSILENT /NORESTART` 静默安装
- 这个 legacy 测试夹具会把 `UninstallString` 和 `QuietUninstallString` 都包装为 `SpawnUninstallHelper.exe -> UninstallWorker.ps1 -> unins000.exe` 的进程链，用来验证 application workflow 的 Job Object 等待路径
- 这个 legacy 测试夹具卸载后应保留 `C:\Program Files\RustYu Legacy Test App\logs\leftover.log` 和 `%LocalAppData%\RustYuLegacyTest\Data\leftover-user-profile.json`，便于验证安装目录与 AppData 残留扫描
- tools\test\Verify-InnoLegacyFixture.ps1 默认验证夹具结构和安装包；传入 `-RunLifecycle` 时需要管理员 PowerShell，会安装夹具、检查卸载命令是否走 spawn wrapper，并调用 application workflow 集成测试验证 `waitforjobs`

## 8. 任务结束前的强制交付验收

- **每次任务结束前必须由代理亲自完成编译、测试和运行，不得把这些工作留给用户。** 即使本次只修改文档、配置或前端，也必须确认完整项目仍能编译，并启动当前 checkout 生成的桌面程序。
- 编译前必须按“强制 x64 编译链”检查 `rustc -vV`，确认 `host: x86_64-pc-windows-msvc`；编译后必须检查最终 EXE/安装包的 PE 架构为 AMD64。
- 必须至少完成以下自动化验证：Rust workspace 编译、Rust workspace 测试、WebUI production build 和 WebUI tests。任务涉及 lint、安装包、发布脚本或特定模块时，还必须运行对应检查，不能用局部语法检查替代完整用户路径。
- 必须从本次实际修改所在的 checkout 启动刚编译的 `rust-yu-tauri.exe`，确认进程没有立即退出。涉及桌面 UI 的任务还必须使用可用的 Windows GUI/Computer Use 工具进入受影响页面并检查真实界面；浏览器预览不能替代 Tauri 桌面验证。
- 必须区分并如实记录四种证据：`编译通过`、`自动化测试通过`、`桌面进程成功启动`、`真实 GUI/UAC/安装卸载生命周期已交互验证`。前三级不能冒充第四级。
- 如果因为 UAC、桌面自动化、缺少测试夹具或外部环境而无法完成必要的运行验证，任务仍视为未完全验收：必须继续排查；确实无法推进时，要明确报告阻塞点和未验证范围，禁止使用“已完成”“没有问题”等表述。
- 运行验证必须安全：不得为了验收直接清理用户真实数据。卸载、leftover 清理、恢复等破坏性路径优先使用仓库提供的 MSI/Inno/7-Zip 测试夹具，并保留确认、备份和恢复边界。

---

**Think Step-by-Step for every generation:**

1. **Safety Check:** Does this code delete files? If yes, is there a confirmation step or backup mechanism?
2. **Windows Compat:** Does this rely on a specific Windows version API?
3. **Async:** Is this blocking the UI thread?
