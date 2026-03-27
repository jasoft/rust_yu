# 卸载路由分流设计

## 背景

当前 `yu uninstall <target>` 的搜索逻辑会统一查找已安装程序，但执行路径仍然近似为“拿到卸载字符串后直接执行”。这对 `legacy` 程序基本可用，但对 `MSI` 和 `Store` 程序并不可靠。

已确认的实际问题：

- `OpenAI.ChatGPT-Desktop` 在当前机器上属于 `Store` 应用。
- 现有 `uninstall` 查找逻辑默认只走标准源，未覆盖 `Store`，导致命中失败。
- 即使后续命中 `Store` 程序，`Store` 的完成判定和残留策略也不能直接复用传统程序逻辑。

本设计将“统一搜索”和“分流处理”分开：搜索可以聚合，执行必须按类型独立处理。

## 目标

- 让 `yu uninstall chatgpt` 能正确命中 `Store` 应用并走正确卸载路径。
- 将卸载处理明确拆分为 `legacy`、`msi`、`store` 三条独立执行链。
- 让不同类型程序拥有各自的命令构造、完成判定、错误语义和残留策略。
- 保持 CLI 调用入口不变，避免用户为了不同应用类型切换命令。

## 非目标

- 本次不引入新的前端交互。
- 本次不新增 `winget`、`portable` 等新的卸载类型。
- 本次不扩展 `Store` 应用的激进残留清理，只做保守处理。
- 本次不重构整套列表缓存结构，只做兼容性扩展。

## 核心原则

- 统一搜索：匹配目标程序时同时搜索 `legacy`、`msi`、`store`。
- 分流执行：命中目标后，必须按卸载类型调用不同处理器。
- 分流校验：不同类型程序采用不同的完成判定规则。
- 分流残留策略：不同类型程序采用不同的残留扫描与清理边界。
- 安全优先：所有删除和卸载流程继续要求显式错误传播，不使用 `unwrap()`/`expect()` 处理关键路径。

## 术语

### legacy

所有非 `MSI`、非 `Store` 的传统卸载器，通常来自注册表的 `UninstallString` 或 `QuietUninstallString`。

### msi

通过 MSI 产品数据库或等价 MSI 元数据发现的程序，卸载命令统一归一为 `msiexec /x ...` 语义。

### store

通过 `Get-AppxPackage` 枚举出的 AppX/MSIX/微软商店程序，卸载语义为移除对应包，而不是运行传统卸载器。

## 数据模型调整

在 `InstalledProgram` 上保留现有 `install_source`，新增 `uninstall_kind`：

- `Legacy`
- `Msi`
- `Store`

约束如下：

- `install_source` 表示该程序从哪个扫描源被发现，用于展示、缓存和来源过滤。
- `uninstall_kind` 表示后续必须使用哪条卸载逻辑。
- 两者不能混用，也不能依赖 `uninstall_string` 内容反推卸载类型。

赋值规则：

- `store::list_store_apps()` 产物统一标记为 `uninstall_kind = Store`
- `msi::list_msi_products()` 产物统一标记为 `uninstall_kind = Msi`
- `registry::list_registry_programs()` 产物统一标记为 `uninstall_kind = Legacy`

## 搜索与命中规则

`uninstall` 命令的程序搜索保持统一入口，但必须覆盖三类程序：

- 首先聚合 `legacy + msi + store` 列表
- 然后沿用现有精确匹配优先、模糊匹配次之的策略
- 一旦命中，返回包含 `uninstall_kind` 的 `InstalledProgram`

要求：

- 搜索逻辑不能再默认只扫描标准源。
- `Store` 程序必须能像普通程序一样通过名称关键词命中。
- 同名程序如来自不同类型，仍按现有“多匹配则报错”原则处理，避免误卸载。

## 执行架构

`src/commands/uninstall.rs` 保留为统一入口，只负责：

- 参数解析
- 目标搜索与展示
- 管理卸载流程阶段输出
- 根据 `uninstall_kind` 调度对应处理器
- 在处理器返回后决定是否进入残留扫描

新增独立卸载模块：

- `src/modules/uninstall/mod.rs`
- `src/modules/uninstall/legacy.rs`
- `src/modules/uninstall/msi.rs`
- `src/modules/uninstall/store.rs`

可复用的通用进程等待、退出码分类、管理员权限校验等能力继续放在 `common` 或 `uninstall` 公共层，但不能把三类程序重新揉回统一逻辑。

## 三类处理链设计

### legacy 处理链

适用对象：

- 所有非 `MSI`、非 `Store` 的传统安装程序

执行规则：

- 优先使用 `QuietUninstallString`
- 回退使用 `UninstallString`
- 继续使用现有交互式卸载提示和 Job Object 进程链等待逻辑

完成判定：

- 注册表程序项已消失
- 若存在安装目录，则安装目录已不存在

失败语义：

- 用户取消、用户中断、超时、退出码异常继续沿用现有语义

残留策略：

- 可以继续扫描传统程序常见残留位置
- 包括注册表、`Program Files`、`AppData`、快捷方式等

### msi 处理链

适用对象：

- 所有 `uninstall_kind = Msi` 的程序

执行规则：

- 不直接信任原始字符串的维护模式参数
- 统一规范成真正的 `msiexec /x` 卸载语义
- 自动补全 `/quiet /norestart`

完成判定：

- MSI/注册表对应条目消失
- 安装目录只作为辅助手段，不作为唯一判定依据

失败语义：

- 明确按 MSI 退出码解释
- `1602` 视为用户取消
- `1641`、`3010` 视为成功但需要重启

残留策略：

- 比 `legacy` 更保守
- 不因目录残留就直接判定卸载失败

### store 处理链

适用对象：

- 所有 `uninstall_kind = Store` 的程序

执行规则：

- 不再把拼接好的 `powershell -Command "Remove-AppxPackage ..."` 当主入口逻辑
- 直接根据包标识构造专用 Store 卸载命令
- 卸载器核心语义是移除 Appx/MSIX 包

完成判定：

- 包是否仍存在是唯一主判定条件
- 不能再要求 `WindowsApps` 路径消失才算成功

失败语义：

- 命令执行失败时，错误信息必须包含包标识和包存在性结果
- 如因权限、系统限制或包状态导致失败，需要返回明确错误

残留策略：

- 默认不复用 `legacy` 的全量残留扫描
- 只允许后续扩展为保守的用户数据扫描
- 当前版本即使启用 `--clean`，也必须避免误删 `Store` 容器或系统目录

## 安全与权限

- 三类处理链都保留管理员权限校验。
- `Store` 卸载同样可能需要管理员上下文，不能绕过权限检查。
- 涉及删除和状态判定的关键路径禁止使用 `unwrap()` / `expect()`。
- 对于 `Store` 和 `MSI`，不允许因为目录还存在就误判失败。

## 测试策略

先写失败测试，再补实现。最低覆盖如下：

### 搜索与路由

- `uninstall` 搜索能命中 `Store` 程序
- `uninstall` 搜索能命中 `MSI` 程序
- 匹配到程序后会根据 `uninstall_kind` 走对应处理器

### 数据模型

- `InstalledProgram` 默认具备合理的 `uninstall_kind`
- `store` 枚举结果的 `uninstall_kind` 为 `Store`
- `msi` 枚举结果的 `uninstall_kind` 为 `Msi`

### legacy

- 优先选择静默卸载字符串
- 交互式传统卸载器仍能走等待路径

### msi

- MSI 维护模式会被归一为卸载模式
- 退出码 `1602`、`1641`、`3010` 的解释符合预期

### store

- Store 程序不会再走 legacy 完成判定
- Store 完成判定仅依赖包存在性
- Store 卸载命令构造不依赖缓存里的原始字符串格式

## 实施顺序

1. 为 `InstalledProgram` 增加 `uninstall_kind`
2. 调整枚举逻辑，确保三类程序在聚合搜索中带上正确类型
3. 为 `uninstall` 命令补失败测试，先复现 `chatgpt` 无法卸载的问题
4. 拆分三类卸载处理器模块
5. 将 `uninstall` 入口改为统一搜索、按类型分流
6. 替换 `Store` 的完成判定逻辑
7. 调整 `MSI` 的完成判定与命令归一逻辑
8. 验证 CLI 行为、格式化与测试

## 风险与缓解

风险：

- 聚合搜索后，同名程序来自不同类型时可能增加歧义
- `Store` 包存在性检查如果实现过于依赖 PowerShell 输出格式，可能有兼容性问题
- 现有缓存如果缺少新字段，可能触发反序列化兼容问题

缓解：

- 歧义匹配保持失败优先，不自动猜测
- `Store` 状态检查封装为专用函数，统一处理输出解析
- 对新增字段使用兼容反序列化默认值，避免旧缓存直接失效崩溃

## 验收标准

- 在当前机器上，`yu uninstall chatgpt` 能命中 `OpenAI.ChatGPT-Desktop`
- `Store` 程序卸载成功后，不因 `WindowsApps` 目录仍存在而误报失败
- `legacy`、`msi`、`store` 三类程序在代码结构上有独立处理器
- 现有测试继续通过，新增测试覆盖三类路由与关键判定
