# 决策记录：完全抛弃模式 A —— WebView2 成为唯一渲染引擎（单引擎化）

**日期**：2026-08-25
**决策**：完全移除模式 A（系统 Chrome 调用），WebView2（模式 B）成为平台唯一渲染引擎。
**状态**：已采纳（ADR-011）
**关联**：修订/废弃 ADR-002、ADR-004、ADR-009 中模式 A 相关部分；ADR-003 升级为唯一内核决策

---

## 一句话

**模式 A 带来的复杂度远超其价值，整体移除。** 平台从此只有一条渲染路径：内置 WebView2。

## 为什么砍（成本分析）

模式 A（`chrome --app`）在需求演进中不断膨胀出系统性负担：

| 负担 | 说明 |
|---|---|
| **Chrome 探测** | 注册表 HKLM/HKCU/WOW6432Node + App Paths 双保险 + 版本检测 + 失败引导，一套完整子系统 |
| **profile 策略** | 独立/共享之争（O1），"复用登录态"与"应用级身份隔离"不可兼得 |
| **进程树边界** | Chrome browser 进程共享，无法安全"彻底终止"单个应用，误伤用户其它窗口 |
| **CDP 风险** | 深度控制需调试端口 = 本机任意进程可接管浏览器 |
| **身份管理双实现** | cookie 需 Cookies SQLite+DPAPI、扩展 `--load-extension`、密钥需 Bridge 扩展——与 WebView2 三套 API 平行 |
| **后台驻留不一致** | 模式 A 关闭即退出，与模式 B 的"隐藏不销毁"语义分裂 |

**价值侧**：模式 A 唯一卖点是"复用系统 Chrome 登录态/书签/插件"。但 ADR-009（应用级身份）已将默认改为独立 profile——即**该卖点在决策层面已被放弃**。剩余价值趋近于零。

**结论**：成本（一整条平行实现路径 + 安全面 + 心智负担）≫ 价值（已被放弃的卖点）→ 移除。

## 砍掉后失去什么（如实评估）

1. **复用系统 Chrome 登录态**——用户需在应用内重新登录（一次性成本）。
2. **"双模"差异化叙事**——产品不再宣称"两种引擎"。但换来更纯粹的定位：**轻量 WebView2 SSB 管理器**。

**实际保留的"Chrome 生态兼容"**（WebView2 即 Chromium 内核）：
- Chromium 渲染引擎 / 现代 Web 标准；
- 本地 unpacked Chrome 扩展（`AddBrowserExtensionAsync`）；
- DevTools 协议（WebView2 支持，平台默认不向普通用户暴露）。

## 砍掉后简化的东西

| 领域 | 之前 | 现在 |
|---|---|---|
| 渲染路径 | 双引擎（Chrome / WebView2）+ 调度分支 | 单引擎 WebView2，无分支 |
| 身份管理 | cookie/扩展/密钥 × 2 套实现 + 跨内核同步 | 一套 WebView2 实现（CookieManager / AddBrowserExtensionAsync / AddScriptToExecuteOnDocumentCreated） |
| 后台驻留 | 模式 B 隐藏 / 模式 A 关闭即退出 | 统一：隐藏不销毁渲染器 |
| "彻底终止" | 受共享进程树约束 | 自己的进程树，直接终止 |
| 安全面 | CDP 风险 + 共享 profile 风险 | 无调试端口、无共享 profile |
| 配置模型 | renderEngine + chromeProfile + runtimeProfile | 仅 runtimeProfile（evergreen/fixed） |
| 里程碑 | M2 需跨内核 cookie 同步 | 该需求消失 |

## 修订的 ADR

- **ADR-002（模式 A 轻控制）**：废弃——不再有模式 A。
- **ADR-003（WebView2 内核）**：升级——WebView2 是**唯一**渲染引擎；Evergreen 默认、Fixed Version 共享目录 opt-in 不变。
- **ADR-004（后台驻留按模式分机制）**：简化为单一机制——隐藏不销毁渲染器。
- **ADR-009（AppIdentity）**：保留概念，实现收敛为单宿主（WebView2）；"跨内核一致"表述改为"平台统一管理"。

## 需求文档影响（全部已同步）

- `requirements-master.md`：1.1 一句话、2.1 数据模型（删 renderEngine/chromeProfile）、2.2 单引擎、2.3/2.4 统一、3.2 删 Chrome 探测、ADR 表、里程碑；
- `requirements-v1.2-draft.md`：2.1 重写、2.3/2.4/3.2/4/5 同步；
- `docs/design/2026-08-25-tech-selection.md`：组件映射删模式 A；
- `docs/design/2026-08-25-app-identity-across-engines.md`：单宿主化；
- `docs/design/2026-08-25-work-item-lifecycle.md`：退出流程单引擎化；
- `README.md`：双模 → 单引擎。

## 产品定位（更新后）

**轻量级 WebView2 站点特定浏览器（SSB）管理平台**：把任意 Web 应用当作原生桌面应用运行与管理——per-app 身份隔离（cookie/密钥/扩展）、生命周期钩子、工作项驱动生命周期（无常驻托盘）、后台驻留、桌面快捷方式、Web 管理控制台（吃自己的狗粮）。

差异化 vs 现有方案：
- WebCatalog：我们是**Web 控制台**（它原生 UI）+ **工作项生命周期**（无感常驻）+ 更轻（Evergreen 零额外体积）；
- 浏览器"安装为应用"：我们有钩子 / per-app 身份隔离 / 托盘级管理；
- Electron 包装：包体小、更新由平台控制。
