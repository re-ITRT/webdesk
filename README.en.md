# WebDesk — Universal Web App Desktop Manager

> A lightweight, high-control platform for running and managing web apps as native desktop applications, on **four platforms**: Windows / macOS / Linux / HarmonyOS.

English | [中文](./README.md)

## What is WebDesk?

WebDesk turns any web app into a "native desktop app" — with **per-app identity isolation** (cookies/keys/extensions), **lifecycle hooks**, **work-item driven lifecycle** (no always-on tray icon), **background persistence**, **desktop shortcuts**, and a **Web management console** that runs on the platform itself (dogfooding).

## Core Features

- **Single-engine rendering (system WebView via Tauri 2)** — WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux, ArkWeb on HarmonyOS.
- **AppIdentity (ADR-009)** — cookies/keys/extensions isolated per app, managed uniformly by the platform.
- **Lifecycle hooks** — pre-launch / post-exit script hooks; blocking mode, timeout, exit-code capture.
- **Work-item driven lifecycle (ADR-010)** — the platform starts with the first app and exits when the last work item ends; no tray icon by default, appears only when background apps exist.
- **Background persistence** — closing a window hides (not destroys) the renderer; WebSocket stays alive.
- **Customizable UI** — per-app native controls, CSS/JS injection, per-app extensions.
- **Desktop shortcuts** — one-click `.lnk` (or platform equivalent), `webdesk --launch <appId>`.
- **Singleton + scheduling** — single daemon, IPC forwarding, fast launch.
- **Web management console** — the console is itself the platform's first built-in web app.

## Tech Stack (Decided — V1.7)

| Layer | Choice |
|---|---|
| Framework | **Tauri 2** (Rust + Web frontend) |
| Rendering | System WebView — Win=WebView2 / mac=WKWebView / Linux=WebKitGTK / HarmonyOS=ArkWeb |
| Console | Web (HTML/CSS/TS, platform's first app) |
| Local HTTP | Rust `axum` (127.0.0.1 + random port + session token) |
| System integration | Rust + tauri-plugin (tray / singleton / shortcuts / hooks / processes) |
| Cross-platform | **Windows / macOS / Linux / HarmonyOS** |

Full selection rationale: [`docs/design/2026-08-25-tech-selection.md`](docs/design/2026-08-25-tech-selection.md)

## Repository Layout

```
webdesk/
├── src/               # Rust daemon (scheduler / hooks / identity / server / platform)
├── src-tauri/         # Tauri application (Rust + config)
├── src-frontend/      # Management console (Web)
├── docs/              # Requirements / design / ADRs
│   ├── requirements/  # requirements-master.md (source of truth)
│   └── design/        # ADR decision records + API contract
├── AGENTS.md          # Development rules (read first)
└── .github/           # CI/CD + issue/PR templates
```

## Build & Run

### Prerequisites

- [Rust](https://rustup.rs/) (stable, MSVC toolchain on Windows)
- [Node.js](https://nodejs.org/) ≥ 20 + npm
- Windows: WebView2 runtime (preinstalled on Win11); Visual Studio Build Tools (MSVC)
- Linux: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`
- macOS: Xcode Command Line Tools

### Commands

```bash
# Frontend
cd src-frontend && npm install && npm run dev   # dev server (port 1420)

# Rust (Tauri)
cargo tauri dev          # run in dev mode
cargo build              # build Rust only
cargo test               # run tests
cargo clippy             # lint (zero warnings required)
cargo fmt --check        # format check

# Full bundle
cargo tauri build        # production build + installer
```

> Requires `cargo-tauri` CLI: `cargo install tauri-cli --locked`

## API Contract

The management API (REST over localhost) is defined in [`docs/design/api-contract.md`](docs/design/api-contract.md). All `/api/*` endpoints require a `Bearer` session token; the base URL is `http://127.0.0.1:<random-port>`.

## ADR Index

See [`docs/requirements/requirements-master.md`](docs/requirements/requirements-master.md) §4 for the full ADR table (001–011), covering: AppIdentity, work-item lifecycle, single-engine rendering, Tauri four-platform selection, and more.

## Roadmap

- **M0** — Tauri skeleton + axum API + console + singleton (current)
- **M1** — core: multi-window webview + hooks + identity + persistence + work-item lifecycle
- **M2** — macOS/Linux adapters + extensions + self-update
- **M3** — HarmonyOS (follows Tauri `feat/open-harmony`)

## License

[MIT](./LICENSE)