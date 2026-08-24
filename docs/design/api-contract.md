# WebDesk 管理 API 接口文档

**版本**：0.1.0
**状态**：M0 接口契约（多模块协同开发的事实标准）
**约定**：本文档定义管理控制台（前端）与平台 daemon（后端）之间的全部 REST 接口。所有模块（server / frontend / identity / scheduler）必须以此为准实现。

---

## 1. 基础约定

### 1.1 传输
- **本地 HTTP**：仅绑定 `127.0.0.1`，随机端口。
- 前端经 `http://127.0.0.1:<port>/api/...` 访问。
- 端口与 token 由 daemon 在启动时生成，写入共享状态（前端经 Tauri IPC 获取）。

### 1.2 鉴权
- 所有 `/api/*` 请求必须带请求头：`Authorization: Bearer <session-token>`。
- 会话 token 由 daemon 启动时随机生成（32 字节 hex）。
- 无/错 token → `401 {"error":"unauthorized"}`。

### 1.3 响应格式
- 成功：`2xx`，体为数据 JSON。
- 失败：非 2xx，体为 `{"error": "<机器可读 code>", "message": "<人类可读>"}`。
- 错误码：`not_found` / `invalid_input` / `unauthorized` / `conflict` / `internal`。

### 1.4 数据模型（App）

```jsonc
{
  "id": "uuid-string",              // 唯一标识
  "name": "GitHub",                 // 显示名
  "url": "https://github.com",      // 应用地址
  "runtimeProfile": "system",       // "system"（跟随系统 WebView，默认）| "pinned"（锁定版本，暂仅 Windows）
  "closeAction": "background",      // "background"（关窗隐藏驻留，默认）| "quit"（关窗退出）
  "hooks": {                        // 生命周期钩子
    "preLaunch": [],                // 启动前钩子命令
    "postExit": []                  // 关闭后钩子命令
  },
  "hookOptions": {                  // 钩子选项
    "shell": "cmd",                 // "cmd" | "powershell" | "wsl" | "sh"
    "timeoutMs": 30000,             // 超时（毫秒）
    "blocking": true                // true=阻塞等待完成
  },
  "uiControls": {                   // 浏览器原生控件开关
    "addressBar": false,
    "navButtons": true,
    "refresh": true
  },
  "injections": {                   // CSS/JS 注入
    "css": "",
    "js": "",
    "timing": "document_idle"       // "document_start" | "document_idle"
  },
  "extensions": [],                 // 扩展路径列表（本地 unpacked）
  "isSystem": false,                // 系统应用标记（管理控制台本身）
  "launchOnBoot": false,            // 开机自启
  "tags": [],
  "createdAt": "ISO-8601",
  "updatedAt": "ISO-8601"
}
```

---

## 2. REST 端点

### 2.1 健康检查

#### `GET /api/health`
- 返回：`{"status":"ok","version":"0.1.0","platform":"windows","pid":1234}`

### 2.2 应用 CRUD

#### `GET /api/apps`
列出所有应用。
- 返回：`[App, ...]`（不含敏感字段，扩展用路径）

#### `POST /api/apps`
创建应用。
- 体：`{ name, url, ...(可选字段) }`
- 返回：`201` + `App`

#### `GET /api/apps/{id}`
获取单个应用。
- 返回：`App`
- 404：`not_found`

#### `PUT /api/apps/{id}`
更新应用。
- 体：`{ ...部分字段 }`（部分更新，缺失字段保留）
- 返回：`200` + `App`
- 404：`not_found`

#### `DELETE /api/apps/{id}`
删除应用。
- 返回：`204`
- 若为系统应用：`400 {"error":"cannot_delete_system_app"}`（管理控制台）
- 若运行中：先停止再删。

#### `POST /api/apps/{id}/restore`
恢复系统应用（如管理控制台被误删）。
- 返回：`200` + `App`

### 2.3 应用运行控制

#### `POST /api/apps/{id}/launch`
启动应用（如已运行则激活已有窗口）。
- 返回：`200` + `{"status":"running","windowId":"..."}`

#### `POST /api/apps/{id}/activate`
激活已有窗口（后台驻留应用唤出）。
- 返回：`200` + `{"status":"active"}`

#### `POST /api/apps/{id}/terminate`
彻底终止应用（杀进程树）。
- 返回：`200` + `{"status":"terminated"}`
- 若未运行：`200` + `{"status":"not_running"}`

#### `GET /api/apps/{id}/status`
应用运行状态。
- 返回：`{"id":"...","status":"running|background|stopped","windowId":"...","memoryKb":12345,"startedAt":"..."}`

### 2.4 平台状态

#### `GET /api/status`
平台整体状态。
- 返回：`{"running":["appId1","appId2"],"background":["appId3"],"version":"0.1.0","uptimeSec":123,"memoryKb":456,"port":1420}`

### 2.5 身份（cookie/密钥/扩展）管理

> 身份数据按应用隔离（ADR-009）。MVP 提供查询与导出；注入由 daemon 在启动时自动执行。

#### `GET /api/apps/{id}/identity`
查看应用身份摘要（不返回 cookie 明文）。
- 返回：`{"cookieCount":12,"extensions":["path1"],"hasSecrets":true}`

#### `POST /api/apps/{id}/identity/export-cookies`
导出应用 cookie（JSON，仅本机）。
- 返回：`200` + `[{domain,name,value,path,expires}]`

#### `POST /api/apps/{id}/identity/import-cookies`
导入应用 cookie。
- 体：`[{domain,name,value,path,expires}]`
- 返回：`200` + `{"imported":12}`

### 2.6 钩子日志

#### `GET /api/apps/{id}/logs`
应用钩子执行日志（分页）。
- Query：`?limit=50&offset=0`
- 返回：`{"items":[{timestamp,event,shell,command,exitCode,stdout,stderr}],"total":123}`

### 2.7 桌面快捷方式

#### `POST /api/apps/{id}/shortcut`
创建桌面快捷方式。
- 返回：`200` + `{"created":true,"path":"..."}`

#### `DELETE /api/apps/{id}/shortcut`
移除桌面快捷方式。
- 返回：`200` + `{"removed":true}`

---

## 3. 端口与 token 分发（前端获取方式）

前端（在 Tauri 环境内）经 **Tauri IPC command** 获取：

```rust
// Rust 侧（src-tauri/src/server/mod.rs）
#[tauri::command]
fn get_api_config() -> ApiConfig { ... }
```

- 返回：`{"port":1420,"token":"<hex>"}`。
- 前端 `src-frontend/src/api.ts` 负责封装 fetch + token 注入。

---

## 4. 事件（WebSocket / Tauri Event，M1 起）

平台向控制台推送运行状态变化（可选，M0 可轮询 `GET /api/status`）：
- `app-started`：应用启动
- `app-stopped`：应用退出
- `app-crashed`：渲染器崩溃
- `platform-idle`：平台进入空闲（即将退出）

---

## 5. 实现职责划分（多模块契约）

| 模块 | 文件 | 职责 |
|---|---|---|
| **server** | `src-tauri/src/server/` | HTTP 服务、路由、token 鉴权、静态托管、API 实现 |
| **store** | `src-tauri/src/store/` | 应用配置持久化（JSON，`%APPDATA%/WebDesk/config/`） |
| **scheduler** | `src-tauri/src/scheduler/` | 应用启动/激活/终止/驻留、工作项生命周期（ADR-010） |
| **hooks** | `src-tauri/src/hooks/` | 钩子命令执行（cmd/powershell/wsl/sh、超时、日志） |
| **identity** | `src-tauri/src/identity/` | cookie/密钥/扩展的隔离与注入（按应用） |
| **platform** | `src-tauri/src/platform/` | 各平台差异抽象（托盘/快捷方式/路径） |
| **frontend** | `src-frontend/` | 管理控制台（Web），消费本 API |
