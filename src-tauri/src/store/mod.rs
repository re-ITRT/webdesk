//! WebDesk store 模块 —— 应用配置持久化
//!
//! 职责：应用配置 CRUD + JSON 持久化到各平台配置目录。
//! 关联 ADR：ADR-009（身份）、ADR-010（工作项生命周期）。
//! 接口契约：`docs/design/api-contract.md` §5。

pub mod app_store;

pub use app_store::AppStore;
