# 深度分析真值夹具

这组夹具专门验证 Rust Yu 的只读 APP 深度分析，不依赖真实软件内部结构，也不把“看起来合理”当成正确。每个安装器写入一组已知文件、注册表值和 Windows 集成项，`expected-traces.json` 则逐条声明扫描器必须返回的路径、类型、最低分数、证据原因和保护状态。

## 三种安装画像

- `deep-nsis-machine`：X64、机器级 NSIS。覆盖 Program Files、ProgramData、公共开始菜单、公共桌面、HKLM Uninstall、App Paths、Run、URL 协议、环境变量、SharedDLLs、服务和禁用的计划任务。
- `deep-nsis-user`：X64、当前用户 NSIS。覆盖 LocalAppData Programs、Roaming、Local、LocalLow、当前用户开始菜单和桌面的 `.lnk`/`.url`、HKCU Uninstall、App Paths、Run、URL 协议和环境变量。
- `deep-nsis-sparse`：X64、当前用户 NSIS。故意省略 `InstallLocation`、`DisplayIcon`、静默卸载命令和大小，且把卸载器放在独立目录，用来验证扫描器仍能从标准用户安装目录的精确名称发现全部嵌套文件。

三者都使用仓库内已有的 AMD64 NSIS 编译器；载荷由 `x86_64-pc-windows-msvc` Rust 编译器生成。构建脚本会验证所有四个 EXE 的 PE Machine 都是 `0x8664`。

## 构建与结构校验

```powershell
.\.resources\nsis-random-fixtures\Build-Fixtures.ps1
.\.resources\nsis-random-fixtures\Verify-Fixtures.ps1
```

构建输出位于 `output/`，并作为可复现的测试资源提交。默认验证只读取文件，不安装、不卸载、不修改注册表。

## 安装并执行只读真值分析

当前用户夹具不需要提权：

```powershell
.\.resources\nsis-random-fixtures\Verify-Fixtures.ps1 -InstallUserFixtures -RunAnalysis
```

机器级夹具需要管理员 PowerShell：

```powershell
.\.resources\nsis-random-fixtures\Verify-Fixtures.ps1 -InstallMachineFixture -RunAnalysis
```

`-RunAnalysis` 只调用扫描器并对照真值，不调用清理器或卸载器。脚本故意不会自动卸载夹具，以确保“分析测试”与“删除授权”完全分离。需要移除时，必须由测试人员在 Windows 设置或 Rust Yu 中明确选择对应夹具并执行正常卸载。

## 真值失败的含义

测试失败不是通过放宽匹配规则解决。应先判断是哪一层出现偏差：

1. 安装器没有创建声明的目标；
2. 已安装程序清单没有保留必要上下文；
3. 扫描位置遗漏或扫描被截断；
4. 路径存在但类型、分数、证据原因或保护状态错误；
5. 某个扫描区失败/跳过，而 UI 仍错误地声称扫描完整。

`tests/deep_analysis_fixtures.rs` 会逐项核对以上契约，并拒绝任何扫描区报告 `failed` 或 `skipped`。系统 Classes 分支达到明确上限时允许 `partial`，但 UI 必须如实展示，且固定真值仍须全部命中。
