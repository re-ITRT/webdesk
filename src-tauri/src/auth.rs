//! WebLaunch 命令执行授权（auth）模块
//!
//! 安全模型：网页 JS 不能任意调用系统命令。要执行本地命令（如用默认
//! 程序打开文件），网页调用 `window.webdesk.exec(cmd)` → 弹授权框
//! （含「不再提示」勾选）→ 用户确认后执行。已授权的命令
//! （`app_id + command`）持久化到 `grants.json`，下次不再弹框。
//!
//! 存储位置：数据目录下 `WebDesk/auth/grants.json`（数据目录不可用时
//! 回退到系统临时目录）。

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// 一条授权记录
///
/// 以 `app_id + command` 为唯一键，记录用户对该命令的最终决定
/// （允许/拒绝）及是否要求记住该决定。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Grant {
    pub app_id: String,
    pub command: String,
    pub allow: bool, // true=允许执行；false=拒绝
    pub remember: bool,
    pub granted_at: String,
}

/// 授权存储（进程内缓存 + 落盘）
///
/// 内存中以 `RwLock<HashMap>` 缓存全部授权记录，写入时同步持久化
/// 到 `grants.json`，保证重启后授权仍然生效。
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
    ///
    /// 确保存储目录存在，并从磁盘加载既有授权记录；文件缺失或
    /// 损坏时按空存储处理（损坏文件会返回错误）。
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
    ///
    /// 以 NUL 字符连接，避免 app_id 与 command 拼接后产生歧义。
    fn key(app_id: &str, command: &str) -> String {
        format!("{app_id}\u{0}{command}")
    }

    /// 检查是否有已记录的授权决定
    ///
    /// 命中返回 `AuthDecision`（含允许与否及是否被记住），未记录
    /// 返回 `None`（调用方应触发授权流程）。
    pub fn check(&self, app_id: &str, command: &str) -> Option<AuthDecision> {
        let grants = self.grants.read().unwrap();
        grants
            .get(&Self::key(app_id, command))
            .map(|g| AuthDecision {
                allowed: g.allow,
                remembered: g.remember,
            })
    }

    /// 记录授权决定（用户勾选「不再提示」时）
    ///
    /// 写入内存缓存并立即落盘；`allow` 为 false 时同样记录，
    /// 使后续请求直接返回拒绝而不再弹框。
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

    /// 将全部授权记录序列化为美化 JSON 并落盘
    fn persist(&self) -> anyhow::Result<()> {
        let grants = self.grants.read().unwrap();
        let json = serde_json::to_string_pretty(&*grants)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

/// 从磁盘加载授权记录
///
/// 文件不存在时返回空映射（首次运行）。
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

    /// 构造指向临时目录的授权存储（不落盘到真实数据目录）
    fn tmp_auth() -> AuthStore {
        let dir = std::env::temp_dir().join(format!("webdesk-auth-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("grants.json");
        AuthStore {
            grants: RwLock::new(HashMap::new()),
            path,
        }
    }

    /// 未记录的命令返回 None
    #[test]
    fn check_no_grant_returns_none() {
        let a = tmp_auth();
        assert!(a.check("app1", "explorer.exe C:/x").is_none());
    }

    /// 记录后查询返回允许且被记住
    #[test]
    fn record_then_check_returns_allowed() {
        let a = tmp_auth();
        a.record("app1", "explorer.exe C:/x", true).unwrap();
        let d = a.check("app1", "explorer.exe C:/x").unwrap();
        assert!(d.allowed);
        assert!(d.remembered);
    }

    /// 键按应用隔离：不同应用同一命令互不冲突
    #[test]
    fn key_is_app_scoped() {
        assert_ne!(AuthStore::key("a", "cmd"), AuthStore::key("b", "cmd"));
        assert_eq!(AuthStore::key("a", "x"), AuthStore::key("a", "x"));
    }
}
