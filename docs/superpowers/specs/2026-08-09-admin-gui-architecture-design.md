# 管理员 GUI 与同步卸载工作流架构设计

**状态：** 已批准

**日期：** 2026-08-09

**适用产品：** Rust Yu Windows/Tauri GUI

**对应实施计划：** `docs/superpowers/plans/2026-08-09-admin-gui-implementation.md`

## 1. 决策摘要

Rust Yu 定位为 GUI 产品，不再把 `yu.exe` 作为用户可见、需要长期兼容的产品入口。

最终架构采用：

- 只发布 Tauri GUI；
- GUI 进程整体运行在管理员完整令牌下；
- 首次按 Windows 用户启动时通过一次 UAC 创建该用户专属的高权限计划任务；
- 后续由普通权限启动阶段验证并按需运行该任务，避免每次启动重复弹出 UAC；
- Task Scheduler 只负责启动固定路径的 Rust Yu GUI，不承载卸载、扫描、清理等业务参数；
- 卸载流程集中到可测试的 Rust application/workflow 层；
- Tauri command 只负责 DTO 转换、状态协调和事件转发；
- 程序自带卸载器必须执行完并完成移除验证，之后才允许扫描残留；
- 残留清理继续遵守“扫描、展示、明确选择、精确复核、执行”的安全链路。

## 2. 已确认的产品约束

1. Rust Yu 是 GUI 产品，CLI 不是正式产品能力。
2. 不要求 GUI 功能与 CLI 功能保持对齐。
3. GUI 应以管理员权限运行。
4. 卸载是前台同步流程，同一时间只允许一个卸载任务。
5. 卸载期间使用模态界面，程序自带卸载器返回前不能进入残留扫描。
6. 接受安装到 `Program Files`。
7. 接受每个 Windows 用户第一次启动时弹出一次 UAC，用于建立该用户的可信启动任务。

## 3. 当前仓库的问题

### 3.1 已经正确的部分

列表能力已经采用共享引擎：

- CLI 的 `src/commands/list.rs` 调用 `rust_yu_lib::lister`；
- Tauri 的 `src-tauri/src/commands/list.rs` 也调用同一个 `lister`；
- Tauri 使用 `spawn_blocking` 承载注册表、MSI、Store 等阻塞扫描。

这证明核心能力与入口适配器分离是可行的。

### 3.2 必须修复的部分

当前卸载流程在 CLI 与 Tauri 中分别编排，导致行为漂移：

- `src/commands/uninstall.rs` 拥有一套完整流程；
- `src-tauri/src/commands/uninstall.rs` 又拥有一套完整流程；
- 非管理员 Tauri 路径会查找相邻的 `yu.exe`；
- 该路径硬编码 `--confirm --clean`，忽略 GUI 原始的 `confirm` 和 `clean_after`；
- Tauri bundle 没有声明或保证安装 `yu.exe`；
- GUI 当前在首次确认后直接传入 `clean_after: true, confirm: true`，没有在扫描后逐项确认残留。

此外，当前发布和安全配置仍以旧结构为基础：

- 发布脚本主要打包 `yu.exe`；
- Scoop 文档与脚本只面向 CLI；
- Tauri CSP 允许 `script-src 'unsafe-inline' 'unsafe-eval'`；
- shell plugin 已注册，但前端没有实际使用；
- 自定义 Tauri commands 尚未建立显式 capability allowlist；
- 安装器尚未强制 per-machine/`Program Files` 安装。

## 4. 目标架构

```text
普通权限启动阶段
  -> 检查当前进程和安装位置
  -> 验证当前用户的 Rust Yu 计划任务
  -> 运行计划任务并退出
  -> 管理员权限 Rust Yu GUI

首次启动或任务失效
  -> ShellExecuteW("runas")，仅传固定内部启动参数
  -> 管理员实例创建或修复当前用户任务
  -> 管理员权限 Rust Yu GUI

React WebUI
  -> 薄 Tauri commands
  -> UninstallCoordinator（单任务状态与并发保护）
  -> rust_yu_lib::application::uninstall（业务工作流）
  -> lister / uninstall / scanner / cleaner / storage
```

### 4.1 分层职责

#### Core modules

现有 `lister`、`uninstall`、`scanner`、`cleaner`、`storage` 等模块继续负责具体系统能力，不感知 React、Tauri window 或终端输出。

#### Application/workflow

新增 `src/application/`，负责：

- 目标精确解析；
- 卸载计划生成；
- 工作流状态转换；
- 卸载器启动与等待；
- 实际移除验证；
- 卸载后残留扫描；
- 清理计划的目标 ID 校验；
- 进度事件生成；
- 统一错误代码和最终结果。

Application 层不能直接输出到 stdout，也不能调用 Tauri `emit`。

#### Tauri adapter

`src-tauri/src/commands/` 只负责：

- 反序列化请求 DTO；
- 调用 coordinator/application；
- 将领域错误映射成 `CommandError`；
- 将领域事件映射成 Tauri event；
- 返回稳定 DTO。

#### React GUI

前端只负责：

- 展示卸载计划；
- 初次确认是否执行卸载；
- 展示不能关闭的运行阶段；
- 展示扫描出的残留；
- 让用户明确选择要清理的残留；
- 展示最终结果和可恢复错误。

## 5. 管理员启动设计

### 5.1 支持边界

本设计面向“当前登录用户属于本地 Administrators 组、当前进程使用拆分的普通令牌启动”的 Windows 账户。

纯标准用户使用其他管理员凭据进行 over-the-shoulder UAC 时会切换用户身份、配置目录和 Task Scheduler principal，不属于本阶段支持范围。此场景必须显示明确错误，不能静默为另一个账户创建任务。

### 5.2 安装位置约束

只有同时满足以下条件，才允许创建持久高权限启动任务：

- release build；
- EXE 位于 `Program Files` 或经过等价管理员 ACL 保护的位置；
- EXE 是普通文件，不是符号链接、junction 重定向或网络路径；
- EXE 和父目录不能被当前普通用户写入；
- 任务 Action 的规范化绝对路径与当前 EXE 完全一致。

debug build、工作树、`target/debug`、`%LOCALAPPDATA%`、`%TEMP%` 等用户可写路径严禁注册持久的 highest-privilege 任务。

### 5.3 内部启动模式

GUI EXE 可以保留少量内部维护参数，但它们不是公开 CLI：

- `--elevated-entry`：声明此实例应已通过可信高权限入口启动；
- `--repair-launch-task`：管理员实例为当前用户创建或修复启动任务；
- `--remove-launch-tasks`：安装器卸载阶段删除 Rust Yu 创建的任务。

这些参数不能接收程序名、卸载命令、文件路径、注册表路径或清理目标。

### 5.4 首次启动

1. 普通权限 release 实例发现任务缺失或验证失败。
2. 检查当前用户属于 Administrators 组。
3. 使用 `ShellExecuteW` 和 `runas` 启动当前 EXE：
   `Rust Yu.exe --elevated-entry --repair-launch-task`。
4. 普通实例等待启动接管信号后退出。
5. 管理员实例再次验证自身令牌和安装路径。
6. 管理员实例为当前用户 SID 创建任务。
7. 管理员实例启动 Tauri GUI。

### 5.5 后续启动

1. 普通权限实例读取任务定义和 ACL。
2. 校验任务 principal、run level、action path、固定参数、working directory 和 settings。
3. 校验成功后调用 `IRegisteredTask::Run`，不传任何运行参数。
4. 普通实例等待管理员实例就绪信号并退出。
5. 若任务无法运行、定义不一致或超时，回退到一次 `runas` 并修复任务。

### 5.6 计划任务定义

任务必须满足：

- 专用 folder：`\Rust Yu`；
- 每个用户 SID 一个任务，名称稳定且不会泄露原始用户名；
- principal user id：当前用户 SID；
- logon type：`TASK_LOGON_INTERACTIVE_TOKEN`；
- run level：`TASK_RUNLEVEL_HIGHEST`；
- action path：受保护安装目录中的当前 GUI EXE；
- arguments：固定 `--elevated-entry`；
- working directory：EXE 所在目录；
- `AllowDemandStart = true`；
- 不自动随登录或时间触发；若 API/Schema 要求 trigger，则创建 disabled trigger 并用集成测试证明不会自动运行；
- `MultipleInstances = IgnoreNew`；
- 不因电池供电拒绝启动；
- GUI 任务不设置短执行超时；
- 任务 DACL：当前用户只具备读取和执行，Administrators 与 SYSTEM 具备管理权限；
- 普通权限当前用户修改、删除任务必须返回拒绝访问。

### 5.7 单实例与启动接管

- 管理员 GUI 使用按用户命名的 Windows mutex 保证单实例；
- Task Scheduler 同时使用 `IgnoreNew` 作为第二层保护；
- 普通启动阶段不持有 GUI mutex；
- 管理员 GUI 创建命名 ready event，普通启动阶段最多等待有限时间；
- ready event 只用于用户体验同步，不作为安全判断依据；
- 如果已有实例存在，应激活主窗口而不是启动第二个卸载任务。

### 5.8 开发模式

- debug build 不自动创建计划任务；
- 普通权限开发运行允许浏览和扫描只读数据；
- destructive commands 继续返回 `admin_required`；
- 需要测试真实卸载时，从管理员终端运行 `cargo tauri dev`；
- 不提供能在 release 中绕过管理员校验的环境变量或前端开关。

## 6. 同步卸载工作流

### 6.1 状态机

```text
Idle
  -> Planned
  -> Prechecking
  -> RunningUninstaller
  -> VerifyingRemoval
  -> ScanningResidues
  -> AwaitingCleanupConfirmation
  -> Cleaning
  -> Completed

任意执行阶段 -> Failed
Planned / AwaitingCleanupConfirmation -> Cancelled
```

状态转换必须集中实现并测试，不能由 React 随意推断。

### 6.2 Tauri command 契约

最终命令集合：

1. `plan_uninstall(program_id)`
   - 只读；
   - 按稳定 ID 精确定位程序，不再只传名称；
   - 返回目标快照、卸载类型、风险提示和 job id。

2. `execute_uninstall(job_id)`
   - 只接受仍处于 `Planned` 的任务；
   - 执行前重新验证目标指纹；
   - 启动并等待程序自带卸载器；
   - 验证程序确实移除；
   - 然后扫描残留；
   - 返回残留审查清单或直接完成。

3. `clean_uninstall_residues(job_id, trace_ids, confirm)`
   - 必须 `confirm = true`；
   - 只接受 `AwaitingCleanupConfirmation`；
   - trace id 必须来自当前 job 的扫描快照；
   - 执行前重新检查存在性、类型、规范化路径和关键路径保护；
   - 未选择任何项等价于跳过，不自动全选。

4. `finish_uninstall(job_id)`
   - 允许用户在残留审查阶段跳过清理；
   - 清理 coordinator 中已结束任务；
   - 失效程序列表与相关缓存。

5. `get_uninstall_job(job_id)`
   - 用于 WebView 重载或事件遗漏后的状态恢复；
   - 不允许修改任务。

旧 `uninstall_program` 在迁移完成后删除。

### 6.3 Job 数据与一致性

每个 job 至少保存：

- 随机 job id；
- 单调递增事件 sequence；
- 当前 phase；
- 程序稳定 ID；
- 卸载前 `InstalledProgram` 快照；
- 目标指纹；
- 卸载路由；
- 开始与更新时间；
- 卸载执行结果；
- 移除验证结果；
- 扫描出的残留快照；
- 用户选择的 trace ids；
- 清理结果；
- 失败错误代码和用户可读消息。

目标指纹至少覆盖：

- program id；
- uninstall kind；
- registry key path / MSI product code / Store package identity；
- 规范化卸载命令摘要；
- 安装位置。

执行前发现指纹变化时返回 `target_changed`，要求用户重新生成计划。

### 6.4 并发规则

- 同一进程最多一个未结束卸载 job；
- coordinator 检测到活动 job 时返回 `job_conflict`；
- 不允许持有同步 mutex 跨越 `.await`、外部卸载器等待或文件扫描；
- 状态转换时短暂加锁，实际执行使用不可变快照，结束后再提交结果；
- event 必须携带 job id 和 sequence，前端忽略旧 job 或乱序事件。

### 6.5 取消规则

- `Planned` 可以安全取消；
- `RunningUninstaller` 不从 Rust Yu 强杀第三方卸载器；
- 用户可以在第三方卸载器自己的 UI 中取消；
- Job Object 继续负责等待进程链和识别中断；
- `AwaitingCleanupConfirmation` 可以跳过清理并完成；
- 模态窗口在运行卸载器、验证和清理期间禁止关闭。

## 7. 残留清理安全模型

- 扫描结果默认不勾选；
- UI 显示 trace 类型、路径、置信度、来源和风险说明；
- 低置信度目标永不自动选择；
- 用户必须明确选择具体 trace ids；
- 后端拒绝不属于当前 job 的 trace id；
- 后端根据扫描快照重建真实目标，不信任前端回传的路径；
- 删除前执行精确目标复核和关键路径保护；
- 注册表和文件删除路径禁止 `unwrap()`/`expect()`；
- 清理结果逐项返回成功、失败、跳过及原因；
- dry-run/preview 能力保留在核心层并可独立测试。

## 8. Tauri 高权限安全加固

由于 WebView 与 Rust 后端处于管理员完整令牌，以下项目是发布阻断项：

1. 移除生产 CSP 中的 `script-src 'unsafe-inline'` 和 `'unsafe-eval'`。
2. 增加 `object-src 'none'`、`base-uri 'none'`、`frame-ancestors 'none'`。
3. 移除未使用的 `tauri-plugin-shell` 和前端 shell package。
4. 在 `build.rs` 声明自定义 command manifest。
5. capability 文件只允许主窗口调用实际需要的 commands。
6. 所有 destructive commands 都做后端权限、状态和目标复核。
7. 不加载远程脚本、远程 HTML 或任意本地文件。
8. 图标 asset protocol 继续限制在图标缓存目录。
9. 生产日志不得记录完整敏感命令、用户令牌或任意计划任务凭据。
10. 修复内联 style 依赖，使严格 CSP 在生产构建中可用。

## 9. CLI 退役

最终发布不再生成 `yu.exe`：

- 删除根 `[[bin]]` 和 `src/main.rs`；
- 删除面向 clap 的 `src/commands/`；
- 将仍被 GUI 使用的目标解析移动到 application/lister 层；
- 移除不再使用的 clap、unicode-width 等依赖；
- 删除 Scoop CLI 发布路径；
- 发布脚本改为构建和上传 Tauri NSIS GUI 安装器；
- CLI 历史设计文档保留为历史资料，但明确标注已退役；
- 生命周期测试改为直接调用 application workflow，不依赖产品 CLI。

内部 `--elevated-entry` 等固定维护参数不构成公开 CLI，也不允许调用业务操作。

## 10. 安装、更新与卸载生命周期

### 安装

- Windows 正式发布以 NSIS per-machine 安装器为唯一受支持入口；
- 安装目录位于 `Program Files`；
- 安装器本身需要管理员权限；
- 首次用户启动时再创建该用户专属任务，而不是安装时猜测交互用户。

### 更新

- 更新保持同一受保护安装路径；
- 每次管理员 GUI 启动都轻量验证任务定义；
- 版本升级后如固定定义变化，由管理员实例原子修复任务；
- 修复失败时显示错误，但不放宽任务校验。

### 卸载

- NSIS pre-uninstall hook 在删除 EXE 前调用固定维护模式删除 `\Rust Yu` 下的产品任务；
- 卸载只删除 Rust Yu 自己创建且定义匹配的任务；
- 删除失败必须在安装器日志中明确记录；
- 正在运行的 GUI 必须先关闭；
- 不遗留可执行的高权限任务指向已删除或可被替换的路径。

## 11. 错误代码

至少提供以下稳定错误代码：

- `admin_required`
- `unsupported_standard_user`
- `unsafe_install_location`
- `elevation_task_missing`
- `elevation_task_invalid`
- `elevation_task_access_denied`
- `elevation_launch_failed`
- `elevation_launch_timeout`
- `job_conflict`
- `job_not_found`
- `invalid_job_state`
- `target_changed`
- `confirmation_required`
- `trace_not_in_plan`
- `uninstaller_cancelled`
- `uninstaller_failed`
- `removal_not_confirmed`
- `residue_scan_failed`
- `cleanup_failed`

错误消息面向用户使用中文，内部日志保留可诊断上下文，但不得泄漏敏感数据。

## 12. 测试策略

### 单元测试

- 任务定义 builder；
- 路径保护与用户可写目录拒绝；
- 管理员令牌状态映射；
- job 状态机合法和非法转换；
- 目标指纹变化；
- trace id 越权和 confirmation 校验；
- CLI 退役后依赖清理。

### Tauri adapter 测试

- DTO 到 application request 的映射；
- `CommandError` code 保真；
- event 包含 job id 和 sequence；
- coordinator 单任务冲突；
- mutex 不跨 await 的结构性回归测试或代码审查检查。

### 前端测试

- 计划确认；
- 运行阶段禁止关闭；
- 卸载完成后才显示残留；
- 残留默认不选；
- 选择后明确确认；
- 跳过清理；
- 错误与状态恢复。

### Windows 集成测试

- 使用受保护的系统二进制作为测试 Task Action，创建唯一临时任务并可靠清理；
- 验证普通用户能读取/运行但不能修改/删除测试任务；
- 验证 release 安装路径检查；
- 验证首次 UAC、第二次无 UAC、任务损坏后的修复；
- 验证安装、升级、卸载后无遗留任务；
- 使用 7-Zip、Xplorer MSI、Inno Legacy fixture 验证三类卸载链；
- Inno fixture 继续验证 Job Object 等待完整进程树与卸载后残留扫描。

## 13. 发布验收标准

只有以下条件全部满足才能发布：

- 安装器将 GUI 安装到 `Program Files`；
- 不再产出或上传 `yu.exe`；
- 每个管理员用户首次启动只出现一次 UAC；
- 第二次启动通过已验证计划任务进入管理员 GUI；
- 普通权限用户不能修改或删除该计划任务；
- 任务 action 不接受动态业务参数；
- debug/worktree 不会创建持久高权限任务；
- GUI 能唯一定位程序并完成模态同步卸载；
- 第三方卸载器结束前不会扫描残留；
- 残留默认不选择，后端只清理用户确认且复核通过的目标；
- 7-Zip、MSI、Store、Inno Legacy 路径通过验证；
- 严格 CSP 下 GUI 正常运行；
- Rust、前端、安装器、发布脚本测试全部通过；
- 卸载 Rust Yu 后不存在 `\Rust Yu` 高权限启动任务。

## 14. 官方参考

- Microsoft Task Scheduler `LogonType`：<https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-logontype-principaltype-element>
- Microsoft Task Scheduler `RunLevel`：<https://learn.microsoft.com/en-us/windows/win32/taskschd/principal-runlevel>
- Microsoft `RegisterTaskDefinition` 与任务 SDDL：<https://learn.microsoft.com/en-us/windows/win32/taskschd/taskfolder-registertaskdefinition>
- Microsoft 任务安全上下文：<https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks>
- Microsoft `AllowDemandStart`：<https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-allowdemandstart>
- Tauri Windows per-machine 安装：<https://v2.tauri.app/distribute/windows-installer/>
- Tauri CSP：<https://v2.tauri.app/security/csp/>
- Tauri capabilities：<https://v2.tauri.app/security/capabilities/>
