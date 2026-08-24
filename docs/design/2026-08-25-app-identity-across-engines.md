# 决策记录：应用级身份（AppIdentity）—— Cookie / 密钥 / 插件按应用隔离并跨内核一致

**日期**：2026-08-25
**决策**：平台引入"应用身份（AppIdentity）"概念——Cookie、密钥（Secrets）、扩展（Extensions）三类身份数据**按 Web 应用隔离**，且**与渲染内核解耦**：用户为同一应用切换内核（系统 Chrome ↔ WebView2）时，注入相同的一份身份数据。
**状态**：已采纳（落入需求规格；修订 ADR-002 与 O1）
**关联**：`docs/design/2026-08-25-management-console-as-web-app.md`（同一架构演进方向）

---

## 一句话

**身份跟"应用"走，不跟"内核"走。** 渲染内核只是应用身份的宿主之一，平台保证切到任一内核时，应用拿到同一份 cookie、密钥、插件。

## 用户需求（原话转述）

- cookie **按 Web App 隔离**；
- 用户为应用选择**不同内核**时，注入**相同的 cookie**；
- **密钥、插件**同理（按应用隔离、跨内核一致）；
- 其余待定项按建议默认。

## 架构变化

```
   ┌─────────────────────────────────────────────────────────┐
   │  应用身份（AppIdentity）—— 平台统一管理，跨内核一致        │
   │                                                          │
   │   · Cookie   ：应用级 cookie 仓库（加密）                 │
   │   · 密钥     ：应用级 secrets 仓库（DPAPI 加密）          │
   │   · 扩展     ：应用级扩展列表（来源=本地 unpacked 等）     │
   └───────────────┬──────────────────────────┬──────────────┘
                   │ 注入                      │ 注入
      ┌────────────▼──────────┐   ┌────────────▼──────────┐
      │ 模式 B：WebView2       │   │ 模式 A：系统 Chrome    │
      │ 独立 UserDataFolder    │   │ 独立 profile           │
      │ CookieManager /        │   │ --user-data-dir /      │
      │ AddScript /            │   │ --load-extension /     │
      │ AddBrowserExtension    │   │  Cookies 库(DPAPI)     │
      └────────────────────────┘   └───────────────────────┘
```

- 每个 Web 应用拥有一个"身份容器"：
  - **模式 B** → 独立 WebView2 `UserDataFolder`（既有 ADR-003）；
  - **模式 A** → 独立 Chrome profile（`--user-data-dir=<appDir>`，**本决策新增**）。
- 平台在两类容器之间同步/注入同一份身份数据 → **切换内核不丢登录、不重装插件**。

## 关键 trade-off（必须摊开）

| | 共享 profile（旧默认） | **独立 profile（本决策）** |
|---|---|---|
| 复用系统 Chrome 登录态/书签 | ✅ 卖点 | ❌ 需在应用内重新登录 |
| cookie 按应用隔离 | ❌ 混入主 profile | ✅ |
| 平台可读写 cookie（跨内核搬运） | ❌ 主 profile 不可碰 | ✅ |
| 按应用加载扩展 | ❌ 扩展是 profile 级 | ✅（`--load-extension`） |
| 密钥注入 | ❌ | ✅（经配套通道） |
| 后台驻留 / 可终止 | ❌（共享进程树） | ✅（独立进程树） |
| 安全隔离（工作/个人互不污染） | ❌ | ✅ |

**结论**：用户明确选择"应用级身份隔离 + 跨内核一致"，因此**模式 A 默认独立 profile**；"复用系统 Chrome 登录态"降级为**可选兼容模式**（共享 profile，见下）。

## 对既有决策的修订

### 修订 ADR-002（模式 A：轻控制 → profile 级控制）
- **原**：模式 A 仅轻控制（启动/激活/终止/钩子），不承诺 CSS/JS 注入、扩展隔离、隐藏导航。
- **新**：模式 A 基于**独立 profile** 提供——应用级 cookie 隔离与跨内核共享、按应用扩展加载（`--load-extension`）、可后台驻留、可终止。**仍不承诺** CDP 级深控（运行时 JS 注入、DOM 操作、隐藏导航等）——那是模式 B 专属。
- 理由：独立 profile + 标准 Chrome 命令行机制即可实现上述能力，**无需 CDP**，守住安全边界。

### 关闭 O1（模式 A 的 profile 策略）
- **已定**：模式 A 默认**独立 profile**（每应用独立 `--user-data-dir`）；提供 **共享 profile** 作为显式 opt-in（用于"复用系统 Chrome 登录态"的个人简单应用；该模式不隔离、不注入、关闭即退出，回到原 2.1.1 行为）。

## 实现机制

| 身份数据 | 模式 B（WebView2） | 模式 A（Chrome 独立 profile） |
|---|---|---|
| **Cookie** | `CoreWebView2.CookieManager` 原生读写 ↔ 平台仓库 | Cookies SQLite（DPAPI，同用户可解）；应用未运行窗口期同步：启动前注入 / 退出后抓取（MVP：提供"一键导入/导出"） |
| **密钥** | `AddScriptToExecuteOnDocumentCreatedAsync` 注入（如 `window.__webdesk__`） | 经配套扩展 WebDesk Bridge（`chrome.storage`）——MVP 后置 |
| **扩展** | `AddBrowserExtensionAsync`（unpacked） | `--load-extension=<dirs>` 启动参数 |

**平台侧存储**
- Cookie 仓库：应用级、加密存储（不落明文）。
- 密钥仓库：Windows DPAPI（CurrentUser）加密，`%APPDATA%\WebDesk\secrets\<appId>.json`。
- 扩展：记录扩展路径/标识 + 来源（仅可信来源：本地 unpacked，用户显式添加）。

## 安全边界

- 身份数据（cookie/密钥）仓库一律加密，仅本用户可解（DPAPI）。
- 扩展仅限可信来源；用户显式添加。
- 应用间身份隔离：独立容器 + 独立注入命名空间，互不可见。
- 模式 A 独立 profile 不开放 CDP 调试端口，避免本机进程接管风险。

## MVP 范围

**M0/M1 做**：
- 模式 B 完整身份管理（CookieManager / 密钥注入 / 扩展加载）；
- 模式 A 独立 profile 启动（`--user-data-dir`）+ `--load-extension` 扩展加载；
- 平台身份仓库（加密）与 CRUD。

**M2 及后置**：
- 模式 A ↔ 模式 B 的 cookie **自动**跨内核同步（窗口期自动注入/抓取）；
- 模式 A 密钥注入（WebDesk Bridge 扩展）。

**不承诺**（长期）
- 共享 profile 模式下的身份注入（隔离与复用登录态互斥，用户自行权衡）。

## 与"管理控制台 = 第一个应用"的一致性

管理控制台同样拥有独立身份容器；其 cookie/密钥由平台管理，与其它应用隔离。平台自身能力（身份注入、插件加载）持续被控制台自测（dogfooding）。
