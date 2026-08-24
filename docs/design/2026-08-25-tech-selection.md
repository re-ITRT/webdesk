# 技术选型（正式）—— 2026-08-25

**状态**：已定案
**修订**：ADR-008（C#/.NET 8 → **C#/.NET 10 LTS + WinForms 薄壳**）；精化 ADR-003（Fixed Version 共享化）

---

## 0. 结论（TL;DR）

| 层 | 选型 |
|---|---|
| **语言/运行时** | C# / **.NET 10 LTS**（支持至 2028-11） |
| **原生薄壳** | **WinForms**（仅承担：模式 B 窗口承载 + 动态托盘 + 通知） |
| **模式 B 内核** | **WebView2**（Microsoft.Web.WebView2，官方 .NET SDK；**唯一渲染引擎**，ADR-011） |
| **本地 HTTP（管理 API + 静态控制台）** | **ASP.NET Core Minimal API**（Kestrel，仅回环 + 随机端口 + 会话 token） |
| **系统集成** | 原生 .NET：命名互斥体 / 命名管道 / Job Object(P/Invoke) / 注册表 / UAC / .lnk(COM) |
| **单例 + IPC** | Named Mutex + NamedPipeServerStream |
| **跨平台** | Windows 优先（WebView2 专属）；macOS/Linux 需换渲染宿主（见 §11 跨平台策略）；鸿蒙=独立项目 |

**一句话**：C#/.NET 10 + WebView2 + WinForms 薄壳 + 内置 Web 控制台。

---

## 1. 为什么是 .NET 10 LTS（不是 .NET 8）

> 研究确认：**.NET 8 / .NET 9 均于 2026-11-10 停止支持**；.NET 10 LTS（2025-11 发布）支持至 2028-11-14，微软明确推荐。

- 本项目 2026-08 立项 → 用 .NET 8 会在发布初期就进入无支持期，**必须用 .NET 10 LTS**。
- .NET 10 的 NativeAOT / 裁剪 / 启动性能均优于 8；WinForms + WebView2 在 .NET 10 上成熟。

## 2. 为什么是 WinForms 薄壳（不是 WPF）

管理界面已定为 **Web 控制台**（ADR-007），因此原生 UI 只剩"看不见的部分"：

- 模式 B 应用窗口（WebView2 承载）；
- 动态托盘图标（有驻留 app 时才出现）；
- 系统通知。

**结论**：原生壳极薄，不需要 WPF 的重量。
- **WinForms**：`WebView2` 控件成熟、`NotifyIcon` 内置（托盘零额外依赖）、内存更轻 → **选它**。
- WPF 留作备选：若未来需要更复杂的原生 UI 再考虑。
- 托盘动态出现：WinForms `NotifyIcon.Visible` 开关，天然支持 ADR-010。

## 3. 为什么模式 B 用 WebView2（官方 .NET SDK）

研究确认 WebView2 完整覆盖本项目模式 B 需求，且均为**稳定 API**：

| 需求 | WebView2 API | 状态 |
|---|---|---|
| per-app 身份隔离 | 每应用独立 `UserDataFolder`（环境级） | ✅ |
| cookie 管理 | `CoreWebView2.CookieManager`（增删查改） | ✅ |
| **扩展加载（per-app）** | `Profile.AddBrowserExtensionAsync(path)`（本地 unpacked） | ✅ 稳定 |
| CSS/JS 注入 | `AddScriptToExecuteOnDocumentCreatedAsync` | ✅ |
| 注入时机 | 默认 document_start 等价；可控时机 | ✅ |
| 内核版本 | Evergreen（默认）/ Fixed Version（共享目录） | ✅ |
| 后台驻留 | 隐藏窗口不销毁渲染器 → WebSocket 保活 | ✅ |

**Fixed Version 精化（ADR-003 修订）**：研究确认 Fixed Version 运行时 **250MB+**。per-app 各自打包不现实。**改为：平台可选携带一份共享 Fixed Version 目录（安装时可选装），需要锁版本的应用 opt-in 指向该目录**；默认仍走 Evergreen。

## 4. 为什么本地 HTTP 用 ASP.NET Core Minimal API

- 管理 API + 静态控制台托管在同一进程（daemon 内），Kestrel 成熟稳定；
- 仅绑定 `127.0.0.1` + 随机端口 + 会话 token（ADR V1.3 安全要求）；
- 备选 `HttpListener`（零依赖更省内存）——若内存预算紧张可降级，默认用 Minimal API（可维护性优先）。

## 5. 为什么不是 Tauri（Rust）—— 本轮重点澄清

Tauri 2 已稳定、包体小、跨平台，但**与本项目核心需求错配**：

| 维度 | Tauri | C#/WebView2 |
|---|---|---|
| WebView2 模型 | **单 webview**（一个 app 一个窗口） | 原生支持 **N 个隔离 WebView2 环境**（每应用独立 UserDataFolder） |
| per-app cookie/扩展 | 需自己打穿 wry/webview2-com 底层 | 官方 API 直出（CookieManager / AddBrowserExtensionAsync） |
| Fixed Version WebView2 | 不支持（只走系统 WebView2） | 原生支持 |
| Win32 深度（Job Object/UAC/管道/注册表/.lnk） | 需大量自写 Rust + windows crate | .NET 第一公民 + 少量 P/Invoke |
| 内存/包体 | 更优（~30-50MB / 3-10MB） | 达标（自包含裁剪后 ~60-100MB 磁盘 / 空闲 <50MB） |

**判断**：Tauri 的价值在"单 WebView + 轻量跨平台"。我们的负载是"多隔离 WebView2 + 深 Win32 系统集成"，Tauri 的抽象反而打架（它假设一个 app 一个 webview）。**当 macOS 成为硬需求时再评估 Tauri**；当前 Windows 优先，C#/WebView2 直连官方 API 是最低风险路径。

## 6. 为什么不是其它

- **Electron**：被需求自身否定（包体 300MB+、更新滞后、无法利用系统 Chrome 生态）。❌
- **Go**：无官方 WebView2 身份级绑定；托盘/管道/Job Object 生态弱。❌
- **Node/Electron 变体（NW.js 等）**：同 Electron。❌

## 7. 组件 → 技术映射（全量）

| 需求 | 实现 | 备注 |
|---|---|---|
| 单例 | `Mutex`（Named） | 原生 |
| IPC / 指令转发 | `NamedPipeServerStream` + 启动器客户端 | 原生 |
| 轻量启动器 | `WebDesk.Launcher.exe`（独立小进程） | .NET 单文件 |
| 工作项驱动生命周期 | daemon 状态机：驻留型 app / 执行中钩子 / 控制台打开 | ADR-010 |
| 本地 HTTP | ASP.NET Core Minimal API + 静态文件 | 仅回环 + token |
| 模式 B 承载 | WinForms + `WebView2` 控件，per-app Environment | 独立 UserDataFolder |
| 身份（cookie） | WebView2 CookieManager ↔ 平台加密仓库 | ADR-009 |
| 身份（密钥注入） | WebView2 `AddScriptToExecuteOnDocumentCreated` | |
| 身份（扩展） | WebView2 `AddBrowserExtensionAsync` | 本地 unpacked |
| 钩子执行 | 后台进程 + `taskkill /T`；WSL 进程组单独处理 | cmd/powershell/wsl |
| 进程树回收 | Job Object（`CreateJobObject` P/Invoke） | 孤儿回收 |
| 托盘（动态） | WinForms `NotifyIcon` | ADR-010 按需出现 |
| 桌面快捷方式 | WScript.Shell COM 建 `.lnk` | 或 IShellLink |
| 开机自启 | HKCU Run 注册表项 | 安装时可选 |
| UAC 提权 | per-app manifest `requireAdministrator` / `runas` verb | O17 |
| 配置持久化 | JSON（System.Text.Json），`%APPDATA%\WebDesk\config\` | |
| 日志 | Serilog / 自研轻量，`%APPDATA%\WebDesk\logs\` | |

## 8. 资源与性能预估

| 指标 | 目标 | 实现路径 |
|---|---|---|
| 空闲内存（无应用） | <50MB | daemon 惰性加载 WebView2（无 app 时不建环境）；自包含裁剪发布 |
| 空闲 CPU | ≈0 | 无轮询，事件驱动（管道/HTTP/信号） |
| 启动：快捷方式→转发 | <100ms | Launcher 单文件极轻 |
| 窗口渲染完成 | <2s | WebView2 预热 / 快速环境创建 |

> 说明：单进程模型（daemon + WebView2 宿主同进程）。无 app 时 WebView2 环境不创建 → 空闲进程保持轻量；有 app 时按工作负载自然增长（那是应用的内存，不是平台空载）。WebView2 渲染器崩溃（ProcessFailed 事件）不会拖垮宿主进程。

## 9. 风险与缓解

| 风险 | 缓解 |
|---|---|
| **Fixed Version 250MB** | 共享一份 runtime 目录，per-app opt-in（ADR-003 精化） |
| WinForms 视觉"老气" | 管理界面是 Web（现代）；原生壳只有窗口+托盘，几乎不可见 |
| macOS 移植 | 管理 Web 零改动；薄壳重写（AppKit + WKWebView）是独立工作项，M2 后评估 |
| WebView2 多环境内存 | 接受：应用运行时内存增长属预期工作负载 |
| Kestrel 内存 | 若超预算降级 HttpListener（零依赖） |
| .NET 自包含体积 | 裁剪（Trimmed，ReadyToRun）；不追求极致 AOT（WinForms AOT 尚实验性） |

## 10. 里程碑落点

- **M0**：.NET 10 + WinForms + WebView2 空壳 + ASP.NET Core 本地 API + 静态控制台页 + 单例/管道 → 验证内存/启动/吃狗粮
- **M1**：全能力（单引擎 WebView2 + 钩子 + 身份 + 驻留 + 快捷方式 + 工作项生命周期）
- **M2**：Fixed Version 共享 / 自身更新 / 多语言 / macOS 评估

---

**附：研究依据**
- WebView2 SDK 稳定版 API（AddBrowserExtensionAsync、CookieManager、AddScriptToExecuteOnDocumentCreated）
- WebView2 Fixed Version = 250MB+（微软官方分发文档）
- .NET 8/9 EOL 2026-11-10，.NET 10 LTS → 2028-11（微软官方博客）
- Tauri 2 稳定（2026），Windows 走系统 WebView2，单 webview 模型

---

## 11. 跨平台策略（渲染宿主抽象）

### 事实：WebView2 是 Windows 专属

| 平台 | Web 渲染引擎 | WebView2 支持 |
|---|---|---|
| Windows | WebView2（微软） | ✅ 原生 |
| macOS | WKWebView（Apple WebKit） | ❌ |
| Linux | WebKitGTK | ❌ |
| 鸿蒙 HarmonyOS | ArkWeb（华为自研内核） | ❌（与 Chromium 家族无关） |

### 但"看得见的部分"已经天然跨平台

ADR-007 让管理界面 = Web 控制台，被管理的 Web 应用也是 Web。因此：

- **管理控制台（Web）**：完全跨平台；
- **被管理的 Web 应用**：完全跨平台；
- **daemon 原生壳**（WinForms/.NET/WebView2）：Windows 专属。

### 策略：渲染宿主抽象（`IWebViewHost`），为跨平台铺路不拖慢 Windows

**daemon 核心逻辑（钩子 / 身份管理 / 生命周期 / 调度 / 单例 / IPC）不与 WebView2 强绑定**，仅通过一个宿主抽象接口触达渲染：

```
┌────────────────────────────────────────────┐
│  daemon 核心（跨平台可移植）                 │
│  · 生命周期 / 钩子 / 身份 / 调度 / IPC       │
└──────────────┬─────────────────────────────┘
               │ IWebViewHost（抽象接口）
   ┌───────────┼───────────────┬──────────────┐
   ▼           ▼               ▼              ▼
 WebView2    WKWebView      WebKitGTK      ArkWeb(鸿蒙)
 (Windows)   (macOS)        (Linux)        (独立项目)
```

- **Windows**：实现 = WebView2（当前）。
- **macOS/Linux**：未来实现 = WKWebView / WebKitGTK（管理控制台零改动，只需移植 daemon 壳 + 宿主实现）。
- **鸿蒙**：内核与 Chromium 无关，扩展/注入机制不通用——是独立项目（重写宿主 + 身份注入），非"同步"。

### 落地约束

- 抽象只覆盖**渲染宿主接口**，不抽象系统集成（托盘/注册表/Job Object/UAC 本就平台原生，各平台自实现）。
- Windows 优先不妥协：接口设计以 WebView2 能力为准，避免为"未来可能"过度设计。
- 身份注入（cookie/扩展）是宿主接口的一部分——跨平台时各宿主自实现，平台仓库统一。
