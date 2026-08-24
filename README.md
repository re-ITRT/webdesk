# WebDesk — 通用 Web 应用桌面化管理平台

> 轻量级、高可控性的 Web 应用桌面化运行与管理平台。将 Web 应用以"原生桌面应用"的形态呈现，提供深度生命周期钩子、后台驻留能力及高度自由的界面定制功能，**四平台**（Windows / macOS / Linux / 鸿蒙）一键直达。

[中文](./README.md) | English: [README.en.md](./README.en.md)

## WebDesk 是什么

把任意 Web 应用当作"原生桌面应用"来运行与管理：**per-app 身份隔离**（cookie/密钥/扩展）+ **生命周期钩子** + **工作项驱动生命周期**（无常驻托盘）+ **后台驻留** + **桌面快捷方式** + **Web 管理控制台**（平台自身的第一个应用，吃自己的狗粮）。

## 核心特性

- **单引擎渲染（Tauri 2 + 各平台系统 WebView）**：Windows=WebView2 / macOS=WKWebView / Linux=WebKitGTK / 鸿蒙=ArkWeb。
- **应用级身份 AppIdentity（ADR-009）**：cookie/密钥/扩展按应用隔离，平台统一管理。
- **生命周期钩子**：启动前/关闭后脚本钩子；阻塞模式、超时、退出码采集。
- **工作项驱动生命周期（ADR-010）**：平台随第一个 app 启动、最后一个工作项结束而启停；默认无托盘图标，仅存在后台驻留应用时动态出现。
- **后台持久化驻留**：关窗隐藏（不销毁）渲染器，WebSocket 自然保活。
- **界面高度定制**：按应用控制原生控件、CSS/JS 注入、扩展按需加载。
- **桌面快捷方式**：一键创建，`webdesk --launch <appId>` 直达。
- **单例 + 调度**：唯一 daemon，IPC 转发，极速启动。
- **Web 管理控制台**：控制台即平台第一个内置应用。

## 技术栈（已定案 V1.7）

| 层 | 选型 |
|---|---|
| 框架 | **Tauri 2**（Rust + Web 前端） |
| 渲染引擎 | 各平台系统 WebView（Win=WebView2 / mac=WKWebView / Linux=WebKitGTK / 鸿蒙=ArkWeb） |
| 管理控制台 | Web（HTML/CSS/TS，平台第一个内置应用） |
| 本地 HTTP | Rust `axum`（仅 127.0.0.1 + 随机端口 + 会话 token） |
| 系统集成 | Rust + tauri-plugin（托盘 / 单例 / 快捷方式 / 钩子 / 进程） |
| 跨平台 | **Windows / macOS / Linux / 鸿蒙** |

完整选型论证：[`docs/design/2026-08-25-tech-selection.md`](docs/design/2026-08-25-tech-selection.md)

## 仓库结构

```
webdesk/
├── src/               # Rust daemon（调度 / 钩子 / 身份 / server / platform）
├── src-tauri/         # Tauri 应用（Rust + 配置）
├── src-frontend/      # 管理控制台（Web）
├── docs/              # 需求 / 设计 / 决策记录
│   ├── requirements/  # requirements-master.md（唯一需求总览）
│   └── design/        # ADR 决策记录 + API 契约
├── AGENTS.md          # 开发规范（必读）
└── .github/           # CI/CD + issue/PR 模板
```

## 构建与运行

### 前置依赖

- [Rust](https://rustup.rs/)（stable，Windows 需 MSVC 工具链）
- [Node.js](https://nodejs.org/) ≥ 20 + npm
- Windows：WebView2 runtime（Win11 自带）；Visual Studio Build Tools（MSVC）
- Linux：`libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`
- macOS：Xcode Command Line Tools

### 常用命令

```bash
# 前端
cd src-frontend && npm install && npm run dev   # 开发服务器（端口 1420）

# Rust（Tauri）
cargo tauri dev          # 开发运行
cargo build              # 仅构建 Rust
cargo test               # 运行测试
cargo clippy             # lint（零警告）
cargo fmt --check        # 格式检查

# 完整打包
cargo tauri build        # 生产构建 + 安装包
```

> 需 `cargo-tauri` CLI：`cargo install tauri-cli --locked`

## 管理 API

管理 API（localhost REST）定义见 [`docs/design/api-contract.md`](docs/design/api-contract.md)。所有 `/api/*` 需 Bearer 会话 token，基址 `http://127.0.0.1:<随机端口>`。

## ADR 索引

完整 ADR 表（001–011）见 [`docs/requirements/requirements-master.md`](docs/requirements/requirements-master.md) §4——涵盖应用级身份、工作项生命周期、单引擎渲染、Tauri 四平台选型等。

## 里程碑

- **M0** — Tauri 骨架 + axum API + 控制台 + 单例（当前）
- **M1** — 核心：多窗口 webview + 钩子 + 身份 + 驻留 + 工作项生命周期
- **M2** — macOS/Linux 适配 + 扩展 + 自更新
- **M3** — 鸿蒙（跟随 Tauri `feat/open-harmony`）

## License

[MIT](./LICENSE)
