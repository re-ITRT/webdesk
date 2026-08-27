# WebLaunch Code Map (Feature → Code)

> Organized in a **general-to-specific** structure: overall architecture first, then per-feature code locations.
> To modify a feature, jump to its section.

---

## General: Architecture

```
┌─────────────────────────────────────────────────────────┐
│  weblaunch.exe (single binary: CLI + daemon)             │
│                                                         │
│  main.rs ── entry: CLI subcommand → cli::run_cli()       │
│       │        otherwise → webdesk_lib::run() (daemon)   │
│       ▼                                                  │
│  lib.rs ── Tauri assembly: singleton / autostart / log   │
│       │                                                  │
│       ├── app_state.rs ── global shared state           │
│       ├── server/ ── local HTTP API (axum, :3070)       │
│       │     ├── mod.rs   ── spawn / api.json             │
│       │     └── routes.rs ── all REST endpoints          │
│       ├── scheduler/ ── app lifecycle (windows)          │
│       ├── hooks/ ── lifecycle hook executor              │
│       ├── platform/ ── tray/shortcuts/icons/autostart/   │
│       │                    AUMID                          │
│       ├── identity/ ── per-app identity isolation        │
│       ├── auth.rs ── command authorization store          │
│       ├── store/ ── app config persistence               │
│       ├── types.rs ── shared types                       │
│       └── cli.rs ── CLI command implementations         │
│                                                         │
│  src-frontend/ ── management console (Web)                │
│       ├── index.html ── page + styles                    │
│       └── src/                                          │
│             ├── api.ts ── API client (snake_case)        │
│             └── main.ts ── views/i18n/toast/approval     │
└─────────────────────────────────────────────────────────┘
```

**Data flow (launching an app)**:
```
CLI/console → POST /api/apps/{id}/launch
  → routes.rs::launch_app
  → scheduler::launch_by_id (dispatch to Tauri runtime)
  → hooks::run_pre_launch (pre-launch hooks)
  → spawn_window (create WebviewWindow + icon + AUMID)
  → inject_webview (inject window.webdesk bridge)
```

---

## Specific: Feature → Code

### 1. App Management (CRUD / persistence)

| Feature | Code |
|---|---|
| App CRUD API | `src-tauri/src/server/routes.rs` — `list_apps` / `create_app` / `get_app` / `update_app` / `delete_app` |
| Partial-field create/update | `types.rs::App::from_partial` |
| Config persistence (JSON) | `store/app_store.rs` — `create` / `update` / `delete` / `list` |
| App data structure | `types.rs::App` (hooks / icon / close_action / injections) |
| System app seeding (console) | `app_state.rs::ensure_system_apps` |

### 2. Launch / Activate / Terminate

| Feature | Code |
|---|---|
| Launch app (create window) | `scheduler/mod.rs::launch_by_id` → `spawn_window` |
| Activate background app | `scheduler/mod.rs::activate` / `activate_app_cmd` |
| Terminate app | `scheduler/mod.rs::terminate_app` |
| Reload after config change | `scheduler/mod.rs::reload_app` |
| Window label rule | `scheduler/mod.rs::window_label` (`win-{app_id}`) |
| Singleton forwarding (re-click icon) | `lib.rs` singleton plugin callback |

### 3. Lifecycle Hooks

| Feature | Code |
|---|---|
| Hook executor | `hooks/mod.rs::run_hook` (timeout/process tree/exit code) |
| Pre-launch hooks | `hooks/mod.rs::run_pre_launch` |
| Post-exit hooks | `hooks/mod.rs::run_post_exit` |
| **bat code auto-write** | `hooks/mod.rs::run_hook` (multi-line/@echo off → .bat) |
| Hook shell selection | `hooks/mod.rs::shell_command` |
| Hook config types | `types.rs::HookConfig` / `HookOptions` |
| Hook logging | `hooks/mod.rs::append_log` (JSONL) |

### 4. Local HTTP API

| Feature | Code |
|---|---|
| HTTP server startup | `server/mod.rs::spawn` (fixed 127.0.0.1:3070) |
| Route registration | `server/routes.rs::build_router` |
| Health check | `routes.rs::health` |
| Platform status | `routes.rs::status` |
| Static console hosting | `routes.rs::console_index` + `frontend_dist_dir` |
| API config persistence | `server/mod.rs::persist_api_config` (api.json) |

### 5. Command Authorization Bridge

| Feature | Code |
|---|---|
| Auth check endpoint | `server/routes.rs::exec_command` (needs_approval) |
| Auth approve endpoint | `server/routes.rs::exec_approve` |
| Command execution | `server/routes.rs::run_shell_command` |
| Grant storage | `auth.rs::AuthStore` (grants.json, per app+command) |
| **Bridge JS injection** | `scheduler/mod.rs::inject_webview` (window.webdesk.exec + global spawn/require interception + approval dialog) |

### 6. Desktop Shortcuts + Icons

| Feature | Code |
|---|---|
| Create shortcut | `platform/mod.rs::create_shortcut` |
| Shortcut endpoint | `server/routes.rs::create_shortcut` (prefers app.icon) |
| Icon fetch (favicon→ICO) | `platform/mod.rs::fetch_app_icon` (SVG→PNG→ICO) |
| Icon bound to app | `types.rs::App.icon` + `store/app_store.rs::update` |
| Window icon | `scheduler/mod.rs::spawn_window` (window.set_icon) |
| Shortcut filename | `platform/mod.rs::shortcut_filename` / `sanitize_filename` |

### 7. Independent Taskbar (AUMID)

| Feature | Code |
|---|---|
| Set per-window AUMID | `platform/mod.rs::set_window_taskbar_identity` |
| Call site | `scheduler/mod.rs::spawn_window` (after build) |
| WS_EX_APPWINDOW | `platform/mod.rs::set_window_taskbar_identity` |

### 8. Tray / Autostart

| Feature | Code |
|---|---|
| Dynamic tray | `platform/mod.rs::show_tray_if_needed` / `rebuild_tray` / `hide_tray` |
| Tray menu actions | `platform/mod.rs::parse_menu_id` |
| Show main panel | `platform/mod.rs::show_main_panel` |
| Activate from tray | `platform/mod.rs::activate_background_app` |
| Quit all | `platform/mod.rs::quit_all_apps` |
| Autostart | `platform/mod.rs::set_autostart` / `autostart_enabled` |

### 9. App Identity Isolation

| Feature | Code |
|---|---|
| Identity manager | `identity/mod.rs::IdentityManager` |
| Per-app data dir | `identity/mod.rs::data_dir` |
| Cookies | `identity/mod.rs::cookies_dir` / `get_cookie_count` / `export_cookies` / `import_cookies` |
| Extensions | `identity/mod.rs::extensions_dir` / `list_extensions` |
| Secrets | `identity/mod.rs::secrets_dir` / `has_secrets` |
| Identity summary endpoint | `server/routes.rs::identity_summary` |

### 10. CLI

| Feature | Code |
|---|---|
| CLI entry | `main.rs` → `cli.rs::run_cli` |
| Command definitions | `cli.rs::Commands` / `AppCommands` (clap derive) |
| Add app | `cli.rs::cmd_add` (addweb alias) |
| App management | `cli.rs::cmd_list` / `cmd_get` / `cmd_remove` / `cmd_launch` / `cmd_stop` / `cmd_activate` / `cmd_app_status` / `cmd_shortcut` |
| Platform status | `cli.rs::cmd_platform_status` |
| Open console | `cli.rs::cmd_console` |
| Daemon bootstrap | `cli.rs::ensure_daemon` / `spawn_daemon` |
| Multi-char short options | `cli.rs::run_cli` (-url → --url) |

### 11. Management Console (Frontend)

| Feature | Code |
|---|---|
| API client | `src-frontend/src/api.ts` (snake_case, fixed 3070) |
| View rendering | `src-frontend/src/main.ts::render` (sidebar + apps/settings) |
| App grid | `main.ts::renderGrid` |
| Edit/add modal | `main.ts::openModal` |
| Shortcut dialog (icon picker) | `main.ts::shortcutDialog` |
| Toast notifications | `main.ts::toast` |
| i18n | `main.ts::I18N` (zh/en) + `t()` |
| Settings page | `main.ts::renderSettings` |
| Styles | `src-frontend/index.html` |

### 12. Logging

| Feature | Code |
|---|---|
| Log init | `lib.rs` setup (tauri-plugin-log, debug+release) |
| Log files | `%APPDATA%/WebDesk/logs/` |
| Hook logs | `hooks/mod.rs::append_log` (JSONL) |

---

## Appendix: Key Data Locations (Windows)

| Data | Path |
|---|---|
| App configs | `%APPDATA%/com.webdesk.desktop/WebDesk/config/*.json` |
| App icon cache | `%APPDATA%/WebDesk/icons/apps/*.ico` |
| Command grants | `%APPDATA%/WebDesk/auth/grants.json` |
| Logs | `%APPDATA%/WebDesk/logs/` |
| API config | `%APPDATA%/WebDesk/api.json` |
| Hook bat temp files | `%APPDATA%/WebDesk/hooks/` |
| Identity data | `%APPDATA%/com.webdesk.desktop/WebDesk/identity/{app_id}/` |
