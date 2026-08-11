# Project Context: Modern Windows Uninstaller (Tauri v2 + Rust)

## 1. Project Overview & Role

你是一个精通 Rust 系统编程和 Windows 内部机制的专家开发者。
当前项目是一个 **Windows 专用反安装工具**（类似 Your Uninstaller 的现代化重写版）。
目标是利用 **Tauri v2** 构建一个高性能、轻量级、且界面现代化的桌面应用。

## 2. Tech Stack Constraints

- **Core Framework:** Tauri v2.0+ (Must use `tauri-plugin` ecosystem).
- **Backend:** Rust (Edition 2021).
- **Frontend:** React + TypeScript + Vite.
- **Styling:** Tailwind CSS + 项目现有 Fluent Light 语义样式与 UI primitives（追求 Clean, Modern Windows 11 风格）。新增页面必须复用现有设计令牌和语义类，禁止重新引入一套不一致的深色主题；shadcn/ui 仅在与现有设计系统兼容时使用。
- **State Management:** Zustand (Frontend) / Tauri State (Backend).
- **Windows API:** `windows-rs` (用于底层 API), `winreg` (用于注册表操作).

### X64 工具链（强制）

- 开发机虽然是 Windows 11 ARM，但 Rust Yu **只允许**使用 `x86_64-pc-windows-msvc` 构建和发布；Node.js、npm 原生依赖、Rust、MSVC linker 也必须全部为 X64，禁止退回 ARM64 GNU/LLVM 或 ARM64 Node。
- 在任何 Cargo、Tauri、测试或 GUI 命令前，必须从仓库根目录运行 `tools\dev\Initialize-Worktree.ps1`（可按任务使用 `-SkipFrontend` / `-RunCheck`），并确认输出中的 Rust host、Node architecture 和 `link.exe` 都是 X64。
- 不得直接依赖提权 PowerShell 继承的 PATH。`%USERPROFILE%\.cargo\bin` 必须优先于系统中旧的 ARM Rust，Node/npm/npx 必须来自同一个 X64 Node.js 目录。

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

## 8. 交付验收门禁（强制，不可用 `cargo check` 替代）

### A. 证据等级

- `cargo check` 只证明代码通过类型检查和元数据/部分构建脚本检查；它通常不生成最终可执行文件，也不能证明最终链接、资源嵌入、前端构建、原生 Node binding、UAC、Tauri 启动或窗口渲染成功。
- `cargo build` / `cargo test` 证明链接或测试产物能够生成，但仍不能证明管理员 GUI 能正常启动和交互。
- 浏览器预览、Vite 页面、进程存在、端口监听或脚本语法检查都不能替代真实 Tauri 桌面窗口验收。

### B. 每次交付必须完成

1. 确认当前实际运行的 checkout 就是本次修改所在 checkout，并检查 `git status`，不得覆盖用户或其他任务的并发改动。
2. 运行 `tools\dev\Initialize-Worktree.ps1`，确认 Rust、Node 和 MSVC linker 均为 X64。
3. 运行与改动相关的完整 Rust 测试；涉及 workspace/Tauri 集成时，必须运行 `cargo test --workspace -- --test-threads=1`。GUI 正在运行并锁住默认产物时，使用当前 checkout 下独立的 `CARGO_TARGET_DIR`，不得因此跳过测试。
4. 涉及 WebUI 时，在 `src-tauri\src-frontends\webui` 运行 `npm test`、`npm run lint`、`npm run build`；界面文案变更必须包含 `test:i18n`。
5. **必须实际运行 `tools\dev\Run-Gui.ps1`，接受 UAC，并看到当前 checkout 构建出的 Rust Yu Tauri 窗口成功显示。** 至少确认窗口可响应、没有启动错误页、能够读取原生已安装程序数据；涉及的页面/功能必须在真实桌面窗口中打开并完成最小安全交互验证。
6. 验证启动的 `rust-yu-tauri.exe` 为 X64 PE，且进程路径属于当前 checkout 的构建输出；不得拿旧进程、其他 worktree、安装版旧 EXE 或浏览器页面充当本次验收。
7. `Run-Gui.ps1` 报错、UAC 未完成、窗口不可见、窗口使用错误 checkout、原生数据不可用或功能未在真实 GUI 中验证时，交付状态必须标为失败/未完成；必须修复根因并重新走完整门禁，禁止仅报告 `cargo check` 成功。

### C. 交付报告

- 最终交付必须分别列出：X64 工具链证据、Rust 测试、WebUI test/lint/build、`Run-Gui.ps1` 退出/启动结果、可见 GUI 验证、真实 UAC/系统操作验证。
- 未执行的层级必须明确写“未验证”，不得用较低证据等级推断较高等级通过。

---

**Think Step-by-Step for every generation:**

1. **Safety Check:** Does this code delete files? If yes, is there a confirmation step or backup mechanism?
2. **Windows Compat:** Does this rely on a specific Windows version API?
3. **Async:** Is this blocking the UI thread?
