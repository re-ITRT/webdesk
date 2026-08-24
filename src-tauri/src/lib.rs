//! WebDesk —— 通用 Web 应用桌面化管理平台（Tauri 2，四平台）
//!
//! 本 crate 是平台 daemon 核心，包含：
//! - `types`：共享类型层（所有模块的事实标准）
//! - `store`：应用配置持久化
//! - `hooks`：生命周期钩子执行器
//! - `scheduler`：应用生命周期调度（M1 起实做）
//! - `identity`：应用身份隔离（M1 起实做）
//! - `platform`：平台差异抽象
//! - `server`：本地 HTTP 管理 API（axum）
//!
//! 关联 ADR：ADR-007（管理控制台 Web 化）、ADR-009（应用身份）、
//! ADR-010（工作项生命周期）、ADR-011（单引擎）。开发范式见
//! `docs/design/dev-paradigm.md`，接口契约见 `docs/design/api-contract.md`。

mod platform;
mod server;
mod store;
mod types;
mod util;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
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
        .invoke_handler(tauri::generate_handler![server::get_api_config])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
