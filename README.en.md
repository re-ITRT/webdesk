# WebLaunch — Launch & host any web app as a native desktop app

> A lightweight, high-control platform for running and managing web apps as native desktop applications: independent taskbar icons, background persistence, lifecycle hooks, a command-authorization bridge, and desktop shortcuts — across **four platforms** (Windows / macOS / Linux / HarmonyOS).

English | [中文](./README.md)

## What is WebLaunch?

WebLaunch turns any web app into a "native desktop app":

- **Single-engine rendering** (Tauri 2 + system WebView)
- **Per-app independent taskbar**: distinct AppUserModelID + icon, separated from the WebLaunch host
- **Lifecycle hooks**: pre-launch / post-exit script hooks (paste bat code directly)
- **Command-authorization bridge**: web pages may safely request local command execution; an approval dialog appears on first use, with an optional "don't ask again"
- **Background persistence**: closing a window hides (not destroys) it; WebSocket stays alive
- **Desktop shortcuts**: one-click, icon bound to the app itself
- **Web management console**: the platform's own first built-in app (dogfooding)

## Core Features

- **Single-engine rendering** — WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux, ArkWeb on HarmonyOS.
- **Per-app identity isolation** — cookies / keys / extensions isolated per app.
- **Lifecycle hooks** — pre-launch / post-exit, blocking/timeout/exit-code, bat script support.
- **Command-authorization bridge** — `window.webdesk.exec()` authorizes local command execution, with "don't ask again".
- **Background persistence** — window hides on close, process stays alive.
- **Desktop shortcuts** — icon bound to the app, `weblaunch --launch <appId>`.
- **UI customization** — per-app CSS/JS injection.
- **Web console** — Chinese/English bilingual, auto-dismissing toast notifications.

## Quick Start

```bash
# Single binary: weblaunch.exe is both CLI and daemon
weblaunch addweb -url https://example.com -name "Example"
weblaunch app list         # list apps
weblaunch app launch <id>  # launch (separate window + taskbar icon)
weblaunch console          # open management console
weblaunch status           # platform status
```

> First launch auto-starts the daemon; the console opens at `http://127.0.0.1:3070`.

## Tech Stack (Decided — V1.7)

| Layer | Choice |
|---|---|
| Framework | **Tauri 2** (Rust + Web frontend) |
| Rendering | System WebView (Win=WebView2 / mac=WKWebView / Linux=WebKitGTK / HarmonyOS=ArkWeb) |
| Console | Web (HTML/CSS/TS, platform's first app) |
| Local HTTP | Rust `axum` (`127.0.0.1:3070`, fixed port, no auth) |
| System integration | Rust + tauri-plugin |

## Build &amp; Run

```bash
# Prereqs: Rust / Node ≥20 / WebView2 (Windows)
cd src-frontend && npm install && npm run dev
cargo tauri dev          # run in dev mode
cargo build              # build Rust
cargo test               # run tests
cargo clippy             # lint (zero warnings)
cargo tauri build        # bundle installers
```

## Docs

- Usage guide: [`docs/guide/usage.md`](./docs/guide/usage.md) (hooks / shortcuts / authorization bridge)
- **Code map**: [`docs/guide/code-map.md`](./docs/guide/code-map.md) (feature → code, general-to-specific)
- API contract: [`docs/design/api-contract.md`](./docs/design/api-contract.md)
- ADRs / requirements: [`docs/requirements/requirements-master.md`](./docs/requirements/requirements-master.md)

## License

[MIT](./LICENSE)
