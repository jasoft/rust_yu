# 卸载向导 fixtures E2E

`tools/test/Run-UninstallFixturesE2E.ps1` 是会真实修改当前 Windows 测试机的桌面端 E2E。它连接 Rust Yu 的 Tauri WebView2 页面，并通过用户可见的 UI 完成以下流程：

1. 请求管理员权限并执行 X64 worktree 初始化；
2. 打开开发者模式，在开发者工具页安装白名单中的 Xplorer MSI 与 RustYu Legacy Inno fixtures；
3. 回到应用列表，逐个执行确认、卸载、残留扫描、结果停留、人工“下一步”、残留复核/清理和完成报告；
4. 断言扫描完成后至少停留 1.2 秒且仍处于扫描阶段，防止再次出现自动跳页；
5. 在 `target/test-logs/uninstall-fixtures-e2e/<时间戳>/` 保存各阶段截图、Tauri 日志和 `evidence.json`；
6. 无论测试是否通过，都只按两个 fixture 的精确 MSI ProductCode、Inno 安装目录和固定 LocalAppData 目录收尾；产品保守保留的低置信度项会先进入截图与 JSON 证据，再由测试收尾删除。

运行：

```powershell
pwsh -NoProfile -File tools/test/Run-UninstallFixturesE2E.ps1
```

如果测试进程被外部中断，可只执行精确环境收尾：

```powershell
pwsh -NoProfile -File tools/test/Run-UninstallFixturesE2E.ps1 -CleanupOnly
```

测试通过 `src-tauri/tauri.e2e.conf.json` 为 E2E 窗口配置独立 WebView2 数据目录，并使用仅绑定本机的 CDP 调试端口 `9223`。环境变量也只对子进程生效，并在测试结束后恢复。GUI/编译启动等待上限为 120 秒；超过上限会终止本次已跟踪的 Tauri 进程树，符合项目的两分钟编译止损规则。
