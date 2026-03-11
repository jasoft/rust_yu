# Uninstall Interactive And Job Wait Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为交互式卸载器增加清晰提示，并在 Windows 上用 `Job Object` 等待整条卸载进程链，同时把结果区分为成功、取消、失败或用户中断。

**Architecture:** 在 `src/modules/common/process.rs` 中集中放置进程等待、静默/交互检测和退出码分类逻辑；`src/commands/uninstall.rs` 继续负责卸载流程编排与最终状态确认。Windows 上通过 `CreateProcessW + CREATE_SUSPENDED + AssignProcessToJobObject + ResumeThread` 启动卸载器，并在等待进程链结束后再保留现有注册表/安装目录轮询做最终验收。

**Tech Stack:** Rust 2021, tokio, windows-rs。

---

### Task 1: 结果分类与提示测试

**Files:**
- Create: `src/modules/common/process.rs`
- Modify: `src/modules/common/mod.rs`

**Step 1: Write the failing test**

添加测试验证：
- 非静默 EXE 卸载命令会被识别为可能需要用户交互
- MSI `1602` / `1223` 会被识别为用户取消
- 失败或取消时会生成清晰的中文提示

**Step 2: Run test to verify it fails**

Run: `cargo test process::tests -- --nocapture`
Expected: FAIL，因为新模块和分类逻辑尚未实现。

**Step 3: Write minimal implementation**

实现命令类型检测、退出码分类和提示文案辅助函数。

**Step 4: Run test to verify it passes**

Run: `cargo test process::tests -- --nocapture`
Expected: PASS

### Task 2: Windows 进程链等待

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/modules/common/process.rs`
- Modify: `src/commands/uninstall.rs`

**Step 1: Implement job wait**

为 Windows 卸载执行链路增加 `Job Object` 等待；无法分配到 Job 时回退为等待直接进程，并记录是否走了降级路径。

**Step 2: Integrate command flow**

在 `uninstall` 命令中：
- 启动前输出交互式提示
- 用户取消 / 失败 / 用户中断时停止后续清理
- 仅在确认程序已移除时继续残留清理

**Step 3: Run focused verification**

Run: `cargo test uninstall::tests -- --nocapture`
Expected: PASS

**Step 4: Run build verification**

Run: `cargo build`
Expected: PASS
