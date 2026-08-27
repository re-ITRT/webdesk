/**
 * WebLaunch 管理 API 客户端。
 *
 * 所有请求均通过 HTTP 发送到本机固定端口（3070）上的后端服务；
 * 请求/响应字段名与后端 API 严格采用 snake_case 契约，禁止在此处做字段名转换。
 */

/**
 * 应用配置实体，与后端 /api/apps 的存储模型一一对应。
 */
export interface App {
  /** 应用唯一标识 */
  id: string;
  /** 应用显示名称 */
  name: string;
  /** 应用入口 URL */
  url: string;
  /** 运行时配置：system=系统默认，pinned=固定配置 */
  runtime_profile: "system" | "pinned";
  /** 关窗行为：background=驻留后台，quit=退出进程 */
  close_action: "background" | "quit";
  /** 生命周期钩子：启动前 / 退出后执行的命令列表 */
  hooks: { pre_launch: string[]; post_exit: string[] };
  /** 钩子执行选项：shell、超时（毫秒）、是否阻塞 */
  hook_options: { shell: string; timeout_ms: number; blocking: boolean };
  /** 窗口 UI 控件开关：地址栏 / 导航按钮 / 刷新 */
  ui_controls: { address_bar: boolean; nav_buttons: boolean; refresh: boolean };
  /** 页面注入内容与注入时机 */
  injections: { css: string; js: string; timing: "document_start" | "document_idle" };
  /** 图标（本地路径或 URL） */
  icon: string;
  /** 关联的扩展列表 */
  extensions: string[];
  /** 是否为系统内置应用（不可删除） */
  is_system: boolean;
  /** 是否随平台启动自动拉起 */
  launch_on_boot: boolean;
  /** 标签列表 */
  tags: string[];
  /** 创建时间 */
  created_at: string;
  /** 更新时间 */
  updated_at: string;
}

/**
 * 平台运行状态，由 /api/status 返回。
 */
export interface PlatformStatus {
  /** 正在前台运行的应用 ID 列表 */
  running: string[];
  /** 驻留后台的应用 ID 列表 */
  background: string[];
  /** 平台版本号 */
  version: string;
  /** 平台进程已运行时长（秒） */
  uptime_sec: number;
  /** 平台进程内存占用（KB） */
  memory_kb: number;
  /** 后端服务监听端口 */
  port: number;
}

/**
 * 健康检查结果，由 /api/health 返回。
 */
export interface Health {
  /** 健康状态描述（如 ok） */
  status: string;
  /** 平台版本号 */
  version: string;
  /** 运行平台标识 */
  platform: string;
  /** 平台进程 PID */
  pid: number;
}

/** 后端 API 端口（模块级可变状态，初始化或手动指定后生效） */
let apiPort = 0;

/** 固定端口（用户指定，后端默认监听端口） */
const FIXED_PORT = 3070;

/**
 * 初始化 API 客户端：将端口固定为 3070。
 * @returns 初始化是否成功（当前恒为 true）
 */
export async function initApi(): Promise<boolean> {
  apiPort = FIXED_PORT;
  return true;
}

/**
 * 手动指定 API 端口（覆盖默认固定端口）。
 * @param port 后端服务端口
 */
export function setApiManual(port: number) {
  apiPort = port;
}

/**
 * 判断 API 客户端是否已就绪（端口已确定）。
 */
export function isApiReady(): boolean {
  return apiPort > 0;
}

/**
 * 通用请求封装：向本机后端发起 JSON 请求并解析响应。
 *
 * - 统一注入 Content-Type: application/json（可被调用方覆盖）；
 * - 204 无内容响应返回 null；
 * - 非 2xx 状态码抛出包含后端错误消息的异常。
 *
 * @param path API 路径（如 /api/apps）
 * @param options fetch 请求选项
 * @returns 解析后的响应体
 */
async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const res = await fetch(`http://127.0.0.1:${apiPort}${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(options.headers || {}),
    },
  });
  const body = res.status === 204 ? null : await res.json().catch(() => null);
  if (!res.ok) {
    const err = (body as any)?.message || `HTTP ${res.status}`;
    throw new Error(err);
  }
  return body as T;
}

// ---------- API 端点 ----------

/**
 * API 端点集合：每个方法对应一个后端 REST 端点。
 * 返回类型与后端响应体（snake_case）严格对齐。
 */
export const api = {
  /** 健康检查 */
  health: () => request<Health>("/api/health"),
  /** 平台运行状态 */
  status: () => request<PlatformStatus>("/api/status"),
  /** 应用列表 */
  listApps: () => request<App[]>("/api/apps"),
  /** 创建应用 */
  createApp: (app: Partial<App>) =>
    request<App>("/api/apps", { method: "POST", body: JSON.stringify(app) }),
  /** 获取单个应用详情 */
  getApp: (id: string) => request<App>(`/api/apps/${id}`),
  /** 更新应用（部分字段） */
  updateApp: (id: string, patch: Partial<App>) =>
    request<App>(`/api/apps/${id}`, { method: "PUT", body: JSON.stringify(patch) }),
  /** 删除应用 */
  deleteApp: (id: string) => request<null>(`/api/apps/${id}`, { method: "DELETE" }),
  /** 启动应用 */
  launchApp: (id: string) =>
    request<{ status: string; windowId?: string }>(`/api/apps/${id}/launch`, { method: "POST" }),
  /** 激活已运行的应用窗口 */
  activateApp: (id: string) =>
    request<{ status: string }>(`/api/apps/${id}/activate`, { method: "POST" }),
  /** 终止应用 */
  terminateApp: (id: string) =>
    request<{ status: string }>(`/api/apps/${id}/terminate`, { method: "POST" }),
  /** 查询单个应用运行状态 */
  appStatus: (id: string) =>
    request<{ id: string; status: string }>(`/api/apps/${id}/status`),
  /** 创建桌面快捷方式（可选指定图标） */
  createShortcut: (id: string, icon?: string) =>
    request<{ created: boolean; path?: string }>(`/api/apps/${id}/shortcut`, {
      method: "POST",
      body: JSON.stringify(icon ? { icon } : {}),
    }),
  /** 移除桌面快捷方式 */
  removeShortcut: (id: string) =>
    request<{ removed: boolean }>(`/api/apps/${id}/shortcut`, { method: "DELETE" }),
};
