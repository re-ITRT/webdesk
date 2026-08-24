# WebDesk 开发范式（多子代理协同）

**目的**：定义多子代理**并行开发**时的模块边界、接口契约、编码规范与完成标准。每个子代理只读本文档 + 自己的模块，即可独立交付。

---

## 1. 核心原则

1. **接口先定，实现后写**：模块间只通过 `docs/design/api-contract.md` 定义的接口交互，不互相依赖实现细节。
2. **模块隔离**：每个子代理负责一个模块，只改自己目录下的文件，不触碰他人模块。
3. **共享类型层**：跨模块共享的 Rust 类型集中在 `src-tauri/src/types.rs`（单一事实源）。
4. **可编译可测**：每个模块交付时必须 `cargo build` + `cargo test` + `cargo clippy` 通过。
5. **CI 为准**：`cargo fmt --check` / `clippy -D warnings` / `cargo test` 是硬门槛。

## 2. 模块划分与负责人

| 模块 | 目录 | 交付物 | 依赖 |
|---|---|---|---|
| **types** | `src-tauri/src/types.rs` | 全部共享类型 | 无（最先做） |
| **store** | `src-tauri/src/store/` | 应用配置 CRUD + JSON 持久化 | types |
| **hooks** | `src-tauri/src/hooks/` | 钩子执行器（shell/超时/日志） | types |
| **server** | `src-tauri/src/server/` | HTTP 路由 + token 鉴权 + 静态托管 | types, store, scheduler, hooks |
| **scheduler** | `src-tauri/src/scheduler/` | 应用生命周期（启动/激活/终止/驻留） | types, store |
| **identity** | `src-tauri/src/identity/` | cookie/密钥/扩展隔离注入 | types, store |
| **platform** | `src-tauri/src/platform/` | 平台差异（托盘/快捷方式/路径） | types |
| **frontend** | `src-frontend/` | 管理控制台 Web UI | api-contract.md |

## 3. Rust 模块结构（src-tauri/src/）

```
src-tauri/src/
├── main.rs          # 入口（已有）
├── lib.rs           # run()，装配所有模块（负责人：集成者）
├── types.rs         # 共享类型（App, ApiConfig, AppStatus, ...）
├── store/           # 配置持久化
│   ├── mod.rs
│   └── app_store.rs
├── hooks/
│   └── mod.rs       # 钩子执行器
├── server/
│   └── mod.rs       # HTTP 服务（axum）
├── scheduler/
│   └── mod.rs       # 应用生命周期
├── identity/
│   └── mod.rs       # 身份隔离
└── platform/
    └── mod.rs       # 平台抽象（当前 Windows 优先，其余 stub）
```

## 4. 共享类型（types.rs 契约）

所有模块必须使用 `types.rs` 中定义的类型，**不得自行重定义**。核心类型：

```rust
// 应用配置（对应 api-contract.md §1.4）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct App {
    pub id: String,
    pub name: String,
    pub url: String,
    pub runtime_profile: String,   // "system" | "pinned"
    pub close_action: String,      // "background" | "quit"
    pub hooks: HookConfig,
    pub hook_options: HookOptions,
    pub ui_controls: UiControls,
    pub injections: Injections,
    pub extensions: Vec<String>,
    pub is_system: bool,
    pub launch_on_boot: bool,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct HookConfig { pub pre_launch: Vec<String>, pub post_exit: Vec<String> }
pub struct HookOptions { pub shell: String, pub timeout_ms: u64, pub blocking: bool }
pub struct UiControls { pub address_bar: bool, pub nav_buttons: bool, pub refresh: bool }
pub struct Injections { pub css: String, pub js: String, pub timing: String }

// API 配置（前端经 Tauri IPC 获取）
#[derive(Serialize, Clone)]
pub struct ApiConfig { pub port: u16, pub token: String }

// 应用运行状态
#[derive(Serialize, Clone)]
pub struct AppStatus {
    pub id: String,
    pub status: String,   // "running" | "background" | "stopped"
    pub window_id: Option<String>,
    pub memory_kb: Option<u64>,
    pub started_at: Option<String>,
}

// 平台整体状态
#[derive(Serialize, Clone)]
pub struct PlatformStatus {
    pub running: Vec<String>,
    pub background: Vec<String>,
    pub version: String,
    pub uptime_sec: u64,
    pub memory_kb: u64,
    pub port: u16,
}

// 钩子日志条目
#[derive(Serialize, Clone)]
pub struct HookLogEntry {
    pub timestamp: String,
    pub event: String,     // "pre_launch" | "post_exit"
    pub shell: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

// 统一错误
#[derive(Serialize, Clone)]
pub struct ApiError { pub error: String, pub message: String }
```

## 5. 编码规范

- **Rust**：`cargo fmt` + `cargo clippy -D warnings` + `cargo test`；`anyhow`（应用）/ `thiserror`（库）；不用裸 `unwrap()`（测试除外）；异步 `tokio`。
- **前端**：TypeScript strict；不引入重型 UI 框架（原生 TS 或 Svelte 后置）；API 调用统一走 `src/api.ts`。
- **模块注释**：每个模块 `mod.rs` 顶部写职责一句话 + 关联 ADR。

## 6. 完成标准（Definition of Done）

一个模块"完成"= 以下全满足：
1. 实现 api-contract.md 中该模块的职责。
2. `cargo build` 通过（含其他模块已合入的 stub）。
3. `cargo test` 通过（该模块单元测试）。
4. `cargo clippy -D warnings` 零警告。
5. `cargo fmt --check` 通过。
6. 不破坏其他模块（不触碰他人文件）。

## 7. Git 纪律

- Conventional Commits。
- 每模块一个 commit：`feat(module): 描述`。
- 集成者负责最终 merge + 冲突解决。

## 8. 里程碑对齐

- **M0（本次）**：types + store + server + scheduler(stub) + hooks + frontend(骨架) + platform(Windows stub) → 可编译、API 可测、控制台可连。
- **M1+**：identity 实做、scheduler 真启动 WebView、跨平台适配。
