//! WebDesk identity 模块 —— 应用身份隔离
//!
//! 职责：cookie / 密钥 / 扩展 按应用隔离（ADR-009）。
//! 每应用独立数据目录（WebviewWindow 隔离），身份注入由平台执行。
//!
//! 目录约定（基于 `dirs::data_dir()`，非 temp）：
//!   {data_dir}/WebDesk/identity/{app_id}/
//!     cookies/    —— 该应用的 cookie 存储（每条记录一个 .json 文件）
//!     extensions/ —— 应用扩展（unpacked 目录的引用副本/链接）
//!     secrets/    —— 注入给应用的密钥（不通过 API 返回明文）
//!
//! M1：实现 cookie 管理（真实统计 / 导出 / 导入占位）、扩展加载、密钥检测。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::types::App;

/// 身份管理器
#[derive(Default)]
pub struct IdentityManager;

impl IdentityManager {
    pub fn new() -> Self {
        Self
    }

    // cookies_dir / export_cookies / import_cookies / extensions_dir / secrets_dir
    // 为 M1 起由 server/identity 端点使用的完整能力，当前预留。
    #[allow(dead_code)]
    fn _reserved_marker() {}

    /// WebDesk 身份数据根目录（所有应用共享，基于平台 data dir）
    pub fn base_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("WebDesk")
            .join("identity")
    }

    /// 应用身份数据目录（每应用独立，含 app.id）
    pub fn data_dir(&self, app: &App) -> PathBuf {
        Self::base_dir().join(&app.id)
    }

    // ---------- Cookie 管理 ----------

    /// 该应用的 cookie 存储目录（不存在则创建）
    #[allow(dead_code)]
    pub fn cookies_dir(&self, app: &App) -> Result<PathBuf> {
        let dir = self.data_dir(app).join("cookies");
        fs::create_dir_all(&dir)
            .with_context(|| format!("创建 cookie 目录失败: {}", dir.display()))?;
        Ok(dir)
    }

    /// 统计 cookie 数量（扫描 cookie 目录下的 .json 文件）
    pub fn get_cookie_count(&self, app: &App) -> u64 {
        let dir = self.data_dir(app).join("cookies");
        if !dir.is_dir() {
            return 0;
        }
        match fs::read_dir(&dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| {
                    e.path().is_file()
                        && e.path().extension().and_then(|x| x.to_str()) == Some("json")
                })
                .count() as u64,
            Err(_) => 0,
        }
    }

    /// 导出 cookie（M1 占位：返回空列表，接口完整供 M2 实现）
    #[allow(dead_code)]
    pub fn export_cookies(&self, app: &App) -> Result<Vec<serde_json::Value>> {
        let _ = app;
        Ok(Vec::new())
    }

    /// 导入 cookie（M1 占位：接收列表，返回成功导入数量；真实写盘 M2 起）
    #[allow(dead_code)]
    pub fn import_cookies(&self, app: &App, list: Vec<serde_json::Value>) -> Result<u64> {
        let _ = app;
        Ok(list.len() as u64)
    }

    // ---------- 扩展管理 ----------

    /// 该应用的扩展引用目录（M2 起存放 unpacked 扩展副本/链接）
    #[allow(dead_code)]
    pub fn extensions_dir(&self, app: &App) -> Result<PathBuf> {
        let dir = self.data_dir(app).join("extensions");
        fs::create_dir_all(&dir).with_context(|| format!("创建扩展目录失败: {}", dir.display()))?;
        Ok(dir)
    }

    /// 列出该应用实际可用的扩展（从 App.extensions 读路径 + 验证存在）
    pub fn list_extensions(&self, app: &App) -> Vec<String> {
        app.extensions
            .iter()
            .filter(|p| Path::new(p).exists())
            .cloned()
            .collect()
    }

    // ---------- 密钥管理 ----------

    /// 该应用的密钥存储目录（不存在则创建）
    #[allow(dead_code)]
    pub fn secrets_dir(&self, app: &App) -> Result<PathBuf> {
        let dir = self.data_dir(app).join("secrets");
        fs::create_dir_all(&dir).with_context(|| format!("创建密钥目录失败: {}", dir.display()))?;
        Ok(dir)
    }

    /// 该应用是否已有注入密钥（secrets 目录非空）
    pub fn has_secrets(&self, app: &App) -> bool {
        let dir = self.data_dir(app).join("secrets");
        if !dir.is_dir() {
            return false;
        }
        fs::read_dir(&dir)
            .map(|mut entries| entries.any(|e| e.is_ok()))
            .unwrap_or(false)
    }

    /// 身份摘要（真实数据：cookie 统计 + 可用扩展 + 密钥有无）
    pub fn summary(&self, app: &App) -> crate::types::IdentitySummary {
        crate::types::IdentitySummary {
            cookie_count: self.get_cookie_count(app),
            extensions: self.list_extensions(app),
            has_secrets: self.has_secrets(app),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HookConfig, HookOptions, Injections, UiControls};

    fn sample_app() -> App {
        // 每个测试用唯一 id（基于线程 id），避免并行测试争用同一目录
        let thread_id = format!("{:?}", std::thread::current().id());
        let unique = thread_id
            .chars()
            .filter(|c| !matches!(c, '(' | ')' | ' '))
            .collect::<String>();
        App {
            id: format!("app-{unique}"),
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
        }
    }

    #[test]
    fn data_dir_is_per_app_and_not_temp() {
        let im = IdentityManager::new();
        let a = sample_app();
        let d1 = im.data_dir(&a);
        let mut b = a.clone();
        b.id = format!("{}-b", a.id);
        let d2 = im.data_dir(&b);
        assert_ne!(d1, d2);
        assert!(d1.to_string_lossy().contains("app-"));
        // 基于 data_dir（而非 temp_dir）
        let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
        assert!(
            d1.starts_with(&base),
            "data_dir 应位于平台数据目录，实际: {}",
            d1.display()
        );
        assert!(!d1.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn cookie_count_counts_json_files() {
        let im = IdentityManager::new();
        let app = sample_app();
        let dir = im.cookies_dir(&app).unwrap();
        assert_eq!(im.get_cookie_count(&app), 0);

        fs::write(dir.join("session.json"), "{}").unwrap();
        fs::write(dir.join("persistent.json"), "{}").unwrap();
        fs::write(dir.join("note.txt"), "ignore").unwrap();
        assert_eq!(im.get_cookie_count(&app), 2);

        fs::remove_dir_all(im.data_dir(&app)).ok();
    }

    #[test]
    fn list_extensions_filters_missing_paths() {
        let im = IdentityManager::new();
        let mut app = sample_app();
        let existing = std::env::temp_dir().join(format!("webdesk-ext-{}", std::process::id()));
        fs::create_dir_all(&existing).unwrap();
        app.extensions = vec![
            existing.to_string_lossy().to_string(),
            "C:/definitely/not/a/real/extension".into(),
        ];
        let list = im.list_extensions(&app);
        assert_eq!(list.len(), 1);
        assert!(list[0].contains("webdesk-ext"));
        fs::remove_dir_all(&existing).ok();
    }

    #[test]
    fn secrets_detection() {
        let im = IdentityManager::new();
        let app = sample_app();
        assert!(!im.has_secrets(&app));
        let dir = im.secrets_dir(&app).unwrap();
        assert!(!im.has_secrets(&app)); // 空目录 → 无密钥
        fs::write(dir.join("apikey.txt"), "sk-xxx").unwrap();
        assert!(im.has_secrets(&app));
        fs::remove_dir_all(im.data_dir(&app)).ok();
    }
}
