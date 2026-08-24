//! WebDesk 共享类型层（单一事实源）
//!
//! 所有模块（store / server / scheduler / hooks / identity / platform / frontend）
//! 必须使用此处定义的类型，不得自行重定义。
//! 对应接口契约：`docs/design/api-contract.md` §1.4 与 §5。
//! 关联 ADR：ADR-009（应用身份）、ADR-010（工作项生命周期）、ADR-011（单引擎）。

use serde::{Deserialize, Serialize};

/// 应用配置（对应 api-contract.md §1.4）
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub url: String,
    /// "system"（跟随系统 WebView，默认）| "pinned"（锁定版本，暂仅 Windows）
    pub runtime_profile: String,
    /// "background"（关窗隐藏驻留，默认）| "quit"（关窗退出）
    pub close_action: String,
    pub hooks: HookConfig,
    pub hook_options: HookOptions,
    pub ui_controls: UiControls,
    pub injections: Injections,
    /// 扩展路径列表（本地 unpacked）
    pub extensions: Vec<String>,
    /// 系统应用标记（管理控制台本身，不可普通删除）
    pub is_system: bool,
    pub launch_on_boot: bool,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct HookConfig {
    pub pre_launch: Vec<String>,
    pub post_exit: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HookOptions {
    /// "cmd" | "powershell" | "wsl" | "sh"
    pub shell: String,
    /// 超时（毫秒）
    pub timeout_ms: u64,
    /// true=阻塞等待完成
    pub blocking: bool,
}

impl Default for HookOptions {
    fn default() -> Self {
        Self {
            shell: "cmd".into(),
            timeout_ms: 30_000,
            blocking: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct UiControls {
    pub address_bar: bool,
    pub nav_buttons: bool,
    pub refresh: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Injections {
    pub css: String,
    pub js: String,
    /// "document_start" | "document_idle"
    pub timing: String,
}

impl Default for Injections {
    fn default() -> Self {
        Self {
            css: String::new(),
            js: String::new(),
            timing: "document_idle".into(),
        }
    }
}

/// API 配置（前端经 Tauri IPC 获取，见 api-contract.md §3）
#[derive(Serialize, Clone, Debug)]
pub struct ApiConfig {
    pub port: u16,
    pub token: String,
}

/// 应用运行状态
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AppStatus {
    pub id: String,
    /// "running" | "background" | "stopped"
    pub status: String,
    pub window_id: Option<String>,
    pub memory_kb: Option<u64>,
    pub started_at: Option<String>,
}

/// 平台整体状态
#[derive(Serialize, Clone, Debug)]
pub struct PlatformStatus {
    pub running: Vec<String>,
    pub background: Vec<String>,
    pub version: String,
    pub uptime_sec: u64,
    pub memory_kb: u64,
    pub port: u16,
}

/// 钩子日志条目（M1 起由 hooks 模块构造）
#[derive(Serialize, Clone, Debug)]
#[allow(dead_code)]
pub struct HookLogEntry {
    pub timestamp: String,
    /// "pre_launch" | "post_exit"
    pub event: String,
    pub shell: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// 统一 API 错误
#[derive(Serialize, Clone, Debug)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

impl ApiError {
    pub fn new(error: &str, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

/// 身份摘要（不返回 cookie 明文）
#[derive(Serialize, Clone, Debug)]
pub struct IdentitySummary {
    pub cookie_count: u64,
    pub extensions: Vec<String>,
    pub has_secrets: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_defaults_are_sane() {
        let app = App {
            id: "test".into(),
            name: "Test".into(),
            url: "https://example.com".into(),
            runtime_profile: "system".into(),
            close_action: "background".into(),
            hooks: HookConfig::default(),
            hook_options: HookOptions::default(),
            ui_controls: UiControls::default(),
            injections: Injections::default(),
            extensions: vec![],
            is_system: false,
            launch_on_boot: false,
            tags: vec![],
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        assert_eq!(app.hook_options.timeout_ms, 30_000);
        assert_eq!(app.injections.timing, "document_idle");
        assert_eq!(app.close_action, "background");
    }

    #[test]
    fn api_error_shape() {
        let e = ApiError::new("not_found", "应用不存在");
        assert_eq!(e.error, "not_found");
        assert_eq!(e.message, "应用不存在");
    }
}
