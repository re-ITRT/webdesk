//! WebDesk server 模块 —— 本地 HTTP 管理 API
//!
//! 职责：提供 `127.0.0.1` + 随机端口 + 会话 token 的 REST API，
//! 托管管理控制台静态资源，并把 API 实现委托给各业务模块。
//! 接口契约：`docs/design/api-contract.md`。
//! 关联 ADR：ADR-007（控制台 Web 化）、ADR-010（工作项生命周期）。

mod routes;

pub use routes::spawn;

use std::sync::OnceLock;

use crate::types::ApiConfig;

/// 全局 API 配置（启动时写入，前端经 Tauri IPC 读取）
static API_CONFIG: OnceLock<ApiConfig> = OnceLock::new();

/// 记录 API 配置（routes::spawn 启动成功后调用）
pub fn set_api_config(cfg: ApiConfig) {
    // 先落盘供 CLI 发现 daemon，再写入全局
    let _ = persist_api_config(&cfg);
    let _ = API_CONFIG.set(cfg);
}

/// 把 API 配置写入磁盘（供独立 CLI 进程读取连接 daemon）
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

/// 获取当前 API 配置（M1 起供外部使用）
#[allow(dead_code)]
pub fn api_config() -> Option<&'static ApiConfig> {
    API_CONFIG.get()
}

/// 生成会话 token（32 字节 hex）
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
#[tauri::command]
pub fn get_api_config() -> ApiConfig {
    API_CONFIG.get().cloned().unwrap_or_else(|| ApiConfig {
        port: 0,
        token: String::new(),
    })
}
