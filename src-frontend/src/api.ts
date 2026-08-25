// WebDesk 管理 API 客户端
// 字段名与后端 API（snake_case）严格对齐。

export interface App {
  id: string;
  name: string;
  url: string;
  runtime_profile: "system" | "pinned";
  close_action: "background" | "quit";
  hooks: { pre_launch: string[]; post_exit: string[] };
  hook_options: { shell: string; timeout_ms: number; blocking: boolean };
  ui_controls: { address_bar: boolean; nav_buttons: boolean; refresh: boolean };
  injections: { css: string; js: string; timing: "document_start" | "document_idle" };
  extensions: string[];
  is_system: boolean;
  launch_on_boot: boolean;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface PlatformStatus {
  running: string[];
  background: string[];
  version: string;
  uptime_sec: number;
  memory_kb: number;
  port: number;
}

export interface Health {
  status: string;
  version: string;
  platform: string;
  pid: number;
}

let apiPort = 0;

/** 固定端口（用户指定） */
const FIXED_PORT = 3070;

export async function initApi(): Promise<boolean> {
  apiPort = FIXED_PORT;
  return true;
}

export function setApiManual(port: number) {
  apiPort = port;
}

export function isApiReady(): boolean {
  return apiPort > 0;
}

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

// ---------- 端点 ----------

export const api = {
  health: () => request<Health>("/api/health"),
  status: () => request<PlatformStatus>("/api/status"),
  listApps: () => request<App[]>("/api/apps"),
  createApp: (app: Partial<App>) =>
    request<App>("/api/apps", { method: "POST", body: JSON.stringify(app) }),
  getApp: (id: string) => request<App>(`/api/apps/${id}`),
  updateApp: (id: string, patch: Partial<App>) =>
    request<App>(`/api/apps/${id}`, { method: "PUT", body: JSON.stringify(patch) }),
  deleteApp: (id: string) => request<null>(`/api/apps/${id}`, { method: "DELETE" }),
  launchApp: (id: string) =>
    request<{ status: string; windowId?: string }>(`/api/apps/${id}/launch`, { method: "POST" }),
  activateApp: (id: string) =>
    request<{ status: string }>(`/api/apps/${id}/activate`, { method: "POST" }),
  terminateApp: (id: string) =>
    request<{ status: string }>(`/api/apps/${id}/terminate`, { method: "POST" }),
  appStatus: (id: string) =>
    request<{ id: string; status: string }>(`/api/apps/${id}/status`),
  createShortcut: (id: string, icon?: string) =>
    request<{ created: boolean; path?: string }>(`/api/apps/${id}/shortcut`, {
      method: "POST",
      body: JSON.stringify(icon ? { icon } : {}),
    }),
  removeShortcut: (id: string) =>
    request<{ removed: boolean }>(`/api/apps/${id}/shortcut`, { method: "DELETE" }),
};
