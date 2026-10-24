# Inno 脏卸载夹具（dirty uninstall fixture）

模拟"反安装程序留下大量残留"的遗留软件。安装包（Inno Setup 6，AMD64）创建完整软件足迹，
其自带卸载器执行后**只删除**：未标记 `uninsneveruninstall` 的文件（`README.txt`、`config\default.json`、`Register-ExtraTraces.ps1`）、Inno 自己创建的快捷方式、以及 ARP 卸载键。其余全部残留，供 Rust Yu 的残留扫描（deep analysis / leftovers）与清理流程验证。

## 安装后创建的痕迹

| 类别 | 痕迹 | 卸载后 |
|---|---|---|
| 文件（安装目录） | `{app}\RustYuDirtyDaemon.exe`、`RustYuDirtyService.exe`、`logs\service-heartbeat.log` | 保留 |
| 文件（ProgramData） | `%ProgramData%\RustYu Dirty Traces\{telemetry,cache,traces-manifest.json}` | 保留 |
| 文件（LocalAppData） | `%LocalAppData%\RustYu Dirty Traces\leftover-user-state.json` | 保留 |
| 注册表（厂商键） | `HKLM\Software\RustYu Fixture Labs\RustYu Dirty Traces`（含 `Updates` 子键） | 保留 |
| 注册表（启动项） | `HKLM\...\CurrentVersion\Run\RustYuDirtyTraces` | 保留 |
| 注册表（App Paths） | `HKLM\...\App Paths\RustYuDirtyDaemon.exe` | 保留 |
| 注册表（环境变量） | `HKLM\...\Session Manager\Environment\RUSTYU_DIRTY_TRACES_HOME`（刻意不改系统 Path） | 保留 |
| 服务 | `RustYuDirtyTraces`（demand、LocalSystem、**运行中**，真实 SCM 服务） | 保留（仍在运行） |
| 进程 | `RustYuDirtyDaemon.exe`（常驻，直到被杀或触发 `Global\RustYuDirtyTracesStop` 事件） | 保留（仍在运行） |
| 计划任务 | `RustYuDirtyTracesStartTask`（ONSTART / SYSTEM） | 保留 |
| 防火墙 | 入站允许规则 `RustYu Dirty Traces Daemon` | 保留 |
| 快捷方式 | 公共桌面 + `开始菜单\RustYu Fixture Labs\` 下的 `.lnk`（Inno 感知不到的方式创建） | 保留 |

`{app}` 默认为 `C:\Program Files\RustYu Dirty Traces Test App`，ARP 键为
`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1`。

## 构建与验证

```powershell
# 构建（要求 x64 MSVC rustc + Inno Setup 6；脚本会校验 rustc host 与产物 PE 架构）
.resources\inno-dirty-traces\Build-DirtyTracesFixture.ps1

# 结构校验（不需管理员）
tools\test\Verify-DirtyTracesFixture.ps1

# 完整生命周期验证（需管理员 PowerShell）：重置 → 静默安装 → 断言痕迹齐全
# → 执行脏卸载器 → 断言只有 ARP 键和普通文件被删、其余残留保留
tools\test\Verify-DirtyTracesFixture.ps1 -RunLifecycle
# 生命周期后顺带全量清理
tools\test\Verify-DirtyTracesFixture.ps1 -RunLifecycle -CleanAfterLifecycle
```

也可以在 Rust Yu 桌面端「开发者工具」页安装本夹具（id `inno-dirty`）。

## 全量清理

```powershell
.resources\inno-dirty-traces\Reset-DirtyTracesFixture.ps1   # 需管理员
```

按固定名称逐项删除：杀 `RustYuDirtyDaemon` 进程 → 停止/删除服务 → 删除计划任务与防火墙规则 → 删除注册表残留 → 删除快捷方式 → 删除安装目录 / ProgramData / LocalAppData 目录。不触碰任何共享路径。

## 设计说明

- 服务与守护进程均为单文件 Rust 源码（`payload\tools\*.rs`），手工声明 Win32 API，无外部 crate，由构建脚本用 x64 rustc 直接编译。
- 卸载器"脏"的行为来自 Inno 机制（文件 `uninsneveruninstall`）与安装期外部注册（`Register-ExtraTraces.ps1`），不篡改 unins000.exe。注册表残留全部由该脚本写入——Inno `[Registry]` 段创建的值会被卸载器默认删除，且该段没有"卸载时保留"的 flag。
- `Run` / `App Paths` / `Environment` 均位于 `Microsoft\Windows` 路径下，已知会命中残留清理的保护路径拦截（见 fc-broken-fixture README 的同类 bug），本夹具正好用于回归该场景。
- 防火墙规则只放行守护进程可执行文件（不监听任何端口），无实际网络影响。
