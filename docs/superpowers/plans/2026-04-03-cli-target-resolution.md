# CLI Target Resolution Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 统一所有基于用户输入搜索 App 的 CLI 路径，在继续卸载、搜索或清理前先唯一定位目标程序。

**Architecture:** 抽取一个共享的目标解析模块，负责把原始输入解析为“唯一命中 / 未命中 / 多项命中”三种结果，并提供统一的人类可读摘要。`search`、`clean`、`uninstall` 接入该模块，在未唯一命中时立即停止，唯一命中后统一输出当前操作目标，并基于解析出的程序名称继续后续扫描或卸载。

**Tech Stack:** Rust 2021, clap, anyhow, 现有 lister/scanner/uninstall 模块

---

## Chunk 1: 共享目标解析层

### Task 1: 为目标解析结果补测试

**Files:**
- Modify: `src/commands/uninstall.rs`
- Create: `src/commands/target.rs`

- [x] **Step 1: 写失败测试**
- [x] **Step 2: 运行目标测试确认失败**
- [x] **Step 3: 实现最小目标解析逻辑**
- [x] **Step 4: 运行目标测试确认通过**

### Task 2: 输出统一候选摘要

**Files:**
- Create: `src/commands/target.rs`

- [x] **Step 1: 写失败测试，覆盖多候选提示内容**
- [x] **Step 2: 运行目标测试确认失败**
- [x] **Step 3: 实现候选摘要格式化**
- [x] **Step 4: 运行目标测试确认通过**

## Chunk 2: 命令接入

### Task 3: search 命令接入目标解析

**Files:**
- Modify: `src/commands/search.rs`

- [x] **Step 1: 写失败测试，覆盖唯一定位与多项命中时中止**
- [x] **Step 2: 运行目标测试确认失败**
- [x] **Step 3: 实现 search 接入**
- [x] **Step 4: 运行目标测试确认通过**

### Task 4: clean/uninstall 命令接入目标解析

**Files:**
- Modify: `src/commands/clean.rs`
- Modify: `src/commands/uninstall.rs`

- [x] **Step 1: 写失败测试，覆盖唯一定位后再继续执行与多候选时拒绝执行**
- [x] **Step 2: 运行目标测试确认失败**
- [x] **Step 3: 实现 clean/uninstall 接入**
- [x] **Step 4: 运行目标测试确认通过**

## Chunk 3: 验证与收尾

### Task 5: 运行验证并提交

**Files:**
- Modify: `docs/superpowers/plans/2026-04-03-cli-target-resolution.md`

- [x] **Step 1: 运行 `cargo test`**
- [ ] **Step 2: 运行 `cargo clippy --all-targets --all-features -- -D warnings`**
说明：该命令仍被仓库现有的全局 Clippy 告警阻塞，本次改动涉及的命令文件已清理新增告警。
- [x] **Step 3: 更新计划勾选状态并复查输出**
- [ ] **Step 4: `git add`、`git commit`、`git push`**
