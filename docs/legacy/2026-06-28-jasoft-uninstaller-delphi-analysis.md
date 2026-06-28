---
title: "Your Uninstaller! Delphi 版本代码分析"
date: 2026-06-28
status: draft
scope:
  - "jasoft/Uninstaller"
  - "src/v7"
  - "src/common"
tags:
  - uninstaller
  - legacy
  - delphi
  - windows
  - reverse-architecture
---

# Your Uninstaller! Delphi 版本代码分析

> 本文档用于记录 `jasoft/Uninstaller` 仓库中 Delphi 版本的原始实现，供后续重写、迁移和设计复盘时长期查阅。

## 1. 结论摘要

### 1.1 一句话结论

旧版 `Your Uninstaller!` 的核心价值并不是某个单独 UI，而是：

- **“注册表 + MSI + Shell App Manager + 文件扫描 + 强制残留扫描 + 向导式清理”组成的卸载引擎**
- **围绕 `TUninstallItem` / `IInstalledApp2` 建立的统一应用抽象**
- **把卸载、启动项、磁盘清理、开始菜单清理、痕迹擦除放在同一产品壳里的工具箱式架构**

### 1.2 仍然值得继承的资产

1. **统一的应用模型**
   - 经典注册表应用
   - MSI 产品
   - 屏幕保护程序
   - Store / Shell App 尝试

2. **完整卸载链路**
   - 确认入口
   - 可选系统还原点
   - 卸载前快照
   - 执行原生卸载命令
   - 注册表残留扫描
   - 文件/目录残留扫描
   - 向导式结果汇总

3. **“Force Uninstall / 高级扫描”的产品直觉**
   - 通过安装目录、exe/dll 路径、注册表值内容做模糊匹配
   - 分 Safe / Normal / Super 模式控制扫描范围
   - 带超时控制，避免扫描卡死

4. **启动项管理**
   - 覆盖 Run / RunOnce / RunServices / RunOnceEx / Policy Run / 隐藏 Run / Startup Folder
   - Hide / Show / Delete / Save 的统一操作模型

5. **配置与国际化骨架**
   - INI 配置
   - 多语言翻译
   - 模块化 UI 切换

### 1.3 应该抛弃或彻底重做的部分

1. **UI 层不要移植**
   - 大量旧版 VCL Form、自绘控件、皮肤框架耦合，现代化成本远高于重做

2. **不要把所有工具塞回一个大壳**
   - IE 菜单管理、通用磁盘清理、浏览器痕迹擦除，不应再与专业卸载器主线并列

3. **商业与防盗版逻辑必须全部移除**
   - Armadillo / JLock / 在线校验 / 试用控制 / 黑名单处理，都不应进入新版

4. **旧的 COM/WMI 直调代码只能作为“思路参考”**
   - 新版应优先用 Rust + windows crate / WinRT / 更安全封装实现

5. **不要继续沿用“控件驱动”的架构**
   - 旧代码是“Form 驱动业务”，新版应改为 **Engine -> Command -> UI**

### 1.4 对现代 Rust/Tauri 版的直接启发

| 旧版能力 | 新版建议 |
|---|---|
| `EnumApp` + `UninstallManager` | `InstalledAppRegistry` + `InstalledAppStore` |
| `WizProgressForm` 三阶段 | `UninstallJob { precheck -> uninstall -> scan -> clean -> report }` |
| `AppForceRemover` | `TraceScanner { safe, normal, deep }` |
| `StartupManager` | 现有 `startup` 模块继续增强 |
| `SafeRemover` | `BackupSession` / `RestoreSession` / `DryRunSession` |
| `InstallLocationAnalyser` | `InstallLocationResolver` + 安装器类型识别 |
| `Scanner` / `FileFind` | `FilesystemTraceScanner` + async + timeout |

## 2. 仓库结构总览

### 2.1 目录角色

| 目录 | 作用 | 备注 |
|---|---|---|
| `src/v7` | 主程序壳与主界面模块 | `urUninstaller.dpr` 为主入口 |
| `src/common` | 核心业务引擎 | `UninstallItem`, `UninstallManager`, `EnumApp`, `Scanner`, `StartupManager` 等 |
| `src/v7/UninstallWizard` | 卸载向导多步流程 | 信息确认、类型选择、扫描、清理、汇总 |
| `src/v7/InstalledItems` | 更细粒度的 Installed 模型补充 | `InstalledApp`, `InstalledItem`, `InstalledService`, `InstalledScreenSaver` |
| `src/v7/StartMenu` | 开始菜单清理模块 | 死链检测、无效项清理、图标加载 |
| `src/v7/CplManager` | 控制面板项管理 | 旧工具扩展 |
| `src/v7/UninstallHunter` | Hunter / 拖拽快速卸载入口 | 产品差异化功能 |
| `src/v7/DUnit` | 单元测试骨架 | 包含 `RegScanner`, `Utils` 等测试 |
| `lib/urslib` | 自有 UI / 工具 / 系统封装库 | 大量通用 Delphi 库 |
| `doc` | 需求、宣传、任务资料 | 含旧版产品文档、广告文案、需求稿 |

### 2.2 最关键的源码入口

1. `src/v7/urUninstaller.dpr`
2. `src/v7/MainFormUnit.pas`
3. `src/v7/UninstallerFormUnit.pas`
4. `src/common/UninstallItem.pas`
5. `src/common/UninstallManager.pas`
6. `src/common/EnumApp.pas`
7. `src/v7/UninstallWizard/WizProgressFormUnit.pas`
8. `src/common/AppForceRemover.pas`
9. `src/common/StartupManager.pas`

## 3. 主程序入口与启动流程

## 3.1 `urUninstaller.dpr`

这是整个产品的主入口。

### 3.1.1 命令行模式

入口先处理参数化任务：

- `-close`
  - 通过 MemoryMapping 找到旧实例窗口句柄
  - 发送 `UM_CLOSE_INSTANTLY`，实现快速关闭已有实例

- `-buildcache`
  - 直接调用 `TUninstaller.Instance.RebuildCache`
  - 用于离线预生成应用列表/图标缓存

- `-diskclean`
  - 直接创建 `TDiskCleanerForm` 并执行 `SilentClean`
  - 支持命令行触发“一键磁盘清理”

- `-isregistered`
  - 用 ExitCode 返回注册状态
  - 属于旧版商业校验链的一部分

### 3.1.2 单实例控制

主进程通过 `CreateMutex` 保证单实例运行。

### 3.1.3 主启动顺序

1. 初始化语言
2. 显示 Splash
3. 创建 `TDM`（DataModule）
4. 显示 Nag / License 检查
5. 创建 `TUninstallerForm`
6. 创建 `TYUMainForm`
7. 根据参数决定：
   - 直接进入 Hunter 模式
   - 直接卸载某个程序
   - 正常启动 GUI

### 3.1.4 设计启示

旧版已经具备“CLI + GUI 双入口”的雏形，但这些入口是后加到一个 Form 驱动的老程序里的。

新版应反过来：

- 先做 CLI Engine
- 再做 Tauri UI 消费 Engine

## 4. 核心领域模型

## 4.1 `UninstallItem.pas`

这是旧版最重要的抽象之一。

### 4.1.1 核心接口设计

旧版为应用定义了分层接口：

- `IShellApp2`
- `IInstalledApp2`
- `IQueryInfo`

其语义是：

| 接口 | 职责 |
|---|---|
| `IShellApp2` | 获取 AppInfo、SlowAppInfo、PossibleActions、是否损坏、是否安装 |
| `IInstalledApp2` | 在此基础上增加 Delete / ForceUninstall / Modify / Repair / Uninstall / Upgrade |
| `IQueryInfo` | 提供名称和 InfoTip |

### 4.1.2 `TUninstallItem`

这是抽象基类，定义统一行为：

- `GetAppInfo`
- `IsCorrupted`
- `IsInstalled`
- `IsFrom`
- `Uninstall`
- `ForceUninstall`
- `DeleteIdentify`

### 4.1.3 具体实现类

- `TInstalledApp`
  - 经典注册表应用的核心实现
- `TVirtualInstalledApp`
  - 虚拟条目壳
- `TFailedRemoveApp`
  - 卸载失败条目
- `TInstalledScreenSaver`
  - 屏保程序
- `TWindowsUpdateUninstallItem`
  - Windows 更新项
- `TNullUninstallItem`
  - 空实现兜底

### 4.1.4 `TInstalledApp` 的关键字段

该类维护了大量领域信息：

- `DisplayName`
- `Publisher`
- `Key`
- `RootKey`
- `InstallLocation`
- `UninstallString`
- `DisplayIcon`
- `Size`
- `InstallDate`
- `LastAccessDate`
- `PossibleIconFile`

### 4.1.5 `APPINFODATA`

旧版定义了自己的 `APPINFODATA`，字段包括：

- `DisplayName`
- `Version`
- `Publisher`
- `SupportUrl`
- `HelpLink`
- `InstallLocation`
- `InstallSource`
- `InstallDate`
- `Image`
- `ReadmeUrl`
- `UpdateInfoUrl`
- `Key`
- `ParentDisplayName`
- `ParentKeyName`

### 4.1.6 对现代版本的意义

这就是你现在 Rust 里 `InstalledProgram` 的“祖宗模型”。

新版建议保留类似结构，但拆成：

- `InstalledAppIdentity`
- `InstalledAppMetadata`
- `InstalledAppInstallerContext`
- `InstalledAppRuntimeHints`

## 4.2 `InstalledApp.pas`（`src/v7/InstalledItems`）

该文件进一步展示了旧版“属性化模型”的做法。

它保存了：

- `DisplayName`
- `DisplayVersion`
- `Publisher`
- `InstallLocation`
- `UninstallString`
- `ProductCode`
- `HelpLink`
- `URLInfoAbout`
- `URLUpdateInfo`
- `NoModify`
- `NoRemove`
- `IsHidden`
- `IsNew`
- `IsBadItem`
- `LargestFile`
- `RealInstallLocation`

其中 `GetRealInstallLocation` 非常关键：

它说明旧版不是直接信任 `InstallLocation`，而是会尝试“推断真实安装目录”。

## 5. 应用枚举引擎

## 5.1 `EnumApp.pas`

这是旧版扫描已安装软件的核心模块。

### 5.1.1 枚举器接口

旧版定义了统一枚举接口：

```delphi
IYUEnumInstalledApps = interface (IInterface)
  function Next(var ia:IInstalledApp2): Boolean;
  function Reset: Boolean;
end;
```

### 5.1.2 具体枚举器

至少有三类：

1. `TYUEnumInstalledApps`
   - 主枚举器
2. `TShellEnumInstalledApps`
   - Shell App 枚举
3. `TEnumScreenSavers`
   - 屏幕保护程序枚举

### 5.1.3 Legacy 应用枚举

`GetNextLegacyApp` 的核心逻辑：

1. 先枚举 `HKLM\...\Uninstall`
2. 再枚举 `HKCU\...\Uninstall`

对每个子键：

- 读 `DisplayName`
- 读 `UninstallString`
- 排除 `SystemComponent = 1`
- 排除 `WindowsInstaller = 1`（交给 MSI 链路）

### 5.1.4 MSI 应用枚举

`GetNextMsiApp` 的核心逻辑：

- 调用 `MsiEnumProducts`
- 再检查 `SystemComponent`
- 再查询 `MsiQueryProductState`
- 如果产品处于 `INSTALLSTATE_DEFAULT` 或 `INSTALLSTATE_ADVERTISED`，则认为可展示

### 5.1.5 设计启示

旧版已经把“注册表枚举”和“MSI 枚举”分开。

新版应继续强化这个拆法，并增加：

- Store / MSIX / Appx 枚举
- 便携目录推断
- 安装来源标记

## 6. 管理层与渲染层

## 6.1 `UninstallManager.pas`

这是应用管理层。

### 6.1.1 核心职责

- 维护 `FUninstallItems`
- 维护 `FBadItems`
- 维护 `FNewlyInstalledApps`
- 维护 `FFailedRemoveApps`
- 通过 `EnumApps()` 统一加载
- 提供 `Find` / `GetByName` / `FindByFileName`

### 6.1.2 加载过程

`EnumApps()` 的关键动作：

1. 创建多个枚举器
2. 默认加入 `TYUEnumInstalledApps`
3. 如果配置允许，再加入 `TEnumScreenSavers`
4. 遍历每个枚举器
5. 过滤 `ShowWinUpdates` 设置
6. 把应用注册进 `FUninstallItems`

### 6.1.3 刷新动作

`Refresh` 会：

- 重新枚举
- 判断新安装
- 判断失败卸载
- 统计损坏项

## 6.2 `UninstallerFacade.pas`

这是旧版的“门面层”。

它把下面这些能力统一暴露出来：

- `TUninstaller`
- `TUninstallManager`
- `TFileInfoManager`
- `TYUAppRender`

关键方法：

- `Refresh`
- `Render`
- `RebuildCache`
- `Uninstall`

### 6.2.1 `Render(ia)`

这是旧版实现“模型 -> 展示模型”分离的地方。

### 6.2.2 设计启示

新版 Rust/Tauri 应该继续保持这种分层：

- Engine Model
- Presentation Model
- UI Model

不要让前端直接吞后端原始对象。

## 6.3 `AppRender.pas`

这是旧版“展示层转换器”。

### 6.3.1 `IYURenderedApp`

它定义了大量只读展示属性：

- `DisplayName`
- `DisplayVersion`
- `Publisher`
- `InstallDate`
- `InstallLocation`
- `InstallSource`
- `Size`
- `UninstallString`
- `HelpLink`
- `URLUpdateInfo`

### 6.3.2 `TYURenderedApp`

其职责是：

- 包装原始 `IInstalledApp2`
- 懒加载额外信息
- 缓存图标索引
- 缓存 SlowAppInfo

### 6.3.3 对你当前项目的启发

你在前端已经有很多 `ProgramDetails`、`usePrograms`、`mapBackendToProgram`，这其实就是同一层。

建议继续做成明确的：

- `InstalledProgramSnapshot`
- `RenderedProgram`
- `ProgramViewModel`

## 7. GUI 卸载主链路

## 7.1 `GUIUninstallerWrapper.pas`

这是 UI 调用卸载引擎的封装层。

### 7.1.1 BASIC 模式

流程如下：

1. 判断是否卸载自身
2. 判断过期
3. 弹确认对话框
4. 保存安装位置
5. 调用 `Item.Uninstall(Handle)`
6. 根据结果提示成功或失败
7. 成功后从 `UninstallManager` 移除条目

### 7.1.2 PRO 模式

如果 `RunMode = RUNMODE_PRO`，则进入：

- `StartUninstallWizard(Item, FAutomatedUninstall)`

即“向导式卸载”。

### 7.1.3 设计启示

旧版已经天然把：

- 快速卸载
- 专业向导卸载

分成两条路径。

新版建议继续保留：

- Quick Uninstall
- Deep / Guided Uninstall

## 7.2 `UninstallerFormUnit.pas`

这是主程序卸载列表页。

### 7.2.1 职责

它承担了：

- 渲染应用列表
- 搜索
- 分组
- 详情展示
- 右键菜单
- 工具侧栏入口
- Hunter 模式入口
- 批量操作入口

### 7.2.2 关键数据

- `FUninstaller`
- `FCurrentItem`
- `FTotalSize`
- `FUninstalledCount`
- `FGetInfoThread`

### 7.2.3 后台线程

旧版在界面层使用 `TGetInfoThread` 异步加载：

- 额外信息
- 新安装检测
- UI 更新

### 7.2.4 设计启示

旧版问题在于：

**业务逻辑被 UI Form 吃掉了太多。**

新版应明确拆出：

- `InstalledProgramListService`
- `ProgramDetailService`
- `UninstallService`
- `BatchUninstallService`

## 7.3 `MainFormUnit.pas`

这是产品主壳。

### 7.3.1 关键点

它负责：

- 菜单
- 模块切换
- 新闻检查
- 自动更新入口
- License / Nag / Trial 相关控制
- 全局内存映射句柄注册
- 与 `UninstallerForm` 的 Observer 关系

### 7.3.2 模块中心思想

旧版通过 `TFormsManager` 把不同模块登记到统一管理器中，例如：

- `MODULE_UNINSTALLER`
- `MODULE_STARTUP_MANAGER`
- `MODULE_TRACE_ERASER`
- `MODULE_TEMP_FILES`
- `MODULE_START_MENU`
- `MODULE_IE_MENU`
- `MODULE_CPL_MANAGER`
- `MODULE_WINDOWS_TOOLS`

### 7.3.3 设计启示

这种“模块中心”的思路可以保留，但新版建议做成：

- 主产品只保留“卸载相关主线”
- 其它工具做成独立插件或独立命令

## 8. 卸载向导（核心旧逻辑）

## 8.1 `WizIntf.pas`

定义了向导核心数据结构：

### 8.1.1 `TUninstallData`

这是旧版卸载向导的共享上下文，包含：

- `Item`
- `RegKeys`
- `Files`
- `Result`
- `ErrorMsg`
- `UninstallMode`
- `AutomatedUninstall`

### 8.1.2 `TUninstallMode`

分为：

- `umBuiltIn`
- `umSafe`
- `umNormal`
- `umSuper`

这正是后面 `AppForceRemover` 扫描范围的控制方式。

## 8.2 `WizProgressFormUnit.pas`

这是旧版最值得研究的文件之一。

### 8.2.1 主流程

向导分三步：

1. **Uninstall**
2. **ScanRegistry**
3. **ScanInstallLocation**

### 8.2.2 Uninstall 阶段

该阶段会：

1. 渲染应用信息
2. 加载额外信息
3. 记录 `InstallLocation`
4. 如果开启了系统还原，先创建还原点
5. 执行 `Data.item.Uninstall(Handle)`
6. 处理 `EMSProtectedException` / `EScreenSaverException` / `EUninstallFailureException`

### 8.2.3 `PrepareUninstallRegistryKeys`

它先把“已知卸载相关注册表路径”收集起来：

- `HKLM\...\Uninstall\<key>`
- `HKCU\...\Uninstall\<key>`
- `ARPCache\<key>`
- `YUCache\<key>`

如果是 MSI，还会额外收集：

- `MSIGetRegistryKeys(...)`

### 8.2.4 `ScanRegistry`

这部分调用 `TForceUninstaler`，执行注册表残留扫描。

### 8.2.5 `ScanInstallLocation`

这部分扫描：

- 安装目录文件
- Program Files 下可能目录
- Desktop
- Start Menu
- AppData

### 8.2.6 设计启示

旧版其实已经具备“卸载后扫描残留”的完整产品流程。

你当前 Rust 版本中：

- `scanner`
- `cleaner`
- `uninstall`

基本就是这条链路的现代延续。

## 8.3 `WizSummaryFormUnit.pas`

负责展示最终结果：

- 成功
- 失败
- 被取消

## 9. 强制扫描与残留匹配

## 9.1 `AppForceRemover.pas`

这是旧版的“高级扫描引擎”。

### 9.1.1 扫描范围

它会扫描多个注册表根路径：

- `HKCU\...\Uninstall`
- `HKCU\...\ARPCache`
- `HKLM\Wow6432Node\...\Uninstall`
- `HKLM\Wow6432Node\...\ARPCache`
- `HKLM\...\Uninstall`
- `HKLM\...\ARPCache`
- `HKCU\Software`
- `HKLM\Software`
- `HKCR\`

并根据模式决定深度：

| 模式 | 深度 |
|---|---|
| `umSafe` | 到 Uninstall / ARPCache 为止 |
| `umNormal` | 扩展到 `HKCU/HKLM Software` |
| `umSuper` | 进一步进入 `HKCR` |

### 9.1.2 匹配策略

扫描结果不是直接删除，而是先做匹配：

1. 用程序名做 `CompareDisplayName`
2. 用安装目录里的 exe/dll 路径建立匹配表
3. 如果注册表值中的字符串包含已知路径，则认为相关
4. 对命中项进一步展开父键

### 9.1.3 超时机制

旧版通过 `TTimer` 设置超时，避免“全盘注册表扫描”卡死程序。

### 9.1.4 对新版的启发

这是你当前 `scanner/registry.rs`、`scanner/filesystem.rs` 的直接原型。

但旧版的问题是：

- 没有置信度评分体系
- 匹配规则比较经验化
- 缺少删除前快照与回滚能力

## 10. 文件与目录扫描

## 10.1 `Scanner.pas`

该模块封装了通用文件扫描器。

核心是：

- `TFileScanner`
- `TFileScanItem`

### 10.1.1 `TFileScanItem`

这是策略对象，包含：

- `Pattern`
- `Directory`
- `Recursive`
- `CurrentFile`
- `Match()`

### 10.1.2 具体子类

- `TInvalidLinkScanItem`
  - 检测无效快捷方式
- `TXPHotFixScanItem`
  - 检测 `$hf_mig$` / `$NtUninstall` 等旧时代目录

### 10.1.3 设计启示

旧版已经把“扫描”抽象成策略模式。

新版建议增强为：

- `TraceScanStrategy`
- `RegistryTraceScanStrategy`
- `FilesystemTraceScanStrategy`
- `ServiceTraceScanStrategy`
- `TaskTraceScanStrategy`

## 11. 安全删除与回滚骨架

## 11.1 `SafeRemover.pas`

旧版这里有回滚思想的雏形。

### 11.1.1 接口

- `IRemoveSession`
- `ISafeRemover`

### 11.1.2 会话语义

```text
StartSession
Add(file)
Add(registry)
Store
Restore
```

### 11.1.3 当前状态

该模块代码并不完整，更多是骨架。

### 11.1.4 对新版的意义

这说明旧版作者当时已经意识到“删除前需要备份”。

新版应正式实现：

- `BackupSession`
- `SnapshotRegistry`
- `SnapshotFileSystem`
- `SnapshotServices`
- `RestoreSession`
- `UndoJob`

## 12. 启动项管理

## 12.1 `StartupManager.pas`

### 12.1.1 覆盖来源

旧版覆盖了相当全的启动来源：

| 来源 | 说明 |
|---|---|
| `sfAUStartUp` | All Users Startup Folder |
| `sfCUStartUp` | Current User Startup Folder |
| `sfAURun` | HKLM Run |
| `sfCURun` | HKCU Run |
| `sfAURunOnce` / `sfCURunOnce` | RunOnce |
| `sfAURunServices` / `sfCURunServices` | RunServices |
| `sfAURunServicesOnce` / `sfCURunServicesOnce` | RunServicesOnce |
| `sfAUNTRun` / `sfCUNTRun` | NT Run |
| `sfAUNTLoad` / `sfCUNTLoad` | NT Load |
| `sfAURunOnceEx` / `sfCURunOnceEx` | RunOnceEx |
| `sfAUSecretSection1` / `sfCUSecretSection1` | Policy\Explorer\Run |
| `sfAUSecretSection2` / `sfCUSecretSection2` | 隐藏 Run 路径 |

### 12.1.2 操作模型

每个 `TStartupItem` 都支持：

- `Hide`
- `Show`
- `Remove`
- `Save`
- `UpdateStatus`

对于注册表项和文件夹项，行为不同。

### 12.1.3 附加信息

旧版还会补充：

- `Description`
- `FileName`
- `CommandLine`
- `ProductName`
- `Company`
- `Detail`
- `InstalledOn`
- `Is64Bit`

### 12.1.4 对新版的意义

你当前 `startup` 模块已经比旧版更结构化，但仍可参考：

- 旧版的来源分类
- Hide/Show 的产品化语义
- Hidden Startup 的产品概念

## 13. 其他子系统

## 13.1 `InstallLocationAnalyser.pas`

负责从 `UninstallString` / 安装路径中推断“真实安装位置”。

### 13.1.1 分析器链

旧版使用了责任链模式：

- `TDefaultAnalyserFilter`
- `TInstallShieldAnalyserFilter`
- `TWiseAnalyserFilter`

### 13.1.2 过滤逻辑

`TDefaultAnalyserFilter` 会拒绝以下目录：

- 系统目录
- Common Files
- Windows 目录

### 13.1.3 设计启示

新版应继续强化这种“安装器类型识别器”：

- Inno
- NSIS
- InstallShield
- MSI
- Store

## 13.2 `MSIUtils.pas`

用于收集 MSI 相关注册表路径。

关键点：

- 枚举 `Installer\UserData` 下的 SID
- 枚举 `HKLM`, `HKCR`, `HKCU`, `HKU` 下的 MSI 注册表群

## 13.3 `SystemRestore.pas`

旧版在卸载前支持：

- 创建系统还原点
- 枚举还原点
- 从还原点恢复
- 判断还原是否启用

## 13.4 `InternetTraceEraser.pas`

这是旧版浏览器痕迹擦除模块，覆盖：

- Cookies
- Temporary Internet Files
- TypedURLs
- History
- Search
- Passwords
- Saved Forms
- Firefox 对应项

## 13.5 `IEContextMenuManager.pas`

管理 IE 右键菜单扩展。

## 13.6 `uDiskCleanerForm.pas`

磁盘清理模块。

支持：

- 临时目录扫描
- 特定类型文件扫描
- 删除、批量删除、打开目录、详情展示

## 13.7 `uCleanStMenuForm.pas`

开始菜单清理模块。

支持：

- All Users / Current User 切换
- 快捷方式有效性检测
- 死链清理

## 14. 旧版测试与工程现状

## 14.1 测试现状

在 `src/v7/DUnit` 中存在测试骨架：

- `RegScannerTest`
- `UtilsTest`
- `StartupManagerFormTest`

但整体测试覆盖率很低，基本是局部验证。

## 14.2 工程现状

旧版代码呈现明显特征：

- 多版本演化痕迹重
- `src/common` 与 `src/v7` 耦合多
- 很多接口雏形已完成，但后续被业务表单淹没
- 大量逻辑放在 Form / DataModule / 全局变量中

## 15. 给你当前项目的迁移建议

## 15.1 优先迁移逻辑

### 第一优先级

1. 统一应用模型
2. 注册表枚举
3. MSI 枚举
4. 卸载后注册表扫描
5. 卸载后文件扫描
6. 强制扫描模式
7. 向导化多阶段卸载

### 第二优先级

1. 启动项管理增强
2. 安装器类型识别
3. 安装位置推断
4. 报告与导出

### 第三优先级

1. 安全删除会话
2. 卸载前还原点
3. 附加工具模块

## 15.2 不建议迁移的内容

1. UI Form 代码
2. 商业保护逻辑
3. 浏览器清理模块
4. 旧版皮肤框架
5. 过时的 WMI/COM 调用风格

## 15.3 建议形成的 Rust 新模块

| 新模块 | 参考旧源码 |
|---|---|
| `InstalledAppRegistry` | `EnumApp`, `UninstallItem`, `InstalledApp` |
| `InstalledAppStore` | `UninstallManager`, `AppRender` |
| `LegacyUninstallResolver` | `UninstallItem.TInstalledApp.Uninstall` |
| `MsiUninstallResolver` | `EnumApp.GetNextMsiApp`, `MSIUtils` |
| `PostUninstallTraceScanner` | `AppForceRemover`, `Scanner`, `WizProgressFormUnit` |
| `InstallLocationResolver` | `InstallLocationAnalyser` |
| `StartupSourceRegistry` | `StartupManager` |
| `SafeCleanupSession` | `SafeRemover` |
| `UninstallJobRunner` | `WizProgressFormUnit` |
| `ProgramPresentationMapper` | `AppRender`, `TYURenderedApp` |

## 16. 附录：建议重点复读的源文件

## 16.1 必读

- `src/v7/urUninstaller.dpr`
- `src/v7/MainFormUnit.pas`
- `src/v7/UninstallerFormUnit.pas`
- `src/common/UninstallItem.pas`
- `src/common/UninstallManager.pas`
- `src/common/EnumApp.pas`
- `src/common/AppRender.pas`
- `src/v7/UninstallWizard/WizIntf.pas`
- `src/v7/UninstallWizard/WizProgressFormUnit.pas`
- `src/common/AppForceRemover.pas`
- `src/common/StartupManager.pas`

## 16.2 次优先

- `src/common/GUIUninstallerWrapper.pas`
- `src/common/InstallLocationAnalyser.pas`
- `src/common/MSIUtils.pas`
- `src/common/SystemRestore.pas`
- `src/common/SafeRemover.pas`
- `src/common/FindApp.pas`
- `src/common/YUUtils.pas`
- `src/v7/OptionsFormUnit.pas`
- `src/v7/SearchFormUnit.pas`
- `src/v7/uGroupManager.pas`
- `src/v7/InstalledItems/InstalledApp.pas`

## 16.3 可选浏览

- `src/v7/InternetTraceEraser.pas`
- `src/v7/IEContextMenuManager.pas`
- `src/v7/uDiskCleanerForm.pas`
- `src/v7/StartMenu/src/uCleanStMenuForm.pas`
- `src/v7/DUnit`

## 17. 后续建议

如果下一步还要继续深挖，我建议按这个顺序补两份文档：

1. **《Your Uninstaller 卸载链路时序分析》**
   - 按 `confirm -> uninstall -> scan -> clean -> report` 画完整时序图

2. **《Your Uninstaller 旧模型到 Rust 模型映射表》**
   - `Delphi class/interface -> Rust struct/enum/command`

这样会更容易把你现在这个 `rust_yu` 项目和旧版产品建立一一对应的改造依据。
