// WebDesk 管理 API 客户端
//
// 契约：docs/design/api-contract.md
// 所有请求带 Bearer token（从 Tauri IPC 获取；Web 兜底让用户输入）。

export interface App {
  id: string;
  name: string;
  url: string;
  runtimeProfile: "system" | "pinned";
  closeAction: "background" | "quit";
  hooks: { preLaunch: string[]; postExit: string[] };
  hookOptions: { shell: "cmd" | "powershell" | "wsl" | "sh"; timeoutMs: number; blocking: boolean };
  uiControls: { addressBar: boolean; navButtons: boolean; refresh: boolean };
  injections: { css: string; js: string; timing: "document_start" | "document_idle" };
  extensions: string[];
  isSystem: boolean;
  launchOnBoot: boolean;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export interface PlatformStatus {
  running: string[];
  background: string[];
  version: string;
  uptimeSec: number;
  memoryKb: number;
  port: number;
}

export interface Health {
  status: string;
  version: string;
  platform: string;
  pid: number;
}

let apiPort = 0;
let apiToken = "";

/** 从 Tauri IPC 获取 API 配置（M0 前端在浏览器内调试时可手动设置） */
export async function initApi(): Promise<boolean> {
  // @ts-expect-error Tauri IPC（若在 Tauri 环境内）
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (invoke) {
    try {
      const cfg = await invoke("get_api_config");
      apiPort = cfg.port;
      apiToken = cfg.token;
      return true;
    } catch {
      return false;
    }
  }
  return false;
}

export function setApiManual(port: number, token: string) {
  apiPort = port;
  apiToken = token;
}

export function isApiReady(): boolean {
  return apiPort > 0 && apiToken.length > 0;
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const res = await fetch(`http://127.0.0.1:${apiPort}${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${apiToken}`,
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
};