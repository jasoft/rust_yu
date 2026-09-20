# Randomized x64 NSIS fixtures

这套夹具生成多个彼此独立的原生 AMD64 NSIS 安装器，供 Rust Yu 重复测试安装、卸载项扫描和卸载生命周期。

每个 fixture 都有随机且唯一的产品名、产品 GUID、EXE 文件名、安装目录以及 6–13 个不同路径和内容的 payload 文件。构建结果同时保存 SHA-256 清单，验证脚本不会只相信文件名或 NSIS 配置，而会读取 PE 头确认安装器和卸载器的 Machine 均为 `AMD64 (0x8664)`。

## 构建

```powershell
powershell -ExecutionPolicy Bypass -File '.\.resources\nsis-random-fixtures\Build-NsisRandomFixtures.ps1' -Count 6
```

第一次运行会下载固定版本的 negrutiu NSIS AMD64 发行版到本目录下的 `.tools`，下载文件经过固定 SHA-256 校验。官方 NSIS 通常只生成 x86 stub；这里必须使用支持 `Target amd64-unicode` 的发行版。

默认使用随机 seed。需要完全复现某一组时显式传入 seed：

```powershell
powershell -ExecutionPolicy Bypass -File '.\.resources\nsis-random-fixtures\Build-NsisRandomFixtures.ps1' -Count 6 -Seed 20260810
```

输出位于 `output\set-<seed>`，其中 `fixture-set.json` 记录每个包的名称、安装路径、卸载注册表键、文件清单和哈希。相同 seed 已存在时构建会拒绝覆盖；确认重建可加 `-Force`。

## 静态校验

```powershell
powershell -ExecutionPolicy Bypass -File '.\tools\test\Verify-NsisRandomFixtures.ps1' \
  -FixtureSetPath '.\.resources\nsis-random-fixtures\output\set-20260810\fixture-set.json'
```

## 完整安装/卸载验证

fixture 使用当前用户权限安装到 `%LocalAppData%\Programs\RustYuFixtures`，不触发 UAC；卸载项写入 HKCU 的 64 位视图。下面的命令会逐个静默安装、核对所有已安装文件及哈希、确认卸载器为 AMD64、静默卸载，并确认安装目录和卸载项均已删除：

```powershell
powershell -ExecutionPolicy Bypass -File '.\tools\test\Verify-NsisRandomFixtures.ps1' \
  -FixtureSetPath '.\.resources\nsis-random-fixtures\output\set-20260810\fixture-set.json' \
  -RunLifecycle
```

只跑指定 fixture：

```powershell
powershell -ExecutionPolicy Bypass -File '.\tools\test\Verify-NsisRandomFixtures.ps1' \
  -FixtureSetPath '.\.resources\nsis-random-fixtures\output\set-20260810\fixture-set.json' \
  -FixtureIndex 0,2 -RunLifecycle
```

生命周期失败时脚本只会兜底清理已验证处于 `%LocalAppData%\Programs\RustYuFixtures` 下的当前 fixture 目录和对应 HKCU 卸载项。

原生 AMD64 NSIS 卸载器在部分 Windows ARM64/x64 仿真环境中可能短暂占用自身。因此卸载器最后会异步释放一个非提权收尾脚本；它只删除产品目录中固定名为 `uninstall.exe` 的文件，并只对已经为空的产品目录执行非递归 `rd`，不会递归删除任意目标。
