# CLI Bin Name Yu Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 CLI 构建产物名从 `rust-yu.exe` 改为 `yu.exe`，不修改应用名或 Tauri 标识。

**Architecture:** 仅调整根目录 `Cargo.toml` 的 `[[bin]]` 名称来改变产物文件名，并同步更新 `src/main.rs` 中 clap 的命令名，确保 `--help` 的 Usage 与实际可执行文件一致。包名、Tauri 配置、存储目录和日志文件名保持不变，避免引入不必要的外部兼容性变化。

**Tech Stack:** Rust 2021, Cargo, clap。

---

### Task 1: 命令名回归测试

**Files:**
- Modify: `src/main.rs`
- Test: `src/main.rs`

**Step 1: Write the failing test**

添加一个测试，验证 clap 暴露的命令名是 `yu`。

**Step 2: Run test to verify it fails**

Run: `cargo test cli_command_name_is_yu -- --nocapture`
Expected: FAIL，因为当前命令名仍是 `rust-yu`。

**Step 3: Write minimal implementation**

把 `src/main.rs` 的 `#[command(name = ...)]` 改为 `yu`。

**Step 4: Run test to verify it passes**

Run: `cargo test cli_command_name_is_yu -- --nocapture`
Expected: PASS

### Task 2: 构建产物名切换

**Files:**
- Modify: `Cargo.toml`

**Step 1: Rename bin**

将根目录 `Cargo.toml` 的 `[[bin]].name` 从 `rust-yu` 改为 `yu`。

**Step 2: Build verification**

Run: `cargo build`
Expected: PASS，且 `target\\debug\\yu.exe` 存在。
