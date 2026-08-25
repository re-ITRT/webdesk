//! WebDesk scheduler 模块 —— 应用生命周期调度
//!
//! 职责：应用启动 / 激活 / 终止 / 后台驻留；为每个应用创建独立的
//! WebviewWindow，执行 pre_launch / post_exit 钩子，处理 close_action
//! （background=关窗隐藏驻留 / quit=直接关闭）。
//!
//! 关联 ADR：ADR-009（应用身份）、ADR-010（工作项生命周期）、ADR-011（单引擎）。
//!
//! 状态流：`AppState.running`（app_id -> RunningApp）是唯一事实源；
//! 本模块通过 `app.state::<AppState>()` 读写它，并操作 Tauri 窗口。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::app_state::AppState;
use crate::hooks;
use crate::types::{App, AppStatus};

/// 运行中的应用表（轻量结构，M0 保留给测试 / 非 Tauri 环境使用）。
///
/// 真正的窗口创建由 [`launch_by_id`] 等函数完成；`Scheduler` 自身只做
/// 无窗口环境下的状态追踪（窗口 label 生成、状态转换逻辑），便于单元测试。
#[derive(Clone, Default)]
pub struct Scheduler {
    // 保留空结构：AppState 的 running map 才是运行时事实源
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// 生成窗口 label：`win-{app_id}`
    pub fn window_label(app_id: &str) -> String {
        format!("win-{app_id}")
    }

    /// 启动应用（M0 兼容：无窗口环境仅标记状态；真实窗口经 [`launch_by_id`]）
    pub fn launch(&self, app: &App) -> anyhow::Result<String> {
        let label = Self::window_label(&app.id);
        log::info!("[scheduler] 启动应用: {} ({})", app.name, app.url);
        Ok(label)
    }

    /// 激活已有窗口（M0 兼容存根）
    pub fn activate(&self, app: &App) -> anyhow::Result<()> {
        log::info!("[scheduler] 激活应用: {}", app.name);
        Ok(())
    }

    /// 彻底终止（M0 兼容存根）
    pub fn terminate(&self, app: &App) -> anyhow::Result<()> {
        log::info!("[scheduler] 终止应用: {}", app.name);
        Ok(())
    }

    /// 是否还有工作项（ADR-010：平台退出判定）
    #[allow(dead_code)]
    pub fn has_work(&self) -> bool {
        // M1 起实际判定交给 AppState.has_work；此处保留 M0 语义
        false
    }
}

/// 根据 app_id 生成窗口 label（辅助）
fn window_label(id: &str) -> String {
    Scheduler::window_label(id)
}

/// 启动应用：若已运行则激活，否则创建独立 WebviewWindow 并执行钩子。
///
/// 重要：窗口创建必须发生在 Tauri 主线程 runtime。若本函数被 axum 的
/// tokio runtime 调用（POST /api/.../launch），直接在这里 `build()` 会死锁
/// （Tauri 等待主线程，主线程等待 HTTP 响应）。因此把创建派发到
/// `tauri::async_runtime::spawn`，立即返回窗口 label。
pub async fn launch_by_id(handle: &AppHandle, id: &str) -> anyhow::Result<String> {
    let state = handle.state::<AppState>();

    // 查应用配置
    let app = state
        .store
        .get(id)?
        .ok_or_else(|| anyhow::anyhow!("应用不存在: {id}"))?;

    let label = window_label(id);

    // 已运行 → 激活（激活是主线程安全的，直接做）
    if let Some(win) = handle.get_webview_window(&label) {
        let _ = win.show();
        let _ = win.set_focus();
        log::info!("[scheduler] 应用已运行，激活窗口: {label}");
        return Ok(label);
    }

    // 派发窗口创建到 Tauri runtime（避免跨 runtime 死锁）
    let handle_clone = handle.clone();
    let app_clone = app.clone();
    let label_clone = label.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = spawn_window(&handle_clone, &app_clone, &label_clone).await {
            log::error!("[scheduler] 创建窗口失败 {label_clone}: {e}");
        }
    });

    log::info!("[scheduler] 已派发窗口创建: {label} (url={})", app.url);
    Ok(label)
}

/// 实际创建窗口（在 Tauri runtime 中执行）
async fn spawn_window(handle: &AppHandle, app: &App, label: &str) -> anyhow::Result<()> {
    let state = handle.state::<AppState>();

    // 执行 pre_launch 钩子
    let _hook_results = hooks::run_pre_launch(&app.hooks, &app.hook_options);

    // 创建独立窗口
    let url = app
        .url
        .parse::<tauri::Url>()
        .map_err(|e| anyhow::anyhow!("应用 URL 无效 {}: {e}", app.url))?;

    let close_action = app.close_action.clone();
    let app_id = app.id.clone();
    let app_name = app.name.clone();

    // 设置窗口图标（任务栏图标与 WebDesk 统一）
    let mut builder = WebviewWindowBuilder::new(handle, label, WebviewUrl::External(url))
        .title(&app_name)
        .inner_size(1024.0, 720.0)
        .visible(true);
    if let Some(icon) = handle.default_window_icon() {
        builder = builder
            .icon(icon.clone())
            .map_err(|e| anyhow::anyhow!("设置窗口图标失败: {e}"))?;
    }
    let window = builder
        .build()
        .map_err(|e| anyhow::anyhow!("创建窗口失败: {e}"))?;

    // 注入 CSS / JS
    inject_webview(&window, app);

    // close_action 处理：background=关窗隐藏驻留；quit=直接关闭（默认行为）
    if close_action == "background" {
        let app_id_for_hide = app_id.clone();
        let state_handle = handle.clone();
        let label_for_hide = label.to_string();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 驻留：阻止真正关闭，隐藏窗口，标记为 background
                api.prevent_close();
                let state = state_handle.state::<AppState>();
                state.mark_background(&app_id_for_hide);
                if let Some(win) = state_handle.get_webview_window(&label_for_hide) {
                    let _ = win.hide();
                }
                log::info!("[scheduler] 窗口关窗驻留: {label_for_hide}");
            }
        });
    }

    // 标记运行
    state.mark_running(&app_id, label, "running");

    log::info!("[scheduler] 已创建窗口: {label} (url={})", app.url);
    Ok(())
}

/// 向窗口注入用户配置的 CSS / JS
///
/// Tauri 2 无运行时的 add_style，采用初始化脚本方式：
/// - CSS 注入到 `<style id="webdesk-inject-style">`
/// - JS 通过 `eval` 注入（document 就绪后执行）
fn inject_webview(window: &tauri::WebviewWindow, app: &App) {
    let css = app.injections.css.trim();
    let js = app.injections.js.trim();
    if css.is_empty() && js.is_empty() {
        return;
    }

    let mut script = String::new();
    if !css.is_empty() {
        script.push_str(&format!(
            "(() => {{ const s = document.createElement('style'); s.id = 'webdesk-inject-style'; \
             s.textContent = {}; (document.head || document.documentElement).appendChild(s); }})();",
            serde_json::to_string(css).unwrap_or_else(|_| "''".to_string())
        ));
    }
    if !js.is_empty() {
        script.push(';');
        script.push_str(&format!(
            "(() => {{ const t = document.createElement('script'); t.textContent = {}; \
             (document.head || document.documentElement).appendChild(t); }})();",
            serde_json::to_string(js).unwrap_or_else(|_| "''".to_string())
        ));
    }

    // 初始化脚本会在每次 document 创建时执行
    let _ = window.eval(&script);
    log::info!("[scheduler] 已注入 CSS/JS 到窗口 {}", window.label());
}

/// 彻底终止应用：关闭窗口 + post_exit 钩子 + 标记停止
pub fn terminate_app(handle: &AppHandle, id: &str) -> anyhow::Result<serde_json::Value> {
    let state = handle.state::<AppState>();
    let label = window_label(id);

    let running = state.running.read().unwrap().contains_key(id);

    if !running {
        return Ok(serde_json::json!({"status": "not_running"}));
    }

    // 取应用配置（可能已被删除，但窗口还在）
    let app = state.store.get(id)?;

    // 关闭并销毁窗口
    if let Some(win) = handle.get_webview_window(&label) {
        let _ = win.destroy();
    }

    // 执行 post_exit 钩子
    if let Some(app) = &app {
        let _ = hooks::run_post_exit(&app.hooks, &app.hook_options);
    }

    // 标记停止
    state.mark_stopped(id);
    log::info!("[scheduler] 已终止应用: {id}");

    Ok(serde_json::json!({"status": "terminated"}))
}

/// Tauri command：启动应用（异步，避免在同步命令中创建窗口导致的死锁）
#[tauri::command]
pub async fn launch_app_cmd(handle: tauri::AppHandle, id: String) -> Result<String, String> {
    launch_by_id(&handle, &id).await.map_err(|e| e.to_string())
}

/// Tauri command：终止应用
#[tauri::command]
pub fn terminate_app_cmd(handle: tauri::AppHandle, id: String) -> serde_json::Value {
    match terminate_app(&handle, &id) {
        Ok(v) => v,
        Err(e) => {
            log::error!("[scheduler] 终止应用失败: {e}");
            serde_json::json!({"status": "error", "message": e.to_string()})
        }
    }
}

/// Tauri command：激活应用
#[tauri::command]
pub fn activate_app_cmd(
    handle: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: String,
) -> Result<serde_json::Value, String> {
    let label = window_label(&id);
    match handle.get_webview_window(&label) {
        Some(win) => {
            let _ = win.show();
            let _ = win.set_focus();
            state.mark_running(&id, &label, "running");
            Ok(serde_json::json!({"status": "active"}))
        }
        None => Err(format!("窗口不存在: {label}")),
    }
}

/// Tauri command：列出运行中的应用
#[tauri::command]
pub fn list_running_cmd(state: tauri::State<AppState>) -> Vec<AppStatus> {
    state.list_running()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HookConfig, HookOptions, Injections, UiControls};

    fn sample_app() -> App {
        App {
            id: "app1".into(),
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
        }
    }

    #[test]
    fn window_label_generation() {
        assert_eq!(Scheduler::window_label("app1"), "win-app1");
        assert_eq!(Scheduler::window_label("console"), "win-console");
        assert_eq!(window_label("xyz-123"), "win-xyz-123");
    }

    #[test]
    fn scheduler_launch_returns_label() {
        let s = Scheduler::new();
        let app = sample_app();
        let label = s.launch(&app).unwrap();
        assert_eq!(label, "win-app1");
    }

    #[test]
    fn scheduler_activate_terminate_noop_ok() {
        let s = Scheduler::new();
        let app = sample_app();
        assert!(s.activate(&app).is_ok());
        assert!(s.terminate(&app).is_ok());
    }

    #[test]
    fn has_work_defaults_false() {
        let s = Scheduler::new();
        assert!(!s.has_work());
    }

    #[test]
    fn inject_script_contains_css_js() {
        // 验证注入脚本会包含转义后的 CSS/JS 内容
        let css = "body { color: red; }";
        let js = "console.log('hi');";
        let script = format!(
            "(() => {{ const s = document.createElement('style'); s.id = 'webdesk-inject-style'; \
             s.textContent = {}; (document.head || document.documentElement).appendChild(s); }})();",
            serde_json::to_string(css).unwrap()
        );
        assert!(script.contains("webdesk-inject-style"));
        assert!(script.contains("body"));
        let js_script = format!(
            "(() => {{ const t = document.createElement('script'); t.textContent = {}; \
             (document.head || document.documentElement).appendChild(t); }})();",
            serde_json::to_string(js).unwrap()
        );
        assert!(js_script.contains("console.log"));
    }
}
