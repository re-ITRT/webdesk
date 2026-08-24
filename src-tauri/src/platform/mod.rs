//! WebDesk platform 模块 —— 平台差异抽象
//!
//! 职责：托盘 / 快捷方式 / 路径等平台相关能力。
//! 当前 Windows 优先，macOS/Linux/鸿蒙 为 stub（M2 起适配）。
//! 关联 ADR：ADR-010（工作项生命周期，托盘按需出现）。

/// 托盘图标控制（ADR-010：默认无图标，有驻留应用时动态出现）
#[allow(dead_code)]
pub struct TrayController;

#[allow(dead_code)]
impl TrayController {
    pub fn new() -> Self {
        Self
    }

    /// 显示托盘（存在后台驻留应用时）
    pub fn show(&self) {
        log::info!("托盘显示（有后台驻留应用）");
    }

    /// 隐藏托盘（无后台驻留应用）
    pub fn hide(&self) {
        log::info!("托盘隐藏");
    }
}

/// 创建桌面快捷方式（Windows .lnk / macOS alias / Linux .desktop）
#[allow(dead_code)]
pub fn create_shortcut(_app_name: &str, _launch_arg: &str) -> anyhow::Result<()> {
    // M1 起实现（平台分派）
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_controller_constructs() {
        let tray = TrayController::new();
        tray.show();
        tray.hide();
    }
}
