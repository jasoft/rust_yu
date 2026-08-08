# 管理员 GUI 与同步卸载工作流实施计划

> **给实施代理：** 必须严格按顺序逐项执行。每个 Task 都先补测试或可验证的失败条件，再实现，再运行该 Task 的全部验证命令，最后使用约定式提交信息提交。不要把多个 Task 压成一个大提交，不要跳过 Windows 实机验证，不要在用户可写路径创建持久高权限计划任务。

**目标：** 将 Rust Yu 收敛为只发布 Tauri GUI 的管理员 Windows 应用，使用每用户 Task Scheduler 任务完成可信免重复 UAC 启动，并将卸载、等待、验证、残留扫描和确认清理统一为可测试的同步模态工作流。

**设计依据：** `docs/superpowers/specs/2026-08-09-admin-gui-architecture-design.md`

**技术栈：** Rust 2021、Tauri v2、React 19、TypeScript、Zustand、Tokio、windows-rs、Windows Task Scheduler COM、Job Objects、NSIS per-machine installer。

---

## 实施总则

- 所有文件、注册表和计划任务修改路径禁止使用 `unwrap()` / `expect()`。
- 所有 destructive operations 必须在后端重新校验管理员权限。
- 残留删除必须经过扫描快照、明确 trace ids、`confirm = true` 和执行前精确复核。
- Task Scheduler Action 永远不能接收前端输入或卸载业务参数。
- debug/worktree 永远不能注册指向用户可写路径的持久高权限任务。
- 同一时间只允许一个卸载 job。
- 不得持有同步锁跨越 `.await` 或第三方进程等待。
- 每完成一个 Task，先检查 `git diff --check`，再按计划提交。
- 遇到现有未提交改动时必须保留用户改动；若与本计划冲突，停止并请求用户确认。

## 建议提交序列

1. `fix: stop implicit elevated cli cleanup`
2. `refactor: add uninstall application workflow contracts`
3. `refactor: centralize uninstall workflow execution`
4. `feature: add tauri uninstall job coordinator`
5. `feature: implement modal uninstall residue review`
6. `feature: add secure scheduled task launcher`
7. `feature: bootstrap elevated gui startup`
8. `chore: enforce per-machine gui installation`
9. `chore: retire the yu command line product`
10. `fix: harden elevated tauri security boundaries`
11. `test: cover elevated launch and uninstall lifecycle`
12. `chore: publish the tauri gui installer`
13. `docs: document elevated gui operations`

---

## Chunk 0：执行前基线

### Task 0：确认 checkout、创建实施分支并建立基线

**Files:** 无产品文件修改

- [ ] **Step 1：确认真实 checkout 与工作树**

Run:

```powershell
git rev-parse --show-toplevel
git rev-parse --abbrev-ref HEAD
git rev-parse --short HEAD
git status --short
```

Expected:

- 根目录是准备运行和验证的 Rust Yu checkout；
- 明确当前 commit；
- 工作树干净，或已逐项识别并保留用户改动。

- [ ] **Step 2：若当前是 detached HEAD，创建分支**

Run:

```powershell
git switch -c codex/admin-gui-workflow
```

如果已有对应分支，切换到用户指定分支，不要覆盖现有分支。

- [ ] **Step 3：记录 Rust 基线**

Run:

```powershell
cargo test --workspace -- --test-threads=1
cargo fmt --all -- --check
```

Expected: PASS。若失败，保存准确失败信息并先判断是否为已有失败。

- [ ] **Step 4：记录 WebUI 基线**

Run:

```powershell
Set-Location src-tauri\src-frontends\webui
npm ci
npm run lint
npm run build
Set-Location ..\..\..
```

Expected: build PASS；lint 除仓库已知且明确记录的 warning 外无新增问题。

- [ ] **Step 5：保存基线说明**

把命令、commit、已知 warning 和现有失败写到实施日志或最终交付说明中。本 Task 不创建空提交。

---

## Chunk 1：先关闭危险的 CLI 提权分支

### Task 1：非管理员 GUI 不再隐式执行 `yu.exe --confirm --clean`

**Files:**

- Modify: `src-tauri/src/commands/uninstall.rs`
- Modify: `src-tauri/src/commands/error.rs`
- Test: `src-tauri/src/commands/uninstall.rs`
- Test: `src-tauri/src/commands/error.rs`

- [ ] **Step 1：写失败测试**

增加测试，固定以下语义：

- 权限不足映射为 code=`admin_required`；
- Tauri 卸载适配器不构造 `yu.exe` 路径；
- 不存在任何忽略 `clean_after`/`confirm` 并强制清理的参数生成函数；
- `CommandError` 保留领域错误 code。

- [ ] **Step 2：运行测试确认当前行为不满足约束**

Run:

```powershell
cargo test -p rust-yu-tauri uninstall -- --nocapture
cargo test -p rust-yu-tauri command_error -- --nocapture
```

Expected: 至少一个新测试 FAIL。

- [ ] **Step 3：删除危险 fallback**

删除：

- `run_elevated_cli_uninstall`；
- PowerShell `Start-Process` 提权逻辑；
- 相邻 `yu.exe` 假设；
- 硬编码 `--confirm --clean`；
- 不再需要的 `tokio::process::Command` import。

当前进程不是管理员时直接返回结构化 `admin_required`。计划任务启动尚未完成前，用户仍可通过“以管理员身份运行”使用 GUI。

- [ ] **Step 4：运行目标验证**

Run:

```powershell
cargo test -p rust-yu-tauri uninstall -- --nocapture
cargo check -p rust-yu-tauri --lib
cargo fmt --all -- --check
git diff --check
```

Expected: PASS。

- [ ] **Step 5：提交**

```powershell
git add src-tauri/src/commands/uninstall.rs src-tauri/src/commands/error.rs
git commit -m "fix: stop implicit elevated cli cleanup"
```

---

## Chunk 2：建立可测试的 application/workflow 层

### Task 2：定义卸载 job、状态机、错误和目标指纹

**Files:**

- Create: `src/application/mod.rs`
- Create: `src/application/uninstall/mod.rs`
- Create: `src/application/uninstall/models.rs`
- Create: `src/application/uninstall/error.rs`
- Create: `src/application/uninstall/state.rs`
- Create: `src/application/uninstall/fingerprint.rs`
- Create: `src/application/target.rs`
- Modify: `src/lib.rs`
- Modify: `src/commands/target.rs`（临时兼容转发，CLI 退役前保留）
- Test: 新增模块内单元测试

- [ ] **Step 1：写状态机失败测试**

覆盖：

- 合法状态转换；
- `RunningUninstaller -> AwaitingCleanupConfirmation` 不能跳过验证和扫描；
- 执行阶段不能取消；
- `Planned` 与 `AwaitingCleanupConfirmation` 可以取消/完成；
- 已完成 job 不能再次执行；
- job id 唯一；
- event sequence 单调递增。

- [ ] **Step 2：写目标指纹失败测试**

覆盖 registry/MSI/Store 三类程序：

- 相同快照得到相同指纹；
- uninstall kind、包 ID、产品代码、注册表路径、卸载命令或安装位置变化会改变指纹；
- 前端展示字段变化不应造成无意义指纹变化。

- [ ] **Step 3：定义稳定模型**

至少包含：

- `UninstallJobId`；
- `UninstallPhase`；
- `UninstallPlan`；
- `UninstallJobSnapshot`；
- `UninstallEvent { job_id, sequence, phase, payload }`；
- `ResidueReview`；
- `CleanupSelection`；
- `UninstallOutcome`；
- 设计文档列出的稳定错误 code。

所有需要跨 Tauri 边界的结构实现 `Serialize`/`Deserialize`；领域层错误实现 `thiserror::Error`，但用户消息与错误 code 分离。

- [ ] **Step 4：移动目标解析**

把仍被 GUI 使用的唯一目标解析从 `src/commands/target.rs` 移入 `src/application/target.rs`。CLI 文件暂时只做薄转发，避免在中间提交破坏构建。

目标解析必须支持按 `InstalledProgram.id` 精确定位；名称模糊匹配只保留给历史 CLI 兼容层，并在 CLI 退役时删除。

- [ ] **Step 5：运行测试**

Run:

```powershell
cargo test -p rust-yu --lib application::uninstall -- --nocapture
cargo test -p rust-yu --lib application::target -- --nocapture
cargo check --workspace
cargo fmt --all -- --check
git diff --check
```

Expected: PASS。

- [ ] **Step 6：提交**

```powershell
git add src/application src/lib.rs src/commands/target.rs
git commit -m "refactor: add uninstall application workflow contracts"
```

### Task 3：把卸载、等待、验证和扫描编排集中到 workflow

**Files:**

- Create: `src/application/uninstall/ports.rs`
- Create: `src/application/uninstall/workflow.rs`
- Create: `src/application/uninstall/production.rs`
- Modify: `src/application/uninstall/mod.rs`
- Modify: `src/modules/uninstall/*`（仅暴露现有安全原语所需接口）
- Modify: `src/modules/scanner/mod.rs`（仅在需要可注入边界时）
- Modify: `src/modules/cleaner/mod.rs`（仅在需要可注入边界时）
- Modify: `src/modules/lister/storage.rs`（快照/缓存接口）
- Test: `src/application/uninstall/workflow.rs`

- [ ] **Step 1：定义可替换的系统端口并写失败测试**

用 trait 隔离至少以下能力：

- 按 ID 解析当前程序；
- 保存卸载前快照；
- 检查管理员权限；
- 运行卸载器并等待完整进程链；
- 验证 legacy/MSI/Store 已移除；
- 扫描残留；
- 清理精确 traces；
- 失效缓存。

如果采用 async trait，添加单一、明确的 `async-trait` 依赖；不要用散落的 boxed future 增加维护成本。

使用 fake port 写测试，验证严格调用顺序：

```text
resolve -> snapshot -> precheck -> run -> verify -> scan
```

任何阶段失败都不能继续调用后续阶段。

- [ ] **Step 2：实现 `plan_uninstall`**

- 按 program id 精确定位；
- 生成快照和指纹；
- 不执行卸载或删除；
- 返回 `Planned` job 与用户确认信息。

- [ ] **Step 3：实现 `execute_uninstall`**

- 只接受 `Planned`；
- 再次解析目标并比较指纹；
- 检查管理员权限；
- 保存快照；
- 调用现有 Job Object 卸载执行；
- 处理取消、退出码、重启要求和超时；
- 按 uninstall kind 验证移除；
- 只有验证成功才扫描残留；
- 生成 `AwaitingCleanupConfirmation` 或 `Completed`。

- [ ] **Step 4：实现 `clean_uninstall_residues`**

- 要求 phase 正确且 `confirm = true`；
- 只接收 trace ids；
- 从 job 快照取回真实 Trace；
- 拒绝未知/重复/篡改 ID；
- 调用 cleaner 前重新检查存在性与安全边界；
- 返回逐项结果；
- 无选择时安全跳过并完成。

- [ ] **Step 5：实现 production port**

复用现有：

- `modules::common::process`；
- `modules::uninstall::{legacy,msi,store}`；
- `scanner::scan_all_traces`；
- `cleaner::clean_traces`；
- `lister::storage`。

不要在 production port 重新实现第二套卸载器命令解析或删除逻辑。

- [ ] **Step 6：验证失败短路和安全语义**

新增测试覆盖：

- 用户取消卸载后不扫描、不清理；
- 移除未确认时不扫描；
- scan 失败不会自动清理；
- `confirm=false` 永远不清理；
- trace id 不在计划中时不触碰系统；
- Store/MSI/legacy 调用正确验证分支；
- 扫描结果默认没有“自动全选”语义。

- [ ] **Step 7：运行验证**

Run:

```powershell
cargo test -p rust-yu --lib application::uninstall -- --nocapture
cargo test -p rust-yu --lib modules::uninstall -- --nocapture
cargo test -p rust-yu --lib modules::cleaner -- --nocapture
cargo fmt --all -- --check
git diff --check
```

Expected: PASS。

- [ ] **Step 8：提交**

```powershell
git add Cargo.toml Cargo.lock src/application src/modules
git commit -m "refactor: centralize uninstall workflow execution"
```

---

## Chunk 3：Tauri 单任务协调器与结构化事件

### Task 4：新增 uninstall coordinator 和薄 commands

**Files:**

- Create: `src-tauri/src/state/mod.rs`
- Create: `src-tauri/src/state/uninstall_jobs.rs`
- Rewrite: `src-tauri/src/commands/uninstall.rs`
- Modify: `src-tauri/src/commands/error.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/state/uninstall_jobs.rs`
- Test: `src-tauri/src/commands/uninstall.rs`

- [ ] **Step 1：写 coordinator 失败测试**

覆盖：

- 同时只能存在一个活动 job；
- 第二个计划返回 `job_conflict`；
- job id 不存在返回 `job_not_found`；
- 非法状态返回 `invalid_job_state`；
- 完成或取消后可以创建新 job；
- 获取 job 快照不改变状态。

- [ ] **Step 2：实现短锁协调器**

使用 `tauri::State` 管理 coordinator。状态锁只用于：

- 检查当前 job；
- 提交状态转换；
- 保存不可变结果快照。

严禁在持锁期间：

- `.await`；
- 启动或等待卸载器；
- 扫描目录；
- 删除文件/注册表。

- [ ] **Step 3：实现新 commands**

注册：

- `plan_uninstall`；
- `execute_uninstall`；
- `clean_uninstall_residues`；
- `finish_uninstall`；
- `get_uninstall_job`。

删除旧 command 内的业务编排，只调用 application workflow。

- [ ] **Step 4：实现事件映射**

事件名改为 `uninstall-job-progress`。每个 payload 必须携带：

- `job_id`；
- `sequence`；
- `phase`；
- 结构化 payload。

不要把完整卸载命令作为默认 UI 日志输出；仅输出经脱敏的 route、阶段和结果。

- [ ] **Step 5：错误映射**

`CommandError` 必须保留 application error code。前端不得通过匹配中文字符串判断错误类型。

- [ ] **Step 6：运行验证**

Run:

```powershell
cargo test -p rust-yu-tauri uninstall_jobs -- --nocapture
cargo test -p rust-yu-tauri commands::uninstall -- --nocapture
cargo check -p rust-yu-tauri --lib
cargo fmt --all -- --check
git diff --check
```

Expected: PASS。

- [ ] **Step 7：提交**

```powershell
git add src-tauri/src/state src-tauri/src/commands/uninstall.rs src-tauri/src/commands/error.rs src-tauri/src/lib.rs
git commit -m "feature: add tauri uninstall job coordinator"
```

---

## Chunk 4：真正的模态同步卸载体验

### Task 5：前端改为计划、执行、残留审查、确认清理四阶段

**Files:**

- Modify: `src-tauri/src-frontends/webui/src/types/index.ts`
- Rewrite: `src-tauri/src-frontends/webui/src/stores/programs.ts` 中卸载相关状态
- Rewrite: `src-tauri/src-frontends/webui/src/components/UninstallDialog.tsx`
- Modify: `src-tauri/src-frontends/webui/src/hooks/useTauriEvent.ts`（如需 job/sequence 过滤）
- Create: `src-tauri/src-frontends/webui/src/components/uninstall/uninstallState.ts`
- Create: `src-tauri/src-frontends/webui/src/components/uninstall/ResidueReview.tsx`
- Modify: `src-tauri/src-frontends/webui/package.json`
- Modify: `src-tauri/src-frontends/webui/package-lock.json`
- Test: `src-tauri/src-frontends/webui/src/components/uninstall/*.test.tsx`

- [ ] **Step 1：引入前端测试基线**

添加 Vitest、jsdom、React Testing Library 和必要脚本：

```json
"test": "vitest run"
```

先写 reducer/状态转换测试，确保测试确实 FAIL。

- [ ] **Step 2：实现前端卸载状态模型**

前端状态必须显式表示：

- `planning`；
- `awaiting_uninstall_confirmation`；
- `running_uninstaller`；
- `verifying_removal`；
- `scanning_residues`；
- `awaiting_cleanup_confirmation`；
- `cleaning`；
- `completed`；
- `failed`。

状态以后端 job snapshot 为准，前端不得自行跳阶段。

- [ ] **Step 3：按 program id 生成计划**

打开卸载界面时调用 `plan_uninstall(selectedProgram.id)`，展示后端返回的精确程序快照、卸载类型和提示。用户确认后才调用 `execute_uninstall(job_id)`。

- [ ] **Step 4：实现运行阶段模态约束**

在 `running_uninstaller`、`verifying_removal`、`scanning_residues`、`cleaning`：

- 禁用关闭、返回和第二次提交；
- 展示明确阶段和进度日志；
- 不伪造百分比；没有真实进度时使用不确定进度条；
- 保持 WebView 响应，不阻塞渲染线程。

- [ ] **Step 5：实现残留审查**

第三方卸载器结束且验证成功后才展示残留：

- 默认全部不选；
- 显示路径、类型、置信度、来源、大小和风险说明；
- 支持逐项选择；
- “全选”必须二次确认，且不能包含后端标记为不可自动选择的目标；
- 点击清理时只提交 trace ids 和 `confirm: true`；
- 用户可以“跳过清理并完成”。

- [ ] **Step 6：处理事件乱序和恢复**

- 忽略其他 job id 的事件；
- 忽略 sequence 小于等于已处理值的事件；
- WebView 重载或事件丢失时调用 `get_uninstall_job` 恢复；
- 完成后刷新程序列表并清除 coordinator job。

- [ ] **Step 7：测试核心交互**

覆盖：

- 未确认不执行卸载；
- 运行阶段不能关闭；
- 卸载完成前不显示残留；
- 残留默认不选；
- 未确认不清理；
- 跳过清理；
- 乱序事件被忽略；
- `target_changed`、`admin_required`、取消和失败显示正确操作。

- [ ] **Step 8：运行验证**

Run:

```powershell
Set-Location src-tauri\src-frontends\webui
npm run test
npm run lint
npm run build
Set-Location ..\..\..
cargo check -p rust-yu-tauri --lib
git diff --check
```

Expected: PASS。

- [ ] **Step 9：提交**

```powershell
git add src-tauri/src-frontends/webui
git commit -m "feature: implement modal uninstall residue review"
```

---

## Chunk 5：安全的 Task Scheduler 启动器

### Task 6：实现管理员令牌、受保护路径和计划任务原语

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `Cargo.lock`
- Create: `src-tauri/src/elevation/mod.rs`
- Create: `src-tauri/src/elevation/token.rs`
- Create: `src-tauri/src/elevation/install_path.rs`
- Create: `src-tauri/src/elevation/task_definition.rs`
- Create: `src-tauri/src/elevation/task_scheduler.rs`
- Create: `src-tauri/src/elevation/error.rs`
- Test: 同模块单元测试
- Create: `src-tauri/tests/task_scheduler_windows.rs`

- [ ] **Step 1：添加 windows-rs features**

在 Tauri crate 直接依赖与现有版本一致的 `windows`，至少启用实际实现需要的：

- `Win32_Foundation`；
- `Win32_Security`；
- `Win32_System_Com`；
- `Win32_System_TaskScheduler`；
- `Win32_System_Threading`；
- `Win32_UI_Shell`；
- `Win32_UI_WindowsAndMessaging`；
- 必要时的 `Win32_Storage_FileSystem`。

只添加真实使用的 feature。

- [ ] **Step 2：实现真实 elevation 检测**

使用进程 token 的 `TokenElevation`/等价信息区分：

- 当前进程是否完整提升；
- 当前登录身份是否属于 Administrators；
- 纯标准用户与 split-token 管理员。

不要只依赖前端或命令行参数声明已提权。

- [ ] **Step 3：实现受保护安装路径验证**

写失败测试覆盖拒绝：

- `%TEMP%`；
- `%LOCALAPPDATA%`；
- workspace/`target/debug`；
- UNC/network path；
- symlink/junction/reparse point；
- 当前普通用户拥有写权限的 EXE 或父目录。

release 任务注册仅接受受管理员 ACL 保护的本地绝对路径。

- [ ] **Step 4：实现纯 task definition builder**

builder 输出可比较的结构或 XML，包含设计文档规定的：

- per-user principal；
- InteractiveToken；
- Highest；
- fixed action/path/args；
- AllowDemandStart；
- IgnoreNew；
- battery 和 execution time 设置；
- 不自动触发；
- DACL 目标权限。

对 definition 做 snapshot/字段测试，不先操作真实系统。

- [ ] **Step 5：实现 Task Scheduler COM 封装**

提供：

- `inspect_current_user_task`；
- `validate_current_user_task`；
- `create_or_repair_current_user_task`；
- `run_current_user_task`；
- `remove_current_user_task`；
- `remove_all_product_tasks`。

要求：

- COM 初始化/释放封装成 RAII；
- 正确处理线程已有 COM apartment 的情况；
- 所有 HRESULT 映射成稳定错误；
- 创建/修复是可重复的；
- 只管理 `\Rust Yu` 产品 folder 下定义匹配的任务；
- 不删除同名但 action 不匹配的外部任务，改为报冲突。

- [ ] **Step 6：实现任务 ACL**

使用 SDDL 或 `SetSecurityDescriptor` 达成：

- 当前用户：read + execute；
- Administrators：管理；
- SYSTEM：管理；
- 当前用户无 write/delete/WriteDAC。

不要直接修改 `C:\Windows\System32\Tasks` 文件 ACL；只使用 Task Scheduler API。

- [ ] **Step 7：增加安全的 Windows 集成测试**

ignored/admin integration test：

- 使用唯一测试任务名；
- Action 指向受保护的 `%SystemRoot%\System32\whoami.exe` 或等价系统二进制；
- 创建、读取、验证、运行、删除；
- finally/Drop 路径保证清理；
- 验证错误定义会被识别；
- 不允许测试任务指向 workspace 中的测试 EXE。

- [ ] **Step 8：运行验证**

Run:

```powershell
cargo test -p rust-yu-tauri elevation -- --nocapture
cargo check -p rust-yu-tauri --lib
cargo fmt --all -- --check
git diff --check
```

管理员终端额外运行：

```powershell
cargo test -p rust-yu-tauri --test task_scheduler_windows -- --ignored --nocapture
```

Expected: PASS，测试结束后无 `\Rust Yu Tests` 遗留任务。

- [ ] **Step 9：提交**

```powershell
git add src-tauri/Cargo.toml Cargo.lock src-tauri/src/elevation src-tauri/tests/task_scheduler_windows.rs
git commit -m "feature: add secure scheduled task launcher"
```

### Task 7：接入首次 UAC、后续任务启动和单实例 bootstrap

**Files:**

- Create: `src-tauri/src/bootstrap.rs`
- Create: `src-tauri/src/single_instance.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/bootstrap.rs`

- [ ] **Step 1：写启动决策表测试**

用纯函数覆盖：

| Build/令牌/路径/任务 | 期望动作 |
|---|---|
| debug + 非管理员 | 直接开发启动，destructive commands 后端拒绝 |
| release + 已提升 | 验证/修复任务后启动 GUI |
| release + 非提升 + 有效任务 | 运行任务并退出 |
| release + 非提升 + 任务缺失/损坏 | `runas --elevated-entry --repair-launch-task` |
| release + 标准用户 | 显示 `unsupported_standard_user` |
| release + 用户可写安装路径 | 不创建任务；显示 `unsafe_install_location` |
| `--elevated-entry` 但令牌未提升 | 拒绝启动高权限 GUI |

- [ ] **Step 2：实现固定内部参数解析**

只接受设计文档列出的内部参数。未知参数显示错误并退出。不要重新引入 clap 全功能 CLI。

- [ ] **Step 3：实现 `ShellExecuteW("runas")` fallback**

- 文件路径固定为 `current_exe`；
- 参数固定；
- 正确区分用户取消 UAC 与启动失败；
- 不通过 PowerShell、cmd 或字符串拼接启动；
- 普通实例在接管成功或超时后退出。

- [ ] **Step 4：实现任务启动与 ready event**

- 调用任务时不传动态参数；
- 管理员实例创建按用户命名 ready event；
- 普通实例有限等待；
- 超时回退 UAC 修复，不无限循环；
- 日志记录阶段和错误 code。

- [ ] **Step 5：实现单实例**

- 管理员 GUI 按用户 SID 创建 mutex；
- 已有实例时激活主窗口；
- Task Scheduler `IgnoreNew` 是第二层保护；
- 不允许两个 coordinator 同时卸载。

- [ ] **Step 6：接入 Tauri 入口**

`src-tauri/src/main.rs` 在创建 WebView 前完成 bootstrap 决策。启动失败使用原生 Windows message box 展示中文错误，因为此时 React 尚不可用。

- [ ] **Step 7：运行验证**

Run:

```powershell
cargo test -p rust-yu-tauri bootstrap -- --nocapture
cargo test -p rust-yu-tauri single_instance -- --nocapture
cargo check -p rust-yu-tauri
cargo fmt --all -- --check
git diff --check
```

Expected: PASS。

- [ ] **Step 8：开发模式手工验证**

- 普通 PowerShell 启动 `cargo tauri dev` 不创建计划任务；
- destructive command 返回 `admin_required`；
- 管理员 PowerShell 启动 `cargo tauri dev` 可执行测试流程；
- Task Scheduler 中无指向 worktree/`target/debug` 的 Rust Yu highest task。

- [ ] **Step 9：提交**

```powershell
git add src-tauri/src/bootstrap.rs src-tauri/src/single_instance.rs src-tauri/src/main.rs src-tauri/src/lib.rs
git commit -m "feature: bootstrap elevated gui startup"
```

---

## Chunk 6：per-machine 安装和任务生命周期

### Task 8：将正式安装固定到 Program Files，并完整管理任务卸载

**Files:**

- Modify: `src-tauri/tauri.conf.json`
- Create: `src-tauri/windows/hooks.nsh`
- Modify: `src-tauri/src/bootstrap.rs`
- Modify: `src-tauri/src/elevation/task_scheduler.rs`
- Test: installer 配置/维护模式测试

- [ ] **Step 1：配置唯一正式安装器**

将正式 Windows bundle 收敛到 NSIS：

- `targets: ["nsis"]`；
- `bundle.windows.nsis.installMode: "perMachine"`；
- 配置 `installerHooks`；
- 保持系统 WebView2 更新模型；
- 安装路径必须落在 `Program Files`。

不再同时发布缺少同等任务生命周期处理的 MSI。

- [ ] **Step 2：实现任务维护模式**

`--remove-launch-tasks`：

- 必须验证当前进程已提升；
- 只删除 `\Rust Yu` 下定义属于当前产品的任务；
- 删除所有用户任务后尝试删除空产品 folder；
- 失败返回非零进程码并记录原因；
- 不启动 WebView。

- [ ] **Step 3：实现 NSIS hooks**

- pre-uninstall 在删除 EXE 前调用固定维护模式；
- 等待维护模式完成；
- 失败写入安装器日志并阻止留下危险任务；
- 更新/重装保持相同路径并允许管理员 GUI 在下一次启动修复任务；
- 卸载前处理正在运行的 Rust Yu 实例。

- [ ] **Step 4：增加配置验证脚本或测试**

验证：

- installMode 是 perMachine；
- targets 不包含不受支持的 installer；
- hooks 文件存在；
- hook 引用固定安装目录 EXE；
- bundle 不包含 `yu.exe` sidecar。

- [ ] **Step 5：构建安装器**

Run:

```powershell
Set-Location src-tauri
npx tauri build --bundles nsis
Set-Location ..
```

Expected: PASS，并生成 NSIS setup。

- [ ] **Step 6：安装生命周期实测**

在干净测试环境：

1. 安装到 Program Files；
2. 第一次启动出现一次 UAC；
3. 任务 action 指向 Program Files；
4. 第二次启动不出现 UAC；
5. 卸载；
6. EXE、快捷方式和 `\Rust Yu` 任务均不存在。

- [ ] **Step 7：提交**

```powershell
git add src-tauri/tauri.conf.json src-tauri/windows/hooks.nsh src-tauri/src/bootstrap.rs src-tauri/src/elevation/task_scheduler.rs
git commit -m "chore: enforce per-machine gui installation"
```

---

## Chunk 7：退役产品 CLI 与迁移测试入口

### Task 9：删除 `yu.exe` 产品入口，但保留核心引擎可测试性

**Files:**

- Delete: `src/main.rs`
- Delete: `src/commands/*.rs`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/lib.rs`
- Modify: `src/application/target.rs`
- Modify: `src-tauri/src/commands/*` 中残留的 `rust_yu_lib::commands` 引用
- Delete: `docs/scoop-self-bucket.md`
- Delete: `tools/scoop/update-manifest.ps1`
- Delete: `bucket/yu.json`
- Modify: 旧 CLI 计划/规格文档，增加“历史、已退役”标记
- Modify: `.resources/inno-legacy-test/README.md`
- Modify: `tools/test/Verify-InnoLegacyFixture.ps1`
- Create: `tests/windows_uninstall_lifecycle.rs`

- [ ] **Step 1：列出所有 CLI 依赖和引用**

Run:

```powershell
rg -n "yu\.exe|rust-yu\.exe|cargo run --|rust_yu_lib::commands|crate::commands|clap|unicode_width" . `
  -g "!target/**" -g "!node_modules/**" -g "!legacy-delphi/**"
```

逐项分类为产品代码、发布代码、测试资源或历史文档。

- [ ] **Step 2：先迁移 Inno 生命周期测试**

新增 ignored Windows integration test，直接调用 application workflow production adapter，覆盖：

- 安装 Inno fixture；
- 唯一定位目标；
- 运行 spawn-wrapper 卸载链；
- 命中 Job Object 等待路径；
- 验证卸载结束；
- 验证安装目录和 AppData 残留进入 ResidueReview；
- 默认不清理；
- finally 路径卸载/清除 fixture 状态。

PowerShell fixture 验证脚本改为调用该 integration test，不再启动 `yu.exe`。

- [ ] **Step 3：删除 CLI crate 入口**

- 删除 `[[bin]] name="yu"`；
- 删除 `src/main.rs` 与 `src/commands/`；
- `src/lib.rs` 不再公开 commands；
- 所有 GUI 共享逻辑必须已迁入 application/modules；
- 更新根 package description 为 Rust Yu core engine。

- [ ] **Step 4：清理依赖**

确认无引用后移除：

- `clap`；
- `unicode-width`；
- 其他仅 CLI 使用的依赖。

运行 `cargo tree` 和 `rg`，不要删除 reporter/application 仍需要的依赖。

- [ ] **Step 5：删除不安全的 Scoop 分发路径**

Scoop 默认用户可写安装目录，不满足高权限任务目标约束，因此删除当前 CLI bucket、更新脚本和文档。历史 CLI 计划保留，但顶部明确标注已由本设计退役，避免未来代理按旧计划恢复 CLI。

- [ ] **Step 6：运行验证**

Run:

```powershell
cargo test --workspace -- --test-threads=1
cargo check --workspace
cargo fmt --all -- --check
rg -n "yu\.exe|rust_yu_lib::commands|crate::commands" src src-tauri tools .resources `
  -g "!target/**" -g "!node_modules/**"
git diff --check
```

Expected:

- Rust 测试 PASS；
- 产品/发布/测试执行路径中不再依赖 `yu.exe`；
- 允许历史文档出现带“已退役”上下文的文本。

- [ ] **Step 7：提交**

```powershell
git add -A Cargo.toml Cargo.lock src src-tauri/src tools/test .resources/inno-legacy-test docs bucket tools/scoop tests
git commit -m "chore: retire the yu command line product"
```

---

## Chunk 8：高权限 WebView 安全加固

### Task 10：收紧 CSP、capabilities 与 destructive command 边界

**Files:**

- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/*.rs`
- Modify: `src-tauri/src-frontends/webui/package.json`
- Modify: `src-tauri/src-frontends/webui/package-lock.json`
- Modify: `src-tauri/src-frontends/webui/src/App.tsx`
- Modify: `src-tauri/src-frontends/webui/src/**/*.css`（按需）
- Test: command 权限和 CSP 配置测试

- [ ] **Step 1：删除未使用 shell 权限**

- 删除后端 `tauri-plugin-shell`；
- 删除前端 `@tauri-apps/plugin-shell`；
- 删除 capability 中 `shell:allow-open`；
- 更新 lockfiles；
- `rg` 确认没有残留引用。

- [ ] **Step 2：建立自定义 command allowlist**

在 `build.rs` 使用 Tauri AppManifest 声明所有公开 commands，并在 capability 中只允许主窗口所需命令。新增 command 必须同步修改 manifest/capability，否则构建或测试失败。

- [ ] **Step 3：收紧生产 CSP**

生产 CSP 至少满足：

```text
default-src 'self'
connect-src ipc: http://ipc.localhost
img-src 'self' asset: http://asset.localhost data:
style-src 'self'
script-src 'self'
object-src 'none'
base-uri 'none'
frame-ancestors 'none'
```

如果开发 Vite 确实需要更宽策略，只在 Tauri 支持的 dev CSP 配置中限制性开放，不能污染 production CSP。

- [ ] **Step 4：移除内联执行依赖**

重构当前动态 progress ring 的 inline style，使用 SVG 属性、受控 class 或其他不要求 `unsafe-inline` 的方案。全局搜索并移除：

- `dangerouslySetInnerHTML`；
- `eval` / `new Function`；
- 不必要的 inline style/script。

- [ ] **Step 5：审计所有 destructive commands**

至少检查：

- `clean_traces`；
- `clean_cleaner_entries`；
- `clean_browser_data`；
- `apply_startup_action`；
- `rollback_startup_action`；
- `delete_report`；
- 新卸载 execute/clean commands。

每条路径必须：

- 后端管理员校验；
- 输入 DTO 校验；
- 计划/确认语义；
- 目标复核；
- 结构化错误；
- 不允许任意命令或任意路径执行。

- [ ] **Step 6：增加配置测试**

自动断言：

- production CSP 不包含 `unsafe-eval`/`unsafe-inline`；
- shell plugin/package/permission 不存在；
- capability 只绑定 `main`；
- command manifest 与 invoke handler 集合一致。

- [ ] **Step 7：运行验证**

Run:

```powershell
Set-Location src-tauri\src-frontends\webui
npm ci
npm run test
npm run lint
npm run build
Set-Location ..\..\..
cargo test -p rust-yu-tauri --lib
cargo check -p rust-yu-tauri --lib
cargo fmt --all -- --check
git diff --check
```

管理员 Tauri runtime 验证：页面加载正常、控制台无 CSP violation、程序列表/图标/卸载模态均工作。

- [ ] **Step 8：提交**

```powershell
git add src-tauri
git commit -m "fix: harden elevated tauri security boundaries"
```

---

## Chunk 9：Windows 生命周期与发布迁移

### Task 11：补齐提升启动、安装器和三类卸载的 Windows 验证

**Files:**

- Create: `tools/test/Verify-ElevatedGuiLifecycle.ps1`
- Modify: `tools/test/Verify-InnoLegacyFixture.ps1`
- Modify: `.resources/inno-legacy-test/README.md`
- Modify: `tests/windows_uninstall_lifecycle.rs`
- Create/Modify: 其他 `tests/windows_*` 集成测试

- [ ] **Step 1：实现计划任务生命周期验证脚本**

脚本必须验证：

- 安装目录在 Program Files；
- 首次启动任务创建；
- principal 是当前用户 SID；
- InteractiveToken + Highest；
- action path/args/working directory 完全匹配；
- 当前普通用户可运行任务；
- 当前普通用户不能修改或删除任务；
- 第二次启动没有新 UAC；
- 损坏定义会被拒绝并由 UAC 修复；
- 卸载后任务不存在。

脚本必须使用唯一日志目录，不覆盖用户文件，并在失败时输出诊断信息。

- [ ] **Step 2：运行传统程序验证**

- 安装 7-Zip；
- 从 GUI 精确选择；
- 执行静默或交互卸载；
- 等待结束；
- 验证程序条目消失；
- 审查残留；
- 不经选择不删除。

- [ ] **Step 3：运行 MSI 验证**

- 安装 `.resources\Xplorer_0.3.1_x64.msi`；
- GUI 走 MSI 路由；
- 验证 MSI 条目消失；
- 目录残留不被错误视为卸载失败；
- 退出码和重启语义正确。

- [ ] **Step 4：运行 Inno Legacy Job Object 验证**

Run:

```powershell
.\tools\test\Verify-InnoLegacyFixture.ps1 -RunLifecycle
```

Expected:

- spawn wrapper 被命中；
- Rust Yu 等待完整进程链；
- 卸载完成后再扫描；
- `leftover.log` 和 AppData JSON 出现在 ResidueReview；
- 未确认时两个文件仍存在。

- [ ] **Step 5：运行 Store 验证**

使用明确的测试 Store/MSIX 包，验证：

- Store 路由；
- 完成判定只看包身份；
- 不扫描/删除 WindowsApps 系统目录；
- 用户数据残留策略保持保守。

- [ ] **Step 6：提交**

```powershell
git add tools/test .resources/inno-legacy-test tests
git commit -m "test: cover elevated launch and uninstall lifecycle"
```

### Task 12：发布脚本只构建和上传 GUI NSIS 安装器

**Files:**

- Rewrite: `tools/release/publish-release.ps1` 中构建与资产逻辑
- Modify: `tools/release/Test-Publish-Release.ps1`
- Delete/Modify: CLI/Scoop 专属发布逻辑
- Modify: version consistency helpers
- Modify: 发布文档（如存在）

- [ ] **Step 1：先更新发布脚本测试**

测试应期待：

- 版本只从根 Cargo、Tauri Cargo、Tauri config 校验，不再解析 `src/main.rs` CLI version；
- 资产名是 GUI NSIS setup；
- 构建命令是 Tauri NSIS build；
- 不生成 zip 内的 `yu.exe`；
- 不更新 Scoop manifest；
- release asset 路径真实存在；
- dry-run 不执行破坏性操作。

- [ ] **Step 2：运行测试确认失败**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\release\Test-Publish-Release.ps1
```

Expected: 新断言在旧脚本上 FAIL。

- [ ] **Step 3：实现 GUI 发布逻辑**

- 调用 `npx tauri build --bundles nsis` 或仓库统一封装；
- 定位唯一 NSIS setup；
- 使用版本化 GUI 资产名；
- 计算 hash；
- 上传 installer 和必要 checksum；
- 保持 tag、GitHub release、版本 bump 和 clean-tree 安全检查；
- 删除 CLI zip、Scoop 更新和 `CliVersion` 逻辑。

- [ ] **Step 4：运行发布脚本测试**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\release\Test-Publish-Release.ps1
```

Expected: PASS。

- [ ] **Step 5：运行 dry-run**

使用脚本已有 `-DryRun`/`-SkipBuild` 等安全参数验证完整发布路径，不创建真实 tag/release。

- [ ] **Step 6：提交**

```powershell
git add tools/release docs
git commit -m "chore: publish the tauri gui installer"
```

---

## Chunk 10：最终文档与发布门禁

### Task 13：补齐操作文档并完成全量验收

**Files:**

- Create: `docs/elevated-gui-operations.md`
- Modify: 主项目说明文档
- Modify: `docs/testing-summary-*.md` 或新增本次测试报告
- Modify: 本计划，勾选已完成项并记录偏差

- [ ] **Step 1：编写运维说明**

覆盖：

- 为什么使用 per-user Task Scheduler；
- 首次 UAC 和后续启动流程；
- Task folder/name/principal/action/settings；
- 如何检查和修复任务；
- 如何在管理员终端运行开发版；
- 如何证明 debug worktree 没有注册任务；
- 如何卸载并确认无遗留任务；
- 常见错误 code 与用户处理方式；
- 标准用户不支持边界。

- [ ] **Step 2：运行全量 Rust 验证**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace
```

Expected: 全部 PASS。不得以“现有 warning”为理由跳过最终发布门禁；需要修复仓库范围内阻断发布的 warning 或在获得用户明确批准后单独处理。

- [ ] **Step 3：运行全量前端验证**

Run:

```powershell
Set-Location src-tauri\src-frontends\webui
npm ci
npm run test
npm run lint
npm run build
Set-Location ..\..\..
```

Expected: 全部 PASS。

- [ ] **Step 4：运行安装器验证**

Run:

```powershell
Set-Location src-tauri
npx tauri build --bundles nsis
Set-Location ..
```

Expected: PASS；产物只有受支持的 GUI NSIS installer，不包含 `yu.exe`。

- [ ] **Step 5：运行静态安全检查**

Run:

```powershell
rg -n "unsafe-eval|script-src.*unsafe-inline|yu\.exe|tauri_plugin_shell|@tauri-apps/plugin-shell" . `
  -g "!target/**" -g "!node_modules/**" -g "!legacy-delphi/**"
rg -n "unwrap\(|expect\(" src src-tauri/src -g "*.rs"
git diff --check
git status --short
```

Expected:

- 第一条只允许带明确“历史/已退役”说明的文档命中；
- destructive paths 没有 `unwrap`/`expect`；测试代码中的使用逐项确认不在系统修改路径；
- 工作树只包含本 Task 预期文档改动。

- [ ] **Step 6：执行最终 Windows 验收矩阵**

| 场景 | 必须结果 |
|---|---|
| 首次安装/启动 | Program Files + 一次 UAC + 创建安全任务 |
| 第二次启动 | 无 UAC、管理员 GUI、单实例 |
| 任务缺失 | 一次 UAC 修复 |
| 任务被篡改 | 拒绝运行并修复，不执行篡改 action |
| 标准用户 | 明确不支持，不切换账户静默运行 |
| debug worktree | 不创建持久任务 |
| 7-Zip | 等待卸载器并验证移除 |
| MSI | 正确 MSI 完成判定 |
| Store | 正确包完成判定，不触碰 WindowsApps |
| Inno Legacy | 等待完整子进程链，之后扫描残留 |
| 残留清理 | 默认不选，只删已确认且复核通过项 |
| 产品卸载 | 删除 EXE、快捷方式和所有产品任务 |

- [ ] **Step 7：更新实施计划状态和测试报告**

勾选所有完成项，记录：

- 实际文件和架构偏差；
- 运行过的命令；
- Windows fixture 结果；
- 安装器产物路径；
- 任何用户批准的例外。

- [ ] **Step 8：提交文档**

```powershell
git add docs
git commit -m "docs: document elevated gui operations"
```

- [ ] **Step 9：最终工作树确认**

Run:

```powershell
git status --short
git log --oneline --decorate -15
```

Expected: 工作树干净，提交序列与本计划一致。

---

## 完成定义

只有同时满足以下条件，才能将本计划标记完成：

- Rust Yu 正式发行物只有安装在 Program Files 的 GUI；
- `yu.exe` 不再构建、测试、打包或发布；
- 每用户第一次启动通过一次 UAC 建立受保护任务；
- 后续启动不重复弹 UAC；
- 普通权限用户不能修改或删除该任务；
- debug/worktree 不创建持久高权限任务；
- 卸载工作流只有 application 层一套编排；
- Tauri commands 是薄适配器；
- GUI 使用单任务模态状态机；
- 第三方卸载器结束并验证移除后才扫描残留；
- 残留清理需要逐项选择、明确确认和后端复核；
- 高权限 WebView 使用严格 CSP 和最小 capability；
- 安装、更新、卸载、任务修复、7-Zip、MSI、Store、Inno fixture 均验证通过；
- 全量 Rust、前端、安装器和发布脚本验证通过；
- 最终 Git 工作树干净且每个 Task 有独立约定式提交。

## 本轮实施状态（2026-08-09）

代码与文档已按 Task 0–13 的架构顺序落地，并保留独立约定式提交。核心 Rust workflow、Tauri coordinator、管理员启动器、per-machine NSIS 配置、CLI 退役、WebView 安全边界和 GUI 发布脚本均已完成；对应命令与结果记录在 `docs/testing-summary-2026-08-09-admin-gui.md`。

本机为 Windows 11 ARM 环境，Tauri 的 `tauri-winres` 在生成原生资源时被 `windres: Can't detect target endianness and architecture` 阻断，因此 NSIS bundle、Task Scheduler COM 集成和 7-Zip/MSI/Store/Inno 实机矩阵必须在可用的 Windows GNU/LLVM 构建环境中补跑。该环境限制不改变已提交的安全设计，也不应被记录为“已通过”。
