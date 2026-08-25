//! WebDesk server 模块 —— HTTP 路由与端点实现
//!
//! 基于 axum 的本地 REST API。所有 `/api/*` 需 Bearer token。
//! 静态托管管理控制台（`src-frontend/dist`）。

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
            return Err(anyhow::anyhow!("端口 3070 绑定失败（可能已有 WebDesk 在运行）: {e}"));
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

    // 把预置系统应用"WebDesk 控制台"的 URL 更新为真实端口
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

/// 构建路由
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
    let serve_dir = tower_http::services::ServeDir::new(&frontend_dir)
        .fallback(tower_http::services::ServeFile::new(frontend_dir.join("index.html")));

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
        .route(
            "/api/apps/{id}/shortcut",
            post(create_shortcut).delete(remove_shortcut),
        )
        .fallback_service(serve_dir)
        .layer(cors)
        .with_state(state)
}

/// 定位前端 dist 目录（开发 vs 打包）
fn frontend_dist_dir(state: &SharedState) -> std::path::PathBuf {
    // 1) 打包后：resource_dir/webdesk/dist 或 resource_dir/dist
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
    // 2) 开发时：从 exe 向上找含 src-frontend/dist 的项目根
    //    target/debug/webdesk.exe → 向上到项目根 → src-frontend/dist
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
    // 3) 兜底：当前目录 src-frontend/dist（尝试 ../）
    let rel = std::path::PathBuf::from("..").join("src-frontend").join("dist");
    if rel.exists() {
        return rel;
    }
    std::path::PathBuf::from("src-frontend").join("dist")
}

// ---------- 鉴权中间件 ----------

/// 检查 Bearer token
fn check_token(state: &SharedState, headers: &axum::http::HeaderMap) -> Result<(), ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::new("unauthorized", "缺少鉴权 token"))?;
    if token == state.token {
        Ok(())
    } else {
        Err(ApiError::new("unauthorized", "token 无效"))
    }
}

/// 从状态取应用（不存在返回 404）
fn get_app_or_404(state: &SharedState, id: &str) -> Result<App, ApiError> {
    state
        .store
        .get(id)
        .map_err(|e| ApiError::new("internal", format!("读取应用失败: {e}")))?
        .ok_or_else(|| ApiError::new("not_found", "应用不存在"))
}

// ---------- 端点实现 ----------

async fn health(State(state): State<SharedState>, headers: axum::http::HeaderMap) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn status(State(state): State<SharedState>, headers: axum::http::HeaderMap) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn list_apps(State(state): State<SharedState>, headers: axum::http::HeaderMap) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
    match state.store.list() {
        Ok(apps) => Json(apps).into_response(),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("读取应用列表失败: {e}")),
        ),
    }
}

async fn create_app(
    State(state): State<SharedState>,
    headers: axum::http::HeaderMap,
    Json(input): Json<serde_json::Value>,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn get_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
    match get_app_or_404(&state, &id) {
        Ok(app) => Json(app).into_response(),
        Err(e) => err_response(StatusCode::NOT_FOUND, e),
    }
}

async fn update_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(patch): Json<App>,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
    match state.store.update(&id, patch) {
        Ok(Some(app)) => Json(app).into_response(),
        Ok(None) => err_response(
            StatusCode::NOT_FOUND,
            ApiError::new("not_found", "应用不存在"),
        ),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new("internal", format!("更新应用失败: {e}")),
        ),
    }
}

async fn delete_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn restore_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn launch_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn activate_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
    let _app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    Json(serde_json::json!({"status": "active"})).into_response()
}

async fn terminate_app(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn app_status(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
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

async fn identity_summary(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
    let app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    // 用真实 identity 模块统计（per-app cookie/扩展/密钥）
    let im = crate::identity::IdentityManager::new();
    Json(im.summary(&app)).into_response()
}

async fn create_shortcut(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
    let _app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    // M1 起调用 platform 建 .lnk；M0 占位
    Json(serde_json::json!({"created": true, "path": ""})).into_response()
}

async fn remove_shortcut(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(e) = check_token(&state, &headers) {
        return err_response(StatusCode::UNAUTHORIZED, e);
    }
    let _app = match get_app_or_404(&state, &id) {
        Ok(a) => a,
        Err(e) => return err_response(StatusCode::NOT_FOUND, e),
    };
    Json(serde_json::json!({"removed": true})).into_response()
}

// ---------- 辅助 ----------

fn err_response(code: StatusCode, err: ApiError) -> Response {
    (code, Json(err)).into_response()
}

#[allow(dead_code)]
fn _query_param(_q: Option<Query<Value>>) -> Value {
    Value::Null
}
