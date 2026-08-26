# WebLaunch 使用指南

WebLaunch 把任意 Web 应用启动/托管为原生桌面应用。本指南覆盖日常用法：添加应用、启动/停止、桌面快捷方式、生命周期钩子、命令授权桥。

## 目录

1. [安装](#安装)
2. [添加应用](#添加应用)
3. [启动 / 停止 / 激活](#启动--停止--激活)
4. [桌面快捷方式](#桌面快捷方式)
5. [生命周期钩子](#生命周期钩子)
6. [命令授权桥](#命令授权桥)
7. [管理控制台](#管理控制台)
8. [CLI 命令速查](#cli-命令速查)

---

## 安装

1. 从 [Releases](https://github.com/re-ITRT/weblaunch/releases) 下载安装包（`WebLaunch_x.x.x_x64-setup.exe`）。
2. 运行安装。
3. 首次启动自动拉起后台 daemon，并自动打开**管理控制台**（`http://127.0.0.1:3070`）。

> 安装目录包含 `weblaunch.exe`（既是 CLI 又是 daemon，单二进制）。

## 添加应用

**管理控制台**：点右上角 **「+ 添加应用」**，填名称 + URL，保存。

**CLI**：

```bash
weblaunch addweb -url https://example.com -name "Example"
```

> 固定端口 `3070`，无鉴权。应用列表 / 状态在控制台和 CLI 均可查看。

## 启动 / 停止 / 激活

控制台每个应用卡片有 **启动 / 激活 / 终止** 按钮：

- **启动**：打开该应用的独立窗口（独立任务栏图标）。
- **激活**：应用已后台驻留时，唤回窗口。
- **终止**：关闭窗口并停止。

CLI：

```bash
weblaunch app launch <id>
weblaunch app activate <id>
weblaunch app terminate <id>
weblaunch app status <id>
```

## 桌面快捷方式

每个应用可创建桌面快捷方式，**图标绑定到应用本身**（自动抓取应用 favicon 转成 .ico）。

控制台：应用卡片 → **「桌面快捷方式」** → 选图标来源（默认 / 本地 .ico / 自动从网页获取）→ 创建。

CLI：

```bash
weblaunch app shortcut <id>
```

双击快捷方式即以 `--launch <id>` 直达该应用（自动启动 daemon + 打开窗口）。

## 生命周期钩子

**启动前钩子（pre-launch）**：应用窗口弹出前执行，用于准备环境（如启动本地服务、SSH 隧道、数据库）。
**关闭后钩子（post-exit）**：应用关闭后执行，用于清理。

在编辑应用的 **启动前钩子 / 关闭后钩子** 多行输入框里直接填命令。支持直接填 **bat 代码**（含换行 / `@echo off`），WebLaunch 自动落盘为 .bat 并执行。

> 钩子默认在 `cmd` 里运行。若命令需在 WSL 执行，填完整命令，如：
> `wsl -d Ubuntu-24.04 --cd ~ -e /home/user/myapp.sh`

## 命令授权桥

WebLaunch 注入 `window.webdesk.exec(command)` 到每个应用窗口。**网站代码可以安全地请求执行本地命令**（如点击文件在实体机打开）。

安全模型：

- 首次请求命令 → 弹**授权框**，显示要执行的命令。
- 可选勾选 **「以后不再提示」** → 记住本次授权（按 应用+命令 维度）。
- 用户点**允许** → 执行；**拒绝** → 拒绝。
- 已授权的命令后续直接执行，不再弹框。

> 授权记录保存在 `%APPDATA%/WebDesk/auth/grants.json`。

网站代码示例：

```js
// 点击文件在实体机打开（经授权）
const result = await window.webdesk.exec('explorer.exe "C:\\path\\file.txt"');
if (result.ok) console.log("executed");
```

## 管理控制台

- **应用** 页：网格展示所有应用，卡片有启动/终止/编辑/快捷方式/删除。
- **设置** 页：语言切换（中文 / English）。
- 操作结果以**自动消失的 toast 消息**提示（不再弹阻塞对话框）。
- 删除应用 / 移除快捷方式有二次确认。

## CLI 命令速查

```bash
weblaunch addweb -url <url> -name <name> [-hook "cmd"] [-hook_exit "cmd"]  # 添加应用
weblaunch app list        # 列出应用
weblaunch app get <id>    # 查看详情
weblaunch app launch <id> # 启动
weblaunch app stop <id>   # 停止
weblaunch app activate <id> # 激活
weblaunch app status <id> # 状态
weblaunch app shortcut <id> # 桌面快捷方式
weblaunch app remove <id> # 删除
weblaunch console         # 打开管理控制台
weblaunch status          # 平台状态
weblaunch version         # 版本
```

> 数据目录（应用配置 / 图标 / 授权 / 日志）在 `%APPDATA%/WebDesk/`（Windows）。
