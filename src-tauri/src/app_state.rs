//! WebDesk 共享应用状态（AppState）
//!
//! 由 Tauri `manage()` 管理，所有模块经 `tauri::State<AppState>` 访问。
//! 这是各模块（scheduler / server / identity / platform）的共同依赖基础。
//!
//! 关联 ADR：ADR-009（身份）、ADR-010（工作项生命周期）、ADR-011（单引擎）。

use std::collections::HashMap;
use std::sync::RwLock;

use tauri::{AppHandle, Manager};

use crate::scheduler::Scheduler;
use crate::store::AppStore;
use crate::types::{App, AppStatus};

/// 全局共享状态
pub struct AppState {
    /// 应用配置存储
    pub store: AppStore,
    /// 应用调度器（管理 WebviewWindow）
    pub scheduler: Scheduler,
    /// 运行中应用状态：app_id -> 运行信息
    pub running: RwLock<HashMap<String, RunningApp>>,
}

/// 运行中的应用信息
#[derive(Clone, Debug)]
#[allow(dead_code)] // started_at 等字段 M1 起展示
pub struct RunningApp {
    pub app_id: String,
    pub window_label: String,
    pub status: String, // "running" | "background"
    pub started_at: std::time::Instant,
}

impl AppState {
    /// 初始化（创建 store + scheduler）
    pub fn init(app: &AppHandle) -> anyhow::Result<Self> {
        let base_dir = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("WebDesk"));
        let store = AppStore::new(&base_dir)?;
        let scheduler = Scheduler::new();
        Ok(Self {
            store,
            scheduler,
            running: RwLock::new(HashMap::new()),
        })
    }

    /// 标记应用为运行中
    pub fn mark_running(&self, app_id: &str, window_label: &str, status: &str) {
        let mut running = self.running.write().unwrap();
        running.insert(
            app_id.to_string(),
            RunningApp {
                app_id: app_id.to_string(),
                window_label: window_label.to_string(),
                status: status.to_string(),
                started_at: std::time::Instant::now(),
            },
        );
    }

    /// 标记应用为后台驻留
    pub fn mark_background(&self, app_id: &str) {
        if let Some(app) = self.running.write().unwrap().get_mut(app_id) {
            app.status = "background".to_string();
        }
    }

    /// 标记应用停止
    pub fn mark_stopped(&self, app_id: &str) {
        self.running.write().unwrap().remove(app_id);
    }

    /// 查询应用状态
    #[allow(dead_code)] // M1 起 server 使用
    pub fn app_status(&self, app_id: &str) -> String {
        match self.running.read().unwrap().get(app_id) {
            Some(r) => r.status.clone(),
            None => "stopped".to_string(),
        }
    }

    /// 所有运行中/驻留应用
    pub fn list_running(&self) -> Vec<AppStatus> {
        let running = self.running.read().unwrap();
        running
            .values()
            .map(|r| AppStatus {
                id: r.app_id.clone(),
                status: r.status.clone(),
                window_id: Some(r.window_label.clone()),
                memory_kb: None,
                started_at: None,
            })
            .collect()
    }

    /// 是否有工作项（ADR-010：平台退出判定）
    #[allow(dead_code)]
    pub fn has_work(&self) -> bool {
        !self.running.read().unwrap().is_empty()
    }

    /// 取运行中的应用 id 列表
    pub fn running_ids(&self) -> Vec<String> {
        self.running.read().unwrap().keys().cloned().collect()
    }
}

/// 预置系统应用（管理控制台）
///
/// 平台安装后即存在一个"WebDesk 控制台"系统应用（isSystem=true）。
/// 指向本地管理 API 端口（M0 用 127.0.0.1:0 占位，运行时由 server 填充）。
pub fn ensure_system_apps(app: &AppHandle) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let apps = state.store.list()?;

    // 若系统控制台不存在则创建
    let has_console = apps.iter().any(|a| a.is_system && a.id == "console");
    if !has_console {
        let console = App {
            id: "console".to_string(),
            name: "WebDesk 控制台".to_string(),
            url: "http://127.0.0.1:0".to_string(), // 端口运行时由 server 填充
            runtime_profile: "system".to_string(),
            close_action: "background".to_string(),
            hooks: crate::types::HookConfig::default(),
            hook_options: crate::types::HookOptions::default(),
            ui_controls: crate::types::UiControls::default(),
            injections: crate::types::Injections::default(),
            extensions: vec![],
            is_system: true,
            launch_on_boot: false,
            tags: vec!["system".to_string()],
            created_at: crate::util::now_iso(),
            updated_at: crate::util::now_iso(),
        };
        state.store.create(console)?;
        log::info!("已预置系统应用：WebDesk 控制台");
    }
    Ok(())
}

/// 从状态取 App（helper）
pub fn get_app(state: &AppState, id: &str) -> anyhow::Result<Option<App>> {
    state.store.get(id)
}
