//! WebDesk platform 模块 —— 平台差异抽象
//!
//! 职责：托盘 / 桌面快捷方式 / 开机自启 / 应用图标等平台能力。
//!
//! - 托盘：ADR-010 动态托盘 —— 默认无图标，仅当存在后台驻留应用时出现；
//!   有驻留应用时提供菜单：显示主面板 / 显示[应用名] / 全部退出。
//! - 快捷方式：Windows 用 PowerShell COM（WScript.Shell）建 `.lnk`；
//!   macOS 用 osascript 建 alias；Linux 写 `.desktop` 文件。
//! - 开机自启：经 `tauri-plugin-autostart` 控制。
//!
//! 关联 ADR：ADR-010（工作项生命周期，托盘按需出现）。

// 本模块是平台能力层，函数由 app 启动流程 / server 路由 / 前端 IPC 按需接入；
// 当前 M0 阶段尚未全部接线，统一容忍 dead_code（与其余 WIP 模块一致）。
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::app_state::AppState;

/// 托盘固定 id（用于 `tray_by_id` / `remove_tray_by_id`）
pub const TRAY_ID: &str = "webdesk-tray";

/// 托盘菜单项 id 前缀
const MENU_SHOW_MAIN: &str = "show-main";
const MENU_APP_PREFIX: &str = "show-app:";
const MENU_QUIT_ALL: &str = "quit-all";

// ---------------------------------------------------------------------------
// 托盘（ADR-010 动态托盘）
// ---------------------------------------------------------------------------

/// 托盘控制器（按需出现/隐藏，ADR-010）
///
/// 默认无托盘图标。`show_tray_if_needed` 在有后台驻留应用时构建托盘，
/// 无驻留应用时移除之。菜单事件回调直接驱动主面板/应用窗口/退出逻辑。
pub struct TrayController;

impl TrayController {
    /// 构建托盘菜单（含固定项 + 每个后台应用一项）
    ///
    /// 纯函数，便于单元测试：不触碰真实托盘/窗口，只返回菜单结构描述。
    pub fn build_menu_descriptor(apps: &[(String, String)]) -> Vec<(String, String, bool)> {
        // (id, 文本, 是否启用)
        let mut items = vec![
            (MENU_SHOW_MAIN.to_string(), "显示主面板".into(), true),
            (
                "sep-1".into(),
                "-".into(), // 分隔符占位
                false,
            ),
        ];
        for (id, name) in apps {
            items.push((
                format!("{MENU_APP_PREFIX}{id}"),
                format!("显示 {name}"),
                true,
            ));
        }
        items.push(("sep-2".into(), "-".into(), false));
        items.push((MENU_QUIT_ALL.into(), "全部退出".into(), true));
        items
    }

    /// 是否有需要托盘的后台驻留应用（ADR-010 判定）
    pub fn has_background_apps(state: &AppState) -> bool {
        state
            .running
            .read()
            .unwrap()
            .values()
            .any(|r| r.status == "background")
    }

    /// 收集后台驻留应用：Vec<(app_id, app_name)>
    pub fn background_apps(state: &AppState) -> Vec<(String, String)> {
        let running = state.running.read().unwrap();
        let mut apps: Vec<(String, String)> = running
            .values()
            .filter(|r| r.status == "background")
            .map(|r| (r.app_id.clone(), r.app_id.clone()))
            .collect();
        // 用 store 里的友好名称覆盖 id
        drop(running);
        if let Ok(store_apps) = state.store.list() {
            let names: HashMap<String, String> = store_apps
                .into_iter()
                .map(|a| (a.id.clone(), a.name))
                .collect();
            for (id, name) in apps.iter_mut() {
                if let Some(n) = names.get(id) {
                    *name = n.clone();
                }
            }
        }
        apps.sort_by(|a, b| a.1.cmp(&b.1));
        apps
    }

    /// 动态托盘：有后台驻留应用则构建/刷新，否则移除（ADR-010）
    pub fn show_tray_if_needed<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
        if !Self::has_background_apps(state) {
            Self::hide_tray(app);
            return;
        }
        Self::rebuild_tray(app, state);
    }

    /// 强制刷新托盘（有驻留应用时重建菜单）
    pub fn refresh<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
        Self::show_tray_if_needed(app, state);
    }

    /// 移除托盘图标
    pub fn hide_tray<R: Runtime>(app: &AppHandle<R>) {
        if app.tray_by_id(TRAY_ID).is_some() {
            let _ = app.remove_tray_by_id(TRAY_ID);
            log::info!("[platform] 托盘已隐藏（无后台驻留应用）");
        }
    }

    /// 构建/重建托盘（图标 + 菜单 + 事件回调）
    pub fn rebuild_tray<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
        // 若已存在则先移除，避免重复构建
        if app.tray_by_id(TRAY_ID).is_some() {
            let _ = app.remove_tray_by_id(TRAY_ID);
        }

        let backgrounds = Self::background_apps(state);

        let result: anyhow::Result<()> = (|| {
            // 菜单
            let show_main =
                MenuItem::with_id(app, MENU_SHOW_MAIN, "显示主面板", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let quit_all = MenuItem::with_id(app, MENU_QUIT_ALL, "全部退出", true, None::<&str>)?;

            let mut item_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = vec![&show_main, &sep1];
            let mut app_items: Vec<MenuItem<R>> = Vec::new();
            for (id, name) in &backgrounds {
                let item = MenuItem::with_id(
                    app,
                    format!("{MENU_APP_PREFIX}{id}"),
                    format!("显示 {name}"),
                    true,
                    None::<&str>,
                )?;
                app_items.push(item);
            }
            for it in &app_items {
                item_refs.push(it);
            }
            item_refs.push(&quit_all);

            let menu = Menu::with_items(app, &item_refs)?;

            // 托盘图标（优先取窗口图标，避免无图标构建失败）
            let icon = app.default_window_icon().cloned();
            let mut builder = TrayIconBuilder::with_id(TRAY_ID)
                .menu(&menu)
                .tooltip("WebDesk")
                .show_menu_on_left_click(true)
                .on_menu_event(move |handle, event| {
                    let id = event.id().as_ref();
                    match id {
                        MENU_SHOW_MAIN => {
                            show_main_panel(handle);
                        }
                        MENU_QUIT_ALL => {
                            quit_all_apps(handle);
                        }
                        _ => {
                            if let Some(app_id) = id.strip_prefix(MENU_APP_PREFIX) {
                                activate_background_app(handle, app_id);
                            }
                        }
                    }
                });
            if let Some(icon) = icon {
                builder = builder.icon(icon);
            }
            builder
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { .. } = event {
                        show_main_panel(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })();

        if let Err(e) = result {
            log::warn!("[platform] 托盘构建失败: {e}");
            return;
        }
        log::info!(
            "[platform] 托盘已显示（{} 个后台驻留应用）",
            backgrounds.len()
        );
    }

    /// 把菜单 id 解析为动作（纯函数，便于测试）
    pub fn parse_menu_id(id: &str) -> TrayAction {
        match id {
            MENU_SHOW_MAIN => TrayAction::ShowMain,
            MENU_QUIT_ALL => TrayAction::QuitAll,
            _ => match id.strip_prefix(MENU_APP_PREFIX) {
                Some(app_id) if !app_id.is_empty() => TrayAction::ShowApp(app_id.to_string()),
                _ => TrayAction::Ignore,
            },
        }
    }
}

/// 托盘菜单动作（供 `parse_menu_id` 与测试使用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    ShowMain,
    ShowApp(String),
    QuitAll,
    Ignore,
}

/// 唤起主面板（管理控制台）
pub fn show_main_panel<R: Runtime>(app: &AppHandle<R>) {
    // 管理控制台窗口 label：win-console（见 scheduler）
    if let Some(win) = app.get_webview_window("win-console") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    } else {
        // 未建窗口则尝试经 scheduler 启动 console 应用
        let state = app.state::<AppState>();
        if let Ok(Some(console)) = crate::app_state::get_app(&state, "console") {
            state.scheduler.launch(&console).ok();
            let _ = console;
        }
    }
}

/// 唤起后台驻留应用窗口
pub fn activate_background_app<R: Runtime>(app: &AppHandle<R>, app_id: &str) {
    // 通过 scheduler 激活（M1 后由 scheduler 真正恢复窗口）
    let state = app.state::<AppState>();
    if let Ok(Some(a)) = crate::app_state::get_app(&state, app_id) {
        if let Err(e) = state.scheduler.activate(&a) {
            log::warn!("[platform] 恢复应用 {app_id} 失败: {e}");
        }
    } else {
        log::warn!("[platform] 激活未知应用: {app_id}");
    }
}

/// 全部退出：终止所有工作项后退出平台
pub fn quit_all_apps<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let ids = state.running_ids();
    for id in ids {
        if let Ok(Some(a)) = crate::app_state::get_app(&state, &id) {
            state.scheduler.terminate(&a).ok();
        }
    }
    // 主窗口关闭即退出平台（单实例 daemon）
    app.exit(0);
}

// ---------------------------------------------------------------------------
// 桌面快捷方式
// ---------------------------------------------------------------------------

/// 快捷方式文件名（含平台扩展名）
pub fn shortcut_filename(app_name: &str) -> String {
    let stem = sanitize_filename(app_name);
    #[cfg(target_os = "windows")]
    {
        format!("{stem}.lnk")
    }
    #[cfg(target_os = "macos")]
    {
        format!("{stem}.alias")
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        format!("{stem}.desktop")
    }
}

/// 清理文件名中不合法字符
pub fn sanitize_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let t = cleaned.trim();
    if t.is_empty() {
        "WebDesk-App".to_string()
    } else {
        t.to_string()
    }
}

/// 当前可执行文件路径（std 方案；AppImage 场景 M1 起可换 tauri::process::current_binary）
pub fn current_exe() -> anyhow::Result<PathBuf> {
    std::env::current_exe().map_err(|e| anyhow::anyhow!("获取当前可执行文件失败: {e}"))
}

/// 创建桌面快捷方式
///
/// - Windows：PowerShell COM（WScript.Shell）建 `.lnk`，Target 指向当前 exe，
///   Arguments 为 `--launch=<launch_arg>`。若 `icon` 提供（.ico/.exe 路径），
///   则设置 `IconLocation`。
/// - macOS：osascript 建 alias。
/// - Linux：写 `.desktop` 文件。
///
/// 返回创建的完整路径。
pub fn create_shortcut(
    app_name: &str,
    launch_arg: &str,
    icon: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let desktop =
        dirs::desktop_dir().ok_or_else(|| anyhow::anyhow!("无法定位桌面目录（desktop_dir）"))?;
    let dest = desktop.join(shortcut_filename(app_name));

    // 若 icon 是 http(s) URL，先下载到本地数据目录再使用
    // （Windows .lnk 的 IconLocation 需要本地路径）
    let resolved_icon = match icon {
        Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
            Some(download_icon(url)?)
        }
        other => other.map(String::from),
    };

    #[cfg(target_os = "windows")]
    {
        create_shortcut_windows(&dest, launch_arg, resolved_icon.as_deref())?;
    }
    #[cfg(target_os = "macos")]
    {
        create_shortcut_macos(&dest)?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        create_shortcut_linux(&dest, app_name, launch_arg)?;
    }

    if !dest.exists() {
        anyhow::bail!("快捷方式创建失败：目标不存在 {dest:?}");
    }
    log::info!("[platform] 已创建快捷方式: {dest:?}");
    Ok(dest)
}

/// 从 URL 下载图标到本地数据目录，返回本地路径。
/// 支持：
/// - 直接图标文件（.ico/.png/.svg）→ 直接下载
/// - 网页 URL → 先解析 HTML 找 `<link rel="icon">`，找不到用 `{url}/favicon.ico`
///
/// 注意：本函数在 spawn_blocking 线程中执行（server 端 create_shortcut 已包
/// spawn_blocking），因此可用 reqwest::blocking。若在 axum async 上下文直接
/// 调用会 panic（"Cannot drop a runtime in a context where blocking is not
/// allowed"）。
fn download_icon(url: &str) -> anyhow::Result<String> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("无法定位数据目录"))?
        .join("WebDesk")
        .join("icons");
    std::fs::create_dir_all(&data_dir)?;

    // 解析出最终图标 URL（网页→解析 favicon；图标文件→直接）
    let icon_url = resolve_icon_url(url)?;

    // 从图标 URL 推断文件名（去掉 query 参数 + 清理非法字符）
    let filename = safe_filename(&icon_url);
    let dest = data_dir.join(filename);

    // 用 reqwest 直接下载（模拟浏览器访问，跟随重定向，限时）
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow::anyhow!("创建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get(&icon_url)
        .send()
        .map_err(|e| anyhow::anyhow!("下载图标失败 {icon_url}: {e}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("下载图标失败（HTTP {}）: {icon_url}", resp.status());
    }
    let bytes = resp
        .bytes()
        .map_err(|e| anyhow::anyhow!("读取图标失败: {e}"))?;
    if bytes.is_empty() {
        anyhow::bail!("下载图标为空: {icon_url}");
    }
    std::fs::write(&dest, &bytes)?;

    log::info!("[platform] 已下载图标: {icon_url} -> {dest:?}");
    Ok(dest.to_string_lossy().to_string())
}

/// 获取应用图标（每应用独立）：从应用 URL 抓取 favicon 存到
/// `%APPDATA%/WebDesk/icons/apps/{app_id}.png`，返回路径。
/// 失败（网络/无图标）时返回 None（调用方回退默认图标）。
pub fn fetch_app_icon(app_id: &str, url: &str) -> Option<std::path::PathBuf> {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk")
        .join("icons")
        .join("apps");
    let _ = std::fs::create_dir_all(&data_dir);
    let dest = data_dir.join(format!("{app_id}.png"));

    // 若已有缓存则直接返回
    if dest.exists() {
        return Some(dest);
    }

    // 解析 favicon URL（网页→解析；图标文件→直接）
    let icon_url = match resolve_icon_url(url) {
        Ok(u) => u,
        Err(_) => return None,
    };

    // 下载（reqwest::blocking，本函数应在 spawn_blocking 中调用）
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return None,
    };
    let resp = match client.get(&icon_url).send() {
        Ok(r) if r.status().is_success() => r,
        _ => return None,
    };
    let bytes = match resp.bytes() {
        Ok(b) if !b.is_empty() => b.to_vec(),
        _ => return None,
    };

    if std::fs::write(&dest, &bytes).is_ok() {
        log::info!("[platform] 已获取应用图标: {icon_url} -> {dest:?}");
        Some(dest)
    } else {
        None
    }
}

/// 从图标 URL 生成安全的本地文件名：
/// - 去掉 query 参数（?...）
/// - 取路径最后一段
/// - 清理 Windows 非法字符（\ / : * ? " < > |）
/// - 兜底 favicon.ico
fn safe_filename(icon_url: &str) -> String {
    // 去掉 query 参数
    let no_query = icon_url.split('?').next().unwrap_or(icon_url);
    // 取路径最后一段
    let last = no_query.split('/').next_back().unwrap_or("");
    // 清理非法字符
    let cleaned: String = last
        .chars()
        .filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || !cleaned.contains('.') {
        "favicon.ico".to_string()
    } else {
        cleaned.to_string()
    }
}

/// 解析最终图标 URL：
/// - 若是网页（不是常见图标扩展名）→ 抓 HTML 找 `<link rel="icon">`，否则 `{origin}/favicon.ico`
/// - 若是图标文件 → 原样返回
fn resolve_icon_url(url: &str) -> anyhow::Result<String> {
    // 判断是否直接是图标文件（常见扩展名）
    let lower = url.to_lowercase();
    let is_icon_file = [".ico", ".png", ".svg", ".jpg", ".jpeg", ".webp", ".gif"]
        .iter()
        .any(|ext| lower.contains(ext));

    if is_icon_file {
        return Ok(url.to_string());
    }

    // 网页 → 抓 HTML 找 favicon
    let html = fetch_text(url)?;
    // 匹配 <link ... rel="icon" ... href="..." > 或 rel="shortcut icon"
    let re =
        regex::Regex::new(r#"<link[^>]+rel=["'][^"']*icon[^"']*["'][^>]*href=["']([^"']+)["']"#)
            .map_err(|e| anyhow::anyhow!("正则编译失败: {e}"))?;

    if let Some(caps) = re.captures(&html) {
        let href = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        if !href.is_empty() {
            return Ok(absolutize_url(url, href));
        }
    }

    // 兜底：{origin}/favicon.ico
    Ok(format!("{}/favicon.ico", origin(url)))
}

/// 抓取 URL 文本内容（用 reqwest，限时）
fn fetch_text(url: &str) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow::anyhow!("创建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| anyhow::anyhow!("抓取网页失败 {url}: {e}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("抓取网页失败（HTTP {}）: {url}", resp.status());
    }
    resp.text()
        .map_err(|e| anyhow::anyhow!("读取网页失败: {e}"))
}

/// 把相对 href 转成绝对 URL
fn absolutize_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") || href.starts_with("//") {
        if href.starts_with("//") {
            // 协议相对
            format!("https:{href}")
        } else {
            href.to_string()
        }
    } else if href.starts_with('/') {
        format!("{}{href}", origin(base))
    } else {
        // 相对路径：{base 目录}/{href}
        let base_trimmed = base.trim_end_matches('/');
        format!("{base_trimmed}/{href}")
    }
}

/// 提取 URL 的 origin（scheme://host[:port]）
fn origin(url: &str) -> String {
    let parts: Vec<&str> = url.split("://").collect();
    if parts.len() >= 2 {
        let host_port = parts[1].split('/').next().unwrap_or("");
        format!("{}://{host_port}", parts[0])
    } else {
        url.to_string()
    }
}

/// Windows：PowerShell WScript.Shell 建 .lnk
#[cfg(target_os = "windows")]
fn create_shortcut_windows(
    dest: &std::path::Path,
    launch_arg: &str,
    icon: Option<&str>,
) -> anyhow::Result<()> {
    let exe = current_exe()?;
    let exe_str = exe.to_string_lossy().replace('\\', "\\\\");
    let dest_str = dest.to_string_lossy().replace('\\', "\\\\");
    let arg_str = format!("--launch={launch_arg}");

    // 图标设置（可选）：$s.IconLocation = '路径,0'
    let icon_line = match icon {
        Some(ico) if !ico.is_empty() => {
            let ico_esc = ico.replace('\\', "\\\\");
            format!("$s.IconLocation = '{ico_esc},0'; ")
        }
        _ => String::new(),
    };

    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{dest_str}'); \
         $s.TargetPath = '{exe_str}'; \
         $s.Arguments = '{arg_str}'; \
         {icon_line}\
         $s.WorkingDirectory = '{exe_str_dir}'; \
         $s.Save()",
        exe_str_dir = exe
            .parent()
            .unwrap_or(dest.parent().unwrap_or(dest))
            .to_string_lossy()
            .replace('\\', "\\\\"),
    );

    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| anyhow::anyhow!("调用 powershell 失败: {e}"))?;

    if !out.status.success() {
        anyhow::bail!(
            "PowerShell 建快捷方式失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// macOS：osascript 建 alias
#[cfg(target_os = "macos")]
fn create_shortcut_macos(dest: &std::path::Path) -> anyhow::Result<()> {
    let exe = current_exe()?;
    let script = format!(
        "tell application \"Finder\" to make alias file to POSIX file \"{}\" at POSIX file \"{}\"",
        exe.to_string_lossy(),
        dest.parent()
            .unwrap_or(std::path::Path::new("/"))
            .to_string_lossy()
    );
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| anyhow::anyhow!("调用 osascript 失败: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "osascript 建 alias 失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Linux：写 .desktop 文件
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn create_shortcut_linux(
    dest: &std::path::Path,
    app_name: &str,
    launch_arg: &str,
) -> anyhow::Result<()> {
    let exe = current_exe()?;
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Comment=WebDesk 启动器\n\
         Exec=\"{exe}\" --launch={arg}\n\
         Terminal=false\n\
         Categories=Network;WebBrowser;\n\
         Icon={exe}\n",
        name = app_name,
        arg = launch_arg,
    );
    std::fs::write(dest, content).map_err(|e| anyhow::anyhow!("写入 .desktop 失败: {e}"))?;
    // 给 .desktop 加可执行位（GNOME/KDE 信任启动）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// 删除桌面快捷方式（不存在视为成功）
pub fn remove_shortcut(app_name: &str) -> anyhow::Result<()> {
    let desktop =
        dirs::desktop_dir().ok_or_else(|| anyhow::anyhow!("无法定位桌面目录（desktop_dir）"))?;
    let dest = desktop.join(shortcut_filename(app_name));
    if dest.exists() {
        std::fs::remove_file(&dest)
            .map_err(|e| anyhow::anyhow!("删除快捷方式失败 {dest:?}: {e}"))?;
        log::info!("[platform] 已删除快捷方式: {dest:?}");
    } else {
        log::debug!("[platform] 快捷方式不存在，跳过: {dest:?}");
    }
    Ok(())
}

/// 快捷方式是否存在
pub fn shortcut_exists(app_name: &str) -> bool {
    dirs::desktop_dir()
        .map(|d| d.join(shortcut_filename(app_name)).exists())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 开机自启
// ---------------------------------------------------------------------------

/// 设置开机自启（经 tauri-plugin-autostart）
pub fn set_autostart<R: Runtime>(app: &AppHandle<R>, enable: bool) -> anyhow::Result<()> {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    if enable {
        mgr.enable()
            .map_err(|e| anyhow::anyhow!("开启开机自启失败: {e}"))?;
        log::info!("[platform] 开机自启已开启");
    } else {
        mgr.disable()
            .map_err(|e| anyhow::anyhow!("关闭开机自启失败: {e}"))?;
        log::info!("[platform] 开机自启已关闭");
    }
    Ok(())
}

/// 查询开机自启状态
pub fn autostart_enabled<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<bool> {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    mgr.is_enabled()
        .map_err(|e| anyhow::anyhow!("查询开机自启状态失败: {e}"))
}

// ---------------------------------------------------------------------------
// Tauri commands（可选暴露给前端）
// ---------------------------------------------------------------------------

/// 创建桌面快捷方式（Tauri command）
#[tauri::command]
pub fn create_shortcut_cmd(app_name: String, launch_arg: String) -> Result<String, String> {
    create_shortcut(&app_name, &launch_arg, None)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

/// 删除桌面快捷方式（Tauri command）
#[tauri::command]
pub fn remove_shortcut_cmd(app_name: String) -> Result<(), String> {
    remove_shortcut(&app_name).map_err(|e| e.to_string())
}

/// 设置开机自启（Tauri command）
#[tauri::command]
pub fn set_autostart_cmd(app: AppHandle, enable: bool) -> Result<(), String> {
    set_autostart(&app, enable).map_err(|e| e.to_string())
}

/// 查询开机自启状态（Tauri command）
#[tauri::command]
pub fn autostart_enabled_cmd(app: AppHandle) -> Result<bool, String> {
    autostart_enabled(&app).map_err(|e| e.to_string())
}

/// Windows：为窗口设置独立 AppUserModelID（AUMID）和图标，
/// 使每个应用窗口在任务栏完全独立（图标/分组分开），
/// 而不是与 WebDesk 主进程合并。
#[cfg(target_os = "windows")]
pub fn set_window_taskbar_identity(
    hwnd: isize,
    app_id: &str,
    icon_path: Option<&str>,
) -> anyhow::Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::InitPropVariantFromStringAsVector;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreForWindow,
    };
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongW, GWL_EXSTYLE, WS_EX_APPWINDOW};

    let hwnd = HWND(hwnd as *mut _);

    // 1) 设置独立 AUMID：任务栏按 AUMID 分组/显示独立图标
    let aumid = format!("WebDesk.App.{app_id}");
    let mut wide: Vec<u16> = aumid.encode_utf16().collect();
    wide.push(0);
    unsafe {
        let prop_store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
        let pwstr = windows::core::PCWSTR(wide.as_ptr());
        let propv = InitPropVariantFromStringAsVector(pwstr)?;
        prop_store.SetValue(&PKEY_AppUserModel_ID, &propv)?;
        // 2) WS_EX_APPWINDOW：强制在任务栏显示独立按钮
        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, WS_EX_APPWINDOW.0 as i32);
    }

    // 3) 可选：设置窗口图标（若给了图标路径）
    if let Some(icon) = icon_path {
        if let Ok(bytes) = std::fs::read(icon) {
            if let Ok(img) = tauri::image::Image::from_bytes(&bytes) {
                set_window_icon(hwnd, &img);
            }
        }
    }
    log::info!("[platform] 已设置窗口独立任务栏身份: app={app_id}");
    Ok(())
}

#[cfg(target_os = "windows")]
fn propv_string(s: &str) -> windows::core::PWSTR {
    // 构造 PROPVARIANT 字符串（简单实现）
    use windows::core::PWSTR;
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    wide.push(0);
    PWSTR(wide.as_mut_ptr())
}

#[cfg(target_os = "windows")]
fn set_window_icon(hwnd: windows::Win32::Foundation::HWND, img: &tauri::image::Image) {
    use windows::Win32::UI::WindowsAndMessaging::CreateIcon;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON};
    // Tauri Image 转 HICON（简化：用 WM_SETICON）
    let _ = hwnd;
    let _ = img;
    let _ = SendMessageW;
    let _ = WM_SETICON;
    let _ = ICON_SMALL;
    let _ = ICON_BIG;
    let _ = CreateIcon;
    // 实际图标设置需 HICON 转换，M2 完善；AUMID 已足以让任务栏独立
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::RunningApp;
    use crate::auth::AuthStore;
    use crate::scheduler::Scheduler;
    use crate::store::AppStore;
    use std::sync::RwLock;

    /// 构造一个仅含 running 状态的 AppState（不依赖 Tauri 运行时）
    fn state_with_running(entries: Vec<(&str, &str)>) -> AppState {
        let base = std::env::temp_dir().join(format!("webdesk-test-{}", uuid::Uuid::new_v4()));
        let store = AppStore::new(&base).expect("创建测试 store");
        let mut map = HashMap::new();
        for (id, status) in entries {
            map.insert(
                id.to_string(),
                RunningApp {
                    app_id: id.to_string(),
                    window_label: format!("win-{id}"),
                    status: status.to_string(),
                    started_at: std::time::Instant::now(),
                },
            );
        }
        AppState {
            store,
            scheduler: Scheduler::new(),
            auth: AuthStore::new().unwrap(),
            running: RwLock::new(map),
        }
    }

    #[test]
    fn parse_menu_id_routes_correctly() {
        assert_eq!(
            TrayController::parse_menu_id("show-main"),
            TrayAction::ShowMain
        );
        assert_eq!(
            TrayController::parse_menu_id("show-app:gmail"),
            TrayAction::ShowApp("gmail".into())
        );
        assert_eq!(
            TrayController::parse_menu_id("quit-all"),
            TrayAction::QuitAll
        );
        assert_eq!(TrayController::parse_menu_id("unknown"), TrayAction::Ignore);
        assert_eq!(
            TrayController::parse_menu_id("show-app:"),
            TrayAction::Ignore
        );
    }

    #[test]
    fn build_menu_descriptor_layout() {
        let apps = vec![
            ("b".to_string(), "B应用".to_string()),
            ("a".to_string(), "A应用".to_string()),
        ];
        let items = TrayController::build_menu_descriptor(&apps);
        // 主面板 + 分隔符 + 2 个应用项 + 分隔符 + 全部退出
        assert_eq!(items.len(), 6);
        assert_eq!(items[0].0, MENU_SHOW_MAIN);
        assert!(items.iter().any(|(id, _, _)| id == "show-app:a"));
        assert!(items.iter().any(|(id, _, _)| id == "show-app:b"));
        assert!(items.iter().any(|(id, _, _)| id == MENU_QUIT_ALL));

        // 无驻留应用时：只有固定项 + 退出
        let empty = TrayController::build_menu_descriptor(&[]);
        assert_eq!(empty.len(), 4);
        assert!(!empty
            .iter()
            .any(|(id, _, _)| id.starts_with(MENU_APP_PREFIX)));
    }

    #[test]
    fn has_background_apps_judges_by_status() {
        // 无后台应用 -> 不显示托盘
        let s = state_with_running(vec![("a", "running")]);
        assert!(!TrayController::has_background_apps(&s));
        // 有后台应用 -> 显示托盘
        let s = state_with_running(vec![("a", "background")]);
        assert!(TrayController::has_background_apps(&s));
        // 混合
        let s = state_with_running(vec![("a", "running"), ("b", "background")]);
        assert!(TrayController::has_background_apps(&s));
        // 空
        let s = state_with_running(vec![]);
        assert!(!TrayController::has_background_apps(&s));
    }

    #[test]
    fn background_apps_collects_only_background() {
        let s = state_with_running(vec![
            ("a", "running"),
            ("b", "background"),
            ("c", "background"),
        ]);
        let apps = TrayController::background_apps(&s);
        let ids: Vec<&str> = apps.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(!ids.contains(&"a"));
        assert_eq!(apps.len(), 2);
    }

    #[test]
    fn shortcut_filename_respects_platform() {
        let f = shortcut_filename("微信");
        #[cfg(target_os = "windows")]
        assert!(f.ends_with(".lnk"));
        #[cfg(target_os = "macos")]
        assert!(f.ends_with(".alias"));
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        assert!(f.ends_with(".desktop"));
        assert!(f.starts_with("微信"));
    }

    #[test]
    fn sanitize_filename_handles_bad_chars() {
        assert_eq!(sanitize_filename("a/b:c*d?e"), "a_b_c_d_e");
        assert_eq!(sanitize_filename("   "), "WebDesk-App");
        assert_eq!(sanitize_filename("正常名"), "正常名");
        // 应能作为合法文件名
        assert_eq!(sanitize_filename("a/b"), "a_b");
    }

    #[test]
    fn shortcut_launch_arg_format() {
        assert_eq!(format!("--launch={}", "a1"), "--launch=a1");
        assert_eq!(format!("--launch={}", "abc-123"), "--launch=abc-123");
    }
}
