# 文件粉碎器设计说明

## GitHub 调研

实现前按 GitHub star、Windows 适配度、维护状态和安全设计筛选了以下项目：

- [devs-krd/permadelete](https://github.com/devs-krd/permadelete)（MIT）：Windows GUI、递归目录、取消/进度、覆盖后截断、随机改名、重解析点与 SSD 提示。
- [bleachbit/bleachbit](https://github.com/bleachbit/bleachbit)（GPL-3.0）：将文件粉碎作为隐私清理产品中的独立能力，并明确区分普通删除和覆盖删除。
- [Kostassoid/lethe](https://github.com/Kostassoid/lethe)（Apache-2.0）：Rust 分块写入、阶段化进度、覆盖后回读验证和失败重试思路。
- [laurentiu021/SystemManager](https://github.com/laurentiu021/SystemManager)（MIT）：Windows 关键路径保护、同一句柄完成覆盖与截断、硬链接拒绝、目录重解析点防护和清晰的失败结果。

本项目没有复制上述项目源码；实现使用 Rust Yu 自身的数据结构、错误模型、Tauri 命令和 Fluent UI。调研只用于确定产品能力和安全边界。

## 集成功能

- 左侧导航新增“文件粉碎”，支持多选文件和文件夹。
- 快速（1 遍）、标准（3 遍）和彻底（7 遍）三种策略。
- 先分析再执行；计划令牌绑定规范路径、文件大小、最后写入时间、卷序列号和文件索引。
- 执行前重建整个计划，并在打开的文件句柄上再次核对文件身份。
- 每遍覆盖后强制落盘并回读逐字节验证；随后截断、随机改名三次并删除。
- 递归目录不跟随链接，从最深层开始仅删除空目录；失败文件不会退化为普通删除。
- 阻止磁盘根目录、Windows、Program Files、ProgramData、本应用目录、重解析点和多硬链接文件。
- Tauri 后台阻塞任务执行，事件流报告逐遍进度；前端必须输入“永久粉碎”才能执行。
- 20,000 文件上限，避免一次误选导致无界扫描和长时间占用。

## 安全与介质限制

文件级覆盖主要面向传统机械硬盘。SSD 磨损均衡、TRIM、控制器缓存、文件系统日志、卷影副本、备份和云同步可能保留数据。UI 必须持续显示该限制；对极高敏感数据应使用存储设备厂商提供的整盘 Secure Erase / Sanitize 能力。

粉碎属于不可恢复操作，没有备份或撤销机制。安全模型因此采用“选择 → 只读分析 → 展示精确范围 → 输入确认短语 → 重新验证 → 执行”的强确认流程，而不是乐观删除。

## 验证

- Rust 单元测试使用真实临时文件验证计划去重、计划变化拒绝、dry-run 保留，以及覆盖/校验/删除完整链路。
- Tauri `cargo check` 验证命令、事件、Dialog 插件和 ACL 清单。
- WebUI 测试、lint 和 production build 验证类型与页面集成。
