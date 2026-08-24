//! WebDesk CLI —— 与 Web 管理控制台功能对等的命令行界面
//!
//! 独立轻量二进制，通过本地 HTTP 管理 API 与 daemon 通信。
//! 命令体系见 `docs/design/cli-commands.md`。

use std::process::Command;

use clap::{Parser, Subcommand};
use serde_json::Value;

/// WebDesk 命令行工具
#[derive(Parser)]
#[command(
    name = "webdesk",
    version,
    about = "WebDesk 通用 Web 应用桌面化管理平台 CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 添加 Web 应用（别名 addweb）
    #[command(alias = "addweb")]
    Add {
        /// 应用名称
        #[arg(short = 'n', long)]
        name: String,
        /// 应用 URL
        #[arg(short = 'u', long = "url")]
        url: String,
        /// 关窗行为：background（驻留）/ quit（退出）
        #[arg(long, default_value = "background")]
        close: String,
        /// 启动前钩子
        #[arg(long = "pre")]
        pre: Option<String>,
        /// 关闭后钩子
        #[arg(long = "post")]
        post: Option<String>,
        /// 钩子 shell
        #[arg(long, default_value = "cmd")]
        shell: String,
        /// 钩子超时（毫秒）
        #[arg(long, default_value_t = 30000)]
        timeout: u64,
    },
    /// 应用管理
    #[command(subcommand)]
    App(AppCommands),
    /// 平台状态
    Status,
    /// 打开管理控制台
    Console,
    /// 显示版本
    Version,
}

#[derive(Subcommand)]
enum AppCommands {
    /// 列出全部应用
    List,
    /// 查看单个应用
    Get { id: String },
    /// 删除应用
    Remove { id: String },
    /// 启动应用
    Launch { id: String },
    /// 停止应用
    Stop { id: String },
    /// 激活后台驻留应用
    Activate { id: String },
    /// 查看应用状态
    Status { id: String },
    /// 创建桌面快捷方式
    Shortcut { id: String },
}

fn main() {
    // 兼容用户习惯的多字符短选项：-url xxx / -name xxx / -hook xxx / -hook_exit xxx
    // clap 只支持单字符短选项，这里把常见多字符"短选项"改写成 --long 形式
    let args: Vec<String> = std::env::args()
        .map(|a| match a.as_str() {
            "-url" => "--url".to_string(),
            "-name" | "-n" => "--name".to_string(),
            "-hook" => "--pre".to_string(),
            "-hook_exit" | "-post" => "--post".to_string(),
            "-close" => "--close".to_string(),
            "-shell" => "--shell".to_string(),
            "-timeout" => "--timeout".to_string(),
            other => other.to_string(),
        })
        .collect();

    let cli = Cli::parse_from(args);
    let result = match &cli.command {
        Commands::Add {
            name,
            url,
            close,
            pre,
            post,
            shell,
            timeout,
        } => cmd_add(
            name,
            url,
            close,
            pre.as_deref(),
            post.as_deref(),
            shell,
            *timeout,
        ),
        Commands::App(cmd) => match cmd {
            AppCommands::List => cmd_list(),
            AppCommands::Get { id } => cmd_get(id),
            AppCommands::Remove { id } => cmd_remove(id),
            AppCommands::Launch { id } => cmd_launch(id),
            AppCommands::Stop { id } => cmd_stop(id),
            AppCommands::Activate { id } => cmd_activate(id),
            AppCommands::Status { id } => cmd_app_status(id),
            AppCommands::Shortcut { id } => cmd_shortcut(id),
        },
        Commands::Status => cmd_platform_status(),
        Commands::Console => cmd_console(),
        Commands::Version => cmd_version(),
    };
    if let Err(e) = result {
        eprintln!("错误: {e}");
        std::process::exit(1);
    }
}

// ---------- daemon 发现 ----------

/// 读取 daemon API 配置（port + token）
fn load_api_config() -> Option<(u16, String)> {
    let path = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk")
        .join("api.json");
    let content = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&content).ok()?;
    Some((
        v.get("port")?.as_u64()? as u16,
        v.get("token")?.as_str()?.to_string(),
    ))
}

/// 确保 daemon 在运行（未运行则拉起），返回 (port, token)
fn ensure_daemon() -> Result<(u16, String), String> {
    if let Some(cfg) = load_api_config() {
        // 健康检查：daemon 是否真的活着
        if ping_daemon(cfg.0, &cfg.1) {
            return Ok(cfg);
        }
    }
    // daemon 未运行 → 拉起
    eprintln!("WebDesk daemon 未运行，正在启动…");
    spawn_daemon()?;
    // 等待健康检查（最多 ~5s）
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(cfg) = load_api_config() {
            if ping_daemon(cfg.0, &cfg.1) {
                return Ok(cfg);
            }
        }
    }
    Err("daemon 启动超时".into())
}

/// ping daemon 健康检查
fn ping_daemon(port: u16, token: &str) -> bool {
    let client = reqwest::blocking::Client::new();
    client
        .get(format!("http://127.0.0.1:{port}/api/health"))
        .header("Authorization", format!("Bearer {token}"))
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// 拉起 daemon（后台运行，隐藏窗口）
fn spawn_daemon() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // CLI 旁可能没有 daemon；找 WebDesk.exe
    let daemon_path = exe
        .parent()
        .map(|p| p.join("WebDesk.exe"))
        .filter(|p| p.exists())
        .ok_or_else(|| "找不到 WebDesk.exe（请先启动 WebDesk 桌面应用）".to_string())?;
    let child = Command::new(&daemon_path)
        .arg("--hidden")
        .spawn()
        .map_err(|e| format!("启动 daemon 失败: {e}"))?;
    let _ = child;
    Ok(())
}

// ---------- HTTP 请求 ----------

fn api_url(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{port}{path}")
}

/// 发 GET 请求
fn get_json(port: u16, token: &str, path: &str) -> Result<Value, String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(api_url(port, path))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .map_err(|e| format!("请求失败: {e}"))?;
    parse_response(resp)
}

/// 发 POST 请求（可选 body）
fn post_json(port: u16, token: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    let client = reqwest::blocking::Client::new();
    let mut req = client
        .post(api_url(port, path))
        .header("Authorization", format!("Bearer {token}"));
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().map_err(|e| format!("请求失败: {e}"))?;
    parse_response(resp)
}

fn parse_response(resp: reqwest::blocking::Response) -> Result<Value, String> {
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if status.is_success() {
        if body.trim().is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str(&body).map_err(|_| format!("响应解析失败: {body}"))
        }
    } else {
        let msg = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
            .unwrap_or(body);
        Err(format!("HTTP {status}: {msg}"))
    }
}

// ---------- 命令实现 ----------

fn cmd_add(
    name: &str,
    url: &str,
    close: &str,
    pre: Option<&str>,
    post: Option<&str>,
    shell: &str,
    timeout: u64,
) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let url = normalize_url(url)?;
    let body = serde_json::json!({
        "name": name,
        "url": url,
        "close_action": close,
        "hooks": {
            "pre_launch": pre.map(split_hooks).unwrap_or_default(),
            "post_exit": post.map(split_hooks).unwrap_or_default(),
        },
        "hook_options": {
            "shell": shell,
            "timeout_ms": timeout,
            "blocking": true,
        },
    });
    let resp = post_json(port, &token, "/api/apps", Some(body))?;
    let id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    println!("✅ 已添加应用：{name}");
    println!("   id: {id}");
    println!("   url: {url}");
    Ok(())
}

/// 归一化 URL（无协议时补 http://；保留用户输入字面值含尾部斜杠）
fn normalize_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("URL 不能为空".into());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("http://{trimmed}"))
    }
}

/// 分号分隔的钩子列表
fn split_hooks(s: &str) -> Vec<String> {
    s.split(';')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

fn cmd_list() -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let apps = get_json(port, &token, "/api/apps")?;
    let arr = apps.as_array().ok_or("响应格式错误")?;
    if arr.is_empty() {
        println!("（无应用。用 `webdesk addweb -n 名称 -url 地址` 添加）");
        return Ok(());
    }
    println!("{:4}  {:20}  {:8}  {:<30}", "ID", "名称", "状态", "URL");
    println!(
        "{}",
        "-".repeat(4).to_string()
            + "  "
            + &"-".repeat(20)
            + "  "
            + &"-".repeat(8)
            + "  "
            + &"-".repeat(30)
    );
    for app in arr {
        let id = app.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let name = app.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let is_system = app
            .get("is_system")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let url = app.get("url").and_then(|v| v.as_str()).unwrap_or("?");
        let sys = if is_system { "[系统]" } else { "" };
        // 状态（查 running 列表）
        let status = app_running_state(port, &token, id)?;
        println!("{:4}  {:20}  {:8}  {} {}", id, name, status, url, sys);
    }
    Ok(())
}

/// 查询应用运行状态
fn app_running_state(port: u16, token: &str, id: &str) -> Result<String, String> {
    let st = get_json(port, token, &format!("/api/apps/{id}/status"))?;
    Ok(st
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string())
}

fn cmd_get(id: &str) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let app = get_json(port, &token, &format!("/api/apps/{id}"))?;
    println!("{}", serde_json::to_string_pretty(&app).unwrap_or_default());
    Ok(())
}

fn cmd_remove(id: &str) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let client = reqwest::blocking::Client::new();
    let resp = client
        .delete(api_url(port, &format!("/api/apps/{id}")))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .map_err(|e| format!("请求失败: {e}"))?;
    if resp.status().is_success() {
        println!("✅ 已删除应用：{id}");
        Ok(())
    } else {
        parse_response(resp).map(|_| ())
    }
}

fn cmd_launch(id: &str) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let resp = post_json(port, &token, &format!("/api/apps/{id}/launch"), None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    if status == "running" {
        println!("✅ 应用 {id} 已启动");
    } else {
        println!("ℹ️ 应用 {id}: {status}");
    }
    Ok(())
}

fn cmd_stop(id: &str) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let resp = post_json(port, &token, &format!("/api/apps/{id}/terminate"), None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("ℹ️ 应用 {id}: {status}");
    Ok(())
}

fn cmd_activate(id: &str) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let resp = post_json(port, &token, &format!("/api/apps/{id}/activate"), None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("ℹ️ 应用 {id}: {status}");
    Ok(())
}

fn cmd_app_status(id: &str) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let st = get_json(port, &token, &format!("/api/apps/{id}/status"))?;
    let status = st.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("应用 {id} 状态: {status}");
    Ok(())
}

fn cmd_shortcut(id: &str) -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let resp = post_json(port, &token, &format!("/api/apps/{id}/shortcut"), None)?;
    let created = resp
        .get("created")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if created {
        println!("✅ 已创建桌面快捷方式：{id}");
    } else {
        println!("ℹ️ 快捷方式创建结果：{resp}");
    }
    Ok(())
}

fn cmd_platform_status() -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    let st = get_json(port, &token, "/api/status")?;
    println!("WebDesk 平台状态");
    println!(
        "  版本: {}",
        st.get("version").and_then(|v| v.as_str()).unwrap_or("?")
    );
    println!(
        "  端口: {}",
        st.get("port").and_then(|v| v.as_u64()).unwrap_or(0)
    );
    println!("  运行中: {}", json_str_list(st.get("running")));
    println!("  后台: {}", json_str_list(st.get("background")));
    println!(
        "  运行时长: {}s",
        st.get("uptime_sec").and_then(|v| v.as_u64()).unwrap_or(0)
    );
    Ok(())
}

fn json_str_list(v: Option<&Value>) -> String {
    v.and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "（无）".into())
}

fn cmd_console() -> Result<(), String> {
    let (port, token) = ensure_daemon()?;
    // 启动/激活 console 系统应用
    let resp = post_json(port, &token, "/api/apps/console/launch", None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("ℹ️ 控制台: {status}");
    Ok(())
}

fn cmd_version() -> Result<(), String> {
    println!("webdesk {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
