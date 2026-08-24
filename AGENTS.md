# AGENTS.md — WebDesk 项目开发规范

本文件为参与 WebDesk 开发的所有 Agent / AI 助手 / 协作者的**唯一事实来源**。开始任何工作前必读。

## 项目是什么

**WebDesk**：把任意 Web 应用当作"原生桌面应用"来运行与管理的**四平台**（Windows / macOS / Linux / 鸿蒙）SSB 管理平台。核心能力：系统 WebView 单引擎渲染 + per-app 身份隔离（cookie/密钥/扩展）+ 生命周期钩子 + 工作项驱动生命周期 + 后台驻留 + 桌面快捷方式 + Web 管理控制台。

## 技术栈（已定案，勿改）

| 层 | 选型 |
|---|---|
| 框架 | **Tauri 2**（Rust + Web 前端） |
| 渲染引擎 | 各平台系统 WebView（Win=WebView2 / mac=WKWebView / Linux=WebKitGTK / 鸿蒙=ArkWeb） |
| 前端 | Web（管理控制台，React/Vue/Svelte 自选，推荐 Svelte） |
| 后端 | Rust（daemon + 本地 HTTP，`axum`） |
| 本地 HTTP | 仅 `127.0.0.1` + 随机端口 + 会话 token |
| 构建 | `cargo`（Rust）+ 前端包管理器（npm） |

> 完整选型论证见 `docs/design/2026-08-25-tech-selection.md`。**这是经过多轮决策的定案，不要擅自改技术栈**。

## 仓库结构

```
webdesk/
├── src/               # Rust 后端（daemon / 调度 / 钩子 / 身份 / 生命周期）
├── src-frontend/      # 管理控制台前端（Web）
├── docs/              # 需求 / 设计 / 决策记录
│   ├── requirements/  # 需求规格（master 为总览）
│   └── design/        # ADR 决策记录
├── AGENTS.md          # 本文件
├── .github/           # CI/CD + issue/PR 模板
```

## 常用命令

```bash
# 前端
cd src-frontend && npm install && npm run dev     # 前端开发服务器
# Rust（Tauri）
cargo tauri dev        # 开发运行（Tauri + 前端）
cargo build            # 仅构建 Rust
cargo test             # 运行测试
cargo clippy           # lint
cargo fmt --check      # 格式检查
```

## 规范（强制）

### 1. 决策记录（ADR）纪律
- **任何架构/技术/需求决策**必须先写进 `docs/design/YYYY-MM-DD-<slug>.md`，再改代码。
- 需求变更同步更新 `docs/requirements/requirements-master.md`（唯一需求总览）。
- 现有 ADR 编号见 master 文档 §4 汇总表。

### 2. Git 提交规范
- **Conventional Commits**：`feat:` / `fix:` / `docs:` / `chore:` / `refactor:` / `test:` / `ci:`。
- 每次提交只做一件事，信息写清"为什么"。
- 本地仓库 = 唯一开发仓库；**推送 GitHub** 前确保本地测试通过。
- 不提交：`bin/`、`obj/`、`target/`、`node_modules/`、`dist/`、`.env*`、`config/`（见 `.gitignore`）。

### 3. Rust 规范
- `cargo fmt` 格式化 + `cargo clippy` 零警告（提交前必须通过）。
- 错误处理用 `anyhow`（应用层）/ `thiserror`（库层），不用 `unwrap()` 裸奔（测试除外）。
- 异步用 `tokio`。
- 模块：`src/` 下按职责分 `app/`（调度）/ `hooks/` / `identity/` / `server/`（HTTP）/ `platform/`（各平台实现）。

### 4. 前端规范
- 管理控制台是**平台第一个内置应用**（吃自己的狗粮）。
- 不引入重型 UI 框架，保持轻量（Svelte 或原生）。
- 所有管理 API 调用经本地 HTTP + token。

### 5. 平台注意
- **Windows**：WebView2 runtime 需存在（本机已装）；MSVC 工具链（本机 VS 已装）。
- **macOS/Linux**：各自系统 WebView 依赖。
- **鸿蒙**：Tauri `feat/open-harmony` 分支，开发中——涉及鸿蒙的改动需标注"鸿蒙支持进行中"。
- 跨平台文件路径用 crate（如 `dirs` / `tauri-plugin-fs`），不用硬编码 `%APPDATA%` 等。

### 6. CI/CD
- 见 `.github/workflows/`。主流程：`ci.yml`（多平台构建测试）+ `release.yml`（发版）。
- 提交前本地跑 `cargo test` + `cargo clippy` + `cargo fmt --check`。

## 需求来源（必读）

1. `docs/requirements/requirements-master.md` — **需求总览（唯一权威）**
2. `docs/design/` — 各 ADR 决策记录
3. `docs/requirements/requirements-v1.2-draft.md` — 历史演进（部分被 ADR 覆盖，以 master 为准）

## 当前状态

- 技术栈：Tauri 四平台（已定案 V1.7）
- 阶段：M0 原型（Tauri 骨架 + 管理控制台 + 单例启动器）
- 里程碑：M0 原型 → M1 核心能力 → M2 macOS/Linux → M3 鸿蒙
