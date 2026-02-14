# Project Context: Modern Windows Uninstaller (Tauri v2 + Rust)

## 1. Project Overview & Role

你是一个精通 Rust 系统编程和 Windows 内部机制的专家开发者。
当前项目是一个 **Windows 专用反安装工具**（类似 Your Uninstaller 的现代化重写版）。
目标是利用 **Tauri v2** 构建一个高性能、轻量级、且界面现代化的桌面应用。

## 2. Tech Stack Constraints

- **Core Framework:** Tauri v2.0+ (Must use `tauri-plugin` ecosystem).
- **Backend:** Rust (Edition 2021).
- **Frontend:** React + TypeScript + Vite.
- **Styling:** Tailwind CSS + shadcn/ui (追求 Clean, Modern Windows 11 风格).
- **State Management:** Zustand (Frontend) / Tauri State (Backend).
- **Windows API:** `windows-rs` (用于底层 API), `winreg` (用于注册表操作).

## 3. Critical Backend Rules (Rust)

### A. Safety & Error Handling (最重要的约束)

作为一个系统清理工具，**安全性是第一位的**。

- **No `unwrap()` / `expect()`:** 在涉及文件删除、注册表修改的代码路径中，严禁使用 `unwrap`。必须使用 `Result<T, E>` 并通过 `anyhow` 或 `thiserror` 传播错误。
- **Dry Run Mode:** 设计删除逻辑时，必须支持“模拟执行”模式（只列出要删除的项目，不实际执行），以便在 UI 中向用户确认。
- **Privilege Checks:** 在执行卸载命令前，必须检查当前进程是否拥有 **Administrator** 权限。如果不足，应返回明确错误提示前端请求提权。

### B. Windows Registry & System Interaction

- **Registry Traversal:** 使用 `winreg` crate 遍历 `HKLM` 和 `HKCU` 的 `Software\Microsoft\Windows\CurrentVersion\Uninstall` 键值。
- **Encoding:** 注意处理 Windows 的 UTF-16 字符串（`U16String`），防止中文软件名称乱码。
- **Performance:** 扫描已安装软件时属于 I/O 密集型操作，**必须**在 `tokio::spawn` 或 `tauri::async_runtime` 中运行，绝对禁止阻塞主线程。

### C. Command Pattern

- 所有前端调用的 Rust 函数必须用 `#[tauri::command]` 宏装饰。
- 所有的 Command 必须返回 `Result<T, CommandError>`，其中 `CommandError` 需要实现 `serde::Serialize` 以便前端捕获。

## 4. Frontend Rules (React/TS)

### A. UI/UX for System Tools

- **Native Feel:** 使用 Tauri 的 Window Vibrancy (Mica/Acrylic) 效果，使应用看起来像原生 Windows 11 应用。
- **Optimistic UI:** 卸载过程可能需要时间，必须展示准确的 Loading 状态或进度条，不能让界面假死。
- **Log Streaming:** 使用 Tauri 的 Event System (`emit/listen`) 将后端的卸载日志实时传输到前端展示（例如：`Deleting registry key: HKEY_...`）。

### B. Architecture

- **Separation of Concerns:** UI 组件只负责渲染，逻辑处理通过 Custom Hooks 调用 Tauri Commands。
- **Type Safety:** 所有的 Tauri Command 参数和返回值必须在 TypeScript 中定义对应的 Interface/Type，最好使用工具自动生成类型定义。

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

---

**Think Step-by-Step for every generation:**

1.  **Safety Check:** Does this code delete files? If yes, is there a confirmation step or backup mechanism?
2.  **Windows Compat:** Does this rely on a specific Windows version API?
3.  **Async:** Is this blocking the UI thread?
