# Rust Yu 管理员 GUI 运维说明

本说明描述正式 Windows 安装包的提权、启动、卸载和故障处理方式。Rust Yu 是 GUI 产品；`yu.exe`、Scoop 和公开业务 CLI 均已退役。

## 为什么使用 Task Scheduler

Rust Yu 的 GUI 本身必须在管理员令牌下运行。Tauri Windows 应用清单声明了
`requestedExecutionLevel=requireAdministrator`，因此每次从快捷方式或 EXE 启动时，Windows
都会先请求 UAC；用户拒绝后不会创建 WebView，也不会执行任何业务操作。提升后的实例仍会
验证受保护安装路径并注册/修复固定任务，任务动作永远指向已验证的安装目录 EXE 和固定参数
`--elevated-entry`，不能被 WebView 或前端改写。Task Scheduler 用于受保护的启动维护和生命周期
清理，不用于绕过每次启动的 UAC 请求。

这不是公开命令行接口。未知参数会被拒绝，维护参数只供安装器卸载 hook 使用。

## 任务定义

| 属性 | 固定值 |
| --- | --- |
| Folder | `\Rust Yu` |
| Task | `ElevatedGui` |
| Principal | 当前登录用户 SID |
| LogonType | `InteractiveToken` |
| RunLevel | `HighestAvailable` |
| Action | Program Files 中的 Rust Yu GUI EXE |
| Arguments | `--elevated-entry` |
| Trigger | 无自动触发，只允许按需启动 |
| MultipleInstancesPolicy | `IgnoreNew` |
| Security descriptor | 当前用户只读/执行；Administrators 和 SYSTEM 管理 |

任务不能指向 `%TEMP%`、`%LOCALAPPDATA%`、工作树、网络路径、符号链接或普通用户可写目录。任务注册通过 Task Scheduler API 完成，不直接修改 `System32\Tasks` 文件。

## 正常启动流程

1. 从 Program Files 的快捷方式启动 GUI。
2. Windows 根据应用清单显示 UAC；用户接受后才进入 Rust 启动代码。
3. 管理员实例检查令牌、安装路径和任务 XML，并创建/修复固定任务。
4. 管理员 GUI 建立按用户隔离的单实例 mutex 后创建 WebView。

标准用户没有管理员令牌，产品不会静默切换账户，也不会把卸载操作交给任意外部命令；会显示 `unsupported_standard_user`。

## 检查任务

在 PowerShell 中执行：

```powershell
schtasks.exe /Query /TN "\Rust Yu\ElevatedGui" /XML
```

确认 XML 中包含当前用户 SID、`InteractiveToken`、`HighestAvailable`、`--elevated-entry` 和 `IgnoreNew`，动作路径位于 `C:\Program Files\Rust Yu\`。也可以使用仓库提供的验证脚本：

```powershell
.\tools\test\Verify-ElevatedGuiLifecycle.ps1 `
  -InstallerPath .\src-tauri\target\release\bundle\nsis\Rust-Yu_0.1.3_x64-setup.exe
```

脚本会创建唯一临时诊断目录，不会删除 Program Files，也不会把高权限任务指向测试工作树。

## 修复任务

不要手工编辑任务 XML。删除或修改任务后，重新启动正式 GUI 即可触发一次 UAC 修复。安装器维护模式也会在升级/卸载前清理本产品任务：

```powershell
Start-Process -Verb RunAs `
  -FilePath "C:\Program Files\Rust Yu\rust-yu-tauri.exe" `
  -ArgumentList "--remove-launch-tasks" -Wait
```

如果任务动作、参数、principal 或安全描述符不匹配，普通实例不会运行它，而是返回 `elevation_task_invalid` 并走 UAC 修复分支。

## 开发版规则

debug/worktree 启动永远直接进入 GUI，destructive command 仍由后端管理员检查保护；debug 版本不会注册持久高权限任务。验证当前用户任务不存在：

```powershell
schtasks.exe /Query /TN "\Rust Yu\ElevatedGui"
```

推荐使用开发脚本启动。`tools/dev/Run-Gui.ps1` 会在初始化工作树前检查当前
PowerShell 的管理员令牌；普通终端执行它时，脚本会自动以 `RunAs` 重启自身，
然后再运行 `npx tauri dev`：

```powershell
.\tools\dev\Run-Gui.ps1
```

如果直接运行 `cargo tauri dev`，仍需先打开管理员 PowerShell；Cargo 和 Tauri CLI
不会替调用它们的普通终端自动提升。

需要执行真实系统清理时，应在管理员 PowerShell 中运行开发版，并确认 `current_exe` 不会通过路径校验进入正式任务注册流程。

## 卸载工作流

卸载界面是模态的，顺序固定为：解析目标 → 保存快照 → 管理员预检 → 启动并等待完整卸载进程链 → 验证目标已移除 → 扫描残留 → 显示残留审查 → 用户逐项确认清理 → 刷新程序列表。卸载器未结束或验证失败时不会显示可删除残留。

残留默认全部不选。前端只能提交扫描快照中的 trace ID，后端会重新检查 ID、路径安全边界和存在性；没有 `confirm = true` 时不会删除任何文件或注册表项。

## 常见错误

| code | 含义 | 处理 |
| --- | --- | --- |
| `unsupported_standard_user` | 当前身份不是管理员 | 使用管理员账户或让管理员启动产品 |
| `unsafe_install_location` | EXE 不在受保护的 Program Files 路径 | 重新安装正式版本，不把 EXE 复制到工作树/Temp |
| `elevation_task_missing` | 固定任务不存在 | 重新启动 GUI，接受一次 UAC 修复 |
| `elevation_task_invalid` | 任务动作或安全属性被篡改 | 删除外部同名冲突后重新启动正式 GUI |
| `elevation_task_access_denied` | Task Scheduler 拒绝访问 | 以管理员身份启动并检查任务 ACL |
| `elevation_launch_failed` / `elevation_launch_timeout` | 任务启动失败或超时 | 检查任务 XML、事件日志和安装器完整性 |
| `admin_required` | destructive command 未在管理员进程中运行 | 重新启动管理员 GUI，不接受前端自行提权参数 |
| `target_changed` | 计划后的软件快照已变化 | 返回程序列表，重新计划卸载 |
| `confirmation_required` / `removal_not_confirmed` | 未明确确认清理 | 在残留审查界面逐项选择并确认 |

## 卸载产品本身

正式 NSIS 卸载器在删除文件前以管理员维护模式调用 `--remove-launch-tasks`，等待任务清理成功后才继续。卸载完成后检查：

```powershell
schtasks.exe /Query /TN "\Rust Yu\ElevatedGui"
Test-Path "C:\Program Files\Rust Yu"
```

任务查询应返回不存在，安装目录和快捷方式也应被卸载器移除。若维护模式失败，安装器会中止而不会留下未知的高权限任务。
