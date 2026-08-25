// WebDesk 管理控制台（吃自己的狗粮：本页即运行在 WebDesk 上的第一个 Web 应用）
// 功能：应用网格 / 增删改查 / 启动·激活·终止 / 平台状态

import { api, initApi, setApiManual, isApiReady, App, PlatformStatus } from "./api";

const appEl = document.getElementById("app")!;

function h(tag: string, attrs: Record<string, string> = {}, children: (string | HTMLElement)[] = []): HTMLElement {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  for (const c of children) el.append(c instanceof HTMLElement ? c : document.createTextNode(c));
  return el;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K): HTMLElementTagNameMap[K] {
  return document.createElement(tag);
}

function render(): void {
  appEl.innerHTML = "";
  appEl.append(
    h("div", { class: "toolbar" }, [
      h("h1", {}, ["WebDesk 控制台"]),
      h("button", { id: "btn-refresh" }, ["刷新"]),
      h("button", { id: "btn-new" }, ["+ 添加应用"]),
    ]),
    h("div", { id: "status-bar", class: "status-bar" }, ["平台状态加载中…"]),
    h("div", { id: "grid", class: "grid" }),
    h("div", { id: "modal", class: "modal hidden" }),
  );

  document.getElementById("btn-refresh")!.onclick = refreshAll;
  document.getElementById("btn-new")!.onclick = () => openModal(null);
  refreshAll();
}

async function refreshAll(): Promise<void> {
  const statusEl = document.getElementById("status-bar")!;
  const grid = document.getElementById("grid")!;
  grid.innerHTML = "";
  try {
    const [status, apps] = await Promise.all([api.status(), api.listApps()]);
    statusEl.textContent = `版本 ${status.version} · 运行 ${status.running.length} · 后台 ${status.background.length} · 端口 ${status.port} · 内存 ${(status.memoryKb / 1024).toFixed(1)} MB`;
    renderGrid(apps, status);
  } catch (e) {
    statusEl.textContent = `连接失败: ${(e as Error).message}`;
  }
}

function renderGrid(apps: App[], status: PlatformStatus): void {
  const grid = document.getElementById("grid")!;
  for (const app of apps) {
    const running = status.running.includes(app.id);
    const background = status.background.includes(app.id);
    const stateLabel = running ? "运行中" : background ? "后台" : "已停止";
    const favicon = faviconUrl(app.url);

    const card = h("div", { class: `card ${app.isSystem ? "system" : ""}` }, [
      h("div", { class: "card-header" }, [
        h("img", { class: "app-icon", src: favicon, alt: "", loading: "lazy" }),
        h("strong", {}, [app.name]),
        h("span", { class: `badge ${stateLabel}` }, [stateLabel]),
        app.isSystem ? h("span", { class: "badge system" }, ["系统"]) : h("span", {}),
      ]),
      h("div", { class: "card-body" }, [
        h("div", { class: "app-url" }, [`${app.url}`]),
        h("div", { class: "muted" }, [`引擎 ${app.runtimeProfile} · 关窗 ${app.closeAction === "background" ? "驻留" : "退出"}`]),
      ]),
      h("div", { class: "card-actions" }, [
        h("button", { "data-act": "launch", "data-id": app.id }, running ? ["激活"] : ["启动"]),
        h("button", { "data-act": "terminate", "data-id": app.id, disabled: !running && !background ? "true" : "" }, ["终止"]),
        h("button", { "data-act": "edit", "data-id": app.id }, ["编辑"]),
        app.isSystem ? h("span", {}) : h("button", { "data-act": "delete", "data-id": app.id, class: "danger" }, ["删除"]),
      ]),
    ]);
    grid.append(card);
  }
  grid.querySelectorAll<HTMLButtonElement>("[data-act]").forEach((btn) => {
    btn.onclick = () => handleAction(btn.dataset.act!, btn.dataset.id!);
  });
}

/** 从应用 URL 提取 favicon 地址（Google favicon 服务，无后端依赖） */
function faviconUrl(url: string): string {
  try {
    const host = new URL(url).hostname;
    return `https://www.google.com/s2/favicons?domain=${host}&sz=32`;
  } catch {
    return "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><rect width='32' height='32' fill='%23888' rx='6'/><text x='16' y='22' text-anchor='middle' font-size='16' fill='white'>W</text></svg>";
  }
}

async function handleAction(act: string, id: string): Promise<void> {
  try {
    if (act === "launch") {
      const r = await api.launchApp(id);
      alert(r.status === "running" ? "已启动" : r.windowId ? `已启动 (${r.windowId})` : "操作完成");
    } else if (act === "terminate") {
      await api.terminateApp(id);
    } else if (act === "edit") {
      const app = await api.getApp(id);
      openModal(app);
    } else if (act === "delete") {
      if (confirm("确定删除该应用？")) await api.deleteApp(id);
    }
    refreshAll();
  } catch (e) {
    alert(`操作失败: ${(e as Error).message}`);
  }
}

function openModal(app: App | null): void {
  const modal = document.getElementById("modal")!;
  modal.classList.remove("hidden");
  modal.innerHTML = "";
  modal.append(
    h("div", { class: "modal-content" }, [
      h("h2", {}, [app ? `编辑 ${app.name}` : "添加应用"]),
      field("名称", "f-name", app?.name || ""),
      field("URL", "f-url", app?.url || "https://"),
      field("关窗行为", "f-close", app?.closeAction || "background", [
        opt("background", "后台驻留"),
        opt("quit", "退出"),
      ]),
      field("启动前钩子 (分号分隔)", "f-pre", (app?.hooks.preLaunch || []).join("; ")),
      field("关闭后钩子 (分号分隔)", "f-post", (app?.hooks.postExit || []).join("; ")),
      field("注入 CSS", "f-css", app?.injections.css || "", [], true),
      field("注入 JS", "f-js", app?.injections.js || "", [], true),
      h("div", { class: "modal-actions" }, [
        h("button", { id: "modal-save" }, ["保存"]),
        h("button", { id: "modal-cancel" }, ["取消"]),
      ]),
    ]),
  );
  document.getElementById("modal-cancel")!.onclick = () => modal.classList.add("hidden");
  document.getElementById("modal-save")!.onclick = async () => {
    const payload: Partial<App> = {
      name: val("f-name"),
      url: val("f-url"),
      closeAction: val("f-close") as "background" | "quit",
      hooks: {
        preLaunch: val("f-pre").split(";").map((s) => s.trim()).filter(Boolean),
        postExit: val("f-post").split(";").map((s) => s.trim()).filter(Boolean),
      },
      injections: { css: val("f-css"), js: val("f-js"), timing: "document_idle" },
    };
    try {
      if (app) await api.updateApp(app.id, payload);
      else await api.createApp(payload);
      modal.classList.add("hidden");
      refreshAll();
    } catch (e) {
      alert(`保存失败: ${(e as Error).message}`);
    }
  };
}

function val(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}

function field(label: string, id: string, value: string, options: HTMLOptionElement[] = [], textarea = false): HTMLElement {
  const wrap = h("div", { class: "field" }, [h("label", { for: id }, [label])]);
  if (textarea) {
    const ta = el("textarea");
    ta.id = id;
    ta.value = value;
    ta.rows = 2;
    wrap.append(ta);
  } else if (options.length) {
    const sel = el("select");
    sel.id = id;
    sel.append(...options);
    sel.value = value;
    wrap.append(sel);
  } else {
    const input = el("input");
    input.id = id;
    input.value = value;
    wrap.append(input);
  }
  return wrap;
}

function opt(value: string, label: string): HTMLOptionElement {
  const o = el("option");
  o.value = value;
  o.textContent = label;
  return o;
}

// ---------- 启动 ----------

(async function bootstrap() {
  // 固定 127.0.0.1:3070，无鉴权，直接连接
  await initApi();
  render();
})();
