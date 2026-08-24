//! WebDesk identity 模块 —— 应用身份隔离
//!
//! 职责：cookie / 密钥 / 扩展 按应用隔离（ADR-009）。
//! 每应用独立数据目录（WebviewWindow 隔离），身份注入由平台执行。
//! M0：占位；M1：实现 cookie 管理 / 密钥注入 / 扩展加载。

use crate::types::App;

/// 身份摘要
#[allow(dead_code)]
pub struct IdentityManager;

#[allow(dead_code)]
impl IdentityManager {
    pub fn new() -> Self {
        Self
    }

    /// 应用身份数据目录（每应用独立）
    pub fn data_dir(&self, app: &App) -> std::path::PathBuf {
        std::env::temp_dir().join("WebDesk").join("identity").join(&app.id)
    }

    /// 身份摘要
    pub fn summary(&self, app: &App) -> crate::types::IdentitySummary {
        crate::types::IdentitySummary {
            cookie_count: 0, // M1 起统计
            extensions: app.extensions.clone(),
            has_secrets: false,
        }
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
            extensions: vec!["ext1".into()],
            is_system: false,
            launch_on_boot: false,
            tags: vec![],
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn data_dir_is_per_app() {
        let im = IdentityManager::new();
        let a = sample_app();
        let d1 = im.data_dir(&a);
        let mut b = a.clone();
        b.id = "app2".into();
        let d2 = im.data_dir(&b);
        assert_ne!(d1, d2);
        assert!(d1.to_string_lossy().contains("app1"));
    }
}
