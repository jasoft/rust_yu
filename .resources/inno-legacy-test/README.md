# Inno Setup Legacy Test Fixture

这个目录提供一个可复编译、并随仓库保存已生成产物的 legacy 非 MSI 测试安装器，用于验证：

- 标准 Win32 卸载项枚举
- `QuietUninstallString` 优先使用
- 卸载命令先派生子进程再退出，便于验证 application workflow 的 Job Object 等待
- 卸载后的安装目录残留
- 卸载后的 `LocalAppData` 残留

## 目录结构

- `LegacyUninstallTest.iss`: Inno Setup 主脚本
- `payload/`: 安装时复制的样例文件
- `output/`: 已生成并随仓库保存的安装包输出目录
- `tools/SpawnUninstallHelper.rs`: 原生卸载包装器源码
- `Build-InnoLegacyFixture.ps1`: 统一编译 helper 和安装包

## 卸载进程模型

- 注册表中的 `UninstallString` 和 `QuietUninstallString` 都不会直接执行 `unins000.exe`
- 它们会先调用 `SpawnUninstallHelper.exe`
- `SpawnUninstallHelper.exe` 会再派生 `UninstallWorker.ps1` 并立即退出
- `UninstallWorker.ps1` 会延迟启动真正的 `unins000.exe`，然后等待卸载结束

这个链式模型由 `tests/windows_uninstall_lifecycle.rs` 验证：父进程已退出时，application workflow 仍依赖 Job Object 等待整条卸载进程链结束

## 使用现成安装包

正常验证直接使用以下已生成文件，不需要每次重新编译：

```text
.resources\inno-legacy-test\output\RustYuLegacyTestSetup.exe
```

结构校验和生命周期测试都会读取这个文件。只有修改了 `LegacyUninstallTest.iss`
或卸载 helper，需要刷新测试夹具时，才显式运行下面的构建脚本。

## 重新编译（仅在需要刷新夹具时）

```powershell
powershell -ExecutionPolicy Bypass -File '.\.resources\inno-legacy-test\Build-InnoLegacyFixture.ps1'
```

默认会生成：

```text
.resources\inno-legacy-test\output\RustYuLegacyTestSetup.exe
```

结构校验：

```powershell
powershell -ExecutionPolicy Bypass -File '.\tools\test\Verify-InnoLegacyFixture.ps1'
```

生命周期校验需要管理员 PowerShell：

```powershell
powershell -ExecutionPolicy Bypass -File '.\tools\test\Verify-InnoLegacyFixture.ps1' -RunLifecycle
```

## 安装

GUI 安装：

```powershell
Start-Process '.\.resources\inno-legacy-test\output\RustYuLegacyTestSetup.exe' -Wait
```

静默安装：

```powershell
Start-Process '.\.resources\inno-legacy-test\output\RustYuLegacyTestSetup.exe' -ArgumentList '/VERYSILENT','/NORESTART' -Verb RunAs -Wait
```

## 卸载

静默卸载：

```powershell
Start-Process 'C:\Program Files\RustYu Legacy Test App\unins000.exe' -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -Verb RunAs -Wait
```

也可以直接读取卸载注册表中的 `QuietUninstallString`。

推荐用 application workflow 的 Windows 集成测试走完整 waitforjobs 验证：

```powershell
cargo test --test windows_uninstall_lifecycle -- --ignored --nocapture
```

## 预期残留

卸载完成后，以下路径应仍然存在：

- `C:\Program Files\RustYu Legacy Test App\logs\leftover.log`
- `%LocalAppData%\RustYuLegacyTest\Data\leftover-user-profile.json`

其余主程序文件和快捷方式应被卸载器删除。
