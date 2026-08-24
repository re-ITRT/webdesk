# WebDesk CLI 命令体系

**目的**：提供与 Web 管理控制台（管理 UI）**功能对等**的命令行操作。CLI 是独立轻量二进制（`webdesk-cli`），通过本地 HTTP 管理 API 与 daemon 通信。

## 发现 daemon

- daemon 启动时把 `{port, token}` 写入 `%APPDATA%/WebDesk/api.json`。
- CLI 读取该文件获取 API 地址与鉴权 token。
- 若 daemon 未运行，CLI 自动拉起 daemon（`webdesk.exe --hidden`），等待健康检查通过后执行命令。

## 命令总览（与 Web UI 功能对等）

| 命令 | 等价 Web UI | 说明 |
|---|---|---|
| `webdesk-cli app add -n <name> -u <url> [--close background\|quit] [--pre <hook>] [--post <hook>] [--shell cmd\|powershell\|wsl\|sh] [--timeout <ms>]` | 添加应用 | 新增应用（CLI 短名 `addweb`） |
| `webdesk-cli app list` | 应用网格 | 列出全部应用（含状态/URL/系统标记） |
| `webdesk-cli app get <id>` | 编辑页 | 查看单个应用详情 |
| `webdesk-cli app remove <id>` | 删除应用 | 删除应用 |
| `webdesk-cli app launch <id>` | 启动 | 启动应用（已运行则激活） |
| `webdesk-cli app stop <id>` | 终止 | 彻底终止应用 |
| `webdesk-cli app activate <id>` | 唤出 | 激活后台驻留应用 |
| `webdesk-cli app status <id>` | 状态徽标 | 查看应用运行状态 |
| `webdesk-cli app shortcut <id>` | 发送到桌面 | 创建桌面快捷方式 |
| `webdesk-cli status` | 平台状态栏 | 平台整体状态（运行/驻留/版本/端口） |
| `webdesk-cli console` | 显示主面板 | 打开 WebDesk 管理控制台 |
| `webdesk-cli version` | 版本 | 显示版本 |

## 命令格式（用户示例）

```bash
# 添加 Web 应用
webdesk-cli addweb -url localhost:3000 -name "Dev Server" -hook "echo pre" -hook_exit "echo post"

# 别名（addweb = app add）
webdesk-cli addweb -u http://localhost:8648/ -n "本地服务"

# 列表 / 状态
webdesk-cli app list
webdesk-cli status

# 启动 / 停止
webdesk-cli app launch <id>
webdesk-cli app stop <id>
```

## 实现要点

- 独立 bin：`src-tauri/src/bin/webdesk-cli.rs`（`cargo build --bin webdesk-cli`）。
- 依赖：`clap`（参数解析）、`reqwest`（blocking，HTTP）、`serde_json`。
- 错误处理：daemon 不可达 → 提示"daemon 未运行，正在启动…"并拉起。
- 输出：人类可读文本（非 JSON），含颜色标记（绿色=成功/红色=错误，可选）。
