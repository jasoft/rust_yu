# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

这是一个 **Windows 卸载工具 CLI 应用** (rust-yu)，用于列出已安装程序、搜索残留痕迹、清理并生成报告。

## Build & Run Commands

```bash
# 构建项目
cargo build

# 运行（带参数示例）
cargo run -- list
cargo run -- search "Visual Studio"
cargo run -- clean "VSCode" --preview
cargo run -- clean "App" --confirm --report

# 运行测试
cargo test

# 单个测试
cargo test <test_name>

# 代码检查
cargo clippy
cargo fmt
```

## CLI Commands

| Command  | Description        |
| -------- | ------------------ |
| `list`   | 列出所有已安装程序 |
| `search` | 搜索程序残留痕迹   |
| `clean`  | 清理程序残留痕迹   |
| `report` | 查看卸载报告       |

## Architecture

```
src/
├── main.rs          # CLI 入口，使用 clap 解析命令
├── lib.rs           # 库入口，导出模块
├── commands/        # 命令层（list, search, clean, report）
└── modules/         # 核心业务逻辑
    ├── lister/      # 列出已安装程序（注册表、MSI、商店）
    ├── scanner/     # 扫描残留痕迹（注册表、文件、AppData、快捷方式）
    ├── cleaner/     # 清理残留痕迹
    ├── reporter/    # 生成 HTML 报告
    └── common/      # 公共工具（日志、错误处理）
```

## Safety Rules

- **No unwrap**: 涉及文件删除、注册表修改的代码路径严禁使用 `unwrap`，必须使用 `Result<T, E>` 传播错误
- **Dry Run Mode**: 删除逻辑必须支持预览模式（只列出不执行），便于用户确认
- **Privilege Checks**: 执行清理前检查管理员权限

## Windows API Usage

- 注册表操作：`winreg` crate
- 系统 API：`windows-rs` crate
- 异步扫描：在 `tokio::spawn` 中运行，避免阻塞主线程

## 提交代码

- 用cargo clippy检验代码, 修复所有的warnings和 errors
- 检验通过后运行 /commit-command:commit 命令提交代码
