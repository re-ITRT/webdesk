//! WebDesk 共享类型层（单一事实来源）
//!
//! 所有模块（store / server / scheduler / hooks / identity / platform / frontend）
//! 必须使用此处定义的类型，不得自行重定义。
//! 对应接口契约：`docs/design/api-contract.md` §1.4 与 §5。
//! 关联 ADR：ADR-009（应用身份）、ADR-010（工作项生命周期）、ADR-011（单引擎）。

use serde::{Deserialize, Serialize};

/// 应用配置（对应 api-contract.md §1.4）
///
/// 描述一个可被平台托管启动的 Web 应用：入口 URL、运行时配置、
/// 生命周期钩子、UI 控件与注入脚本等。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct App {
    /// 应用唯一标识（由 store 生成）
    pub id: String,
    /// 应用显示名称
    pub name: String,
    /// 入口 URL
    pub url: String,
    /// 运行时配置："system"（跟随系统 WebView，默认）| "pinned"（锁定版本，暂仅 Windows）
    pub runtime_profile: String,
    /// 窗口关闭行为："background"（关窗隐藏驻留，默认）| "quit"（关窗退出）
    pub close_action: String,
    /// 生命周期钩子配置（pre_launch / post_exit）
    pub hooks: HookConfig,
    /// 钩子执行选项（shell / 超时 / 是否阻塞）
    pub hook_options: HookOptions,
    /// 窗口 UI 控件开关（地址栏 / 导航按钮 / 刷新）
    pub ui_controls: UiControls,
    /// 页面注入配置（CSS / JS / 注入时机）
    pub injections: Injections,
    /// 应用图标（本地路径或 URL，用于窗口 / 快捷方式 / 任务栏）
    pub icon: String,
    /// 扩展路径列表（本地 unpacked 目录）
    pub extensions: Vec<String>,
    /// 系统应用标记（管理控制台本身，不可普通删除）
    pub is_system: bool,
    /// 是否随平台启动自动拉起
    pub launch_on_boot: bool,
    /// 标签列表
    pub tags: Vec<String>,
    /// 创建时间（ISO-8601）
    pub created_at: String,
    /// 最后更新时间（ISO-8601）
    pub updated_at: String,
}

/// 生命周期钩子配置：应用启动前（pre_launch）与退出后（post_exit）执行的命令列表。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct HookConfig {
    /// 启动前钩子命令列表
    pub pre_launch: Vec<String>,
    /// 退出后钩子命令列表
    pub post_exit: Vec<String>,
}

/// 钩子执行选项：指定执行 shell、超时与阻塞行为。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HookOptions {
    /// 执行 shell："cmd" | "powershell" | "wsl" | "sh"
    pub shell: String,
    /// 单条命令超时（毫秒）
    pub timeout_ms: u64,
    /// true=阻塞等待命令完成
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

/// 窗口 UI 控件开关：控制 Webview 窗口内嵌的地址栏、导航按钮与刷新按钮是否显示。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct UiControls {
    /// 是否显示地址栏
    pub address_bar: bool,
    /// 是否显示前进/后退导航按钮
    pub nav_buttons: bool,
    /// 是否显示刷新按钮
    pub refresh: bool,
}

/// 页面注入配置：向目标页面注入自定义 CSS 与 JS。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Injections {
    /// 注入的 CSS 内容
    pub css: String,
    /// 注入的 JS 内容
    pub js: String,
    /// 注入时机："document_start" | "document_idle"
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

/// 管理 API 连接配置（前端经 Tauri IPC 获取，见 api-contract.md §3）
#[derive(Serialize, Clone, Debug)]
pub struct ApiConfig {
    /// 本地 HTTP 服务监听端口
    pub port: u16,
    /// 访问令牌
    pub token: String,
}

/// 应用运行状态快照（供管理 API / 前端轮询展示）
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AppStatus {
    pub id: String,
    /// "running"（前台运行）| "background"（后台驻留）| "stopped"（已停止）
    pub status: String,
    /// 关联窗口标签
    pub window_id: Option<String>,
    /// 内存占用（KB，可选）
    pub memory_kb: Option<u64>,
    /// 启动时间（ISO-8601，可选）
    pub started_at: Option<String>,
}

/// 平台整体状态（管理 API 状态查询响应）
#[derive(Serialize, Clone, Debug)]
pub struct PlatformStatus {
    /// 前台运行中的应用 id 列表
    pub running: Vec<String>,
    /// 后台驻留中的应用 id 列表
    pub background: Vec<String>,
    /// 平台版本号
    pub version: String,
    /// 平台运行时长（秒）
    pub uptime_sec: u64,
    /// 平台进程内存占用（KB）
    pub memory_kb: u64,
    /// 管理 API 监听端口
    pub port: u16,
}

/// 钩子执行日志条目（M1 起由 hooks 模块构造）
#[derive(Serialize, Clone, Debug)]
#[allow(dead_code)]
pub struct HookLogEntry {
    /// 执行时间戳（ISO-8601）
    pub timestamp: String,
    /// 钩子事件："pre_launch" | "post_exit"
    pub event: String,
    /// 执行 shell
    pub shell: String,
    /// 执行的命令
    pub command: String,
    /// 进程退出码（未执行完为 None）
    pub exit_code: Option<i32>,
    /// 标准输出内容
    pub stdout: String,
    /// 标准错误内容
    pub stderr: String,
}

/// 统一 API 错误响应体（error=机器可读错误码，message=人类可读描述）
#[derive(Serialize, Clone, Debug)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

impl ApiError {
    /// 构造错误响应。
    pub fn new(error: &str, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

/// 身份摘要（不返回 cookie 明文，仅返回统计信息）
#[derive(Serialize, Clone, Debug)]
pub struct IdentitySummary {
    /// cookie 数量
    pub cookie_count: u64,
    /// 已安装扩展列表
    pub extensions: Vec<String>,
    /// 是否存有凭据（密码等敏感信息）
    pub has_secrets: bool,
}

impl App {
    /// 从部分字段构造 App：仅 name/url 必填，其余字段取默认值。
    ///
    /// 供 POST /api/apps 使用（Web UI 与 CLI 均只提交部分字段）；
    /// id 与时间戳留空，由 store::create 在落库时生成。
    pub fn from_partial(input: &serde_json::Value) -> Result<Self, String> {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or("字段 name 必填")?
            .to_string();
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or("字段 url 必填")?
            .to_string();

        let close_action = input
            .get("close_action")
            .and_then(|v| v.as_str())
            .unwrap_or("background")
            .to_string();
        let runtime_profile = input
            .get("runtime_profile")
            .and_then(|v| v.as_str())
            .unwrap_or("system")
            .to_string();

        // 解析 hooks：缺省时保持默认空列表。
        let mut hooks = HookConfig::default();
        if let Some(h) = input.get("hooks") {
            hooks.pre_launch = h
                .get("pre_launch")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            hooks.post_exit = h
                .get("post_exit")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
        }

        // 解析 hook_options：缺省时使用默认值（cmd / 30s / 阻塞）。
        let mut hook_options = HookOptions::default();
        if let Some(o) = input.get("hook_options") {
            if let Some(s) = o.get("shell").and_then(|v| v.as_str()) {
                hook_options.shell = s.to_string();
            }
            if let Some(t) = o.get("timeout_ms").and_then(|v| v.as_u64()) {
                hook_options.timeout_ms = t;
            }
            if let Some(b) = o.get("blocking").and_then(|v| v.as_bool()) {
                hook_options.blocking = b;
            }
        }

        // 解析 ui_controls / injections / extensions：缺失或反序列化失败时回退默认值。
        let ui_controls = input
            .get("ui_controls")
            .and_then(|v| serde_json::from_value::<UiControls>(v.clone()).ok())
            .unwrap_or_default();
        let injections = input
            .get("injections")
            .and_then(|v| serde_json::from_value::<Injections>(v.clone()).ok())
            .unwrap_or_default();
        let extensions = input
            .get("extensions")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // 应用图标：本地路径或 URL，缺省为空。
        let icon = input
            .get("icon")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let is_system = input
            .get("is_system")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let launch_on_boot = input
            .get("launch_on_boot")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let tags = input
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(App {
            id: String::new(), // id 由 store::create 在落库时自动生成
            name,
            url,
            runtime_profile,
            close_action,
            hooks,
            hook_options,
            ui_controls,
            injections,
            icon,
            extensions,
            is_system,
            launch_on_boot,
            tags,
            created_at: String::new(),
            updated_at: String::new(),
        })
    }
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
            icon: String::new(),
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
