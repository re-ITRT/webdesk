//! WebDesk hooks 模块 —— 生命周期钩子执行器
//!
//! 职责：执行 preLaunch / postExit 钩子命令（cmd/powershell/wsl/sh），
//! 支持阻塞/非阻塞、超时、退出码采集、stdout/stderr 日志。
//! 接口契约：`docs/design/api-contract.md` §2.6。
//! 关联 ADR：V1.2（钩子执行协议）、ADR-010（工作项生命周期）。

use std::process::Command;

use crate::types::{HookConfig, HookOptions};

/// 钩子执行结果
#[derive(Debug, Clone)]
pub struct HookResult {
    pub event: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// 执行单个钩子命令（M0 简化：同步执行；M1 起支持异步 + 超时）
pub fn run_hook(event: &str, command: &str, options: &HookOptions) -> HookResult {
    let shell = options.shell.as_str();
    let (program, args) = shell_command(shell, command);
    let start = std::time::Instant::now();
    let output = Command::new(program).args(args).output();
    let elapsed = start.elapsed();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            log::info!(
                "钩子[{event}] 完成: exit={:?}, 耗时={:?}, cmd={command}",
                out.status.code(),
                elapsed
            );
            HookResult {
                event: event.into(),
                command: command.into(),
                exit_code: out.status.code(),
                stdout,
                stderr,
            }
        }
        Err(e) => {
            log::error!("钩子[{event}] 执行失败: {e}, cmd={command}");
            HookResult {
                event: event.into(),
                command: command.into(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("{e}"),
            }
        }
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

/// 根据 shell 类型构造执行命令
fn shell_command(shell: &str, command: &str) -> (&'static str, Vec<String>) {
    match shell {
        "powershell" => ("powershell", vec!["-NoProfile", "-Command", command].into_iter().map(String::from).collect()),
        "wsl" => ("wsl", vec![command.to_string()]),
        "sh" => ("sh", vec!["-c".into(), command.into()]),
        _ => ("cmd", vec!["/C".into(), command.into()]),
    }
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
}
