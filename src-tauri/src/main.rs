// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 单二进制：CLI 子命令 → CLI 模式；否则 daemon 模式
    let args: Vec<String> = std::env::args().skip(1).collect();

    // CLI 子命令（add/app/status/console/version 等）走 CLI 模式
    let is_cli = args
        .iter()
        .any(|a| {
            matches!(
                a.as_str(),
                "add" | "addweb" | "app" | "status" | "console" | "version" | "help" | "--help"
                    | "-h"
            )
        })
        // --launch 是单例转发指令（daemon 处理），--hidden 是 daemon 隐藏模式
        && !args.iter().any(|a| a == "--launch" || a == "--hidden");

    if is_cli {
        std::process::exit(webdesk_lib::cli::run_cli());
    }

    // 否则 daemon 模式
    webdesk_lib::run();
}
