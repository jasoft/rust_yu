# 留痕夹具（leftover-trace fixtures）

`developer.rs` 开发者夹具目录中 `leftover-machine` / `leftover-user` / `leftover-sparse` 的实现。
每个夹具安装一个完整的软件足迹，其 `UninstallString` 是一个**脏卸载器**：执行后只删除自身的
ARP 卸载键，其余痕迹原样保留，用来验证 Rust Yu 的残留扫描（deep analysis / leftovers）。

## 各夹具足迹

| 夹具 | ARP 键 | 留下的痕迹 |
|---|---|---|
| `leftover-machine`（需管理员） | `HKLM\...\Uninstall\RustYuLeftoverMachine` | 安装目录（`Program Files\RustYuLeftoverMachine`）、`ProgramData\RustYuLeftoverMachine`、HKLM 厂商键 `Software\RustYu Fixture Labs\RustYuLeftoverMachine`（含 `Components` 子键）、HKLM Run 启动项 |
| `leftover-user` | `HKCU\...\Uninstall\RustYuLeftoverUser` | 用户安装目录（`%LocalAppData%\Programs\RustYuLeftoverUser`）、Roaming/Local AppData、HKCU 厂商键、HKCU Run、桌面与开始菜单个人快捷方式 |
| `leftover-sparse` | `HKCU\...\Uninstall\RustYuLeftoverSparse` | 泛型显示名（`RustYu Shared Helper`）+ 非标准位置（`%LocalAppData%\RustYuSharedTools\Helper`）+ 稀疏 HKCU 键（`Software\GenericToolsCo\SharedHelper`），ARP 键故意缺 Publisher/DisplayIcon 等值 |

## 用法

```powershell
# 安装（machine 需管理员；可逗号分隔多个 id）
powershell -NoProfile -ExecutionPolicy Bypass -File .resources\leftover-trace-fixtures\Install-LeftoverFixtures.ps1 -FixtureIds leftover-machine
# 清理指定夹具的全部痕迹
powershell -NoProfile -ExecutionPolicy Bypass -File .resources\leftover-trace-fixtures\Install-LeftoverFixtures.ps1 -FixtureIds leftover-machine -Cleanup
```

推荐直接在 Rust Yu 桌面端「开发者工具」页安装（Script 通道会以 System32 Windows PowerShell
调用本脚本，避免 MSIX pwsh 的注册表虚拟化）。

## 验证残留发现

安装后，在 Rust Yu 中对夹具执行卸载：卸载命令会删除 ARP 键（应用从列表消失），随后残留扫描
应发现上表中的全部痕迹。注意 HKLM Run 值位于 `Microsoft\Windows` 路径下，已知会命中残留
清理的保护路径拦截（fc-broken-fixture 暴露的同类 bug），本夹具可用于回归该场景。

## 安全边界

- 安装幂等（先清理后安装）；`-Cleanup` 只按固定键名/路径逐项删除，不使用通配符。
- 占位 `.exe`/`.dll` 只是文本文件，不会被任何路径执行。
- 脏卸载器只删除夹具自己的 ARP 键，不含任何破坏性操作。
