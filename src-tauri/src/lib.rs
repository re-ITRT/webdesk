//! WebDesk —— 通用 Web 应用桌面化管理平台（Tauri 2，四平台）
//!
//! 本 crate 是平台 daemon 核心，包含：
//! - `types`：共享类型层（所有模块的事实标准）
//! - `store`：应用配置持久化
//! - `hooks`：生命周期钩子执行器
//! - `scheduler`：应用生命周期调度（创建/管理 WebviewWindow）
//! - `identity`：应用身份隔离
//! - `platform`：平台差异抽象（托盘/快捷方式/自启）
//! - `server`：本地 HTTP 管理 API（axum）
//!
//! 关联 ADR：ADR-007（管理控制台 Web 化）、ADR-009（应用身份）、
//! ADR-010（工作项生命周期）、ADR-011（单引擎）。开发范式见
//! `docs/design/dev-paradigm.md`，接口契约见 `docs/design/api-contract.md`。

mod app_state;
pub mod cli;
mod hooks;
mod identity;
mod platform;
mod scheduler;
mod server;
mod store;
mod types;
mod util;

pub use cli::run_cli;

use app_state::AppState;
use tauri::Manager;

/// 应用启动入口
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // 单例：第二个实例启动时把参数转发给主实例
    builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
        let handle = app.app_handle().clone();
        // 若带了 --launch=<id>，唤起对应应用；否则（无参双击图标）默认唤起控制台
        let target = argv
            .iter()
            .find(|a| a.starts_with("--launch="))
            .and_then(|a| a.split('=').nth(1))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "console".to_string());

        tauri::async_runtime::spawn(async move {
            log::info!("[单例转发] 唤起应用: {target}");
            if let Err(e) = scheduler::launch_by_id(&handle, &target).await {
                log::error!("[单例转发] 启动应用失败: {e}");
            }
        });
    }));

    // 开机自启插件（默认关闭，经平台模块控制）
    builder = builder.plugin(
        tauri_plugin_autostart::Builder::new()
            .args(["--hidden"])
            .build(),
    );

    builder
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // 初始化共享状态（store + scheduler）
            let state = AppState::init(app.handle())?;
            app.manage(state);

            // 预置系统应用（管理控制台）
            if let Err(e) = app_state::ensure_system_apps(app.handle()) {
                log::error!("预置系统应用失败: {e}");
            }

            // 启动本地 HTTP 管理 API（后台 tokio 任务）
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
                rt.block_on(async move {
                    if let Err(e) = server::spawn(handle).await {
                        log::error!("启动管理 API 失败: {e}");
                    }
                });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            server::get_api_config,
            scheduler::launch_app_cmd,
            scheduler::terminate_app_cmd,
            scheduler::activate_app_cmd,
            scheduler::list_running_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
