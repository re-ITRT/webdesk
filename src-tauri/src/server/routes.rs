//! WebLaunch server 模块 —— HTTP 路由与端点实现
//!
//! 基于 axum 的本地 REST API，绑定 `127.0.0.1:3070`。所有 `/api/*`
//! 端点设计上要求 Bearer token 鉴权（token 由 [`crate::server::generate_token`]
//! 生成并随 [`crate::types::ApiConfig`] 持久化；当前尚未接入强制校验中间件）。
//! 同时以静态文件方式托管管理控制台前端（`src-frontend/dist`）。

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use tauri::Manager;
use tower_http::cors::CorsLayer;

use crate::store::AppStore;
use crate::types::{ApiConfig, ApiError, App, AppStatus, PlatformStatus};

/// 共享应用状态（axum 侧）
///
/// 通过 `Arc` 在路由处理器间共享：持有应用配置存储、会话 token、
/// 服务启动时刻、实际监听端口及 Tauri 句柄。
pub struct AppState {
    pub store: AppStore,
    pub token: String,
    pub started_at: std::time::Instant,
    /// 兼容字段（M2 起移除；当前用全局 AppState.running）
    #[allow(dead_code)]
    pub running: RwLock<Vec<String>>,
    #[allow(dead_code)]
    pub background: RwLock<Vec<String>>,
    pub port: RwLock<u16>,
    /// Tauri 句柄（经它访问全局 AppState：scheduler / identity / hooks）
    pub app_handle: tauri::AppHandle,
}

pub type SharedState = Arc<AppState>;

/// 启动 HTTP 服务（后台任务）
///
/// 在独立 tokio runtime 中运行（由 `lib.rs` 的 setup 线程驱动）：
/// 1. 初始化应用配置存储（`AppStore`，位于 Tauri 配置目录）；
/// 2. 生成会话 token 并绑定 `127.0.0.1:3070`（端口被占用时返回错误，
///    提示可能已有 daemon 实例在运行）；
/// 3. 将 API 配置落盘并写入全局，供 CLI / 前端发现；
/// 4. 将预置系统应用「WebLaunch 控制台」的 URL 更新为真实端口；
/// 5. 按启动参数决定是否自动打开应用（`--launch=<id>` 优先，否则控制台；
///    `--hidden` 时不打开任何窗口）；
/// 6. 构建路由（API + 静态控制台托管）并进入服务循环。
///
/// 返回 `Err` 表示服务未能启动（如端口被占用）。
pub async fn spawn(tauri_handle: tauri::AppHandle) -> anyhow::Result<()> {
    // 配置目录
    let base_dir = tauri_handle
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("WebDesk"));
    let store = AppStore::new(&base_dir)?;

    let token = crate::server::generate_token();
    let state = Arc::new(AppState {
        store,
        token,
        started_at: std::time::Instant::now(),
        running: RwLock::new(vec![]),
        background: RwLock::new(vec![]),
        port: RwLock::new(0),
        app_handle: tauri_handle.clone(),
    });

    // 固定端口 3070（用户指定）
    let addr = SocketAddr::from(([127, 0, 0, 1], 3070));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            // 3070 被占（如已有一个 daemon）→ 报错提示
            return Err(anyhow::anyhow!(
                "端口 3070 绑定失败（可能已有 WebDesk 在运行）: {e}"
            ));
        }
    };
    let port = listener.local_addr()?.port();
    *state.port.write().unwrap() = port;

    // 写入 ApiConfig 供 Tauri IPC 读取
    let cfg = ApiConfig {
        port,
        token: state.token.clone(),
    };
    crate::server::set_api_config(cfg);

    // 把预置系统应用"WebLaunch 控制台"的 URL 更新为真实端口
    // （app_state::ensure_system_apps 预置时用 127.0.0.1:0 占位）
    let console_url = format!("http://127.0.0.1:{port}");
    match state.store.get("console") {
        Ok(Some(console_app)) if console_app.is_system => {
            let mut patch = console_app.clone();
            patch.url = console_url.clone();
            match state.store.update("console", patch) {
                Ok(_) => log::info!("控制台系统应用 URL 已更新: {console_url}"),
                Err(e) => log::warn!("更新控制台 URL 失败: {e}"),
            }
        }
        Ok(_) => log::warn!("未找到控制台系统应用（console），跳过 URL 更新"),
        Err(e) => log::warn!("读取控制台应用失败: {e}"),
    }

    log::info!("管理 API 已启动: http://127.0.0.1:{port}");

    // 启动路径（用户定义的默认语义）：
    // - 0 参数 / --console → 默认打开管理控制台（console）
    // - --launch=<id>     → 打开指定应用
    // - --hidden          → 后台服务模式，不弹任何窗口
    {
        let args: Vec<String> = std::env::args().collect();
        let is_hidden = args.iter().any(|a| a == "--hidden");
        if !is_hidden {
            // 决定打开哪个应用：--launch=<id> 优先，否则默认 console
            let target = args
                .iter()
                .find(|a| a.starts_with("--launch="))
                .and_then(|a| a.split('=').nth(1))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "console".to_string());

            let handle = state.app_handle.clone();
            let target_clone = target.clone();
            tauri::async_runtime::spawn(async move {
                log::info!("[启动] 自动打开应用: {target_clone}");
                if let Err(e) = crate::scheduler::launch_by_id(&handle, &target_clone).await {
                    log::error!("[启动] 自动打开应用失败: {e}");
                }
            });
        }
    }

    // 构建路由（含 API + 静态控制台托管）
    let app = build_router(state.clone());

    axum::serve(listener, app).await?;
    Ok(())
}

/// 构建路由表
///
/// 注册全部 `/api/*` 端点（应用 CRUD、launch/terminate、exec 授权、
/// 快捷方式、健康检查与平台状态），并以 `ServeDir` 托管控制台静态资源
/// （SPA 路由回退到 `index.html`）。
fn build_router(state: SharedState) -> Router {
    // CORS：允许任意来源（本地回环 API）。真正的安全边界是 Bearer token，
    // 不是 CORS——打包后 Tauri WebView 的 origin 是 tauri://localhost / http://tauri.localhost，
    // 开发时是 localhost:1420，浏览器调试是任意端口。本地 API 无需 CORS 限制。
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
        ])
        .allow_headers(tower_http::cors::Any);

    // 静态控制台资源目录：
    // - 打包后：resource_dir/webdesk/dist（tauri.conf.json bundle.resources 放入）
    // - 开发时：项目 src-frontend/dist
    let frontend_dir = frontend_dist_dir(&state);
    log::info!("托管前端目录: {}", frontend_dir.display());
    let serve_dir = tower_http::services::ServeDir::new(&frontend_dir).fallback(
        tower_http::services::ServeFile::new(frontend_dir.join("index.html")),
    );

    Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/apps", get(list_apps).post(create_app))
        .route(
            "/api/apps/{id}",
            get(get_app).put(update_app).delete(delete_app),
        )
        .route("/api/apps/{id}/restore", post(restore_app))
        .route("/api/apps/{id}/launch", post(launch_app))
        .route("/api/apps/{id}/activate", post(activate_app))
        .route("/api/apps/{id}/terminate", post(terminate_app))
        .route("/api/apps/{id}/status", get(app_status))
        .route("/api/apps/{id}/identity", get(identity_summary))
        .route("/api/apps/{id}/exec", post(exec_command))
        .route("/api/apps/{id}/exec/approve", post(exec_approve))
        .route(
            "/api/apps/{id}/shortcut",
            post(create_shortcut).delete(remove_shortcut),
        )
        .fallback_service(serve_dir)
        .layer(cors)
        .with_state(state)
}

/// 定位前端 dist 目录（开发 vs 打包）
///
/// 查找顺序：debug 构建沿可执行文件目录向上回溯寻找 `src-frontend/dist`
/// （改代码立即生效）；打包后依次尝试 `resource_dir/webdesk/dist` 与
/// `resource_dir/dist`；最后回退到相对路径 `../src-frontend/dist` 或
/// `src-frontend/dist`。
fn frontend_dist_dir(state: &SharedState) -> std::path::PathBuf {
    // 开发时（debug）：优先用项目 src-frontend/dist（改代码立即生效）
    if cfg!(debug_assertions) {
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent();
            while let Some(d) = dir {
                let candidate = d.join("src-frontend").join("dist");
                if candidate.exists() {
                    return candidate;
                }
                dir = d.parent();
            }
        }
    }
    // 打包后：resource_dir/webdesk/dist 或 resource_dir/dist
    if let Ok(res) = state.app_handle.path().resource_dir() {
        let packaged = res.join("webdesk").join("dist");
        if packaged.exists() {
            return packaged;
        }
        let direct = res.join("dist");
        if direct.exists() {
            return direct;
        }
    }
    // 兜底：当前目录 src-frontend/dist（尝试 ../）
    let rel = std::path::PathBuf::from("..")
        .join("src-frontend")
        .join("dist");
    if rel.exists() {
        return rel;
    }
    std::path::PathBuf::from("src-frontend").join("dist")
}

/// 从状态取应用（不存在返回 404）
///
/// 存储读取失败映射为 `internal` 错误，应用不存在映射为 `not_found`。
fn get_app_or_404(state: &SharedState, id: &str) -> Result<App, ApiError> {
    state
        .store
        .get(id)
        .map_err(|e| ApiError::new("internal", format!("读取应用失败: {e}")))?
        .ok_or_else(|| ApiError::new("not_found", "应用不存在"))
}

// ---------- 端点实现 ----------

/// `GET /api/health` —— 健康检查
///
/// 返回服务状态、版本号、平台标识与进程 id，供 CLI 探测 daemon 存活。
async fn health() -> Response {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "platform": os,
        "pid": std::process::id(),
    }))
    .into_response()
}

/// `GET /api/status` —— 平台运行状态
///
/// 从全局 [`crate::app_state::AppState`] 汇总运行中/后台应用 id 列表，
/// 并返回版本、运行时长、内存占用（当前恒为 0）与实际监听端口。
async fn status(State(state): State<SharedState>) -> Response {
    // 从全局 AppState 读运行状态（与 launch 同一状态源）
    let gstate = state.app_handle.state::<crate::app_state::AppState>();
    let all = gstate.list_running();
    let running = all
        .iter()
        .filter(|a| a.status == "running")
        .map(|a| a.id.clone())
        .collect();
    let background = all
        .iter()
        .filter(|a| a.status == "background")
        .map(|a| a.id.clone())
        .collect();
    let uptime = state.started_at.elapsed().as_secs();
    let port = *state.port.read().unwrap();
    let ps = PlatformStatus {
        running,
        background,
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_sec: uptime,
        memory_kb: 0,
        port,
    };
    Json(ps).into_response()
}

/// `GET /api/apps` —— 列出全部应用
async fn list_apps(State(state): State<SharedState>) -> Response {
    match state.store.list() {
        Ok(apps) => Json(apps).into_response(),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("读取应用列表失败: {e}")),
        ),
    }
}

/// `POST /api/apps` —— 创建应用
///
/// 支持部分字段创建：`name`/`url` 必填，其余字段取默认值
/// （见 [`App::from_partial`]）。成功返回 `201 Created` 与完整应用对象。
async fn create_app(
    State(state): State<SharedState>,
    Json(input): Json<serde_json::Value>,
) -> Response {
    // 支持部分字段创建（name/url 必填，其余用默认）
    match App::from_partial(&input) {
        Ok(app) => match state.store.create(app) {
            Ok(app) => (StatusCode::CREATED, Json(app)).into_response(),
            Err(e) => err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::new("internal", format!("创建应用失败: {e}")),
            ),
        },
        Err(e) => err_response(StatusCode::BAD_REQUEST, ApiError::new("invalid_input", e)),
    }
}

/// `GET /api/apps/{id}` —— 获取单个应用
async fn get_app(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    match get_app_or_404(&state, &id) {
        Ok(app) => Json(app).into_response(),
        Err(e) => err_response(StatusCode::NOT_FOUND, e),
    }
}

/// `PUT /api/apps/{id}` —— 更新应用（部分更新）
///
/// 请求体仅需包含要修改的字段：`name`/`url` 缺失或为空时用现有值补全，
/// 其余字段由 [`AppStore::update`] 按「空值不覆盖」语义合并。
/// 更新成功后若应用正在运行，异步触发 [`crate::scheduler::reload_app`]
/// 重载窗口以应用新配置。
async fn update_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(mut patch): Json<serde_json::Value>,
) -> Response {
    // 支持部分字段更新（前端只发修改的字段）。
    // from_partial 要求 name/url 必填，但 update 可能只改部分字段——缺时用现有值补上。
    if let Ok(Some(existing)) = state.store.get(&id) {
        if patch
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            patch["name"] = serde_json::Value::String(existing.name);
        }
        if patch
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            patch["url"] = serde_json::Value::String(existing.url);
        }
    }
    match App::from_partial(&patch) {
        Ok(app) => match state.store.update(&id, app) {
            Ok(Some(app)) => {
                // 配置已更新：若应用正在运行，立即重载窗口应用新配置
                let handle = state.app_handle.clone();
                let app_id = id.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::scheduler::reload_app(&handle, &app_id).await {
                        log::warn!("[update] 重载应用 {app_id} 失败: {e}");
                    }
                });
                Json(app).into_response()
            }
            Ok(None) => err_response(
                StatusCode::NOT_FOUND,
                ApiError::new("not_found", "应用不存在"),
            ),
            Err(e) => err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::new("internal", format!("更新应用失败: {e}")),
            ),
        },
        Err(e) => err_response(StatusCode::BAD_REQUEST, ApiError::new("invalid_input", e)),
    }
}

/// `DELETE /api/apps/{id}` —— 删除应用
///
/// 系统应用（`is_system`）受保护不可删除，返回 `400`；删除成功返回
/// `204 No Content`，应用不存在返回 `404`。
async fn delete_app(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let app = match get_app_or_404(&state, &id) {
        Ok(app) => app,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    if app.is_system {
        return err_response(
            StatusCode::BAD_REQUEST,
            ApiError::new("cannot_delete_system_app", "系统应用不可删除"),
        );
    }
    match state.store.delete(&id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        _ => err_response(
            StatusCode::NOT_FOUND,
            ApiError::new("not_found", "应用不存在"),
        ),
    }
}

/// `POST /api/apps/{id}/restore` —— 恢复系统应用
///
/// 当前为简化实现：若应用已存在则原样返回，否则返回 `404`。
/// 精确的按 id/类型重建逻辑计划自 M1 起实现。
async fn restore_app(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    // 系统应用恢复：管理控制台（is_system）若缺失则重建
    let apps = match state.store.list() {
        Ok(a) => a,
        Err(e) => {
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::new("internal", format!("读取应用失败: {e}")),
            )
        }
    };
    // 简化：M1 起实现精确恢复逻辑（按 id / 类型）
    if apps.iter().any(|a| a.id == id) {
        let app = get_app_or_404(&state, &id).unwrap();
        Json(app).into_response()
    } else {
        err_response(
            StatusCode::NOT_FOUND,
            ApiError::new("not_found", "应用不存在"),
        )
    }
}

/// `POST /api/apps/{id}/launch` —— 启动应用
///
/// 委托 [`crate::scheduler::launch_by_id`] 创建 WebviewWindow 并执行
/// 生命周期钩子。成功返回 `{"status":"running","windowId":<label>}`。
async fn launch_app(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    // 经 app_handle 调用真实 scheduler（创建 WebviewWindow + 钩子 + 驻留）
    match crate::scheduler::launch_by_id(&state.app_handle, &id).await {
        Ok(label) => {
            log::info!("[api] 启动应用: {id} → {label}");
            Json(serde_json::json!({"status": "running", "windowId": label})).into_response()
        }
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("启动应用失败: {e}")),
        ),
    }
}

/// `POST /api/apps/{id}/activate` —— 激活后台驻留应用
///
/// 当前为占位实现：仅校验应用存在并返回 `{"status":"active"}`，
/// 未实际唤起窗口。
async fn activate_app(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let _app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    Json(serde_json::json!({"status": "active"})).into_response()
}

/// `POST /api/apps/{id}/terminate` —— 终止应用
///
/// 委托 [`crate::scheduler::terminate_app`] 关闭应用窗口并执行退出钩子。
async fn terminate_app(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let _app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    // 经 app_handle 调用真实 scheduler 终止
    match crate::scheduler::terminate_app(&state.app_handle, &id) {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("终止应用失败: {e}")),
        ),
    }
}

/// `GET /api/apps/{id}/status` —— 查询应用运行状态
///
/// 从全局 [`crate::app_state::AppState`] 读取状态（与 launch 同一状态源），
/// 返回 `AppStatus`（窗口 id、内存、启动时刻当前均未填充）。
async fn app_status(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let _app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    // 从全局 AppState 查运行状态（与 launch 同一状态源）
    let gstate = state.app_handle.state::<crate::app_state::AppState>();
    let st = gstate.app_status(&id);
    Json(AppStatus {
        id,
        status: st,
        window_id: None,
        memory_kb: None,
        started_at: None,
    })
    .into_response()
}

/// `GET /api/apps/{id}/identity` —— 应用身份隔离摘要
///
/// 经 [`crate::identity::IdentityManager`] 统计该应用的 cookie / 扩展 /
/// 密钥等隔离资源概况。
async fn identity_summary(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    // 用真实 identity 模块统计（per-app cookie/扩展/密钥）
    let im = crate::identity::IdentityManager::new();
    Json(im.summary(&app)).into_response()
}

/// `POST /api/apps/{id}/exec` —— 网页请求执行命令（安全桥接）
///
/// body: `{ "command": "explorer.exe C:\\file.txt", "remember": bool }`
///
/// 行为：
/// - 该 `app_id + command` 已授权且允许 → 直接执行，返回
///   `{"status":"executed", ...}`；
/// - 已授权但拒绝 → 返回 `403 denied`；
/// - 未授权 → 返回 `{"status":"needs_approval", app_id, command}`，
///   由桥接 JS 弹出授权框。
async fn exec_command(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let command = match body
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
    {
        Some(c) if !c.is_empty() => c,
        _ => {
            return err_response(
                StatusCode::BAD_REQUEST,
                ApiError::new("invalid_input", "command 必填"),
            )
        }
    };

    // 检查是否已授权（app + command）
    let gstate = state.app_handle.state::<crate::app_state::AppState>();
    if let Some(decision) = gstate.auth.check(&id, &command) {
        if decision.allowed {
            // 已授权允许 → 执行
            return run_shell_command(&id, &command);
        } else {
            // 已授权拒绝 → 拒绝
            return err_response(
                StatusCode::FORBIDDEN,
                ApiError::new("denied", "该命令已被拒绝执行"),
            );
        }
    }

    // 未授权 → 需要弹授权框
    Json(serde_json::json!({
        "status": "needs_approval",
        "app_id": id,
        "command": command,
    }))
    .into_response()
}

/// `POST /api/apps/{id}/exec/approve` —— 用户授权后执行
///
/// body: `{ "command": "...", "allow": true, "remember": true }`
///
/// 行为：
/// - `remember=true` → 将授权决定（允许/拒绝）持久化到
///   [`crate::auth::AuthStore`]，此后不再弹框；
/// - `allow=true` → 执行命令；否则返回 `403 denied`。
async fn exec_approve(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let command = match body
        .get("command")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
    {
        Some(c) if !c.is_empty() => c,
        _ => {
            return err_response(
                StatusCode::BAD_REQUEST,
                ApiError::new("invalid_input", "command 必填"),
            )
        }
    };
    let allow = body.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
    let remember = body
        .get("remember")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let gstate = state.app_handle.state::<crate::app_state::AppState>();

    // 记录授权（用户勾选"不再提示"）
    if remember {
        if let Err(e) = gstate.auth.record(&id, &command, allow) {
            log::warn!("[exec] 记录授权失败: {e}");
        }
    }

    if allow {
        run_shell_command(&id, &command)
    } else {
        err_response(
            StatusCode::FORBIDDEN,
            ApiError::new("denied", "用户拒绝执行该命令"),
        )
    }
}

/// 执行命令（spawn，不阻塞等待完成；返回进程信息）
///
/// 以空白字符切分命令：首段为可执行程序，其余为参数。成功返回
/// `{"status":"executed","pid":<pid>,"command":<原命令>}`。
fn run_shell_command(app_id: &str, command: &str) -> Response {
    log::info!("[exec] 应用 {app_id} 执行命令: {command}");
    // 解析命令：以空格分隔首段为程序，其余为参数
    let mut parts = command.split_whitespace();
    let program = parts.next().unwrap_or("");
    if program.is_empty() {
        return err_response(
            StatusCode::BAD_REQUEST,
            ApiError::new("invalid_input", "命令不能为空"),
        );
    }
    let args: Vec<&str> = parts.collect();

    match std::process::Command::new(program).args(&args).spawn() {
        Ok(child) => Json(serde_json::json!({
            "status": "executed",
            "pid": child.id(),
            "command": command,
        }))
        .into_response(),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("命令执行失败: {e}")),
        ),
    }
}

/// `POST /api/apps/{id}/shortcut` —— 创建桌面快捷方式
///
/// 图标来源优先级：请求体显式指定的 `icon` > 应用已绑定的 `app.icon`。
/// 实际创建委托 [`crate::platform::create_shortcut`]，并在独立线程池中
/// 执行（`spawn_blocking`），避免阻塞 axum 响应线程——否则平台实现内部
/// 回访本服务的请求会因自锁而超时。
async fn create_shortcut(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: Option<axum::extract::Json<serde_json::Value>>,
) -> Response {
    let app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    // 图标来源优先级：body 里显式指定的 icon > 应用已绑定的 app.icon
    let body_icon =
        body.and_then(|Json(v)| v.get("icon").and_then(|i| i.as_str()).map(String::from));
    let icon = body_icon.or_else(|| {
        if app.icon.is_empty() {
            None
        } else {
            Some(app.icon.clone())
        }
    });
    let name = app.name.clone();
    // 用平台能力真实创建桌面快捷方式（在独立线程池执行，避免阻塞 axum 响应线程，
    // 否则内部的 curl 访问本服务时自锁超时）
    match tokio::task::spawn_blocking(move || {
        crate::platform::create_shortcut(&name, &id, icon.as_deref())
    })
    .await
    {
        Ok(Ok(path)) => Json(serde_json::json!({
            "created": true,
            "path": path.to_string_lossy().to_string()
        }))
        .into_response(),
        Ok(Err(e)) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("创建快捷方式失败: {e}")),
        ),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("任务执行失败: {e}")),
        ),
    }
}

/// `DELETE /api/apps/{id}/shortcut` —— 移除桌面快捷方式
///
/// 当前为占位实现：仅校验应用存在并返回 `{"removed":true}`，
/// 未实际删除快捷方式文件。
async fn remove_shortcut(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let _app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    Json(serde_json::json!({"removed": true})).into_response()
}

// ---------- 辅助 ----------

/// 构造统一错误响应（HTTP 状态码 + `ApiError` JSON 体）
fn err_response(code: StatusCode, err: ApiError) -> Response {
    (code, Json(err)).into_response()
}

#[allow(dead_code)]
fn _query_param(_q: Option<Query<Value>>) -> Value {
    Value::Null
}
