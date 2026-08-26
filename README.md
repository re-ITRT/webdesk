# WebLaunch — 把任意 Web 应用启动/托管为原生桌面应用

> 轻量、高可控的 Web 应用桌面化运行与管理平台。把 Web 应用以"原生桌面应用"形态呈现：独立任务栏图标、后台驻留、生命周期钩子、命令授权桥接、桌面快捷方式，**四平台**（Windows / macOS / Linux / 鸿蒙）。

[中文](./README.md) | English: [README.en.md](./README.en.md)

## WebLaunch 是什么

把任意 Web 应用当作"原生桌面应用"来运行与管理：

- **单引擎渲染**（Tauri 2 + 各平台系统 WebView）
- **每应用独立任务栏**：独立 AppUserModelID + 独立图标，与 WebLaunch 主进程分开
- **生命周期钩子**：启动前 / 关闭后脚本钩子（支持直接填 bat 代码）
- **命令授权桥**：网页可安全请求执行本地命令，首次弹授权框，可选"不再提示"
- **后台驻留**：关窗隐藏不销毁，WebSocket 自然保活
- **桌面快捷方式**：一键创建，图标绑定到应用本身
- **Web 管理控制台**：平台自身的第一个内置应用（吃自己的狗粮）

## 核心特性

- **单引擎渲染**：Windows=WebView2 / macOS=WKWebView / Linux=WebKitGTK / 鸿蒙=ArkWeb。
- **应用级身份隔离**：cookie / 密钥 / 扩展按应用隔离。
- **生命周期钩子**：启动前 / 关闭后，阻塞/超时/退出码，支持 bat 脚本。
- **命令授权桥**：`window.webdesk.exec()` 授权执行本地命令，含"不再提示"。
- **后台驻留**：关窗隐藏，进程保活。
- **桌面快捷方式**：图标绑定应用，`weblaunch --launch <appId>` 直达。
- **界面定制**：按应用 CSS/JS 注入。
- **Web 管理控制台**：中文/英文双语，自动消息提醒（toast）。

## 快速开始

```bash
# 单二进制：weblaunch.exe 既是 CLI 又是 daemon
weblaunch addweb -url https://example.com -name "Example"
weblaunch app list        # 列出应用
weblaunch app launch <id> # 启动（独立窗口 + 独立任务栏图标）
weblaunch console         # 打开管理控制台
weblaunch status          # 平台状态
```

> 首次启动自动拉起 daemon，管理控制台自动打开于 `http://127.0.0.1:3070`。

## 技术栈（已定案 V1.7）

| 层 | 选型 |
|---|---|
| 框架 | **Tauri 2**（Rust + Web 前端） |
| 渲染引擎 | 各平台系统 WebView |
| 管理控制台 | Web（HTML/CSS/TS，平台第一个内置应用） |
| 本地 HTTP | Rust `axum`（`127.0.0.1:3070` 固定端口，无鉴权） |
| 系统集成 | Rust + tauri-plugin |

## 构建与运行

```bash
# 前置：Rust / Node ≥20 / WebView2（Windows）
cd src-frontend && npm install && npm run dev
cargo tauri dev          # 开发运行
cargo build              # 构建
cargo test               # 测试
cargo clippy             # lint（零警告）
cargo tauri build        # 打包安装包
```

## 文档

- 使用指南：[`docs/guide/usage.md`](./docs/guide/usage.md)（含钩子 / 快捷方式 / 授权桥用法）
- 管理 API 契约：[`docs/design/api-contract.md`](./docs/design/api-contract.md)
- ADR / 需求：[`docs/requirements/requirements-master.md`](./docs/requirements/requirements-master.md)

## License

[MIT](./LICENSE)
