//! WebDesk scheduler 模块 —— 应用生命周期调度
//!
//! 职责：应用启动 / 激活 / 终止 / 后台驻留；工作项驱动生命周期
//! （ADR-010：平台随第一个 app 启动、最后一个工作项结束而启停）。
//! M0：状态标记实现（挂到 server 的 running/background 集合）。
//! M1：真正创建/管理 WebviewWindow。

use std::sync::{Arc, RwLock};

use crate::types::App;

/// 运行中的应用表
#[derive(Clone, Default)]
pub struct Scheduler {
    running: Arc<RwLock<Vec<String>>>,
    background: Arc<RwLock<Vec<String>>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动应用（M0：标记运行；M1：真正建窗口）
    pub fn launch(&self, app: &App) -> anyhow::Result<String> {
        let mut running = self.running.write().unwrap();
        if !running.contains(&app.id) {
            running.push(app.id.clone());
        }
        let mut background = self.background.write().unwrap();
        background.retain(|x| x != &app.id);
        log::info!("[scheduler] 启动应用: {} ({})", app.name, app.url);
        Ok(format!("win-{}", app.id))
    }

    /// 激活已有窗口
    pub fn activate(&self, app: &App) -> anyhow::Result<()> {
        log::info!("[scheduler] 激活应用: {}", app.name);
        Ok(())
    }

    /// 彻底终止
    pub fn terminate(&self, app: &App) -> anyhow::Result<()> {
        let mut running = self.running.write().unwrap();
        running.retain(|x| x != &app.id);
        let mut background = self.background.write().unwrap();
        background.retain(|x| x != &app.id);
        log::info!("[scheduler] 终止应用: {}", app.name);
        Ok(())
    }

    /// 是否还有工作项（驻留 + 运行）——ADR-010 平台退出判定
    pub fn has_work(&self) -> bool {
        !self.running.read().unwrap().is_empty() || !self.background.read().unwrap().is_empty()
    }
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
    fn launch_terminate_tracks_work() {
        let s = Scheduler::new();
        let app = sample_app();
        assert!(!s.has_work());
        s.launch(&app).unwrap();
        assert!(s.has_work());
        s.terminate(&app).unwrap();
        assert!(!s.has_work());
    }
}