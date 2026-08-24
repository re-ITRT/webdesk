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
///   Arguments 为 `--launch=<launch_arg>`。
/// - macOS：osascript 建 alias。
/// - Linux：写 `.desktop` 文件。
///
/// 返回创建的完整路径。
pub fn create_shortcut(app_name: &str, launch_arg: &str) -> anyhow::Result<PathBuf> {
    let desktop =
        dirs::desktop_dir().ok_or_else(|| anyhow::anyhow!("无法定位桌面目录（desktop_dir）"))?;
    let dest = desktop.join(shortcut_filename(app_name));

    #[cfg(target_os = "windows")]
    {
        create_shortcut_windows(&dest, launch_arg)?;
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

/// Windows：PowerShell WScript.Shell 建 .lnk
#[cfg(target_os = "windows")]
fn create_shortcut_windows(dest: &std::path::Path, launch_arg: &str) -> anyhow::Result<()> {
    let exe = current_exe()?;
    let exe_str = exe.to_string_lossy().replace('\\', "\\\\");
    let dest_str = dest.to_string_lossy().replace('\\', "\\\\");
    let arg_str = format!("--launch={launch_arg}");

    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{dest_str}'); \
         $s.TargetPath = '{exe_str}'; \
         $s.Arguments = '{arg_str}'; \
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
    create_shortcut(&app_name, &launch_arg)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::RunningApp;
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
