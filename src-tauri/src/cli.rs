//! WebLaunch 单二进制 CLI/daemon —— CLI 模式核心逻辑
//!
//! 同一个可执行文件既是 CLI 又是 daemon（服务器）：
//! - 无参数 / `--hidden` → daemon 模式（`webdesk_lib::run()`）
//! - `addweb` / `app ...` / `status` 等 → CLI 模式（本模块）
//!
//! CLI 模式：若 daemon 未运行，自动以 `--hidden` 重启自身作为 daemon
//! （单二进制自举），再通过本地 HTTP 管理 API 通信。

use std::process::Command;

use clap::{Parser, Subcommand};
use serde_json::Value;

/// WebLaunch 命令行工具
#[derive(Parser)]
#[command(
    name = "weblaunch",
    version,
    about = "WebLaunch — 把 Web 应用启动托管为桌面应用（CLI + daemon 合一）"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// CLI 子命令集合
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

/// 应用管理子命令
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

/// CLI 入口（main.rs 检测到 CLI 参数时调用）
///
/// 返回进程退出码：0 成功、1 命令执行失败、2 参数解析错误。
pub fn run_cli() -> i32 {
    // 兼容用户习惯的多字符短选项：-url xxx / -name xxx / -hook xxx / -hook_exit xxx
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

    let cli = match Cli::try_parse_from(&args) {
        Ok(c) => c,
        Err(e) => {
            // 参数错误时显示帮助并退出
            eprintln!("{e}");
            return 2;
        }
    };

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

    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("错误: {e}");
            1
        }
    }
}

// ---------- daemon 发现 ----------

/// 确保 daemon 在运行，返回其监听端口
///
/// 先探测固定端口 3070；未响应则以 `--hidden` 自举启动 daemon，
/// 并轮询等待其就绪（最长约 5 秒）。超时返回错误。
fn ensure_daemon() -> Result<u16, String> {
    const PORT: u16 = 3070;
    if ping_daemon(PORT) {
        return Ok(PORT);
    }
    eprintln!("WebLaunch daemon 未运行，正在启动…");
    spawn_daemon()?;
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if ping_daemon(PORT) {
            return Ok(PORT);
        }
    }
    Err("daemon 启动超时".into())
}

/// 探测 daemon 健康端点（500ms 超时，失败视为未运行）
fn ping_daemon(port: u16) -> bool {
    let client = reqwest::blocking::Client::new();
    client
        .get(format!("http://127.0.0.1:{port}/api/health"))
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// 以 --hidden 重启自身作为 daemon（单二进制自举）
///
/// 子进程句柄立即丢弃：daemon 生命周期独立于 CLI 进程。
fn spawn_daemon() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let child = Command::new(&exe)
        .arg("--hidden")
        .spawn()
        .map_err(|e| format!("启动 daemon 失败: {e}"))?;
    let _ = child;
    Ok(())
}

// ---------- HTTP 请求 ----------

/// 拼接本地 API 完整 URL
fn api_url(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{port}{path}")
}

/// GET 请求并解析 JSON 响应
fn get_json(port: u16, path: &str) -> Result<Value, String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(api_url(port, path))
        .send()
        .map_err(|e| format!("请求失败: {e}"))?;
    parse_response(resp)
}

/// POST 请求（可选 JSON body）并解析响应
fn post_json(port: u16, path: &str, body: Option<Value>) -> Result<Value, String> {
    let client = reqwest::blocking::Client::new();
    let mut req = client.post(api_url(port, path));
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().map_err(|e| format!("请求失败: {e}"))?;
    parse_response(resp)
}

/// 统一解析 HTTP 响应
///
/// 成功：空 body 视为 `Null`，否则解析为 JSON；
/// 失败：优先提取响应体中的 `message` 字段，回退到原始 body。
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

/// `addweb`：添加 Web 应用
///
/// 确保 daemon 运行后，经 `POST /api/apps` 创建应用；URL 自动补全
/// `http://` 前缀，钩子参数以分号分隔多条命令。成功后打印应用 id。
fn cmd_add(
    name: &str,
    url: &str,
    close: &str,
    pre: Option<&str>,
    post: Option<&str>,
    shell: &str,
    timeout: u64,
) -> Result<(), String> {
    let port = ensure_daemon()?;
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
    let resp = post_json(port, "/api/apps", Some(body))?;
    let id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    println!("✅ 已添加应用：{name}");
    println!("   id: {id}");
    println!("   url: {url}");
    Ok(())
}

/// 规范化 URL：去首尾空白，缺协议时补 `http://`
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

/// 按分号拆分钩子命令串（去空白、过滤空项）
fn split_hooks(s: &str) -> Vec<String> {
    s.split(';')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// `app list`：列出全部应用（含运行状态与系统标记）
fn cmd_list() -> Result<(), String> {
    let port = ensure_daemon()?;
    let apps = get_json(port, "/api/apps")?;
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
        let sys = if is_system { " [系统]" } else { "" };
        let status = app_running_state(port, id)?;
        println!("{:4}  {:20}  {:8}  {}{}", id, name, status, url, sys);
    }
    Ok(())
}

/// 查询单个应用的运行状态字符串
fn app_running_state(port: u16, id: &str) -> Result<String, String> {
    let st = get_json(port, &format!("/api/apps/{id}/status"))?;
    Ok(st
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string())
}

/// `app get`：以美化 JSON 输出单个应用详情
fn cmd_get(id: &str) -> Result<(), String> {
    let port = ensure_daemon()?;
    let app = get_json(port, &format!("/api/apps/{id}"))?;
    println!("{}", serde_json::to_string_pretty(&app).unwrap_or_default());
    Ok(())
}

/// `app remove`：删除应用
fn cmd_remove(id: &str) -> Result<(), String> {
    let port = ensure_daemon()?;
    let client = reqwest::blocking::Client::new();
    let resp = client
        .delete(api_url(port, &format!("/api/apps/{id}")))
        .send()
        .map_err(|e| format!("请求失败: {e}"))?;
    if resp.status().is_success() {
        println!("✅ 已删除应用：{id}");
        Ok(())
    } else {
        parse_response(resp).map(|_| ())
    }
}

/// `app launch`：启动应用
fn cmd_launch(id: &str) -> Result<(), String> {
    let port = ensure_daemon()?;
    let resp = post_json(port, &format!("/api/apps/{id}/launch"), None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    if status == "running" {
        println!("✅ 应用 {id} 已启动");
    } else {
        println!("ℹ️ 应用 {id}: {status}");
    }
    Ok(())
}

/// `app stop`：终止应用
fn cmd_stop(id: &str) -> Result<(), String> {
    let port = ensure_daemon()?;
    let resp = post_json(port, &format!("/api/apps/{id}/terminate"), None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("ℹ️ 应用 {id}: {status}");
    Ok(())
}

/// `app activate`：激活后台驻留应用
fn cmd_activate(id: &str) -> Result<(), String> {
    let port = ensure_daemon()?;
    let resp = post_json(port, &format!("/api/apps/{id}/activate"), None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("ℹ️ 应用 {id}: {status}");
    Ok(())
}

/// `app status`：查看应用状态
fn cmd_app_status(id: &str) -> Result<(), String> {
    let port = ensure_daemon()?;
    let st = get_json(port, &format!("/api/apps/{id}/status"))?;
    let status = st.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("应用 {id} 状态: {status}");
    Ok(())
}

/// `app shortcut`：创建桌面快捷方式
fn cmd_shortcut(id: &str) -> Result<(), String> {
    let port = ensure_daemon()?;
    let resp = post_json(port, &format!("/api/apps/{id}/shortcut"), None)?;
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

/// `status`：平台整体状态（版本 / 端口 / 运行中与后台应用 / 运行时长）
fn cmd_platform_status() -> Result<(), String> {
    let port = ensure_daemon()?;
    let st = get_json(port, "/api/status")?;
    println!("WebLaunch 平台状态");
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

/// 将 JSON 字符串数组格式化为逗号分隔文本（空列表显示「无」）
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

/// `console`：打开管理控制台
fn cmd_console() -> Result<(), String> {
    let port = ensure_daemon()?;
    let resp = post_json(port, "/api/apps/console/launch", None)?;
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("ℹ️ 控制台: {status}");
    Ok(())
}

/// `version`：显示版本号
fn cmd_version() -> Result<(), String> {
    println!("weblaunch {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
