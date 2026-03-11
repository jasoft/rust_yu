# List Sort Direction Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 让 `list` 命令默认按名称 `A-Z` 输出，并保留 `--ascending` 同时新增互斥的 `--descending`。

**Architecture:** 在 `src/commands/list.rs` 中增加新的方向参数，并把排序逻辑抽成可测试的辅助函数。默认不反转结果，只有显式传入 `--descending` 时才做反转，这样 `name`、`date`、`size` 的默认方向都统一为升序。

**Tech Stack:** Rust 2021, clap, anyhow。

---

### Task 1: 排序方向回归测试

**Files:**
- Modify: `src/commands/list.rs`
- Test: `src/commands/list.rs`

**Step 1: Write the failing test**

添加测试验证：
- 默认名称排序为 `A-Z`
- `--descending` 能被解析
- `--ascending --descending` 同时传入会被 clap 拒绝

**Step 2: Run test to verify it fails**

Run: `cargo test list::tests -- --nocapture`
Expected: FAIL，因为当前默认仍会反转名称排序，且还没有 `--descending`。

**Step 3: Write minimal implementation**

新增 `descending` 字段，并让 `ascending` / `descending` 互斥；提取排序辅助函数，默认保持升序，仅在 `descending` 为真时反转。

**Step 4: Run test to verify it passes**

Run: `cargo test list::tests -- --nocapture`
Expected: PASS

### Task 2: CLI 帮助验证

**Files:**
- Modify: `src/commands/list.rs`

**Step 1: Run help verification**

Run: `cargo run -- list --help`
Expected: 输出中同时包含 `--ascending` 与 `--descending`。
