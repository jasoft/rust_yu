---
title: "Windows 卸载器竞品研究与 Rust Yu 功能路线图"
date: 2026-08-10
status: active
scope: Windows 10/11 desktop uninstaller
---

# Windows 卸载器竞品研究与 Rust Yu 功能路线图

## 1. 研究结论

本次选取 Revo Uninstaller、IObit Uninstaller、Geek Uninstaller、Bulk Crap Uninstaller（BCUninstaller）、Total Uninstall 和 Wise Program Uninstaller 作为对照样本。这里的“主流”表示用户认知度高、产品长期维护或在功能上具有代表性；厂商没有提供统一口径的销量数据，因此本文不把它当作市场份额排名。

主流产品真正反复销售的不是“删除一个程序”，而是一条完整的信任链：

```text
发现更多应用
  -> 调用原厂卸载器
  -> 等待并确认主体已移除
  -> 扫描文件、用户数据、注册表和系统集成残留
  -> 逐项审查、可回滚地清理
  -> 给出可追溯的结果报告
```

其中最稳定的共同卖点是：

1. **彻底但可审查的残留清理**：Revo、Geek、BCUninstaller、Wise 都把 built-in uninstall 之后的 leftovers 扫描作为核心叙事；差异在于扫描深度、默认选择和回滚能力。
2. **失败程序的救援路径**：Revo 的 Forced Uninstall/Hunter Mode、Geek 的 Force Removal、BCUninstaller 的 Force Uninstall，以及 Wise 的 Forced/Custom Uninstall，解决“程序不在列表中、卸载器损坏或卸载卡死”。
3. **批量和自动化**：BCUninstaller 将批量、静默、碰撞防护和低人工介入放在产品中心；IObit、Wise、Total Uninstall 也提供批量或自动化流程。
4. **更完整的应用发现**：BCUninstaller 特别强调隐藏/损坏/便携应用、Windows Features、Store、Steam/Chocolatey 等来源；IObit、Geek、Wise 也覆盖 Windows/Store 应用。
5. **安装监控而非事后猜测**：IObit、Total Uninstall、Ashampoo 和 HiBit 都把安装前后快照或实时监控作为高级能力。它能把“猜测残留”升级为“记录过的变更”。
6. **安全感和可恢复性**：Revo 的回收站/注册表备份、Total Uninstall 的快照反向应用、Wise 的删除前备份，说明用户愿意为“误删后能恢复”买单。
7. **低摩擦入口**：Geek/Wise 的便携单文件、Revo 的 Hunter Mode、右键菜单和拖拽入口，减少用户寻找目标程序的成本。
8. **工具箱扩展**：启动项、浏览器插件、垃圾文件、文件粉碎、软件健康、评分/分组和导出报告，承担留存和差异化；但不应稀释卸载主线。

### 1.1 竞品卖点矩阵

| 产品 | 主要定位 | 官方资料反复强调的卖点 | 对 Rust Yu 的启示 |
|---|---|---|---|
| [Revo Uninstaller](https://www.revouninstaller.com/products/revo-uninstaller-pro/) | 面向普通用户的深度卸载器 | 先跑原厂卸载器，再扫文件/文件夹/注册表残留；Forced Uninstall；Quick Uninstall；Pro 版 Hunter Mode；删除残留默认进入回收站并备份注册表 | 保留“原厂卸载优先”，把强制模式设为明确的救援分支；提供目标识别入口和删除前恢复点 |
| [IObit Uninstaller](https://www.iobit.com/en/advanceduninstaller.php) | 卸载 + 软件健康工具箱 | 批量卸载、顽固软件、隐藏文件、bundleware、Store/Windows App、浏览器扩展、Install Monitor、Software Health、文件粉碎 | 批量队列和安装监控是高价值差异；健康/工具箱应建立在安全结果模型之上 |
| [Geek Uninstaller](https://geekuninstaller.com/?lang=en) | 轻量、便携、快速救援 | Clean Removal、Force Removal、单 EXE 便携、32/64 位 Windows、Store 应用支持 | 便携发布和“快速完成一次卸载”是体验目标；不能为了轻量牺牲确认和错误可见性 |
| [BCUninstaller](https://www.bcuninstaller.com/) | 高级用户/技术人员的批量自动化 | 发现隐藏/损坏/便携/Store/Windows Features/更新/Steam/Chocolatey；批量、静默、碰撞防护、无卸载器也能处理、启动项和评分 | 先实现可靠的串行批量队列和每项隔离，再考虑并发；扩大来源前必须保留来源、权限和置信度 |
| [Total Uninstall](https://www.martau.com/document/total-uninstall.php) | 安装变更审计和可逆卸载 | 已安装程序分析日志；安装前后快照对比；监控注册表/文件系统；按日志反向移除；备份/恢复；详细日志和导出 | 安装监控应使用快照差异作为高置信度证据；清理操作需要真正的备份/恢复，而不是只有元数据快照 |
| [Wise Program Uninstaller](https://www.wisecleaner.com/wise-program-uninstaller-user-guide.html) | 轻量、安全卸载 + 强制/自定义卸载 | Safe Uninstall 后扫描残留；Forced/Custom Uninstall；Windows/UWP；删除前备份；最近版本增加残留导出 | 强制卸载必须明确警示并允许审查；报告导出和失败项导出是低成本高价值能力 |

### 1.2 产品策略

Rust Yu 不复制“清理一切”的激进叙事，而采用 **原厂卸载优先、证据驱动、低置信度默认保留、每个破坏性动作可解释** 的路线。这样既覆盖竞品核心需求，也把它们最容易造成误删的地方变成产品优势。

## 2. 当前代码能力盘点

截至本研究日，仓库已经具备以下基础：

| 能力 | 当前证据 | 状态 |
|---|---|---|
| 注册表/MSI/Store 应用发现、缓存、图标和来源筛选 | `src/modules/lister/`、`src-tauri/src-frontends/webui/src/lib/programFilters.ts`、`stores/programs.ts` | 已实现 |
| 原厂卸载、管理员检查、目标指纹、移除验证 | `src/application/uninstall/`、`src/modules/uninstall/` | 已实现 |
| 文件/AppData/注册表/快捷方式残留扫描和置信度 | `src/modules/scanner/` | 已实现 |
| Windows 服务、计划任务和驱动残留证据 | `src/modules/system_integration.rs`、`scanner`、`cleaner` | 本轮已完成；服务/任务二次验证，驱动只读且默认保留 |
| 逐项残留审核、低置信度默认保留、清理结果 | `ResidueReview.tsx`、`cleaner` 模块 | 已实现 |
| 实时阶段事件、扫描进度和可审查卸载报告 | `5b5d217 feature: add live uninstall scan report`、`uninstallReport.ts` | 本轮已完成 |
| 自启动多来源管理、计划/确认/快照/回滚 | `src/modules/startup/`、`StartupManager.tsx` | 已实现 |
| 系统清理和浏览器插件/缓存清理 | `src/modules/fluent_cleaner/`、`src/modules/browser_cleaner/` | 已实现 |
| 强制/自定义卸载未登记程序 | `src/application/force_uninstall.rs`、`ForceUninstallModal`、路径/快捷方式/猎手/右键入口 | 本轮已完成；所有入口都汇聚到管理员计划、指纹复核和显式确认 |
| 批量卸载队列 | 列表多选、串行调度、独立状态和冲突暂停 | 本轮已实现 |
| 安装监控 | `src/modules/install_monitor.rs`、Tauri `install_monitor` 命令、`InstallMonitorManager.tsx` | 本轮已完成；受限范围的前后快照、差异筛选、JSON/CSV 导出和 Trace 证据转换 |
| 文件/注册表删除前备份、恢复中心 | `src/modules/backup.rs`、清理器会话记录、Tauri `backup` 命令、`BackupCenter.tsx` | 本轮已完成；文件/目录/注册表键值可验证恢复，拒绝覆盖并支持失败重试 |
| 卸载报告历史与导出 | `src/modules/reporter/history.rs`、Tauri `report` 命令、`ReportCenter.tsx` | 本轮已完成；终态任务保存 JSON/HTML/TXT，历史可重开，失败项和阶段事件可审查，导出失败可见 |
| 便携/可部署分发 | `src-tauri/src/bootstrap.rs`、`tools/release/package-portable.ps1`、`docs/deployment.md` | 本轮已完成实现；`portable.flag`/`--portable` 将缓存、日志、备份、监控和报告收拢到 EXE 同级 `data`，便携实例隔离且不创建产品计划任务 |
| 软件健康与更新提示 | `src/modules/health.rs`、Tauri `health` 命令、`HealthCenter.tsx` | 本轮已完成；按卸载元数据、重复项、自启动和本机使用缓存给出可解释分数，只提供 HTTP(S) 厂商页面手动入口，不联网、不静默升级 |

## 3. 实施计划表

优先级含义：P0 是卸载主线和安全边界；P1 是能形成明显竞品差异的工作流；P2 是入口、工具箱和分发增强。每项都要求先写领域测试，再接 Tauri 命令和 UI；涉及真实文件/注册表时必须有 dry-run、确认、保护路径和可验证结果。

| ID | 优先级 | 功能 | 竞品依据 | 实施范围 | 验收标准 | 状态 |
|---|---|---|---|---|---|---|
| F-01 | P0 | 多来源应用清单与缓存 | BCU/IObit/Geek/Wise | Registry、MSI、Store；缓存、图标、大小、来源标签 | 启动/刷新/每个来源标签都保留一致列表和图标；测试覆盖来源计数与缓存 | 已完成 |
| F-02 | P0 | 安全标准卸载主流程 | 所有样本 | 目标指纹、管理员、原厂卸载器、Job Object 等待、移除验证、残留审查、日志 | 目标变化/提权失败/卸载失败/超时均显示明确结果；不自动删除低置信度项目 | 已完成 |
| F-03 | P0 | 实时进度与完整报告 | Revo/Total/Wise | 阶段事件、扫描区域统计、清理结果、报告明细 | 事件在阶段发生时发送；报告只使用任务快照，不展示估算数字；前后端测试通过 | 已完成 |
| F-04 | P0 | 强制/自定义卸载 | Revo/Geek/BCU/Wise | 输入程序目录或快捷方式；构造未登记目标；先扫描再确认；仅清理明确关联项 | 标准卸载失败或无卸载器时可进入；强制模式不能绕过预览、管理员和保护路径；dry-run 有测试 | 本轮已完成 |
| F-05 | P0 | 批量卸载队列 | BCU/IObit/Wise | 多选、串行队列、每项独立 Job、自动跳过/暂停/取消、冲突防护 | 任一程序失败不污染其他项；每项有状态/错误/报告；默认不并发操作共享系统资源 | 本轮已完成 |
| F-06 | P0 | 服务/计划任务残留 | BCU/Total/IObit | 识别明确指向目标路径的 Windows Service、Scheduled Task、Driver | 扫描可部分失败；共享/系统项标危险并默认保留；服务/任务删除前二次验证目标路径；驱动仅展示证据 | 本轮已完成 |
| F-07 | P0 | 删除备份与恢复中心 | Revo/Total/Wise | 文件、目录、注册表键/值的备份清单、恢复、失败项重试 | dry-run 与实际计划一致；恢复可验证；删除异常不留下“成功”假象 | 本轮已完成 |
| F-08 | P1 | 安装监控 | IObit/Total/Ashampoo/HiBit | 安装前快照、安装后差异、文件/注册表变更日志、绑定目标程序 | 无管理员时明确限制；大目录扫描异步；差异可筛选、导出、用于卸载证据 | 本轮已完成 |
| F-09 | P1 | 强制卸载入口与 Hunter/右键 | Revo/Geek/Wise | 拖入 EXE/快捷方式/窗口识别；右键菜单调用；目标预览 | 不能凭窗口标题直接删除；必须解析路径并展示目标、来源、风险和确认 | 本轮已完成 |
| F-10 | P1 | 报告导出与历史 | Total/Wise/BCU | JSON/HTML/文本报告、失败项导出、历史任务列表、重开报告 | 导出内容来自不可变任务快照；敏感路径不上传；导出失败可见 | 本轮已完成 |
| F-11 | P1 | 便携/可部署分发 | Geek/Wise/BCU | 便携构建、配置/缓存位置、无安装运行、ARM64/Win32 兼容说明 | 便携版不写入工作区以外的隐式状态；安装版和便携版行为一致 | 本轮已完成实现；真实 Tauri 包构建受 ARM64 GNU windres 环境阻塞 |
| F-12 | P2 | 软件健康与更新提示 | IObit/Wise/BCU 评分 | 过期/重复/最近使用/启动影响展示；只提示，不静默升级 | 所有评分有来源和时间；不把营销/遥测混入核心卸载 | 本轮已完成；更新判断保持手动入口 |
| F-13 | P2 | 工具箱收敛 | IObit/HiBit/旧 Your Uninstaller | 复用已有系统清理、浏览器插件、自启动页，补搜索、权限提示、空状态 | 工具页不绕过各自安全模型；导航和主界面 Fluent 风格一致 | 部分完成 |

## 4. 当前执行顺序

按依赖关系，不按营销数量堆功能：

1. F-04 强制/自定义卸载：已完成，已把“找不到或坏掉的程序”纳入同一个安全计划模型。
2. F-05 批量卸载队列：已完成，复用单项 Job，保证每项隔离并保留独立结果；批量模式默认跳过残留删除。
3. F-06 服务/计划任务扫描：已完成，扩充了服务、计划任务和驱动证据来源，但没有扩大默认删除范围。
4. F-07 删除备份与恢复：已完成；清理前建立持久会话，文件/目录/注册表键值使用同一份只读计划，恢复校验且不覆盖新内容，失败项目可重试。
5. F-08 安装监控：已完成；范围限制为程序安装目录、相关用户数据、卸载键、精确推导注册表键和用户追加目录，快照在后台执行并保留读取警告。
6. F-09 强制卸载入口：已完成；支持原生拖放、猎手模式获取前台窗口 EXE、当前用户右键菜单和 `--force-uninstall` 启动参数，均回到同一份安全计划。
7. F-10 报告导出与历史：已完成；终态卸载任务自动保存不可变快照，历史页支持重开详情、失败项审查和 JSON/HTML/TXT 导出。
8. F-11 便携/可部署分发：已完成实现；`--portable`/`portable.flag` 统一状态根目录，便携实例不注册产品计划任务，ZIP 脚本和多架构发布说明已补齐；真实 Tauri 包需在可用的 Windows 资源编译工具链上验证。
9. F-12 软件健康与更新提示：已完成；健康结果标注本机证据和评估时间，覆盖缺卸载器、位置不可读、重复条目、自启动影响、最近使用和手动更新页，不联网不自动升级。
10. F-13 工具箱收敛：下一阶段补已有清理、自启动和浏览器模块的统一入口与空状态。

## 5. 参考资料与限制

- [Revo Uninstaller Pro 产品页](https://www.revouninstaller.com/products/revo-uninstaller-pro/) 与 [官方支持页](https://www.revouninstaller.com/support/)
- [IObit Uninstaller 官方产品页](https://www.iobit.com/en/advanceduninstaller.php) 与 [官方手册](https://www.iobit.com/product-manuals/iu-help/)
- [Geek Uninstaller 官方产品页](https://geekuninstaller.com/?lang=en)
- [Bulk Crap Uninstaller 官方产品页](https://www.bcuninstaller.com/)
- [Total Uninstall 官方功能页](https://www.martau.com/document/total-uninstall.php) 与 [安装监控说明](https://www.martau.com/document/installation-monitor.php)
- [Wise Program Uninstaller 官方用户指南](https://www.wisecleaner.com/wise-program-uninstaller-user-guide.html)、[版本更新](https://www.wisecleaner.com/update-wiseprogramuninstaller.html)

厂商产品页是营销与功能声明，不等同于独立效果基准；后续应使用仓库中的 Inno/MSI/普通 Win32 测试夹具，对“能否发现、能否等待、误报率、删除后是否可恢复”做可重复测试。任何未经测试的“比竞品更彻底”都不应写入产品承诺。
