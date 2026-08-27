# WebLaunch 代码导航（功能 → 代码）

> 本文档以**总分结构**组织：先给整体架构（总），再按功能模块列出对应代码位置（分）。
> 想改哪个功能，直接跳到对应小节。

---

## 总：整体架构

```
┌─────────────────────────────────────────────────────────┐
│  weblaunch.exe（单二进制：CLI + daemon 合一）             │
│                                                         │
│  main.rs ── 入口：检测 CLI 子命令 → cli::run_cli()        │
│       │        否则 → webdesk_lib::run()（daemon）        │
│       ▼                                                  │
│  lib.rs ── Tauri 装配：单例插件 / 自启插件 / 日志 / setup  │
│       │                                                  │
│       ├── app_state.rs ── 全局共享状态（store/scheduler/  │
│       │                    auth/running）                 │
│       ├── server/ ── 本地 HTTP 管理 API（axum, :3070）    │
│       │     ├── mod.rs   ── spawn / api.json 落盘         │
│       │     └── routes.rs ── 全部 REST 端点                │
│       ├── scheduler/ ── 应用生命周期（窗口创建/激活/终止）  │
│       ├── hooks/ ── 生命周期钩子执行器（bat 落盘/超时）     │
│       ├── platform/ ── 平台能力（托盘/快捷方式/图标/自启/  │
│       │                    AUMID）                        │
│       ├── identity/ ── per-app 身份隔离（cookie/密钥/扩展）│
│       ├── auth.rs ── 命令授权存储（grants.json）           │
│       ├── store/ ── 应用配置持久化（JSON 文件）            │
│       ├── types.rs ── 共享类型层（App/HookConfig/...）     │
│       └── cli.rs ── CLI 命令实现（addweb/app/status/...）  │
│                                                         │
│  src-frontend/ ── 管理控制台（Web）                        │
│       ├── index.html ── 页面 + 样式                        │
│       └── src/                                          │
│             ├── api.ts ── API 客户端（snake_case 契约）    │
│             └── main.ts ── 视图/交互/i18n/toast/授权框     │
└─────────────────────────────────────────────────────────┘
```

**数据流（启动一个应用）**：
```
CLI/控制台 → POST /api/apps/{id}/launch
  → routes.rs::launch_app
  → scheduler::launch_by_id（派发到 Tauri runtime）
  → hooks::run_pre_launch（执行启动前钩子）
  → spawn_window（创建 WebviewWindow + 图标 + AUMID）
  → inject_webview（注入 window.webdesk 桥接）
```

---

## 分：功能 → 代码

### 1. 应用管理（增删改查 / 持久化）

| 功能 | 代码位置 |
|---|---|
| 应用 CRUD API | `src-tauri/src/server/routes.rs` — `list_apps` / `create_app` / `get_app` / `update_app` / `delete_app` |
| 部分字段创建/更新 | `types.rs::App::from_partial`（name/url 必填，其余默认） |
| 配置持久化（JSON 文件） | `store/app_store.rs` — `create` / `update` / `delete` / `list` |
| 应用数据结构 | `types.rs::App`（含 hooks / icon / close_action / injections） |
| 系统应用预置（console） | `app_state.rs::ensure_system_apps` |

### 2. 启动 / 激活 / 终止（生命周期）

| 功能 | 代码位置 |
|---|---|
| 启动应用（建窗口） | `scheduler/mod.rs::launch_by_id` → `spawn_window` |
| 激活后台应用 | `scheduler/mod.rs::activate` / `activate_app_cmd` |
| 终止应用 | `scheduler/mod.rs::terminate_app`（关窗 + post_exit 钩子） |
| 配置修改后重载 | `scheduler/mod.rs::reload_app`（销毁重建窗口） |
| 窗口 label 规则 | `scheduler/mod.rs::window_label`（`win-{app_id}`） |
| 单例转发（再次点图标唤起） | `lib.rs` 单例插件回调（无参默认唤起 console） |

### 3. 生命周期钩子

| 功能 | 代码位置 |
|---|---|
| 钩子执行器 | `hooks/mod.rs::run_hook`（超时/进程树/退出码） |
| 启动前钩子 | `hooks/mod.rs::run_pre_launch` |
| 关闭后钩子 | `hooks/mod.rs::run_post_exit` |
| **bat 代码自动落盘** | `hooks/mod.rs::run_hook`（识别多行/@echo off → 写 .bat 执行） |
| 钩子 shell 选择 | `hooks/mod.rs::shell_command`（cmd/powershell/wsl/sh） |
| 钩子配置结构 | `types.rs::HookConfig` / `HookOptions`（shell/timeout/blocking） |
| 钩子日志 | `hooks/mod.rs::append_log`（JSONL 到 logs/hooks.log） |

### 4. 本地 HTTP 管理 API

| 功能 | 代码位置 |
|---|---|
| HTTP 服务启动 | `server/mod.rs::spawn`（固定 127.0.0.1:3070） |
| 路由注册 | `server/routes.rs::build_router` |
| 健康检查 | `routes.rs::health` |
| 平台状态 | `routes.rs::status` |
| 静态控制台托管 | `routes.rs::console_index` + `frontend_dist_dir` |
| API 配置落盘 | `server/mod.rs::persist_api_config`（api.json） |

### 5. 命令授权桥（网页安全执行本地命令）

| 功能 | 代码位置 |
|---|---|
| 授权检查端点 | `server/routes.rs::exec_command`（未授权返回 needs_approval） |
| 授权执行端点 | `server/routes.rs::exec_approve`（记录 + 执行） |
| 命令执行 | `server/routes.rs::run_shell_command` |
| 授权存储 | `auth.rs::AuthStore`（grants.json，按 app+command） |
| **注入桥接 JS** | `scheduler/mod.rs::inject_webview`（window.webdesk.exec + 全局 spawn/require 拦截 + 授权框） |

### 6. 桌面快捷方式 + 图标

| 功能 | 代码位置 |
|---|---|
| 创建快捷方式 | `platform/mod.rs::create_shortcut`（Windows .lnk / macOS alias / Linux .desktop） |
| 快捷方式端点 | `server/routes.rs::create_shortcut`（优先用 app.icon） |
| 图标抓取（favicon→ICO） | `platform/mod.rs::fetch_app_icon`（SVG→PNG→ICO 转换） |
| 图标绑定到应用 | `types.rs::App.icon` + `store/app_store.rs::update`（icon 字段合并） |
| 窗口图标设置 | `scheduler/mod.rs::spawn_window`（window.set_icon） |
| 快捷方式文件名 | `platform/mod.rs::shortcut_filename` / `sanitize_filename` |

### 7. 任务栏独立（AUMID）

| 功能 | 代码位置 |
|---|---|
| 设置窗口独立 AUMID | `platform/mod.rs::set_window_taskbar_identity`（SHGetPropertyStoreForWindow + PKEY_AppUserModel_ID） |
| 调用时机 | `scheduler/mod.rs::spawn_window`（窗口 build 后） |
| WS_EX_APPWINDOW | `platform/mod.rs::set_window_taskbar_identity`（强制独立任务栏按钮） |

### 8. 托盘 / 开机自启

| 功能 | 代码位置 |
|---|---|
| 动态托盘（有后台应用才出现） | `platform/mod.rs::show_tray_if_needed` / `rebuild_tray` / `hide_tray` |
| 托盘菜单动作 | `platform/mod.rs::parse_menu_id`（TrayAction） |
| 显示主面板 | `platform/mod.rs::show_main_panel` |
| 激活后台应用（托盘） | `platform/mod.rs::activate_background_app` |
| 全部退出 | `platform/mod.rs::quit_all_apps` |
| 开机自启 | `platform/mod.rs::set_autostart` / `autostart_enabled` |

### 9. 应用身份隔离（cookie/密钥/扩展）

| 功能 | 代码位置 |
|---|---|
| 身份管理器 | `identity/mod.rs::IdentityManager` |
| per-app 数据目录 | `identity/mod.rs::data_dir`（按 app_id 隔离） |
| cookie 目录/统计 | `identity/mod.rs::cookies_dir` / `get_cookie_count` |
| cookie 导入导出 | `identity/mod.rs::export_cookies` / `import_cookies` |
| 扩展目录 | `identity/mod.rs::extensions_dir` / `list_extensions` |
| 密钥目录 | `identity/mod.rs::secrets_dir` / `has_secrets` |
| 身份摘要端点 | `server/routes.rs::identity_summary` |

### 10. CLI 命令

| 功能 | 代码位置 |
|---|---|
| CLI 入口 | `main.rs`（检测子命令）→ `cli.rs::run_cli` |
| 命令定义 | `cli.rs::Commands` / `AppCommands`（clap derive） |
| 添加应用 | `cli.rs::cmd_add`（addweb 别名） |
| 应用管理 | `cli.rs::cmd_list` / `cmd_get` / `cmd_remove` / `cmd_launch` / `cmd_stop` / `cmd_activate` / `cmd_app_status` / `cmd_shortcut` |
| 平台状态 | `cli.rs::cmd_platform_status` |
| 打开控制台 | `cli.rs::cmd_console` |
| daemon 自举 | `cli.rs::ensure_daemon` / `spawn_daemon`（--hidden 重启自身） |
| 多字符短选项兼容 | `cli.rs::run_cli`（-url → --url 预处理） |

### 11. 管理控制台（前端）

| 功能 | 代码位置 |
|---|---|
| API 客户端 | `src-frontend/src/api.ts`（snake_case 契约，固定 3070） |
| 视图渲染 | `src-frontend/src/main.ts::render`（侧边栏 + 应用/设置） |
| 应用网格 | `main.ts::renderGrid`（卡片 + 启动/终止/编辑/快捷方式/删除） |
| 编辑/添加弹窗 | `main.ts::openModal` |
| 快捷方式弹窗（图标选择） | `main.ts::shortcutDialog` |
| 消息提醒（toast） | `main.ts::toast`（自动消失） |
| i18n | `main.ts::I18N`（zh/en）+ `t()` |
| 设置页（语言切换） | `main.ts::renderSettings` |
| 样式 | `src-frontend/index.html`（侧边栏/卡片/toast/模态） |

### 12. 日志系统

| 功能 | 代码位置 |
|---|---|
| 日志初始化 | `lib.rs` setup（tauri-plugin-log，debug+release 都启用） |
| 日志落盘 | `%APPDATA%/WebDesk/logs/`（LogDir target） |
| 钩子日志 | `hooks/mod.rs::append_log`（JSONL） |

---

## 附：关键数据位置（Windows）

| 数据 | 路径 |
|---|---|
| 应用配置 | `%APPDATA%/com.webdesk.desktop/WebDesk/config/*.json` |
| 应用图标缓存 | `%APPDATA%/WebDesk/icons/apps/*.ico` |
| 命令授权 | `%APPDATA%/WebDesk/auth/grants.json` |
| 日志 | `%APPDATA%/WebDesk/logs/` |
| API 配置 | `%APPDATA%/WebDesk/api.json`（port/token） |
| 钩子 bat 临时文件 | `%APPDATA%/WebDesk/hooks/` |
| 身份数据 | `%APPDATA%/com.webdesk.desktop/WebDesk/identity/{app_id}/` |
