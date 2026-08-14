# 残留扫描安全边界设计

## 问题

残留扫描不能把“路径中包含程序名”当作归属证明。例如程序名为
`Xplorer` 时，`Internet Explorer` 会因为包含 `xplorer` 子串而命中，进而把
固定任务栏、浏览器快捷方式和其他用户数据列为可清理目标。这类误报比漏报更危险，
因为清理动作具有破坏性。

## 设计原则

1. **先划范围，再做匹配。** 扫描器只能在明确允许的根目录和注册表分支中工作；
   禁止区域不会以“低置信度”返回，而是直接不生成候选。
2. **组件相等，不做任意子串。** `Xplorer` 只匹配完整路径组件 `Xplorer`、
   文件主名 `Xplorer.exe` 或严格等价的紧凑标识 `RustYuLegacyTest`；不匹配
   `Internet Explorer`、`xplorer-backup`、`another-xplorer-tool`。
3. **证据分层。** 卸载前快照中的明确安装目录内容为高置信度；受限根目录下的名称
   命中只能是中置信度；无法证明归属的候选保持低置信度或不展示。
4. **默认选择最小化。** 默认只选择高置信度、非关键项目；中低置信度可以展示，
   但必须由用户逐项选择并确认。清理后端仍执行关键路径、目标快照、备份和删除后校验。
5. **宁可漏报，不可误删。** 桌面、公共文档、固定任务栏、系统 Shell 数据和
   Windows 共享目录不进行名称启发式扫描；用户仍可通过明确的安装目录证据审查真实残留。

## 扫描范围

### 允许的名称扫描根

- `Program Files`、`Program Files (x86)` 的直接应用树；
- `ProgramData` 中未落入受保护供应商/系统树的应用目录；
- 当前用户 `AppData\Roaming`、`Local`、`LocalLow` 中未落入受保护树的应用目录；
- 当前用户和所有用户开始菜单的 `Programs` 目录，仅匹配完整快捷方式主名。

桌面、公共桌面、公共文档、下载目录、Quick Launch、User Pinned、TaskBar 不做
名称扫描。快捷方式清理不是“看到名字就删”，而是需要精确文件名或明确安装快照证据。

### 永久保护区域

文件系统保护 Windows、System32、SysWOW64、WinSxS、Servicing、Boot、EFI、
DriverStore、Drivers、Fonts、WindowsApps、Common Files、Packages、
ConnectedDevicesPlatform、Internet Explorer、Quick Launch、User Pinned、
TaskBar 等组件及其子树，并保护 AppData/ProgramData 下的 Microsoft Windows
共享树。

注册表名称扫描不递归 `HKCR`，并跳过 Classes、Windows、Windows NT、Explorer、
Shell Folders、User Shell Folders、Run、RunOnce 等共享分支。卸载项只检查受限的
HKLM/HKCU Uninstall 根，并以 DisplayName 或键名的完整身份匹配。

## 归属匹配算法

`ScanIdentity` 是所有扫描器共用的身份对象：

- 保留显示名；
- 生成只由 ASCII 字母数字组成的紧凑标识；
- 仅允许去掉一个已知通用尾缀（`App`、`Software`、`Client`、`Suite`、`Tool`）；
- 路径组件、文件主名、注册表键名必须与身份别名完全相等；
- 卸载 DisplayName 可以额外接受“完整身份 + 纯数字版本号”，不能接受任意文字后缀。

因此 `RustYu Legacy Test App` 可以匹配夹具目录 `RustYuLegacyTest`，但不能匹配
`RustYuLegacyTesting`；`Xplorer` 不能匹配 `Internet Explorer`。

## 证据与清理策略

| 证据 | 展示 | 默认选择 | 清理条件 |
| --- | --- | --- | --- |
| 卸载前快照明确安装目录内文件 | 高置信度 | 是（非关键项） | 用户确认、备份、删除后校验 |
| 受限根下完整身份目录/文件名 | 中置信度 | 否 | 用户逐项选择并确认 |
| 仅弱关联或无法证明归属 | 低置信度或不展示 | 否 | 只有用户明确选择，仍需安全门禁 |
| 保护区域/关键系统项 | 不展示或标记保护 | 否 | 后端拒绝删除 |

“全选”是用户主动操作，不改变保护区域和关键项规则；确认对话框显示中低置信度
项目数量，避免用户把默认选择误认为系统已经证明归属。

## 回归门禁

每次新增扫描器或扩大扫描根，都必须加入以下反例测试：

- `Xplorer` 不命中 `Internet Explorer` 及其 Quick Launch/TaskBar 子文件；
- 不命中 `xplorer-backup`、`another-xplorer-tool` 等前后缀相似名称；
- Windows/System32、AppData Microsoft、Packages、HKCR/Explorer 分支不会产生候选；
- 明确安装目录仍能发现 Inno 夹具 `leftover.log` 和 AppData 夹具残留；
- 默认选择只包含高置信度非关键项，中低置信度必须人工确认。

任何一项失败都阻止发布；不能用“清理前会弹确认”替代扫描范围和归属证据修复。
