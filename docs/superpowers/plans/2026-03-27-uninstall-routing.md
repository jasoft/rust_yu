# Uninstall Routing Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `yu uninstall` 统一搜索已安装程序，但在执行阶段按 `legacy`、`msi`、`store` 三类卸载方式分流，修复 `Store` 程序无法正确卸载的问题。

**Architecture:** 在 `InstalledProgram` 上增加独立的 `uninstall_kind`，由各枚举器在列表阶段写入；`src/commands/uninstall.rs` 继续作为统一 CLI 入口，但将真实卸载逻辑下沉到 `src/modules/uninstall/` 下的三类处理器。`Store` 完成判定只检查包存在性，不再复用传统程序目录判定。

**Tech Stack:** Rust 2021、clap、serde、tokio、PowerShell、windows-rs、现有 lister/common 模块

---

## Chunk 1: 数据模型与测试入口

### Task 1: 增加 `uninstall_kind` 数据模型

**Files:**
- Modify: `src/modules/lister/models.rs`
- Test: `src/modules/lister/models.rs`

- [ ] **Step 1: 写失败测试**

为 `InstalledProgram::new()` 默认值和序列化兼容性添加测试，覆盖：
- 默认 `uninstall_kind = Legacy`
- 旧缓存缺失该字段时仍能反序列化成功

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test uninstall_kind`
Expected: FAIL，提示字段或断言不存在

- [ ] **Step 3: 最小实现**

在 `InstalledProgram` 上新增 `UninstallKind` 枚举与字段，使用 `#[serde(default)]` 保持兼容。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test uninstall_kind`
Expected: PASS

## Chunk 2: 列表层标注卸载类型

### Task 2: 让 registry/msi/store 枚举结果带上正确类型

**Files:**
- Modify: `src/modules/lister/registry.rs`
- Modify: `src/modules/lister/msi.rs`
- Modify: `src/modules/lister/store.rs`
- Test: `src/modules/lister/msi.rs`
- Test: `src/modules/lister/store.rs`

- [ ] **Step 1: 写失败测试**

补测试覆盖：
- registry 程序默认是 `Legacy`
- MSI 解析结果为 `Msi`
- Store 解析结果为 `Store`

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test parse_store_apps cargo test parse_msi_products`
Expected: 至少有关于 `uninstall_kind` 的断言失败

- [ ] **Step 3: 最小实现**

在三个枚举器里写入对应 `uninstall_kind`。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test parse_store_apps`
Run: `cargo test parse_msi_products`
Expected: PASS

## Chunk 3: 统一搜索与按类型分流

### Task 3: 为 `uninstall` 先写失败测试

**Files:**
- Modify: `src/commands/uninstall.rs`
- Test: `src/commands/uninstall.rs`

- [ ] **Step 1: 写失败测试**

增加单元测试覆盖：
- 搜索结果可命中 `Store` 程序
- 搜索结果可命中 `MSI` 程序
- 路由根据 `uninstall_kind` 返回不同处理路径标识

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test select_matching_program`
Expected: FAIL，原因是默认搜索与路由能力不足

- [ ] **Step 3: 最小实现准备**

提炼 `resolve_program_uninstall_kind()` 或等价纯函数，先让路由判断可测试。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test select_matching_program`
Expected: PASS

## Chunk 4: 三类卸载处理器

### Task 4: 拆分独立处理器模块

**Files:**
- Create: `src/modules/uninstall/mod.rs`
- Create: `src/modules/uninstall/legacy.rs`
- Create: `src/modules/uninstall/msi.rs`
- Create: `src/modules/uninstall/store.rs`
- Modify: `src/modules/mod.rs`
- Test: `src/modules/uninstall/store.rs`
- Test: `src/modules/uninstall/msi.rs`

- [ ] **Step 1: 写失败测试**

覆盖：
- Store 完成判定只看包存在性
- MSI 完成判定不会因为目录残留直接失败
- legacy 处理器仍优先静默字符串

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test store`
Run: `cargo test msi`
Expected: FAIL，提示模块或函数不存在

- [ ] **Step 3: 最小实现**

实现：
- `legacy` 处理器复用现有命令执行与等待逻辑
- `msi` 处理器统一规范命令与判定
- `store` 处理器提供专用命令构造与包存在性检查

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test store`
Run: `cargo test msi`
Expected: PASS

## Chunk 5: 接线与回归

### Task 5: 将 CLI 入口切到统一搜索 + 分流执行

**Files:**
- Modify: `src/commands/uninstall.rs`
- Modify: `src/modules/lister/mod.rs`（如需公开辅助函数）
- Modify: `src/modules/common/utils.rs`（仅在确有必要时）
- Test: `src/commands/uninstall.rs`

- [ ] **Step 1: 写失败测试**

增加回归测试，覆盖 `chatgpt` 这种 `Store` 搜索命中场景，以及 `wait_for_program_removal` 对 `Store` 的特殊分支。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test chatgpt`
Expected: FAIL

- [ ] **Step 3: 最小实现**

让 `find_and_save_program()` 使用聚合搜索源；让 `wait_for_program_removal()` 按 `uninstall_kind` 分流完成判定。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test uninstall`
Expected: PASS

## Chunk 6: 全量验证与交付

### Task 6: 运行格式化、测试、提交并推送

**Files:**
- Modify: `docs/superpowers/plans/2026-03-27-uninstall-routing.md`

- [ ] **Step 1: 运行格式化**

Run: `cargo fmt --all`
Expected: exit 0

- [ ] **Step 2: 运行测试**

Run: `cargo test`
Expected: 全部通过

- [ ] **Step 3: 运行目标回归验证**

Run: `cargo run -- list --source all --format json --search chatgpt`
Expected: 输出 `OpenAI.ChatGPT-Desktop`

- [ ] **Step 4: 提交**

```bash
git add src docs
git commit -m "Implement uninstall routing by program kind"
```

- [ ] **Step 5: 推送**

Run: `git push`

- [ ] **Step 6: 发送通知**

Run: `curl "https://api.day.app/LkBmavbbbYqtmjDLVvsbMR/<内容>"`
