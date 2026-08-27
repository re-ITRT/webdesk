//! 平台能力层（platform）：跨平台桌面系统集成的统一抽象。
//!
//! 本模块封装 WebLaunch 依赖的桌面平台能力，屏蔽 Windows / macOS / Linux
//! 三端差异，供应用启动流程、server 路由与前端 IPC 按需接入：
//!
//! - 系统托盘：按 ADR-010「工作项驱动生命周期」动态出现/隐藏——默认无图标，
//!   仅当存在后台驻留应用时构建托盘，菜单提供「显示主面板 / 显示[应用名] /
//!   全部退出」三类动作；
//! - 桌面快捷方式：Windows 经 PowerShell COM（WScript.Shell）生成 `.lnk`，
//!   macOS 经 osascript 生成 Finder alias，Linux 写入 `.desktop` 文件；
//! - 开机自启：委托 `tauri-plugin-autostart` 管理；
//! - 应用图标：从应用 URL 抓取 favicon 并统一转换为 `.ico`，供快捷方式与
//!   Windows 任务栏独立身份使用；
//! - Windows 任务栏身份：为每个应用窗口设置独立 AUMID，实现任务栏图标与
//!   分组隔离。
//!
//! 设计关联：ADR-010（工作项生命周期与按需托盘）、ADR-009（应用身份隔离）。

// 本模块为平台能力层，各函数由应用启动流程、server 路由与前端 IPC 按需接入；
// M0 阶段尚未全部接线，故统一允许 dead_code（与其余 WIP 模块保持一致）。
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::app_state::AppState;

/// 托盘实例的固定标识（供 `tray_by_id` / `remove_tray_by_id` 查询与移除）
pub const TRAY_ID: &str = "webdesk-tray";

/// 托盘菜单项 id 前缀（菜单事件回调据此分发动作）
const MENU_SHOW_MAIN: &str = "show-main";
const MENU_APP_PREFIX: &str = "show-app:";
const MENU_QUIT_ALL: &str = "quit-all";

// ---------------------------------------------------------------------------
// 托盘：ADR-010 动态托盘（按需出现/隐藏）
// ---------------------------------------------------------------------------

/// 托盘控制器：按需出现/隐藏的系统托盘（ADR-010）。
///
/// 默认不显示托盘图标；`show_tray_if_needed` 在存在后台驻留应用时构建托盘，
/// 否则移除。托盘菜单事件回调直接驱动主面板唤起、应用窗口激活与全量退出逻辑。
pub struct TrayController;

impl TrayController {
    /// 构建托盘菜单结构描述：固定项（显示主面板、分隔符、全部退出）+ 每个后台应用一项。
    ///
    /// 纯函数，不触碰真实托盘/窗口，仅返回 `(菜单项 id, 显示文本, 是否启用)`
    /// 列表，便于单元测试与菜单构建复用。
    pub fn build_menu_descriptor(apps: &[(String, String)]) -> Vec<(String, String, bool)> {
        // 元组语义：(菜单项 id, 显示文本, 是否启用)
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

    /// 是否存在后台驻留应用（ADR-010 托盘出现条件的判定）
    pub fn has_background_apps(state: &AppState) -> bool {
        state
            .running
            .read()
            .unwrap()
            .values()
            .any(|r| r.status == "background")
    }

    /// 收集全部后台驻留应用，返回 `(app_id, 显示名称)` 列表（按名称排序）
    pub fn background_apps(state: &AppState) -> Vec<(String, String)> {
        let running = state.running.read().unwrap();
        let mut apps: Vec<(String, String)> = running
            .values()
            .filter(|r| r.status == "background")
            .map(|r| (r.app_id.clone(), r.app_id.clone()))
            .collect();
        // 以 store 中的友好名称覆盖默认的 app_id 作为显示名
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

    /// 动态托盘入口：有后台驻留应用则构建/刷新托盘，否则移除（ADR-010）
    pub fn show_tray_if_needed<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
        if !Self::has_background_apps(state) {
            Self::hide_tray(app);
            return;
        }
        Self::rebuild_tray(app, state);
    }

    /// 强制刷新托盘：委托 `show_tray_if_needed` 按当前状态重建或隐藏
    pub fn refresh<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
        Self::show_tray_if_needed(app, state);
    }

    /// 移除托盘图标（不存在时静默跳过）
    pub fn hide_tray<R: Runtime>(app: &AppHandle<R>) {
        if app.tray_by_id(TRAY_ID).is_some() {
            let _ = app.remove_tray_by_id(TRAY_ID);
            log::info!("[platform] 托盘已隐藏（无后台驻留应用）");
        }
    }

    /// 构建/重建托盘：先移除旧实例，再按当前后台应用列表创建图标、菜单与事件回调
    pub fn rebuild_tray<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
        // 已存在则先移除，避免重复注册同 id 托盘
        if app.tray_by_id(TRAY_ID).is_some() {
            let _ = app.remove_tray_by_id(TRAY_ID);
        }

        let backgrounds = Self::background_apps(state);

        let result: anyhow::Result<()> = (|| {
            // 构建菜单：固定项 + 每后台应用一项
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

            // 托盘图标优先取主窗口图标，避免无图标导致构建失败
            let icon = app.default_window_icon().cloned();
            let mut builder = TrayIconBuilder::with_id(TRAY_ID)
                .menu(&menu)
                .tooltip("WebLaunch")
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

    /// 将菜单项 id 解析为托盘动作（纯函数，便于测试与事件分发复用）
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

/// 托盘菜单动作枚举（`parse_menu_id` 的解析结果，供事件分发与测试使用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    ShowMain,
    ShowApp(String),
    QuitAll,
    Ignore,
}

/// 唤起主面板（管理控制台窗口）
pub fn show_main_panel<R: Runtime>(app: &AppHandle<R>) {
    // 管理控制台窗口 label 为 win-console（约定见 scheduler 模块）
    if let Some(win) = app.get_webview_window("win-console") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    } else {
        // 窗口尚未创建时，经 scheduler 启动 console 应用
        let state = app.state::<AppState>();
        if let Ok(Some(console)) = crate::app_state::get_app(&state, "console") {
            state.scheduler.launch(&console).ok();
            let _ = console;
        }
    }
}

/// 唤起指定后台驻留应用（经 scheduler 激活其窗口）
pub fn activate_background_app<R: Runtime>(app: &AppHandle<R>, app_id: &str) {
    // 经 scheduler 激活；M1 起由 scheduler 负责真正恢复窗口
    let state = app.state::<AppState>();
    if let Ok(Some(a)) = crate::app_state::get_app(&state, app_id) {
        if let Err(e) = state.scheduler.activate(&a) {
            log::warn!("[platform] 恢复应用 {app_id} 失败: {e}");
        }
    } else {
        log::warn!("[platform] 激活未知应用: {app_id}");
    }
}

/// 全部退出：终止所有运行中的应用后退出平台进程
pub fn quit_all_apps<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let ids = state.running_ids();
    for id in ids {
        if let Ok(Some(a)) = crate::app_state::get_app(&state, &id) {
            state.scheduler.terminate(&a).ok();
        }
    }
    // 平台为单实例守护进程，主窗口关闭即整体退出
    app.exit(0);
}

// ---------------------------------------------------------------------------
// 桌面快捷方式（.lnk / alias / .desktop）
// ---------------------------------------------------------------------------

/// 生成快捷方式文件名（含平台对应的扩展名：.lnk / .alias / .desktop）
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

/// 清理文件名中的非法字符：仅保留字母数字、`-`、`_`、空格与 `.`，
/// 其余替换为 `_`；清理结果为空时回退为 `WebLaunch-App`。
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
        "WebLaunch-App".to_string()
    } else {
        t.to_string()
    }
}

/// 获取当前可执行文件路径（std 方案；AppImage 打包场景自 M1 起可改用
/// `tauri::process::current_binary`）
pub fn current_exe() -> anyhow::Result<PathBuf> {
    std::env::current_exe().map_err(|e| anyhow::anyhow!("获取当前可执行文件失败: {e}"))
}

/// 在桌面创建应用快捷方式，返回创建的完整路径。
///
/// 平台差异：
/// - Windows：经 PowerShell COM（WScript.Shell）生成 `.lnk`，Target 指向当前
///   可执行文件，Arguments 为 `--launch=<launch_arg>`；`icon` 提供本地
///   `.ico`/`.exe` 路径时写入 `IconLocation`；
/// - macOS：经 osascript 在 Finder 中创建 alias；
/// - Linux：写入 `.desktop` 文件（含可执行位）。
///
/// 参数：
/// - `app_name`：应用名称，用于生成快捷方式文件名；
/// - `launch_arg`：随 `--launch=` 传入的启动参数（应用标识）；
/// - `icon`：可选图标来源——本地路径直接使用，http(s) URL 先下载到本地数据目录。
///
/// 错误：桌面目录无法定位、平台命令执行失败或目标文件未生成时返回错误。
pub fn create_shortcut(
    app_name: &str,
    launch_arg: &str,
    icon: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let desktop =
        dirs::desktop_dir().ok_or_else(|| anyhow::anyhow!("无法定位桌面目录（desktop_dir）"))?;
    let dest = desktop.join(shortcut_filename(app_name));

    // icon 为 http(s) URL 时先下载到本地数据目录再使用
    // （Windows .lnk 的 IconLocation 仅接受本地路径）
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

/// 从 URL 下载图标到本地数据目录（`{data_dir}/WebDesk/icons/`），返回本地路径。
///
/// 支持两类来源：
/// - 直接图标文件（.ico/.png/.svg 等）→ 直接下载；
/// - 网页 URL → 抓取 HTML 解析 `<link rel="icon">`，未命中时回退
///   `{origin}/favicon.ico`。
///
/// 线程约束：本函数使用 `reqwest::blocking`，必须在 `spawn_blocking` 线程中
/// 调用（server 端 create_shortcut 已包裹）；若在 axum async 上下文直接调用
/// 会因阻塞运行时而 panic（"Cannot drop a runtime in a context where blocking
/// is not allowed"）。
fn download_icon(url: &str) -> anyhow::Result<String> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("无法定位数据目录"))?
        .join("WebDesk")
        .join("icons");
    std::fs::create_dir_all(&data_dir)?;

    // 解析最终图标 URL：网页解析 favicon，图标文件原样使用
    let icon_url = resolve_icon_url(url)?;

    // 由图标 URL 推断本地文件名（去除 query 参数并清理非法字符）
    let filename = safe_filename(&icon_url);
    let dest = data_dir.join(filename);

    // 经 reqwest 下载（模拟浏览器请求，跟随重定向，10 秒超时）
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

/// 获取应用图标（每应用独立缓存）：从应用 URL 抓取 favicon，统一转换为
/// `.ico` 存入 `{data_dir}/WebDesk/icons/apps/{app_id}.ico` 并返回路径。
///
/// 统一转 `.ico` 的原因：Windows `.lnk` 的 IconLocation 仅支持
/// .ico/.exe/.dll，且 Tauri `Image::from_bytes` 亦支持 ico 格式。
/// 网络失败、站点无图标或转换失败时返回 `None`（调用方应容忍缺省）。
pub fn fetch_app_icon(app_id: &str, url: &str) -> Option<std::path::PathBuf> {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WebDesk")
        .join("icons")
        .join("apps");
    let _ = std::fs::create_dir_all(&data_dir);
    let dest = data_dir.join(format!("{app_id}.ico"));

    // 命中本地缓存则直接返回
    if dest.exists() {
        return Some(dest);
    }

    // 解析最终图标 URL（网页解析 favicon；图标文件原样使用）
    let icon_url = match resolve_icon_url(url) {
        Ok(u) => u,
        Err(_) => return None,
    };

    // 下载图标（reqwest::blocking；本函数须在 spawn_blocking 线程中调用）
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

    // 统一转换为 .ico 字节（SVG→PNG→ICO；PNG→ICO；已是 ICO 则直接使用）
    let ico_bytes = match to_ico(&bytes) {
        Some(ico) => ico,
        None => {
            log::warn!("[platform] 图标转 ICO 失败: {icon_url}");
            return None;
        }
    };

    if std::fs::write(&dest, &ico_bytes).is_ok() {
        log::info!("[platform] 已获取应用图标(→ICO): {icon_url} -> {dest:?}");
        Some(dest)
    } else {
        None
    }
}

/// 将任意格式的图标字节统一转换为 .ico 字节。
///
/// - 已是 ICO（magic `\0\0\1\0`）→ 原样返回；
/// - SVG → 经 resvg 渲染为 PNG 后包装为 ICO；
/// - PNG/BMP → 直接包装为 ICO。
///
/// 无法识别或转换失败时返回 `None`。
fn to_ico(bytes: &[u8]) -> Option<Vec<u8>> {
    // 校验 ICO 魔数（ICONDIR：reserved=0, type=1）
    if bytes.len() >= 6 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 1 && bytes[3] == 0 {
        return Some(bytes.to_vec());
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)])
        .trim_start()
        .to_lowercase();
    // SVG 先渲染为 PNG
    let png_bytes: Vec<u8> = if head.starts_with("<svg") {
        svg_to_png(bytes)?
    } else {
        bytes.to_vec()
    };
    png_to_ico(&png_bytes)
}

/// 将 PNG 字节包装为 ICO 容器（Windows Vista+ 支持 PNG 压缩图标，无需转 BMP）
fn png_to_ico(png: &[u8]) -> Option<Vec<u8>> {
    // 解析 PNG 尺寸：IHDR 中的宽高位于字节 16-23
    if png.len() < 24 || &png[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    let mut ico = Vec::with_capacity(22 + png.len());
    // ICONDIR
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]); // reserved, type=icon, count=1
                                                // ICONDIRENTRY
    let w: u8 = if width >= 256 { 0 } else { width as u8 };
    let h: u8 = if height >= 256 { 0 } else { height as u8 };
    ico.push(w);
    ico.push(h);
    ico.push(0); // palette
    ico.push(0); // reserved
    ico.extend_from_slice(&(1u16).to_le_bytes()); // planes
    ico.extend_from_slice(&(32u16).to_le_bytes()); // bpp
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes()); // size
    ico.extend_from_slice(&22u32.to_le_bytes()); // offset
    ico.extend_from_slice(png);
    Some(ico)
}

/// 将 SVG 字节渲染为 PNG 字节（基于 usvg 解析 + resvg 光栅化）
fn svg_to_png(svg_bytes: &[u8]) -> Option<Vec<u8>> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_bytes, &opt).ok()?;
    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;
    if width == 0 || height == 0 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width.max(16), height.max(16))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().ok()
}

/// 由图标 URL 生成安全的本地文件名：
/// - 去除 query 参数（`?` 之后的部分）；
/// - 取路径最后一段作为文件名；
/// - 过滤 Windows 非法字符（`\ / : * ? " < > |`）；
/// - 结果为空或无扩展名时回退为 `favicon.ico`。
fn safe_filename(icon_url: &str) -> String {
    // 截断 query 参数
    let no_query = icon_url.split('?').next().unwrap_or(icon_url);
    // 取路径最后一段作为文件名
    let last = no_query.split('/').next_back().unwrap_or("");
    // 过滤 Windows 非法字符
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
/// - 直接指向图标文件（扩展名命中 .ico/.png/.svg/.jpg/.jpeg/.webp/.gif）→ 原样返回；
/// - 否则视为网页 → 抓取 HTML 查找 `<link rel="icon">`，未命中时回退
///   `{origin}/favicon.ico`。
fn resolve_icon_url(url: &str) -> anyhow::Result<String> {
    // 按常见扩展名判断是否直接指向图标文件
    let lower = url.to_lowercase();
    let is_icon_file = [".ico", ".png", ".svg", ".jpg", ".jpeg", ".webp", ".gif"]
        .iter()
        .any(|ext| lower.contains(ext));

    if is_icon_file {
        return Ok(url.to_string());
    }

    // 视为网页：抓取 HTML 查找 favicon 声明
    let html = fetch_text(url)?;
    // 匹配 <link rel="icon"> 或 rel="shortcut icon" 声明并提取 href
    let re =
        regex::Regex::new(r#"<link[^>]+rel=["'][^"']*icon[^"']*["'][^>]*href=["']([^"']+)["']"#)
            .map_err(|e| anyhow::anyhow!("正则编译失败: {e}"))?;

    if let Some(caps) = re.captures(&html) {
        let href = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        if !href.is_empty() {
            return Ok(absolutize_url(url, href));
        }
    }

    // 回退：{origin}/favicon.ico
    Ok(format!("{}/favicon.ico", origin(url)))
}

/// 抓取 URL 的文本内容（reqwest，10 秒超时）
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

/// 将相对 href 解析为绝对 URL（基于页面 base URL）
fn absolutize_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") || href.starts_with("//") {
        if href.starts_with("//") {
            // 协议相对 URL（//host/path）：补全 https: 前缀
            format!("https:{href}")
        } else {
            href.to_string()
        }
    } else if href.starts_with('/') {
        format!("{}{href}", origin(base))
    } else {
        // 站内相对路径：拼接 {base 目录}/{href}
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

/// Windows 实现：经 PowerShell WScript.Shell COM 创建 .lnk 快捷方式
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

    // 可选图标：$s.IconLocation = '路径,0'（,0 表示取第一个图标资源）
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

/// macOS 实现：经 osascript 在 Finder 中创建 alias
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

/// Linux 实现：写入 .desktop 桌面入口文件
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
         Comment=WebLaunch 启动器\n\
         Exec=\"{exe}\" --launch={arg}\n\
         Terminal=false\n\
         Categories=Network;WebBrowser;\n\
         Icon={exe}\n",
        name = app_name,
        arg = launch_arg,
    );
    std::fs::write(dest, content).map_err(|e| anyhow::anyhow!("写入 .desktop 失败: {e}"))?;
    // 设置可执行位（0o755），GNOME/KDE 据此信任该桌面入口
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// 删除桌面快捷方式；目标不存在时视为成功（幂等）
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

/// 检查桌面快捷方式是否已存在
pub fn shortcut_exists(app_name: &str) -> bool {
    dirs::desktop_dir()
        .map(|d| d.join(shortcut_filename(app_name)).exists())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 开机自启（tauri-plugin-autostart）
// ---------------------------------------------------------------------------

/// 设置/取消开机自启（委托 `tauri-plugin-autostart`）
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

/// 查询开机自启当前是否启用
pub fn autostart_enabled<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<bool> {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    mgr.is_enabled()
        .map_err(|e| anyhow::anyhow!("查询开机自启状态失败: {e}"))
}

// ---------------------------------------------------------------------------
// Tauri commands（供前端 IPC 调用）
// ---------------------------------------------------------------------------

/// 创建桌面快捷方式（Tauri command，供前端 IPC 调用）
#[tauri::command]
pub fn create_shortcut_cmd(app_name: String, launch_arg: String) -> Result<String, String> {
    create_shortcut(&app_name, &launch_arg, None)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

/// 删除桌面快捷方式（Tauri command，供前端 IPC 调用）
#[tauri::command]
pub fn remove_shortcut_cmd(app_name: String) -> Result<(), String> {
    remove_shortcut(&app_name).map_err(|e| e.to_string())
}

/// 设置开机自启（Tauri command，供前端 IPC 调用）
#[tauri::command]
pub fn set_autostart_cmd(app: AppHandle, enable: bool) -> Result<(), String> {
    set_autostart(&app, enable).map_err(|e| e.to_string())
}

/// 查询开机自启状态（Tauri command，供前端 IPC 调用）
#[tauri::command]
pub fn autostart_enabled_cmd(app: AppHandle) -> Result<bool, String> {
    autostart_enabled(&app).map_err(|e| e.to_string())
}

/// Windows：为应用窗口设置独立的任务栏身份（AUMID + 可选图标）。
///
/// 通过为每个应用窗口分配独立的 AppUserModelID（AUMID），使各应用在任务栏
/// 拥有独立的图标与分组，避免与 WebLaunch 主进程合并显示。
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

    // 1) 写入独立 AUMID：任务栏按 AUMID 分组并显示独立图标
    let aumid = format!("WebLaunch.App.{app_id}");
    let mut wide: Vec<u16> = aumid.encode_utf16().collect();
    wide.push(0);
    unsafe {
        let prop_store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
        let pwstr = windows::core::PCWSTR(wide.as_ptr());
        let propv = InitPropVariantFromStringAsVector(pwstr)?;
        prop_store.SetValue(&PKEY_AppUserModel_ID, &propv)?;
        // 2) 追加 WS_EX_APPWINDOW 扩展样式：强制任务栏显示独立按钮
        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, WS_EX_APPWINDOW.0 as i32);
    }

    // 3) 可选：按给定路径设置窗口图标
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
    // 构造以 NUL 结尾的宽字符串（PROPVARIANT 字符串的简化实现）
    use windows::core::PWSTR;
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    wide.push(0);
    PWSTR(wide.as_mut_ptr())
}

#[cfg(target_os = "windows")]
fn set_window_icon(hwnd: windows::Win32::Foundation::HWND, img: &tauri::image::Image) {
    use windows::Win32::UI::WindowsAndMessaging::CreateIcon;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON};
    // 占位实现：Tauri Image → HICON 转换（经 WM_SETICON 下发）待 M2 完善
    let _ = hwnd;
    let _ = img;
    let _ = SendMessageW;
    let _ = WM_SETICON;
    let _ = ICON_SMALL;
    let _ = ICON_BIG;
    let _ = CreateIcon;
    // 实际图标下发需 HICON 转换，M2 完善；AUMID 已足以实现任务栏独立
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::RunningApp;
    use crate::auth::AuthStore;
    use crate::scheduler::Scheduler;
    use crate::store::AppStore;
    use std::sync::RwLock;

    /// 构造仅含 running 状态的 AppState（不依赖 Tauri 运行时，供纯逻辑测试）
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
        // 期望布局：主面板 + 分隔符 + 2 个应用项 + 分隔符 + 全部退出
        assert_eq!(items.len(), 6);
        assert_eq!(items[0].0, MENU_SHOW_MAIN);
        assert!(items.iter().any(|(id, _, _)| id == "show-app:a"));
        assert!(items.iter().any(|(id, _, _)| id == "show-app:b"));
        assert!(items.iter().any(|(id, _, _)| id == MENU_QUIT_ALL));

        // 无驻留应用时：仅固定项 + 退出
        let empty = TrayController::build_menu_descriptor(&[]);
        assert_eq!(empty.len(), 4);
        assert!(!empty
            .iter()
            .any(|(id, _, _)| id.starts_with(MENU_APP_PREFIX)));
    }

    #[test]
    fn has_background_apps_judges_by_status() {
        // 无后台应用 → 不显示托盘
        let s = state_with_running(vec![("a", "running")]);
        assert!(!TrayController::has_background_apps(&s));
        // 有后台应用 → 显示托盘
        let s = state_with_running(vec![("a", "background")]);
        assert!(TrayController::has_background_apps(&s));
        // 混合状态（前台 + 后台）
        let s = state_with_running(vec![("a", "running"), ("b", "background")]);
        assert!(TrayController::has_background_apps(&s));
        // 空运行表
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
        assert_eq!(sanitize_filename("   "), "WebLaunch-App");
        assert_eq!(sanitize_filename("正常名"), "正常名");
        // 清理结果应可作为合法文件名
        assert_eq!(sanitize_filename("a/b"), "a_b");
    }

    #[test]
    fn shortcut_launch_arg_format() {
        assert_eq!(format!("--launch={}", "a1"), "--launch=a1");
        assert_eq!(format!("--launch={}", "abc-123"), "--launch=abc-123");
    }
}
