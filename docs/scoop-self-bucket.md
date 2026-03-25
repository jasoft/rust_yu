# Scoop 自建仓库发布 `yu.exe`

当前仓库已经包含可直接被 Scoop 使用的 bucket 清单：

- `bucket/yu.json`

当前版本对应的发布资产：

- `dist/scoop/yu-0.1.0-windows-amd64.zip`
- SHA256: `1d217a4f282194c101c2991ae9b9c3586f4f75e9e447864ac8c8618b5bd45102`

## 方案

直接把当前源码仓库同时作为 Scoop bucket 仓库使用。

这样发布后，用户可以直接执行：

```powershell
scoop bucket add rust-yu https://github.com/jasoft/rust_yu
scoop install rust-yu/yu
```

## 首次发布步骤

1. 确认版本号一致：
   - `Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - `bucket/yu.json`
2. 构建 CLI：

```powershell
cargo build --release --bin yu
```

3. 打包发布资产：

```powershell
New-Item -ItemType Directory -Force dist\scoop | Out-Null
Compress-Archive -Path target\release\yu.exe -DestinationPath dist\scoop\yu-0.1.0-windows-amd64.zip -Force
```

4. 更新 manifest：

```powershell
powershell -ExecutionPolicy Bypass -File tools\scoop\update-manifest.ps1 -Version 0.1.0
```

5. 提交代码并打 tag：

```powershell
git add bucket/yu.json docs/scoop-self-bucket.md tools/scoop/update-manifest.ps1
git commit -m "Add Scoop bucket manifest for yu"
git tag v0.1.0
git push origin main
git push origin v0.1.0
```

6. 在 GitHub Releases 中创建 `v0.1.0` release，并上传：
   - `dist/scoop/yu-0.1.0-windows-amd64.zip`

发布资产 URL 必须与 manifest 一致：

```text
https://github.com/jasoft/rust_yu/releases/download/v0.1.0/yu-0.1.0-windows-amd64.zip
```

## 后续发版

现在可以直接执行一键发布脚本：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\release\publish-release.ps1
```

脚本会自动完成这些步骤：

- 校验 `Cargo.toml` 与 `src-tauri/tauri.conf.json` 的版本一致性
- 构建 `yu.exe`
- 生成 `dist\scoop\yu-<version>-windows-amd64.zip`
- 更新 `bucket/yu.json`
- 如有变更则自动提交 manifest
- 创建或校验 `v<version>` tag
- 推送当前分支与 tag
- 创建或更新对应的 GitHub Release，并上传 zip 资产

演练时可先执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\release\publish-release.ps1 -DryRun -AllowDirty -SkipBuild -SkipPush -SkipRelease
```

如果只想单独刷新 Scoop manifest，也可以继续用原来的脚本：

```powershell
powershell -ExecutionPolicy Bypass -File tools\scoop\update-manifest.ps1 -Version <new-version>
```

## 验证

发布完成后可以本地验证：

```powershell
scoop bucket add rust-yu https://github.com/jasoft/rust_yu
scoop update
scoop install rust-yu/yu
yu --help
```

如果已经装过旧版本，可执行：

```powershell
scoop update yu
```
