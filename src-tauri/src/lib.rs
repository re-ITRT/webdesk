//! WebDesk —— 通用 Web 应用桌面化管理平台（Tauri 2，四平台）
//!
//! 本 crate 为平台守护进程（daemon）核心，按职责划分为以下模块：
//! - `types`：共享类型层，全平台类型定义的唯一事实来源
//! - `store`：应用配置的持久化存储
//! - `hooks`：应用生命周期钩子（pre_launch / post_exit）执行器
//! - `scheduler`：应用生命周期调度，负责 WebviewWindow 的创建、激活与销毁
//! - `identity`：应用身份隔离（cookie / 扩展 / 凭据）
//! - `platform`：平台差异抽象（托盘 / 快捷方式 / 开机自启）
//! - `server`：本地 HTTP 管理 API（axum 实现）
//!
//! 设计决策参见 ADR-007（管理控制台 Web 化）、ADR-009（应用身份）、
//! ADR-010（工作项生命周期）、ADR-011（单引擎）；开发范式见
//! `docs/design/dev-paradigm.md`，接口契约见 `docs/design/api-contract.md`。

mod app_state;
pub mod auth;
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

/// 应用启动入口：装配 Tauri 运行时并进入事件循环。
///
/// 负责注册单例转发、开机自启、日志、共享状态、系统应用预置与
/// 本地管理 API 等插件及初始化逻辑；移动端入口由
/// `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 自动生成。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // 单例插件：第二个实例启动时不再创建新进程，而是把命令行参数
    // 转发给主实例，由主实例决定唤起哪个应用。
    builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
        let handle = app.app_handle().clone();
        // 解析 --launch=<id> 参数：命中则唤起对应应用；
        // 无参（如双击图标）时默认唤起管理控制台。
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

    // 开机自启插件：默认不启用，由 platform 模块按需开启/关闭。
    builder = builder.plugin(
        tauri_plugin_autostart::Builder::new()
            .args(["--hidden"])
            .build(),
    );

    builder
        .setup(|app| {
            // 日志初始化：debug 构建输出 Debug 级、release 构建输出 Info 级，
            // 同时写入 stdout 与 %APPDATA%/WebDesk/logs/ 目录，
            // 保证 release 版本同样可落盘排查问题。
            let log_dir = dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("WebDesk")
                .join("logs");
            let _ = std::fs::create_dir_all(&log_dir);
            let log_level = if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            };
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log_level)
                    .targets([
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                            file_name: None,
                        }),
                    ])
                    .build(),
            )?;

            // 初始化全局共享状态（配置存储 + 调度器 + 授权存储）并注册为 Tauri 托管状态。
            let state = AppState::init(app.handle())?;
            app.manage(state);

            // 预置系统应用：确保管理控制台（is_system=true）在首次安装后即存在。
            if let Err(e) = app_state::ensure_system_apps(app.handle()) {
                log::error!("预置系统应用失败: {e}");
            }

            // 在独立线程中创建 tokio runtime 并启动本地 HTTP 管理 API，
            // 避免阻塞 Tauri 主线程。
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
