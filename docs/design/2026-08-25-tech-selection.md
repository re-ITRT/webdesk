# 技术选型（正式·四平台版）—— 2026-08-25

**状态**：已定案（V1.7，四平台转向）
**修订历史**：
- ADR-008：.NET 8 → **Tauri (Rust) 四平台**（V1.7，本节为当前有效）
- 历史：.NET 8（V1.2）→ .NET 10 LTS + WinForms + WebView2（V1.5）→ **Tauri 四平台**（V1.7）
- ADR-003/011 精化：WebView2 唯一引擎 → **各平台系统 WebView（Tauri 抽象）**

---

## 0. 结论（TL;DR）

| 层 | 选型 |
|---|---|
| **框架** | **Tauri 2**（Rust + Web 前端） |
| **前端（管理控制台）** | Web（HTML/CSS/JS，框架自选：React/Vue/Svelte，推荐 Svelte 轻量） |
| **渲染引擎** | 各平台系统 WebView（Windows=WebView2 / macOS=WKWebView / Linux=WebKitGTK / **鸿蒙=ArkWeb**） |
| **语言** | Rust（daemon 逻辑 / 系统集成） |
| **本地 HTTP（管理 API + 静态控制台）** | Rust `axum`/`hyper`（或 tauri-plugin-http），仅回环 + 随机端口 + 会话 token |
| **系统集成** | Rust：`windows`/`objc2`/`gtk` crate + tauri-plugin；互斥体 / 管道 / 注册表 / UAC / .lnk |
| **单例 + IPC** | 互斥体 + 命名管道（或 Tauri 原生 IPC） |
| **跨平台** | ✅ **Windows / macOS / Linux / 鸿蒙（OpenHarmony）四平台** |

**一句话**：**Tauri 2（Rust + Web 前端）——唯一天然覆盖 Win/macOS/Linux/鸿蒙 四平台的方案。**

> **核心决策变更**：用户需求升级为"天然四平台"。Tauri 2 是 2026 年唯一官方路线覆盖这四平台的框架（鸿蒙支持为 `feat/open-harmony` 分支，进行中）。原 .NET/WebView2 仅 Windows，无法满足。

---

## 1. 为什么转向 Tauri（核心决策）

### 1.1 需求驱动：四平台
用户明确要求 **天然支持 Windows / macOS / Linux / 鸿蒙 OS 四平台**。
- .NET/WebView2：仅 Windows；
- Electron：Win/macOS/Linux，**无鸿蒙官方**；
- **Tauri：Win/macOS/Linux/鸿蒙 官方路线覆盖（鸿蒙为 `feat/open-harmony` 分支，开发中）**。

### 1.2 单引擎需求恰好匹配 Tauri 的单 WebView 模型
ADR-011 移除模式 A 后，平台单引擎（每应用独立 webview）。这正是 Tauri 的自然模型——每 app 一个 `WebviewWindow`，天然多窗口多 webview。之前"Tauri 单 webview 错配"的判断不再成立。

### 1.3 轻量
Tauri 包体 3-10MB、内存 30-50MB，比 .NET+WebView2 更轻，符合"轻量平台"定位。

### 1.4 Web 控制台跨平台零改动
管理控制台是 Web（ADR-007），任何平台浏览器即渲染——UI 层完全跨平台，只需移植 daemon 壳。

---

## 2. 各平台渲染引擎（Tauri 落地）

| 平台 | 引擎 | 说明 |
|---|---|---|
| Windows | WebView2 | Tauri Windows 默认；runtime 系统自带（Win11），安装器可引导 |
| macOS | WKWebView | Apple 系统 WebKit，稳定 |
| Linux | WebKitGTK | 系统 WebKit，需保证系统依赖 |
| 鸿蒙 OpenHarmony | ArkWeb | Tauri `feat/open-harmony` 分支支持；ArkWeb 基于 Chromium 内核（华为自研），生态兼容 |

> **ArkWeb 关键事实**：HarmonyOS NEXT 的 ArkWeb **基于 Chromium 内核**（华为基于 Chromium 自研）。因此被管理的 Web 应用在鸿蒙上兼容性良好；扩展机制需按鸿蒙生态适配。

## 3. 为什么不是 .NET/WebView2（原方案，已弃）

| 维度 | .NET/WebView2（旧） | Tauri（新） |
|---|---|---|
| Windows | ✅ | ✅ |
| macOS | ❌ | ✅ |
| Linux | ❌ | ✅ |
| 鸿蒙 | ❌ | ✅（`feat/open-harmony`） |
| 包体 | 60-100MB | 3-10MB |
| 空闲内存 | <50MB（需裁剪） | 30-50MB（天然） |
| 身份隔离（cookie/扩展） | WebView2 原生 API | 需按平台 webview 实现（但单引擎已简化） |

**结论**：四平台需求下，Tauri 是唯一满足的，且更轻。原方案弃用。

## 4. 为什么不是其它跨平台方案

| 方案 | 四平台？ | 问题 |
|---|---|---|
| Electron | 否（无鸿蒙） | 包体 300MB+；无鸿蒙官方 |
| Qt (WebEngine) | 部分（鸿蒙复杂） | WebEngine 在鸿蒙复杂；C++ 重；Qt 授权 |
| Flutter | 部分（鸿蒙社区移植） | 鸿蒙非官方主流；非纯 Web 渲染（自绘） |
| **Tauri** | ✅ 全部 | **采纳** |

## 5. 组件 → 技术映射（Tauri 四平台）

| 需求 | 实现 | 备注 |
|---|---|---|
| 单例 | 互斥体（windows `CreateMutex` / macOS `NSLock` / Linux file lock） | tauri-plugin-single-instance |
| IPC / 指令转发 | 命名管道（Windows）/ 域套接字（macOS/Linux）/ Tauri 原生 IPC | 启动器 → daemon |
| 轻量启动器 | `webdesk-launcher`（超小二进制，各平台一个） | 与主服务解耦 |
| 工作项驱动生命周期 | daemon 状态机（ADR-010）：驻留型 app / 执行中钩子 / 控制台打开 | |
| 本地 HTTP | Rust `axum` + 静态文件 | 仅回环 + 随机端口 + token |
| 应用窗口承载 | Tauri `WebviewWindow` / `WebviewWindowBuilder` | 每 app 一个独立 webview |
| 身份（cookie） | WebView2 `CookieManager` / WKWebView `WKWebsiteDataStore` / ArkWeb cookie API | 各平台自实现，平台仓库统一 |
| 身份（密钥注入） | Tauri `add_script_to_execute_on_document_created` / 等价平台 API | |
| 身份（扩展） | WebView2 `AddBrowserExtensionAsync`；macOS/Linux/鸿蒙 按平台扩展机制 | 本地 unpacked（Windows），他平台后置 |
| 钩子执行 | Rust `std::process::Command` + `taskkill`/`kill` | cmd/powershell/wsl（Windows）；sh（类 Unix） |
| 进程树回收 | Job Object（Windows）/ 进程组（类 Unix） | 孤儿回收 |
| 托盘（动态） | `tauri` tray-icon API | ADR-010 按需出现 |
| 桌面快捷方式 | Windows `.lnk` / macOS `.app` alias / Linux .desktop | |
| 开机自启 | tauri-plugin-autostart | 安装时可选 |
| UAC 提权 | tauri-plugin（Windows manifest / runas） | |
| 配置持久化 | JSON，各平台配置目录 | Windows `%APPDATA%` / macOS `~/Library/Application Support` / Linux `~/.config` |
| 日志 | Rust `tracing` / `log` + 各平台日志目录 | |

## 6. 资源与性能预估

| 指标 | 目标 | 实现路径 |
|---|---|---|
| 空闲内存 | 30-50MB | Tauri 天然低内存；daemon 惰性加载 |
| 空闲 CPU | ≈0 | 事件驱动，无轮询 |
| 启动：快捷方式→转发 | <100ms | 轻量启动器 |
| 窗口渲染完成 | <2s | WebView 预热 |

## 7. 风险与缓解

| 风险 | 缓解 |
|---|---|
| **鸿蒙支持进行中**（`feat/open-harmony` 非 release） | Windows/macOS/Linux 先行，鸿蒙作为 Tauri 官方推进即可跟进；ArkWeb 兼容 Chromium 生态 |
| **Rust 学习曲线** | 相比 C# 更陡；但 Tauri 生态成熟，daemon 逻辑可控 |
| 身份隔离各平台自实现 | 用 Tauri WebviewWindow 独立窗口 + 各平台数据隔离；MVP 先做 Windows，其余按平台适配 |
| 扩展加载非 Windows 平台 | 本地 unpacked 扩展以 Windows（WebView2）为先，macOS/Linux/鸿蒙后置 |
| Kestrel→axum | Rust HTTP 生态成熟（axum 是 Tokio 官方推荐） |
| Rust 编译/构建 | CI 矩阵四平台构建；Tauri 的 `tauri-build` 成熟 |

## 8. 里程碑落点（Tauri 版）

- **M0**：Tauri 2 骨架 + 本地 HTTP（axum）+ 静态控制台页 + 单例/启动器 → 验证内存/启动/吃狗粮（Windows 先行）
- **M1**：核心能力（多窗口 Webview + 钩子 + 身份(cookie) + 驻留 + 快捷方式 + 工作项生命周期）
- **M2**：跨平台（macOS/Linux 适配）+ 扩展 + 自身更新 + 多语言 + 性能监控
- **M3**：鸿蒙（跟随 Tauri `feat/open-harmony`）

---

**附：研究依据（2026-08-25）**
- Tauri `feat/open-harmony` 分支：`cargo tauri ohos build`、OHOS_Home 配置、真实设备跑通（tauri-apps/tauri PR #15236/#15237，Islatri 实测，2026-04）
- Eclipse Oniro 官方：Ionic/Capacitor 与 Tauri 移植 OpenHarmony（Tauri 经 NAPI + Window Adoption 模式）
- ArkWeb：HarmonyOS NEXT 的 Web 组件，基于 Chromium 内核（华为自研），支持 Cookie 同步 / WebviewController / JSBridge
- Qt for HarmonyOS 6.12 官方支持；Flutter 鸿蒙为社区移植（非官方主流）
- WebView2 Fixed Version 250MB+（历史背景，Windows 相关）
- .NET 8/9 EOL 2026-11-10（历史背景，原方案依据）