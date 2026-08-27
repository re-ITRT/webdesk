//! WebDesk 通用工具函数

/// 返回当前时间的 ISO-8601 字符串（UTC）。
///
/// M0 阶段为避免引入外部时间库依赖，采用简化实现：以 Unix 纪元为基准
/// 输出秒级偏移（形如 `1970-01-01T00:00:00Z+<秒数>s`），仅保证格式稳定
/// 与取值单调递增，不保证标准 ISO-8601 语义；后续可替换为
/// `time` / `chrono` 等 crate 的标准实现。
pub fn now_iso() -> String {
    // 无外部依赖：基于 std::time 计算自 Unix 纪元以来的时长。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // 秒级精度：以 "1970-01-01T00:00:00Z+<秒数>s" 形式近似表达 UTC 时间。
    format!("1970-01-01T00:00:00Z+{}s", now.as_secs())
}
