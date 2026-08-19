# 开发 HTTP API 局域网访问

Rust Yu 的开发 HTTP API 默认只监听本机 `127.0.0.1:8080`。需要从同一局域网的其他机器访问时，在仓库根目录运行：

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\tools\dev\Run-ApiLan.ps1
```

脚本会以局域网模式启动 Tauri 开发程序，使 API 监听 `0.0.0.0:8080`。其他机器使用运行 Rust Yu 这台电脑的局域网 IPv4 地址访问，例如：

```text
http://192.168.1.20:8080/api/programs
```

也可以指定端口：

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\tools\dev\Run-ApiLan.ps1 -Port 18080
```

环境变量约定：

- `RUST_YU_ENABLE_API=1`：启用开发 HTTP API。
- `RUST_YU_API_MODE=local|lan`：选择本机或局域网模式，默认是 `local`。
- `RUST_YU_API_HOST`：显式覆盖绑定 IP；设置后优先于模式。
- `RUST_YU_API_PORT`：覆盖监听端口，默认 `8080`。

局域网模式只提供当前开发 API 的 GET 路由，不应暴露到互联网。若 Windows 防火墙阻止了其他机器连接，请在“专用网络”配置文件中允许该端口的入站 TCP 连接。
