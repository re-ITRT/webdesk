//! WebDesk 钩子模块 —— 生命周期钩子执行器
//!
//! 职责：执行 preLaunch / postExit 钩子命令（支持 cmd / powershell / wsl / sh 四种
//! shell），提供阻塞 / 非阻塞执行、超时强制终止（含进程树）、退出码采集、
//! stdout/stderr 截断、上下文变量占位符替换与 JSONL 日志落盘。
//!
//! 关键设计：
//! - 多行 bat 代码自动落盘为临时 .bat 文件再执行（见 [`run_hook`]），使钩子可直接
//!   书写 `@echo off` + `start ...` 之类的批处理脚本；
//! - 超时采用轮询 `try_wait` 的方式检测（见 [`wait_with_timeout`]），超时后调用
//!   [`kill_tree`] 连进程树一并终止，避免子进程残留；
//! - 钩子执行结果统一收敛为 [`HookResult`]，调用方（scheduler / server）无需关心
//!   底层 shell 差异。
//!
//! 关联 ADR：V1.2（钩子执行协议）、ADR-010（工作项生命周期）。

use std::io::Write;
use std::process::{Command, Stdio};

use crate::types::{HookConfig, HookLogEntry, HookOptions};

/// 钩子执行结果：记录事件名、原始命令、退出码与截断后的 stdout/stderr。
///
/// 注：log_dir / append_log / substitute_vars 等为 M2 起由 server / scheduler
/// 使用的预留 API。
#[derive(Debug, Clone)]
#[allow(dead_code)] // 字段供 M1 scheduler 读取展示
pub struct HookResult {
    pub event: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// 钩子日志目录（平台数据目录下 WebDesk/logs；数据目录不可用时回退系统临时目录）
#[allow(dead_code)]
fn log_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk")
        .join("logs")
}

/// 以 JSONL 格式追加一条钩子日志到 hooks.log（目录不存在时自动创建；
/// 序列化或写入失败时静默忽略，不阻断调用方）
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

/// 将字符串截断到指定最大长度，超出部分以省略标记替代（防止超长输出撑爆内存）
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…[{} chars omitted]", &s[..max], s.len() - max)
    }
}

/// 上下文变量占位符替换：将命令中的 `{app_id}` / `{url}` / `{port}` 替换为实际值
#[allow(dead_code)]
pub fn substitute_vars(cmd: &str, app_id: &str, url: &str, port: &str) -> String {
    cmd.replace("{app_id}", app_id)
        .replace("{url}", url)
        .replace("{port}", port)
}

/// 根据 shell 类型构造可执行程序与参数列表：
/// powershell → `powershell -NoProfile -Command <cmd>`；wsl → `wsl <cmd>`；
/// sh → `sh -c <cmd>`；其余（含默认）→ `cmd /C <cmd>`
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

/// 强制终止进程树：Windows 用 `taskkill /T /F`（含子进程），类 Unix 用 `kill -9`
fn kill_tree(pid: u32) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .output();
    } else {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
    }
}

/// 执行单个钩子命令（支持超时强制终止）。
///
/// 若命令为多行 bat 代码（含换行或以 `@echo off` 开头）且 shell 为 cmd，会先写入
/// WebDesk 钩子目录下的临时 .bat 文件再执行，以便用户直接在钩子中填写批处理脚本
/// （如 `@echo off` + `start ...` 后台启动服务）；执行完毕后清理临时文件。
pub fn run_hook(event: &str, command: &str, options: &HookOptions) -> HookResult {
    let start = std::time::Instant::now();

    // 判定是否为多行 bat 代码：含换行符或以 @echo off 开头
    let is_bat_code = command.contains('\n')
        || command.contains("\r\n")
        || command.trim_start().starts_with("@echo off");
    let cmd_to_run: String;
    let mut temp_bat: Option<std::path::PathBuf> = None;

    if is_bat_code && options.shell.as_str() == "cmd" {
        // 落盘为 .bat 文件（以事件名 + 进程 ID 命名，避免并发冲突）
        let dir = bat_dir();
        let path = dir.join(format!("{event}_{}.bat", std::process::id()));
        if std::fs::write(&path, command).is_ok() {
            temp_bat = Some(path.clone());
            cmd_to_run = path.to_string_lossy().to_string();
            log::info!("[hooks] 钩子[{event}] 识别为 bat 代码，已落盘: {path:?}");
        } else {
            // 落盘失败时回退为直接执行原始命令
            cmd_to_run = command.to_string();
        }
    } else {
        cmd_to_run = command.to_string();
    }

    let result = run_hook_inner(event, &cmd_to_run, options, start);

    // 清理临时 bat 文件（失败静默忽略）
    if let Some(p) = temp_bat {
        let _ = std::fs::remove_file(&p);
    }
    result
}

/// 钩子执行内部实现：spawn 子进程 → 超时检测 → 采集输出与退出码
fn run_hook_inner(
    event: &str,
    command: &str,
    options: &HookOptions,
    start: std::time::Instant,
) -> HookResult {
    let shell = options.shell.as_str();
    let (program, args) = shell_command(shell, command);

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
                // 超时：连进程树一并强杀，防止子进程残留
                kill_tree(pid);
                log::warn!("钩子[{event}] 超时（{timeout}ms）强制终止，pid={pid}");
            }
            let output = child.wait_with_output();
            match output {
                Ok(out) => HookResult {
                    event: event.into(),
                    command: command.into(),
                    // 超时终止时无正常退出码，置 None 以区分于真实退出
                    exit_code: if timed_out { None } else { out.status.code() },
                    stdout: truncate(&String::from_utf8_lossy(&out.stdout), 4000),
                    stderr: if timed_out {
                        "TIMEOUT: command killed".into()
                    } else {
                        truncate(&String::from_utf8_lossy(&out.stderr), 4000)
                    },
                },
                // 等待输出失败（如进程已被外部终止）
                Err(e) => HookResult {
                    event: event.into(),
                    command: command.into(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("{e}"),
                },
            }
        }
        // spawn 失败（程序不存在、权限不足等）
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

/// 钩子 bat 文件目录（WebDesk 数据目录下 hooks），确保目录存在后返回
fn bat_dir() -> std::path::PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk")
        .join("hooks");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 带超时的等待：每 50ms 轮询子进程退出状态，返回是否超时。
/// timeout_ms 为 0 时视为无限等待（永不超时）。
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

/// 顺序执行 preLaunch 钩子列表，返回各钩子的执行结果
pub fn run_pre_launch(hooks: &HookConfig, options: &HookOptions) -> Vec<HookResult> {
    hooks
        .pre_launch
        .iter()
        .map(|cmd| run_hook("pre_launch", cmd, options))
        .collect()
}

/// 顺序执行 postExit 钩子列表，返回各钩子的执行结果
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
