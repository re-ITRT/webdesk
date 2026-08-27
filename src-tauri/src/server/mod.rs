//! WebLaunch server 模块 —— 本地 HTTP 管理 API
//!
//! 职责：在 `127.0.0.1:3070` 上提供基于 axum 的本地 REST API，
//! 托管管理控制台静态资源，并将 API 配置（端口 + 会话 token）落盘
//! 至 `api.json`，供独立 CLI 进程发现并连接 daemon。
//!
//! 安全模型：API 绑定回环地址，会话 token 由 [`generate_token`] 生成并
//! 随配置持久化；当前 token 尚未由中间件强制校验（见 `routes.rs`）。
//!
//! 接口契约：`docs/design/api-contract.md`。
//! 关联 ADR：ADR-007（控制台 Web 化）、ADR-010（工作项生命周期）。

mod routes;

pub use routes::spawn;

use std::sync::OnceLock;

use crate::types::ApiConfig;

/// 全局 API 配置（启动时写入一次，前端经 Tauri IPC 读取）
static API_CONFIG: OnceLock<ApiConfig> = OnceLock::new();

/// 记录 API 配置（由 `routes::spawn` 在服务启动成功后调用）
///
/// 先落盘再写入全局：落盘供独立 CLI 进程发现 daemon，全局值供
/// 前端经 Tauri IPC 读取。
pub fn set_api_config(cfg: ApiConfig) {
    // 先落盘供 CLI 发现 daemon，再写入全局
    let _ = persist_api_config(&cfg);
    let _ = API_CONFIG.set(cfg);
}

/// 将 API 配置写入磁盘（供独立 CLI 进程读取以连接 daemon）
///
/// 写入路径为数据目录下的 `WebDesk/api.json`（数据目录不可用时
/// 回退到系统临时目录）。失败仅记录，不向上传播——配置落盘属于
/// 尽力而为的辅助能力，不应影响服务启动。
fn persist_api_config(cfg: &ApiConfig) -> anyhow::Result<()> {
    let dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("api.json");
    let json = serde_json::json!({ "port": cfg.port, "token": cfg.token });
    std::fs::write(&path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

/// 从磁盘读取 API 配置（CLI 发现 daemon 用；当前 CLI 内置同等逻辑，保留供诊断）
///
/// 任一环节失败（文件缺失、JSON 非法、字段缺失）均返回 `None`。
#[allow(dead_code)]
pub fn load_api_config_from_disk() -> Option<ApiConfig> {
    let path = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk")
        .join("api.json");
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    Some(ApiConfig {
        port: v.get("port")?.as_u64()? as u16,
        token: v.get("token")?.as_str()?.to_string(),
    })
}

/// 获取当前进程内的 API 配置（M1 起供外部使用）
///
/// 服务尚未启动时返回 `None`。
#[allow(dead_code)]
pub fn api_config() -> Option<&'static ApiConfig> {
    API_CONFIG.get()
}

/// 生成会话 token（32 字节 hex 字符串）
///
/// 以「纳秒时间戳 + 进程 id」为种子，经简单散列扩展为 32 个 hex 字符。
/// 注意：这是轻量级伪随机方案，仅用于本地回环 API 的会话标识，
/// 不适用于高安全场景。
pub fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    // 简易伪随机：时间 + 进程 id + 计数器
    let pid = std::process::id();
    let seed = format!("{nanos:x}-{pid:x}");
    // 用简单散列扩展到 32 hex
    let mut out = String::new();
    for i in 0..32 {
        let c = seed.as_bytes()[i % seed.len()];
        let v = c.wrapping_add((i as u8).wrapping_mul(7));
        out.push_str(&format!("{v:02x}"));
    }
    out
}

/// 获取 API 配置（供前端经 Tauri IPC 调用）
///
/// 服务尚未启动时返回占位配置（端口 0、空 token），由前端自行判断。
#[tauri::command]
pub fn get_api_config() -> ApiConfig {
    API_CONFIG.get().cloned().unwrap_or_else(|| ApiConfig {
        port: 0,
        token: String::new(),
    })
}
