//! WebDesk hooks 模块 —— 生命周期钩子执行器
//!
//! 职责：执行 preLaunch / postExit 钩子命令（cmd/powershell/wsl/sh），
//! 支持阻塞/非阻塞、超时强制终止（进程树）、退出码采集、stdout/stderr 截断、
//! 上下文变量占位符替换、JSONL 日志落盘。
//! 关联 ADR：V1.2（钩子执行协议）、ADR-010（工作项生命周期）。

use std::io::Write;
use std::process::{Command, Stdio};

use crate::types::{HookConfig, HookLogEntry, HookOptions};

/// 钩子执行结果
///
/// 注：log_dir/append_log/substitute_vars 等为 M2 起由 server/scheduler 使用的预留 API。
#[derive(Debug, Clone)]
#[allow(dead_code)] // 字段供 M1 scheduler 读取展示
pub struct HookResult {
    pub event: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// 日志目录（平台数据目录下 WebDesk/logs）
#[allow(dead_code)]
fn log_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk")
        .join("logs")
}

/// 追加一条钩子日志（JSONL）
#[allow(dead_code)]
pub fn append_log(entry: &HookLogEntry) {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("hooks.log");
    let line = serde_json::to_string(entry).unwrap_or_default();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// 截断字符串到最大长度（避免日志爆内存）
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…[{} chars omitted]", &s[..max], s.len() - max)
    }
}

/// 上下文变量占位符替换：{app_id} {url} {port}
#[allow(dead_code)]
pub fn substitute_vars(cmd: &str, app_id: &str, url: &str, port: &str) -> String {
    cmd.replace("{app_id}", app_id)
        .replace("{url}", url)
        .replace("{port}", port)
}

/// 根据 shell 类型构造执行命令
pub fn shell_command(shell: &str, command: &str) -> (&'static str, Vec<String>) {
    match shell {
        "powershell" => (
            "powershell",
            vec!["-NoProfile", "-Command", command]
                .into_iter()
                .map(String::from)
                .collect(),
        ),
        "wsl" => ("wsl", vec![command.to_string()]),
        "sh" => ("sh", vec!["-c".into(), command.into()]),
        _ => ("cmd", vec!["/C".into(), command.into()]),
    }
}

/// 杀进程树（Windows taskkill /T /F；类 Unix kill -9）
fn kill_tree(pid: u32) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .output();
    } else {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
    }
}

/// 执行单个钩子命令（支持超时强制终止）
pub fn run_hook(event: &str, command: &str, options: &HookOptions) -> HookResult {
    let shell = options.shell.as_str();
    let (program, args) = shell_command(shell, command);
    let start = std::time::Instant::now();

    let result = match Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            let timeout = options.timeout_ms;
            let pid = child.id();
            let timed_out = wait_with_timeout(&mut child, timeout);
            if timed_out {
                kill_tree(pid);
                log::warn!("钩子[{event}] 超时（{timeout}ms）强制终止，pid={pid}");
            }
            let output = child.wait_with_output();
            match output {
                Ok(out) => HookResult {
                    event: event.into(),
                    command: command.into(),
                    exit_code: if timed_out { None } else { out.status.code() },
                    stdout: truncate(&String::from_utf8_lossy(&out.stdout), 4000),
                    stderr: if timed_out {
                        "TIMEOUT: command killed".into()
                    } else {
                        truncate(&String::from_utf8_lossy(&out.stderr), 4000)
                    },
                },
                Err(e) => HookResult {
                    event: event.into(),
                    command: command.into(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("{e}"),
                },
            }
        }
        Err(e) => HookResult {
            event: event.into(),
            command: command.into(),
            exit_code: None,
            stdout: String::new(),
            stderr: format!("{e}"),
        },
    };

    log::info!(
        "钩子[{event}] 完成: exit={:?}, 耗时={:?}, cmd={command}",
        result.exit_code,
        start.elapsed()
    );
    result
}

/// 带超时的等待（返回是否超时）
fn wait_with_timeout(child: &mut std::process::Child, timeout_ms: u64) -> bool {
    if timeout_ms == 0 {
        return false; // 0 = 无限等待（不超时）
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Some(status) = child.try_wait().unwrap_or(None) {
            let _ = status;
            return false; // 正常退出
        }
        if std::time::Instant::now() >= deadline {
            return true; // 超时
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// 执行 preLaunch 钩子列表
pub fn run_pre_launch(hooks: &HookConfig, options: &HookOptions) -> Vec<HookResult> {
    hooks
        .pre_launch
        .iter()
        .map(|cmd| run_hook("pre_launch", cmd, options))
        .collect()
}

/// 执行 postExit 钩子列表
pub fn run_post_exit(hooks: &HookConfig, options: &HookOptions) -> Vec<HookResult> {
    hooks
        .post_exit
        .iter()
        .map(|cmd| run_hook("post_exit", cmd, options))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_maps() {
        let (p, args) = shell_command("cmd", "echo hi");
        assert_eq!(p, "cmd");
        assert_eq!(args, vec!["/C", "echo hi"]);
        let (p, args) = shell_command("powershell", "Get-Date");
        assert_eq!(p, "powershell");
        assert!(args.contains(&"Get-Date".to_string()));
    }

    #[test]
    fn run_simple_hook() {
        let opts = HookOptions::default();
        let result = run_hook("test", "echo webdesk", &opts);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("webdesk"));
    }

    #[test]
    fn run_failing_hook_captures_exit() {
        let opts = HookOptions::default();
        let result = run_hook("test", "exit 3", &opts);
        assert_eq!(result.exit_code, Some(3));
    }

    #[test]
    fn timeout_kills_slow_hook() {
        let opts = HookOptions {
            shell: "cmd".into(),
            timeout_ms: 500,
            blocking: true,
        };
        let result = run_hook("test", "ping -n 30 127.0.0.1", &opts);
        assert_eq!(result.exit_code, None);
        assert!(result.stderr.contains("TIMEOUT"));
    }

    #[test]
    fn substitute_vars_works() {
        let cmd = "echo {app_id} {url} {port}";
        let out = substitute_vars(cmd, "abc", "https://x.com", "1420");
        assert_eq!(out, "echo abc https://x.com 1420");
    }

    #[test]
    fn truncate_limits_length() {
        let long = "x".repeat(5000);
        let t = truncate(&long, 4000);
        assert!(t.len() <= 4050);
        assert!(t.contains("omitted"));
    }

    #[test]
    fn append_log_writes_file() {
        let entry = HookLogEntry {
            timestamp: "2026-08-25T00:00:00Z".into(),
            event: "pre_launch".into(),
            shell: "cmd".into(),
            command: "echo hi".into(),
            exit_code: Some(0),
            stdout: "hi".into(),
            stderr: String::new(),
        };
        append_log(&entry);
        let path = log_dir().join("hooks.log");
        assert!(path.exists());
    }
}
