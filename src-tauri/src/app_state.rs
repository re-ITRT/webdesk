//! WebLaunch 全局共享状态（AppState）
//!
//! `AppState` 由 Tauri `manage()` 注册为托管状态，各模块通过
//! `tauri::State<AppState>` 注入访问，是 scheduler / server /
//! identity / platform 等模块的共同依赖基础。
//!
//! 关联 ADR：ADR-009（应用身份）、ADR-010（工作项生命周期）、ADR-011（单引擎）。

use std::collections::HashMap;
use std::sync::RwLock;

use tauri::{AppHandle, Manager};

use crate::auth::AuthStore;
use crate::scheduler::Scheduler;
use crate::store::AppStore;
use crate::types::{App, AppStatus};

/// 全局共享状态：聚合配置存储、调度器、授权存储与运行期应用表。
///
/// 经 Tauri 状态管理（`app.manage(...)`）在进程内单例化，
/// 所有命令处理器与后台任务通过 `tauri::State<AppState>` 获取。
pub struct AppState {
    /// 应用配置持久化存储
    pub store: AppStore,
    /// 应用调度器：负责 WebviewWindow 的创建、激活与销毁
    pub scheduler: Scheduler,
    /// 命令执行授权存储（管理 API 令牌等）
    pub auth: AuthStore,
    /// 运行中应用表：app_id -> 运行信息（含后台驻留的应用）
    pub running: RwLock<HashMap<String, RunningApp>>,
}

/// 单个应用的运行期信息（窗口标签、状态、启动时刻）
#[derive(Clone, Debug)]
#[allow(dead_code)] // started_at 等字段自 M1 起用于状态展示
pub struct RunningApp {
    pub app_id: String,
    pub window_label: String,
    pub status: String, // "running"（前台运行）| "background"（后台驻留）
    pub started_at: std::time::Instant,
}

impl AppState {
    /// 初始化共享状态：创建配置存储、调度器与授权存储。
    ///
    /// 配置目录取 Tauri 的应用配置目录，获取失败时回退到
    /// 系统临时目录下的 WebLaunch 子目录。
    pub fn init(app: &AppHandle) -> anyhow::Result<Self> {
        let base_dir = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("WebDesk"));
        let store = AppStore::new(&base_dir)?;
        let scheduler = Scheduler::new();
        let auth = AuthStore::new()?;
        Ok(Self {
            store,
            scheduler,
            auth,
            running: RwLock::new(HashMap::new()),
        })
    }

    /// 将应用标记为运行中：写入（或覆盖）运行表条目并记录启动时刻。
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

    /// 将应用状态切换为后台驻留（"background"）；应用不在运行表中时静默忽略。
    pub fn mark_background(&self, app_id: &str) {
        if let Some(app) = self.running.write().unwrap().get_mut(app_id) {
            app.status = "background".to_string();
        }
    }

    /// 将应用标记为已停止：从运行表中移除对应条目。
    pub fn mark_stopped(&self, app_id: &str) {
        self.running.write().unwrap().remove(app_id);
    }

    /// 查询应用当前状态；未在运行表中时返回 "stopped"。
    #[allow(dead_code)] // M1 起由 server 模块使用
    pub fn app_status(&self, app_id: &str) -> String {
        match self.running.read().unwrap().get(app_id) {
            Some(r) => r.status.clone(),
            None => "stopped".to_string(),
        }
    }

    /// 汇总所有运行中/驻留应用的状态快照（供管理 API 与前端展示）。
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

    /// 是否存在未结束的工作项（运行中或驻留的应用）。
    ///
    /// 依据 ADR-010，作为平台退出判定的依据。
    #[allow(dead_code)]
    pub fn has_work(&self) -> bool {
        !self.running.read().unwrap().is_empty()
    }

    /// 返回当前运行中应用 id 列表。
    pub fn running_ids(&self) -> Vec<String> {
        self.running.read().unwrap().keys().cloned().collect()
    }
}

/// 预置系统应用（管理控制台）。
///
/// 平台安装后即存在一个"WebLaunch 控制台"系统应用（is_system=true），
/// 其 URL 在 M0 阶段以 127.0.0.1:0 占位，运行时由 server 模块填充实际端口。
/// 仅当存储中尚不存在该应用时创建，已存在则保持原样。
pub fn ensure_system_apps(app: &AppHandle) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let apps = state.store.list()?;

    // 仅当存储中缺少 id 为 console 的系统应用时才创建，避免重复预置。
    let has_console = apps.iter().any(|a| a.is_system && a.id == "console");
    if !has_console {
        let console = App {
            id: "console".to_string(),
            name: "WebDesk 控制台".to_string(),
            url: "http://127.0.0.1:0".to_string(), // 占位端口：实际端口在运行时由 server 模块填充
            runtime_profile: "system".to_string(),
            close_action: "background".to_string(),
            hooks: crate::types::HookConfig::default(),
            hook_options: crate::types::HookOptions::default(),
            ui_controls: crate::types::UiControls::default(),
            injections: crate::types::Injections::default(),
            icon: String::new(),
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

/// 按 id 从配置存储读取应用（便捷封装）。
pub fn get_app(state: &AppState, id: &str) -> anyhow::Result<Option<App>> {
    state.store.get(id)
}
