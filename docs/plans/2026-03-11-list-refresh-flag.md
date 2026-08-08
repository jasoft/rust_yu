# List Refresh Flag Implementation Plan

> 历史文档：CLI 产品已退役，以下命令仅保留作历史记录。

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 `list` 命令增加 `--refresh` 参数，允许跳过读取缓存并立即重扫后回写缓存。

**Architecture:** 在 `src/commands/list.rs` 中新增 CLI 布尔参数，并把它映射到 `ListProgramsQuery.refresh`。缓存读写逻辑保持在 `src/modules/lister/mod.rs`，仅通过现有查询字段驱动，不引入额外分支或重复缓存实现。

**Tech Stack:** Rust 2021, clap, anyhow, 项目现有 lister/storage 模块。

---

### Task 1: CLI 解析与查询映射

**Files:**
- Modify: `src/commands/list.rs`
- Test: `src/commands/list.rs`

**Step 1: Write the failing test**

添加一个解析测试，验证 `list --refresh` 能被 clap 接受，并映射为 `ListProgramsQuery.refresh = true`。

**Step 2: Run test to verify it fails**

Run: `cargo test list::tests::list_command_accepts_refresh_flag -- --nocapture`
Expected: FAIL，因为当前 CLI 还不支持 `--refresh`。

**Step 3: Write minimal implementation**

为 `ListCommand` 增加 `refresh: bool` 字段，并抽取构造查询的辅助函数，确保 `execute()` 使用该函数。

**Step 4: Run test to verify it passes**

Run: `cargo test list::tests::list_command_accepts_refresh_flag -- --nocapture`
Expected: PASS

### Task 2: 回归验证

**Files:**
- Modify: `src/commands/list.rs`

**Step 1: Run focused tests**

Run: `cargo test list::tests -- --nocapture`
Expected: PASS

**Step 2: Run CLI help verification**

Run: `cargo run -- list --help`
Expected: 输出中包含 `--refresh` 说明。
