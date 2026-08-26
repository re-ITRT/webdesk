//! WebDesk 命令执行授权（auth）模块
//!
//! 安全模型：网页 JS 不能任意调系统命令。要执行本地命令（如用默认程序
//! 打开文件），网页调用 `window.webdesk.exec(cmd)` → 弹授权框（含"不再
//! 提示"勾选）→ 用户确认后执行。已授权的命令（app_id + command）持久化，
//! 下次不再弹框。

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// 一条授权记录
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Grant {
    pub app_id: String,
    pub command: String,
    pub allow: bool, // true=允许执行；false=拒绝
    pub remember: bool,
    pub granted_at: String,
}

/// 授权存储（进程内缓存 + 落盘）
pub struct AuthStore {
    grants: RwLock<HashMap<String, Grant>>,
    path: std::path::PathBuf,
}

/// 授权结果
#[derive(Serialize, Clone, Debug)]
pub struct AuthDecision {
    pub allowed: bool,
    pub remembered: bool,
}

impl AuthStore {
    /// 初始化授权存储
    pub fn new() -> anyhow::Result<Self> {
        let dir = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("WebDesk")
            .join("auth");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("grants.json");
        let grants = load_grants(&path)?;
        Ok(Self {
            grants: RwLock::new(grants),
            path,
        })
    }

    /// 命令的唯一键（app_id + command）
    fn key(app_id: &str, command: &str) -> String {
        format!("{app_id}\u{0}{command}")
    }

    /// 检查是否有已记录的授权决定
    pub fn check(&self, app_id: &str, command: &str) -> Option<AuthDecision> {
        let grants = self.grants.read().unwrap();
        grants
            .get(&Self::key(app_id, command))
            .map(|g| AuthDecision {
                allowed: g.allow,
                remembered: g.remember,
            })
    }

    /// 记录授权决定（用户勾选"不再提示"时）
    pub fn record(&self, app_id: &str, command: &str, allow: bool) -> anyhow::Result<()> {
        {
            let mut grants = self.grants.write().unwrap();
            grants.insert(
                Self::key(app_id, command),
                Grant {
                    app_id: app_id.to_string(),
                    command: command.to_string(),
                    allow,
                    remember: true,
                    granted_at: crate::util::now_iso(),
                },
            );
        }
        self.persist()
    }

    /// 落盘
    fn persist(&self) -> anyhow::Result<()> {
        let grants = self.grants.read().unwrap();
        let json = serde_json::to_string_pretty(&*grants)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

/// 从磁盘加载授权
fn load_grants(path: &std::path::Path) -> anyhow::Result<HashMap<String, Grant>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(path)?;
    let map: HashMap<String, Grant> = serde_json::from_str(&content)?;
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_auth() -> AuthStore {
        let dir = std::env::temp_dir().join(format!("webdesk-auth-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("grants.json");
        AuthStore {
            grants: RwLock::new(HashMap::new()),
            path,
        }
    }

    #[test]
    fn check_no_grant_returns_none() {
        let a = tmp_auth();
        assert!(a.check("app1", "explorer.exe C:/x").is_none());
    }

    #[test]
    fn record_then_check_returns_allowed() {
        let a = tmp_auth();
        a.record("app1", "explorer.exe C:/x", true).unwrap();
        let d = a.check("app1", "explorer.exe C:/x").unwrap();
        assert!(d.allowed);
        assert!(d.remembered);
    }

    #[test]
    fn key_is_app_scoped() {
        assert_ne!(AuthStore::key("a", "cmd"), AuthStore::key("b", "cmd"));
        assert_eq!(AuthStore::key("a", "x"), AuthStore::key("a", "x"));
    }
}
