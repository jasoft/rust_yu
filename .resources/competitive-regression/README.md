# Competitive capability regression fixture

这个夹具为 F-21 提供可重复、可量化的竞品能力回归边界。默认结构校验不修改系统；
`-RunLifecycle` 需要管理员权限，会在唯一的 ProgramData 临时目录中创建按需服务和当前
用户登录任务，验证完成后在 `finally` 中删除服务、任务和临时目录。

覆盖场景：安装证据、服务/任务精确归属、更新覆盖文件、异常退出保留部分证据、恢复门禁、
报告来源/结果与 SHA-256 一致性。指标固定为检测率、错误关联数、等待正确性和恢复成功率。

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\test\Verify-CompetitiveFeatureFixtures.ps1
powershell -ExecutionPolicy Bypass -File .\tools\test\Verify-CompetitiveFeatureFixtures.ps1 -RunLifecycle
```

夹具不会创建或启动驱动，不会启动测试服务，也不会把共享系统项作为可删除对象。
