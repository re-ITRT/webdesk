// 防止 release 构建在 Windows 上弹出多余的控制台窗口，请勿删除！！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 单二进制分派：命令行含 CLI 子命令时进入 CLI 模式，否则进入 daemon 模式。
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 识别 CLI 子命令（add/addweb/app/status/console/version/help 等）。
    let is_cli = args
        .iter()
        .any(|a| {
            matches!(
                a.as_str(),
                "add" | "addweb" | "app" | "status" | "console" | "version" | "help" | "--help"
                    | "-h"
            )
        })
        // --launch 是单例转发指令（由 daemon 处理），--hidden 是 daemon 隐藏启动模式，
        // 二者即使与上述子命令同时出现，也仍按 daemon 模式处理。
        && !args.iter().any(|a| a == "--launch" || a == "--hidden");

    if is_cli {
        std::process::exit(webdesk_lib::cli::run_cli());
    }

    // 其余情况进入 daemon 模式（Tauri 应用主循环）。
    webdesk_lib::run();
}
