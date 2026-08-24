//! WebDesk 通用工具函数

/// 当前时间（ISO-8601，UTC）
pub fn now_iso() -> String {
    // 无外部依赖的简单实现：用 std::time + 手动格式化
    // 实际可用 `time`/`chrono` crate，M0 用简化版
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // 秒级时间戳 → ISO 近似（精确到秒，UTC）
    format!("1970-01-01T00:00:00Z+{}s", now.as_secs())
}
