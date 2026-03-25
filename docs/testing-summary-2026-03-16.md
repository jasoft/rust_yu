# 测试项目总结（2026-03-16）

## 1. 本次结论

当前仓库的测试质量比原先明显提升，但**整体覆盖率仍然不够支撑“Windows 卸载/清理工具”这一高风险场景**。

- 改造前 `cargo llvm-cov --workspace --summary-only` 的总行覆盖率约为 `34.83%`
- 本次补测后，在稳定模式下执行
  `cargo llvm-cov --workspace --summary-only --ignore-filename-regex "(src-frontends\\webui\\dist|src-tauri\\gen)" -- --test-threads=1`
  的总行覆盖率提升到 `43.40%`
- 总函数覆盖率从 `26.70%` 提升到 `35.37%`

这说明当前“列表/元数据/部分启动项”已经有一定单测基础，但**真正涉及系统扫描、清理执行、Tauri 命令桥接、报告输出、注册表读取/删除、文件删除**的测试仍然明显不足。

## 2. 我检查了现有测试在做什么

### 2.1 原有测试的主要覆盖面

原有测试主要集中在以下几类：

1. CLI 参数与命令行行为
   - `src/commands/list.rs`
   - `src/commands/prepare.rs`
   - `src/commands/startup.rs`
   - `src/commands/uninstall.rs`
   - `src/main.rs`

   这部分主要验证：
   - 列表命令参数是否接受 `--refresh`、升降序等开关
   - `prepare` 流式输出参数是否稳定
   - `startup` 子命令的 CLI 旗标是否可解析
   - `uninstall` 的参数默认值、程序匹配策略、静默卸载字符串优先级

2. 通用工具逻辑
   - `src/modules/common/utils.rs`
   - `src/modules/common/process.rs`

   这部分主要验证：
   - MSI/EXE 卸载命令规范化
   - Windows 路径和参数拆分
   - 交互式卸载识别
   - 常见退出码分类

3. 已安装程序列表的元数据层
   - `src/modules/lister/enrichment.rs`
   - `src/modules/lister/models.rs`
   - `src/modules/lister/storage.rs`

   这部分是原仓库测试最完整的一块，覆盖了：
   - 安装日期标准化
   - 图标路径解析与缓存文件生成
   - 尺寸预热
   - 缓存读写和 schema 校验
   - 卸载字符串优先级

4. 启动项模块的一部分
   - `src/modules/startup/models.rs`
   - `src/modules/startup/manager.rs`
   - `src/modules/startup/registry_run.rs`
   - `src/modules/startup/rollback.rs`
   - `src/modules/startup/startup_approved.rs`
   - `src/modules/startup/startup_folder.rs`

   这部分主要覆盖：
   - 启动项模型、能力矩阵、编码/解码
   - 注册表 Run 项采集
   - 回滚记录持久化
   - Startup Approved 状态读写
   - 启动文件夹采集

### 2.2 原有测试的明显空白

在本次补测前，下列模块基本没有测试，或者覆盖率接近 0：

- `src/modules/scanner/*`
- `src/modules/cleaner/*`
- `src/modules/reporter/*`
- `src/modules/lister/mod.rs`
- `src/modules/lister/msi.rs`
- `src/modules/lister/store.rs`
- `src-tauri/src/commands/*`
- `src-tauri/src/lib.rs`

这类空白对于卸载工具尤其危险，因为它们靠近：

- 痕迹识别与置信度分配
- 删除前安全校验
- HTML 报告输出
- Tauri 前后端命令参数转换
- 平台枚举解析（MSI / Store）

## 3. 本次新增了哪些测试

### 3.1 新增测试文件覆盖点

本次新增测试覆盖了以下模块：

1. `src/modules/lister/mod.rs`
   - 名称/发布者搜索过滤
   - 去重排序逻辑
   - 非可缓存来源时的缓存状态
   - 元数据预热未选择任务时的错误返回
   - `Unknown` 来源下的预热进度和非缓存状态

2. `src/modules/scanner/mod.rs`
   - 置信度分级：精确命中 / 模糊命中 / 不命中
   - 关键系统路径和关键注册表路径标记

3. `src/modules/scanner/models.rs`
   - `TraceType` 显示文本
   - `Trace` builder 方法

4. `src/modules/cleaner/safety.rs`
   - 已标记关键项拒删
   - 系统目录拒删
   - 关键注册表项拒删
   - 普通路径允许通过

5. `src/modules/reporter/models.rs`
   - `with_results` 是否正确累计释放空间
   - `success` 状态是否正确汇总
   - warning 附加行为

6. `src/modules/reporter/html.rs`
   - 空结果占位输出
   - 类型 badge 渲染
   - HTML 转义行为

7. `src/modules/lister/store.rs`
   - Store 应用 JSON 数组解析
   - Store 单对象 JSON 解析

8. `src/modules/lister/msi.rs`
   - MSI JSON 数组解析
   - MSI 单对象 JSON 解析

9. `src-tauri/src/commands/list.rs`
   - 来源解析别名
   - 非法 selector 拒绝

10. `src-tauri/src/commands/startup.rs`
    - source/scope/state/action 的解析
    - 非法 selector 拒绝

### 3.2 顺手修复的一个问题

在补 `reporter/html` 测试时发现，报告模板对 `program_name` 没做 HTML 转义，存在明显的输出安全问题。本次一起修复为统一转义：

- `src/modules/reporter/html.rs`

这属于真实缺陷修复，不只是“为了让测试通过”。

## 4. 覆盖率数据

### 4.1 总体覆盖率

| 指标 | 补测前 | 补测后 |
| --- | ---: | ---: |
| Regions | 32.22% | 40.87% |
| Functions | 26.70% | 35.37% |
| Lines | 34.83% | 43.40% |

> 说明：补测前的数据来自本次第一次 `llvm-cov` 执行结果；补测后的数据来自单线程稳定执行结果。

### 4.2 这次提升最明显的模块

| 文件 | 补测前行覆盖 | 补测后行覆盖 |
| --- | ---: | ---: |
| `src/modules/cleaner/safety.rs` | 0.00% | 85.07% |
| `src/modules/lister/mod.rs` | 0.00% | 66.77% |
| `src/modules/lister/msi.rs` | 0.00% | 60.42% |
| `src/modules/lister/store.rs` | 0.00% | 64.00% |
| `src/modules/reporter/html.rs` | 0.00% | 98.08% |
| `src/modules/reporter/models.rs` | 0.00% | 92.00% |
| `src/modules/scanner/models.rs` | 0.00% | 80.00% |
| `src/modules/scanner/mod.rs` | 0.00% | 37.82% |
| `src-tauri/src/commands/list.rs` | 0.00% | 18.35% |
| `src-tauri/src/commands/startup.rs` | 0.00% | 34.20% |

### 4.3 目前仍然明显不足的模块

以下模块在本次补测后依然接近 0 或仍偏低：

#### 0% 行覆盖

- `src/modules/cleaner/filesystem.rs`
- `src/modules/cleaner/mod.rs`
- `src/modules/cleaner/registry.rs`
- `src/modules/cleaner/shortcuts.rs`
- `src/modules/scanner/appdata.rs`
- `src/modules/scanner/filesystem.rs`
- `src/modules/scanner/registry.rs`
- `src/modules/scanner/shortcuts.rs`
- `src/modules/lister/registry.rs`
- `src/modules/startup/scheduled_tasks.rs`
- `src/modules/startup/services.rs`
- `src/commands/clean.rs`
- `src/commands/report.rs`
- `src/commands/search.rs`
- `src-tauri/src/commands/report.rs`
- `src-tauri/src/commands/scan.rs`
- `src-tauri/src/commands/search.rs`
- `src-tauri/src/commands/uninstall.rs`
- `src-tauri/src/lib.rs`

#### 已有一定覆盖，但对高风险场景仍偏低

- `src/modules/common/process.rs` `29.51%`
- `src/commands/uninstall.rs` `22.60%`
- `src/commands/startup.rs` `10.10%`
- `src/modules/startup/manager.rs` `17.22%`
- `src/modules/startup/startup_folder.rs` `39.75%`
- `src/modules/startup/registry_run.rs` `42.04%`

## 5. 覆盖率是否足够

结论：**不够。**

原因不是“数字不够好看”，而是下面这些高风险代码仍未被充分验证：

1. 真正执行删除的路径覆盖不足
   - 文件删除
   - 注册表删除
   - 快捷方式删除
   - `clean_traces` 聚合行为

2. 真正扫描系统痕迹的路径覆盖不足
   - 文件系统扫描
   - AppData 扫描
   - 注册表扫描
   - 快捷方式扫描

3. 前后端桥接覆盖不足
   - Tauri `command` 仍以解析辅助函数测试为主
   - 真正的命令包装、错误映射、异步边界、事件发送没有被覆盖

4. Windows 平台依赖行为缺少隔离测试
   - `services.rs`
   - `scheduled_tasks.rs`
   - `lister/registry.rs`
   - `startup/manager.rs` 的实际计划与执行路径

对于普通 CRUD 项目，43% 行覆盖可能还能接受；但对“会删文件、删注册表、运行卸载命令”的工具来说，这个覆盖率仍然偏低。

## 6. 现阶段测试结构的实际问题

### 6.1 `cargo test` 里的测试数量有重复

当前 `cargo test --workspace` 会看到：

- `rust-yu` lib 测试
- `yu` bin 测试
- `rust-yu-tauri` lib 测试

其中 `src/main.rs` 又重新把 `commands`、`modules` 作为 bin 内模块编进来，因此同一批命令/模块测试会在 lib 和 bin 下各跑一次，造成：

- 测试总数看起来很多
- 但实际“独立测试面”没有数量显示得那么大

### 6.2 存在并发脆弱测试

本次 `llvm-cov` 默认并发执行时，出现了两处老测试失败：

- `modules::lister::enrichment::tests::build_icon_assets_generates_cache_paths_for_image_file`
- `modules::startup::rollback::tests::disabled_entry_roundtrip_supports_listing_and_removal`

改为 `-- --test-threads=1` 后恢复稳定。

这说明当前部分测试存在共享环境或共享临时目录的问题，典型风险包括：

- 依赖全局环境变量
- 使用固定的临时目录命名
- 使用共享缓存目录

这类问题虽然不影响普通 `cargo test` 必然通过，但会影响：

- 覆盖率统计
- CI 稳定性
- 后续并行测试扩展

## 7. 下一步建议怎么补

建议按风险优先级继续补，而不是平均撒网。

### P0：必须优先补

1. `src/modules/cleaner/mod.rs`
   - `dry_run` 行为
   - 多类 trace 聚合删除结果
   - 关键项被拦截后的行为

2. `src/modules/cleaner/filesystem.rs`
   - 文件不存在
   - 目录删除
   - 释放空间统计
   - 权限错误传播

3. `src/modules/cleaner/registry.rs`
   - 键删除和值删除分支
   - 关键路径拒删
   - 注册表路径解析失败

4. `src/modules/scanner/{filesystem,appdata,registry,shortcuts}.rs`
   - 保守匹配策略
   - 系统目录过滤
   - 低置信度/高置信度边界
   - “不确定则不删”的输入数据基础

### P1：第二优先级

5. `src/modules/lister/registry.rs`
   - 注册表项解析
   - SystemComponent 过滤
   - DisplayName 缺失时跳过

6. `src/modules/startup/manager.rs`
   - 查询过滤
   - 排序
   - 操作计划
   - `validate_action`

7. `src-tauri/src/commands/*`
   - `CommandError` 映射
   - 输入校验
   - `spawn_blocking` 返回错误转换
   - 事件发射逻辑

### P2：补齐集成层

8. 增加面向 Windows 的集成测试脚本
   - 安装 7-Zip
   - 安装测试 MSI
   - 扫描列表
   - 执行默认卸载/静默卸载
   - 验证残留扫描结果
   - 验证 dry-run 与真实执行差异

## 8. 本次执行的验证命令

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test --workspace
& "$env:USERPROFILE\.cargo\bin\cargo.exe" llvm-cov --workspace --summary-only --ignore-filename-regex "(src-frontends\\webui\\dist|src-tauri\\gen)" -- --test-threads=1
```

## 9. 当前状态总结

可以认为当前测试状态是：

- `列表元数据层`：相对较好
- `启动项部分数据层`：中等
- `扫描/清理执行层`：明显不足
- `Tauri 命令桥接层`：明显不足
- `报告输出层`：本次后已基本到位

如果要把这个项目推进到“可以放心做卸载/清理动作”的阶段，下一轮最应该投入的不是继续补 UI，而是把 `scanner + cleaner + startup manager + tauri command bridge` 的测试补上。
