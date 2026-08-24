# 《WebDesk 通用 Web 应用桌面化管理平台》需求总览（Master）

**版本：** M1.1
**日期：** 2026-08-25
**状态：** 综合草案（覆盖 V1.1 基线 + V1.2/V1.3/V1.4 决策 + 全部待定项已敲定）

> 本文件是**需求总览**：把全量需求条目集中在一处，标注状态与优先级。
> 详细论述仍以 `requirements-v1.2-draft.md`（含 V1.3 修订）与 `docs/design/` 下的决策记录为准。

> **V1.4 追加**：① 应用级身份 **AppIdentity**——cookie/密钥/插件按应用隔离并跨内核一致，模式 A 默认独立 profile（ADR-009，修订 ADR-002）；② **工作项驱动生命周期**——平台随第一个 app 启动/最后一个工作项结束而启停，托盘默认无图标、仅当存在后台驻留应用时动态出现（ADR-010）。详见 `docs/design/2026-08-25-app-identity-across-engines.md` 与 `docs/design/2026-08-25-work-item-lifecycle.md`。
>
> **V1.5（2026-08-25 追加）**：**技术选型定案**——C#/.NET 10 LTS（.NET 8/9 于 2026-11-10 EOL）+ WinForms 薄壳 + WebView2 + ASP.NET Core Minimal API；Fixed Version 运行时改为平台共享目录 + per-app opt-in（250MB+ 不宜 per-app 打包）。详见 `docs/design/2026-08-25-tech-selection.md`。
>
> **V1.6（2026-08-25 追加）**：**完全移除模式 A（系统 Chrome）**——WebView2 成为唯一渲染引擎（ADR-011）。理由：模式 A 复杂度（Chrome 探测/profile 策略/进程树边界/CDP 风险/双引擎协调）≫ 价值（复用系统 Chrome 登录态，已被 ADR-009 放弃）。详见 `docs/design/2026-08-25-drop-mode-a.md`。
>
> **V1.7（2026-08-25 追加）**：**技术栈转向 Tauri（Rust）四平台**——用户需求升级为"天然支持 Windows/macOS/Linux/鸿蒙 四平台"。Tauri 2 是 2026 年唯一官方路线覆盖四平台的框架（鸿蒙为 `feat/open-harmony` 分支，开发中）。原 .NET/WebView2（仅 Windows）弃用。详见 `docs/design/2026-08-25-tech-selection.md`。

**图例**
- ✅ 已定（附决策出处）
- ⚠️ 待定（需用户拍板，见 §5）
- 优先级：**P0** 首版必须 / **P1** 重要 / **P2** 可选、后续

---

## 1. 产品定位

### 1.1 一句话
把任意 Web 应用当作“原生桌面应用”来运行与管理的**四平台（Windows/macOS/Linux/鸿蒙）**平台：**系统 WebView 单引擎渲染（Tauri）+ per-app 身份隔离（cookie/密钥/扩展）+ 生命周期钩子 + 工作项驱动生命周期（无感常驻/按需托盘）+ 后台驻留 + 桌面快捷方式 + Web 管理控制台**。

### 1.2 差异化（对标）
| 方案 | 差距 |
|---|---|
| Chrome/Edge “安装为应用” | 无生命周期钩子、无 per-app 注入/扩展隔离、无托盘级驻留管理 |
| WebCatalog | 原生 UI（我们是 Web 控制台）；无工作项生命周期（无感常驻） |
| Fluid / Coherence | 仅 macOS |
| Ferdium / Rambox | 多应用挤单一窗口，非独立窗口 |
| Electron 包装 | 包体大、更新滞后，正是本项目要避免的 |

**差异点 = per-app 身份隔离（cookie/密钥/扩展）+ 生命周期钩子 + 工作项生命周期 + Web 控制台（吃自己狗粮）** 的组合。

### 1.3 明确不做（Do-not）
- ❌ 应用商店 / 应用分发市场
- ❌ 浏览器调试 UI（不面向普通用户暴露 CDP）
- ❌ **模式 A（系统 Chrome 调用）**——ADR-011，WebView2 单引擎
- ❌ 同应用多窗口并存（默认单窗口，重复启动=激活已有窗口）

---

## 2. 功能需求

### 2.1 应用数据模型（核心实体）

**App 实体字段**（✅=已定；⚠️=待定）：

| 字段 | 类型/取值 | 状态 | 说明 |
|---|---|---|---|
| `id` | string（唯一） | ✅ | 应用唯一标识，供 `--launch <id>` 使用 |
| `name` | string | ✅ | 显示名 |
| `url` | string | ✅ | 应用地址 |
| `runtimeProfile` | `"system"` \| `"pinned"` | ⚠️ | 渲染运行时档位：默认跟随系统 WebView；`pinned`=锁定版本（仅 Windows WebView2 支持 Fixed Version；其余平台系统 WebView 无此概念，暂忽略） |
| `closeAction` | `"background"` \| `"quit"` | ⚠️ O2 | 关窗行为 |
| `hooks` | `{ preLaunch[], postExit[] }` | ⚠️ O3 | 钩子命令列表 |
| `hookOptions` | shell / timeout / blocking | ⚠️ O3 | 每个事件的钩子选项 |
| `uiControls` | 地址栏/导航/刷新… | ✅ | 原生控件开关 |
| `injections` | CSS / JS + 时机 | ⚠️ O4 | |
| `extensions` | 扩展标识列表 | ⚠️ O5 | 本地 unpacked |
| `appIdentity` | 身份容器（cookie / secrets / extensions）| ✅ ADR-009 | 按应用隔离、平台统一管理（ADR-011 单引擎化） |
| `launchOnBoot` | bool | ⚠️ O10 | 开机自启（平台级或应用级） |
| `isSystem` | bool | ✅ | 系统应用标记（管理控制台） |
| `tags` | string[] | ⚠️ O11 | 分组/标签（P2） |
| `createdAt` / `updatedAt` | time | ✅ | |

> 注：ADR-011 移除模式 A 后，`renderEngine`、`chromeProfile` 字段不再存在——引擎单一（WebView2）。

**CRUD**：控制台提供应用增删改查；删除前二次确认；系统应用（管理控制台）受保护——可删除但可经管理 API 恢复重建。

### 2.2 渲染引擎（单引擎：系统 WebView，Tauri 四平台）

> **ADR-011 + V1.7**：完全移除模式 A（系统 Chrome）。渲染引擎由 **Tauri 抽象为各平台系统 WebView**（Windows=WebView2 / macOS=WKWebView / Linux=WebKitGTK / 鸿蒙=ArkWeb），四平台统一。产品定位：**轻量四平台 Web SSB 管理平台**。

**渲染能力（Tauri 各平台系统 WebView）**
| 项 | 状态 | 说明 |
|---|---|---|
| 内核 | ✅ | Tauri 抽象：各平台系统 WebView（Win=WebView2 / mac=WKWebView / Linux=WebKitGTK / 鸿蒙=ArkWeb） |
| 每应用隔离 | ✅ | 每应用独立 WebviewWindow / 独立数据目录 |
| cookie 管理 | ✅ | 各平台 cookie API（Win=CookieManager 等） |
| CSS/JS 注入 | ✅ | `add_script_to_execute_on_document_created`（跨平台） |
| 扩展加载（per-app） | ⚠️ P1 | Windows(WebView2) 本地 unpacked 优先；macOS/Linux/鸿蒙 按平台扩展机制后置 |
| 运行时缺失 | ✅ | Win=WebView2 bootstrap；mac=系统自带；Linux=系统依赖 |
| 后台驻留 | ✅ | 隐藏窗口不销毁渲染器 → WebSocket 保活 |
| 深控 | ✅ | 原生控件开关 / 注入 / 扩展，无 CDP 暴露 |
| DevTools | ✅ | Tauri 支持，默认不对普通用户暴露 |
| **四平台** | ✅ | **Windows / macOS / Linux / 鸿蒙** |

### 2.3 生命周期钩子
| 项 | 状态 |
|---|---|
| preLaunch / postExit 两个事件 | ✅ |
| Shell 环境：cmd / powershell / wsl | ✅ |
| 阻塞 / 非阻塞 | ✅ |
| 超时（默认 30s，可配，超时强制终止） | ✅ |
| 退出码采集 | ✅ |
| stdout / stderr 落盘日志 | ✅ |
| 进程树回收（taskkill /T；WSL 进程组单独处理） | ✅ |
| 多命令顺序执行 + 失败策略 | ⚠️ O3 |
| 注入上下文变量（appId / url / 端口…） | ⚠️ 建议补 |
| 并发：多应用钩子并行执行 | ✅（需设计调度） |

### 2.4 后台持久化驻留
| 项 | 状态 | 说明 |
|---|---|---|
| 关窗=隐藏不销毁渲染器 | ✅ | WebSocket/后台任务自然保活（ADR-011 单引擎后统一） |
| per-app closeAction 可配 | ⚠️ O2 | |
| 托盘：应用列表 / 唤出 / 彻底终止 | ✅ | 按需动态出现（ADR-010） |
| 后台网络保活 | ✅ | 渲染器存活即保活 |

### 2.5 界面及功能栏定制
| 项 | 状态 | 说明 |
|---|---|---|
| 浏览器原生控件开关 | ✅ | WebView2 原生控件开关 |
| CSS / JS 注入 | ✅ | 附安全提示（见 §3.4） |
| 注入时机 | ⚠️ O4 | document_start / document_idle |
| 扩展按需加载（per-app 列表） | ✅ | 本地 unpacked |
| 扩展来源 | ⚠️ O5 | 本地 unpacked / Chrome Web Store |

### 2.6 桌面快捷方式
| 项 | 状态 |
|---|---|
| 一键创建 `.lnk`（“发送到桌面”） | ✅ |
| 启动协议 `WebDesk.exe --launch <appId>` | ✅ |
| 独立轻量启动器（数百 KB，与主服务解耦） | ✅ |
| 移除快捷方式 | ✅ |
| 图标自定义（favicon 自动 / 手动） | ⚠️ O6 |

### 2.7 单例与进程调度
| 项 | 状态 |
|---|---|
| 单例：命名互斥体 | ✅ |
| IPC：命名管道 | ✅ |
| 指令协议：launch / activate / terminate | ✅ |
| 已有窗口→激活；无→按配置创建 | ✅ |
| 重复点击快捷方式→IPC 转发后自身退出 | ✅ |

### 2.8 后台服务（daemon）
| 项 | 状态 | 说明 |
|---|---|---|
| 生命周期：工作项驱动 | ✅ ADR-010 | 平台随“第一个 app 启动/最后一个工作项结束”而启停；非“随窗口数” |
| 常驻进程（非 Windows Service） | ✅ | 规避 Session 0 托盘问题（ADR-005）；有工作项时无感常驻 |
| 托盘 | ✅ 按需 | 默认无图标；仅当存在后台驻留应用时动态出现（ADR-010） |
| 开机自启 | ⚠️ O10 | |
| 本地 HTTP：管理 API + 静态控制台 | ✅ | 仅 127.0.0.1 + 随机端口 + 会话 token |
| Job Object 进程树回收 | ✅ | 主进程崩溃时子进程自动回收（ADR-006） |
| 子进程监控 / 崩溃处理 | ✅ | 需细化 |
| 平台日志（分级） | ✅ | |
| 退出流程：postExit 钩子先善后 | ✅ ADR-010 | 退出前执行所有存活 postExit 钩子（等完成/超时），再终止渲染器/独立 profile |

### 2.9 管理控制台（Web 形态）
| 项 | 状态 | 说明 |
|---|---|---|
| 首个默认应用（isSystem） | ✅ | 平台安装即预置“WebDesk 控制台” |
| 应用网格（模式标签） | ✅ | |
| 编辑页（URL / 引擎 / 钩子 / UI 开关 / 注入 / 扩展） | ✅ | |
| 运行状态展示（运行中 / 后台 / 内存） | ⚠️ O14 | P2 |
| 可删除 / 可经 API 恢复 | ✅ | |
| 恢复通道：浏览器直开 localhost | ✅ | 内置渲染异常时的兜底 |
| 语言（中文 / i18n） | ⚠️ O8 | |

### 2.10 系统托盘
| 项 | 状态 |
|---|---|
| 按需动态出现（默认无图标） | ✅ ADR-010 |
| 仅当存在后台驻留应用时出现 | ✅ ADR-010 |
| 菜单：驻留应用列表 / 唤出 / 终止 / 显示主面板 | ✅ ADR-010 |
| 全部驻留应用结束后图标消失 | ✅ ADR-010 |
| 非驻留场景唤出：桌面快捷方式（IPC） | ✅ |

---

## 3. 非功能需求

### 3.1 性能
| 项 | 目标 | 状态 |
|---|---|---|
| 空闲 CPU | ≈0 | ✅ |
| 空闲内存（无应用运行时） | <50MB | ✅ |
| 后台驻留时（无托盘/无感驻留） | 亦须 CPU≈0 / 内存<50MB | ✅ ADR-010 |
| 启动：快捷方式→转发 | <100ms | ✅（建议值） |
| 启动：窗口渲染完成（不含钩子） | <2s | ✅（建议值） |

### 3.2 兼容性
| 项 | 状态 |
|---|---|
| Windows 10/11 优先；macOS 后续 | ✅ |
| 钩子 shell：cmd / powershell / wsl | ✅ |
| WebView2 运行时缺失检测 + 引导下载 | ✅ |
| DPI / 多显示器 | ⚠️ P2 |
| 网络代理（跟随系统 / per-app） | ⚠️ O17 |

### 3.3 配置持久化
| 项 | 状态 |
|---|---|
| JSON，`%APPDATA%\WebDesk\config\` | ✅ |
| 导出 / 导入（备份与迁移） | ✅ |
| `runtimeProfile` 字段（evergreen/fixed） | ✅ |
| 配置损坏容错（备份 / 恢复） | ⚠️ 建议补 |

### 3.4 安全
| 项 | 状态 |
|---|---|
| 管理 API：仅回环 + 随机端口 + 会话 token | ✅ |
| 注入脚本风险提示（“仅向可信应用注入”） | ✅ |
| 扩展安全（仅可信来源） | ✅ |
| UAC：钩子需管理员时的提权策略 | ⚠️ O18 |
| Job Object 孤儿回收 | ✅ |
| CDP 风险（模式 A 深控，未来） | ✅（记录） |

### 3.5 日志与排障
| 项 | 状态 |
|---|---|
| 钩子输出 + 退出码落盘 | ✅ |
| 平台日志（启动/关闭/IPC/调度）分级 | ✅ |
| 日志目录 `%APPDATA%\WebDesk\logs\` | ✅ |

### 3.6 更新与分发
| 项 | 状态 |
|---|---|
| WebView2 Fixed Version 更新发布机制 | ✅（方向） |
| WebDesk 自身更新（GitHub Releases 自动检查 / 手动） | ⚠️ O9 |
| 安装 / 卸载干净清理（配置/日志/独立 profile） | ⚠️ O15 |

### 3.7 可靠性
| 项 | 状态 |
|---|---|
| 崩溃恢复 / 孤儿进程清理 | ✅（Job Object） |
| 配置损坏容错 | ⚠️ 见 §3.3 |

---

## 4. 架构决策汇总（ADR）

| 编号 | 决策 | 出处 |
|---|---|---|
| ADR-001 | 双模引擎 per-app，非全局开关 | V1.1 |
| ADR-002 | 模式 A = 轻控制（深定制仅模式 B） | V1.2 |
| ADR-002（修订）| 模式 A = profile 级控制：默认独立 profile，应用级身份隔离+跨内核一致；仍不开放 CDP | V1.4 / ADR-009 |
| ADR-011 | **完全移除模式 A（系统 Chrome）**——渲染引擎由 Tauri 抽象为各平台系统 WebView（单引擎化） | V1.6/1.7 |
| ADR-003（修订2）| 渲染引擎 = 各平台系统 WebView（Win=WebView2 / mac=WKWebView / Linux=WebKitGTK / 鸿蒙=ArkWeb），Tauri 抽象 | V1.7 / 技术选型 |
| ADR-004 | 后台驻留按模式分机制（B 隐藏 / A 退出） | V1.2 |
| ADR-010 | 工作项驱动生命周期：平台随“第一个 app 启动/最后一个工作项结束”而启停；托盘按需动态出现 | V1.4 |
| ADR-005 | 后台服务层 = 常驻进程，非 Windows Service | V1.2 |
| ADR-006 | 进程回收 = Job Object | V1.2 |
| ADR-007 | 管理界面 = Web 控制台，且为平台首个默认应用 | V1.3 |
| ADR-008（修订2）| 技术栈 = **Tauri 2 (Rust + Web)**，四平台（Win/macOS/Linux/鸿蒙）| V1.7 / 技术选型 |
| ADR-009 | 应用身份（AppIdentity）：cookie / 密钥 / 插件按应用隔离并跨内核一致；模式 A 默认独立 profile | V1.4 |

---

## 5. 决策记录（原待定事项 O1–O18，均已敲定）

> O1–O18 已全部定案。O1 由用户新增的"应用级身份"需求拍板；O2–O18 采纳默认建议。详细论述见 `docs/design/2026-08-25-app-identity-across-engines.md`。

| # | 决策 | 结论（已定） | 优先级 |
|---|---|---|---|
| O1 | 模式 A profile 策略 | **独立 profile 为默认**，共享 profile 为显式 opt-in（复用登录态场景）；身份跨内核一致 | **P0** ✅ |
| O2 | 关窗行为 | per-app 可配，默认驻留 | P1 |
| O3 | 钩子多条命令 | 支持多条顺序执行；失败策略可配（默认中止） | P1 |
| O4 | 注入执行时机 | document_start 与 DOM 就绪两档 | P1 |
| O5 | 扩展来源 | 先本地 unpacked；Web Store 后置 | P1 |
| O6 | 应用图标 | 自动抓 favicon + 手动上传 | P1 |
| O7 | LAN 远程管理 | **本期不做**，仅回环；未来 opt-in 需 HTTPS+认证 | P2 |
| O8 | 控制台语言 | 中文优先，预留 i18n | P1 |
| O9 | 自身更新 | GitHub Releases 自动检查+提示 | P2 |
| O10 | 开机自启 | 安装时可选；平台默认常驻托盘 | P1 |
| O11 | 分组/标签 | 预留 tags 字段，UI 后置 | P2 |
| O12 | 系统通知 | 跟随 WebView2 默认，平台提供开关 | P1 |
| O13 | 下载/打印 | 默认行为，平台仅记录 | P2 |
| O14 | 性能监控 | MVP 后置 | P2 |
| O15 | 卸载清理 | 提供清理配置/日志/profile 选项 | P1 |
| O16 | 网络代理 | 跟随系统；per-app 后置 | P2 |
| O17 | UAC 提权 | per-app “需要管理员”标记，启动时提升 | P1 |
| O18 | 配置损坏容错 | 自动备份（保留最近 N 份） | P1 |

---

## 6. 里程碑建议

| 里程碑 | 内容 | 目标 |
|---|---|---|
| **M0 原型闭环** | daemon（HTTP + 调度 + 单例 + 托盘）+ 静态控制台页（CRUD）+ WebView2 承载窗口 | 验证：内存 <50MB、启动延迟、控制台吃自己狗粮 |
| **M1 MVP** | WebView2 单引擎 + 生命周期钩子 + 后台驻留 + 桌面快捷方式 + 单例调度 + **应用级身份（cookie/密钥/扩展）** | 核心产品可用；身份按应用隔离、平台统一管理 |
| **M2 完整版** | Fixed Version 共享 / 自身更新 / 多语言 / 性能监控 | 对标 WebCatalog 完整功能 |

> 注：ADR-011 移除模式 A 后，原 M2 的"跨内核 cookie 自动同步"与"模式 A 密钥注入"需求消失。
