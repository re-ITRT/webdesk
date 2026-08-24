//! WebDesk store 模块 —— 应用配置存储
//!
//! 基于 JSON 文件的应用配置持久化。
//! 存储目录：各平台配置目录（Windows=%APPDATA%/WebDesk/config，macOS=~/Library/Application Support/WebDesk/config，Linux=~/.config/WebDesk/config）。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::types::App;

/// 应用配置存储
pub struct AppStore {
    dir: PathBuf,
}

impl AppStore {
    /// 创建存储（目录不存在则创建）
    pub fn new(base_dir: &Path) -> Result<Self> {
        let dir = base_dir.join("WebDesk").join("config");
        fs::create_dir_all(&dir).with_context(|| format!("创建配置目录失败: {}", dir.display()))?;
        Ok(Self { dir })
    }

    /// 配置目录（M1 起供外部使用）
    #[allow(dead_code)]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// 列出所有应用
    pub fn list(&self) -> Result<Vec<App>> {
        let mut apps = Vec::new();
        if !self.dir.exists() {
            return Ok(apps);
        }
        for entry in fs::read_dir(&self.dir)
            .with_context(|| format!("读取配置目录失败: {}", self.dir.display()))?
        {
            let entry = entry?;
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(app) = self.read(&entry.path()) {
                    apps.push(app);
                }
            }
        }
        apps.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(apps)
    }

    fn read(&self, path: &Path) -> Result<App> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("读取配置文件失败: {}", path.display()))?;
        let app: App = serde_json::from_str(&content)
            .with_context(|| format!("解析配置文件失败: {}", path.display()))?;
        Ok(app)
    }

    /// 按 id 获取应用
    pub fn get(&self, id: &str) -> Result<Option<App>> {
        let path = self.path_for(id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(self.read(&path)?))
    }

    /// 创建应用（自动生成 id）
    pub fn create(&self, mut app: App) -> Result<App> {
        if app.id.is_empty() {
            app.id = Uuid::new_v4().to_string();
        }
        let now = crate::util::now_iso();
        if app.created_at.is_empty() {
            app.created_at = now.clone();
        }
        app.updated_at = now;
        self.write(&app)?;
        Ok(app)
    }

    /// 更新应用（部分更新：缺失字段保留）
    pub fn update(&self, id: &str, patch: App) -> Result<Option<App>> {
        let Some(mut existing) = self.get(id)? else {
            return Ok(None);
        };
        // 逐字段合并（None/空串表示不更新该字段）
        if !patch.name.is_empty() {
            existing.name = patch.name;
        }
        if !patch.url.is_empty() {
            existing.url = patch.url;
        }
        if !patch.runtime_profile.is_empty() {
            existing.runtime_profile = patch.runtime_profile;
        }
        if !patch.close_action.is_empty() {
            existing.close_action = patch.close_action;
        }
        if !patch.hooks.pre_launch.is_empty() {
            existing.hooks.pre_launch = patch.hooks.pre_launch;
        }
        if !patch.hooks.post_exit.is_empty() {
            existing.hooks.post_exit = patch.hooks.post_exit;
        }
        if !patch.hook_options.shell.is_empty() {
            existing.hook_options.shell = patch.hook_options.shell;
        }
        if patch.hook_options.timeout_ms != 0 {
            existing.hook_options.timeout_ms = patch.hook_options.timeout_ms;
        }
        existing.hook_options.blocking = patch.hook_options.blocking;
        if !patch.injections.css.is_empty() {
            existing.injections.css = patch.injections.css;
        }
        if !patch.injections.js.is_empty() {
            existing.injections.js = patch.injections.js;
        }
        if !patch.injections.timing.is_empty() {
            existing.injections.timing = patch.injections.timing;
        }
        if !patch.extensions.is_empty() {
            existing.extensions = patch.extensions;
        }
        existing.is_system = patch.is_system;
        existing.launch_on_boot = patch.launch_on_boot;
        if !patch.tags.is_empty() {
            existing.tags = patch.tags;
        }
        existing.updated_at = crate::util::now_iso();
        self.write(&existing)?;
        Ok(Some(existing))
    }

    /// 删除应用
    pub fn delete(&self, id: &str) -> Result<bool> {
        let path = self.path_for(id);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path).with_context(|| format!("删除配置文件失败: {}", path.display()))?;
        Ok(true)
    }

    fn write(&self, app: &App) -> Result<()> {
        let content = serde_json::to_string_pretty(app).context("序列化应用配置失败")?;
        let path = self.path_for(&app.id);
        fs::write(&path, content)
            .with_context(|| format!("写入配置文件失败: {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{App, HookConfig, HookOptions, Injections, UiControls};

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("webdesk-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_app(name: &str) -> App {
        App {
            id: String::new(),
            name: name.into(),
            url: format!("https://{name}.example.com"),
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
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn create_list_get_delete_roundtrip() {
        let dir = temp_dir();
        let store = AppStore::new(&dir).unwrap();
        let app = store.create(sample_app("GitHub")).unwrap();
        assert!(!app.id.is_empty());
        assert!(!app.created_at.is_empty());

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "GitHub");

        let got = store.get(&app.id).unwrap().unwrap();
        assert_eq!(got.url, "https://GitHub.example.com");

        assert!(store.delete(&app.id).unwrap());
        assert!(store.get(&app.id).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_preserves_unset_fields() {
        let dir = temp_dir();
        let store = AppStore::new(&dir).unwrap();
        let app = store.create(sample_app("Test")).unwrap();

        let mut patch = app.clone();
        patch.name = "Renamed".into();
        patch.url = String::new(); // 不更新 url
        let updated = store.update(&app.id, patch).unwrap().unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.url, app.url); // url 保留
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_missing_returns_none() {
        let dir = temp_dir();
        let store = AppStore::new(&dir).unwrap();
        let mut patch = sample_app("X");
        patch.id = "nope".into();
        assert!(store.update("nope", patch).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }
}
