/**
 * WebLaunch 管理控制台（吃自己的狗粮：本页面即运行在 WebLaunch 上的第一个 Web 应用）。
 *
 * 功能：应用网格 / 增删改查 / 启动·激活·终止 / 平台状态 / 侧边栏 / 设置(i18n)。
 * 纯前端实现：不依赖任何框架，通过 api.ts 与后端 REST 服务通信。
 */

import { api, initApi, App, PlatformStatus } from "./api";

/** 应用挂载点（index.html 中的 #app 容器） */
const appEl = document.getElementById("app")!;

// ---------- i18n ----------

/** 界面语言类型 */
type Lang = "zh" | "en";

/**
 * 国际化文案表：按语言分组的 key-value 映射。
 * 所有界面文案必须通过 t() 取词，禁止硬编码。
 */
const I18N: Record<Lang, Record<string, string>> = {
  zh: {
    app: "应用",
    settings: "设置",
    console: "WebLaunch 控制台",
    refresh: "刷新",
    addApp: "+ 添加应用",
    running: "运行中",
    background: "后台",
    stopped: "已停止",
    system: "系统",
    engine: "引擎",
    close: "关窗",
    dwell: "驻留",
    quit: "退出",
    launch: "启动",
    activate: "激活",
    terminate: "终止",
    edit: "编辑",
    shortcut: "桌面快捷方式",
    delete: "删除",
    language: "语言",
    languageHint: "界面显示语言",
    settingsTitle: "设置",
    settingsDesc: "管理 WebDesk 控制台的偏好设置",
    version: "版本",
    port: "端口",
    memory: "内存",
    noApps: "（无应用。用 `webdesk addweb -n 名称 -url 地址` 添加）",
    platformStatus: "平台状态加载中…",
    connectFail: "连接失败",
    save: "保存",
    cancel: "取消",
    editApp: "编辑",
    addAppTitle: "添加应用",
    name: "名称",
    url: "URL",
    closeAction: "关窗行为",
    preHook: "启动前钩子 (分号分隔)",
    postHook: "关闭后钩子 (分号分隔)",
    injectCss: "注入 CSS",
    injectJs: "注入 JS",
    shortcutTitle: "创建桌面快捷方式",
    iconSource: "图标来源",
    defaultIcon: "默认图标",
    localIco: "选择本地 .ico 文件",
    autoIcon: "自动从网页获取图标",
    icoPath: ".ico 文件路径",
    autoHint: "将自动抓取该应用的网页图标",
    create: "创建",
    created: "✅ 已创建桌面快捷方式",
    createFail: "快捷方式创建失败",
    deleteConfirm: "确定删除该应用？",
    unshortcutConfirm: "确定移除桌面快捷方式？",
    launched: "已启动",
    opDone: "操作完成",
    opFail: "操作失败",
    saveFail: "保存失败",
    saved: "已保存",
    createFail2: "创建失败",
  },
  en: {
    app: "Apps",
    settings: "Settings",
    console: "WebLaunch Console",
    refresh: "Refresh",
    addApp: "+ Add App",
    running: "Running",
    background: "Background",
    stopped: "Stopped",
    system: "System",
    engine: "Engine",
    close: "Close",
    dwell: "Dwell",
    quit: "Quit",
    launch: "Launch",
    activate: "Activate",
    terminate: "Terminate",
    edit: "Edit",
    shortcut: "Shortcut",
    delete: "Delete",
    language: "Language",
    languageHint: "UI display language",
    settingsTitle: "Settings",
    settingsDesc: "Manage WebLaunch console preferences",
    version: "Version",
    port: "Port",
    memory: "Memory",
    noApps: "(No apps. Add with `webdesk addweb -n name -url url`)",
    platformStatus: "Loading platform status…",
    connectFail: "Connection failed",
    save: "Save",
    cancel: "Cancel",
    editApp: "Edit",
    addAppTitle: "Add App",
    name: "Name",
    url: "URL",
    closeAction: "Close action",
    preHook: "Pre-launch hook (semicolon separated)",
    postHook: "Post-exit hook (semicolon separated)",
    injectCss: "Inject CSS",
    injectJs: "Inject JS",
    shortcutTitle: "Create Desktop Shortcut",
    iconSource: "Icon source",
    defaultIcon: "Default icon",
    localIco: "Choose local .ico file",
    autoIcon: "Auto fetch from webpage",
    icoPath: ".ico file path",
    autoHint: "Will auto-fetch this app's webpage icon",
    create: "Create",
    created: "✅ Desktop shortcut created",
    createFail: "Shortcut creation failed",
    deleteConfirm: "Delete this app?",
    unshortcutConfirm: "Remove desktop shortcut?",
    launched: "Launched",
    opDone: "Done",
    opFail: "Operation failed",
    saveFail: "Save failed",
    saved: "Saved",
    createFail2: "Create failed",
  },
};

/** 当前语言（从 localStorage 恢复，默认中文） */
let lang: Lang = (localStorage.getItem("webdesk-lang") as Lang) || "zh";

/**
 * 取当前语言的文案；key 不存在时原样返回 key（便于排查缺失词条）。
 */
function t(key: string): string {
  return I18N[lang][key] || key;
}

/**
 * 切换界面语言：更新内存状态、持久化到 localStorage 并整体重渲染。
 */
function setLang(l: Lang) {
  lang = l;
  localStorage.setItem("webdesk-lang", l);
  render();
}

// ---------- DOM helpers ----------

/**
 * 创建带属性与子节点的元素。
 * 字符串子节点自动转为文本节点，避免 XSS 风险（不解析 HTML）。
 */
function h(tag: string, attrs: Record<string, string> = {}, children: (string | HTMLElement)[] = []): HTMLElement {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  for (const c of children) el.append(c instanceof HTMLElement ? c : document.createTextNode(c));
  return el;
}

/**
 * 创建指定标签的强类型元素（返回具体元素类型而非 HTMLElement）。
 */
function el<K extends keyof HTMLElementTagNameMap>(tag: K): HTMLElementTagNameMap[K] {
  return document.createElement(tag);
}

// ---------- 消息提醒（toast，自动消失） ----------

/**
 * 显示一条自动消失的 toast 消息。
 *
 * 首次调用时惰性创建全局容器 #toast-container；
 * 通过 .show 类触发 CSS 过渡，消失动画结束后移除节点。
 *
 * @param message 消息文本
 * @param type 消息类型（error 附加错误样式）
 * @param duration 显示时长（毫秒）
 */
function toast(message: string, type: "info" | "error" = "info", duration = 2500): void {
  let container = document.getElementById("toast-container");
  if (!container) {
    container = document.createElement("div");
    container.id = "toast-container";
    document.body.appendChild(container);
  }
  const t = document.createElement("div");
  t.className = `toast ${type === "error" ? "error" : ""}`;
  t.textContent = message;
  container.appendChild(t);
  // 下一帧再添加 .show，确保入场过渡动画生效
  requestAnimationFrame(() => t.classList.add("show"));
  setTimeout(() => {
    t.classList.remove("show");
    // 等待退场过渡（300ms）结束后再移除 DOM 节点
    setTimeout(() => t.remove(), 300);
  }, duration);
}

// ---------- 视图状态 ----------

/** 当前激活的视图：应用列表或设置 */
let currentView: "apps" | "settings" = "apps";

/**
 * 整体渲染入口：重建布局骨架（侧边栏 + 主区域 + 模态框容器），
 * 绑定导航事件，并按当前视图分发渲染。
 */
function render(): void {
  appEl.innerHTML = "";
  appEl.append(
    h("div", { class: "layout" }, [
      h("aside", { class: "sidebar" }, [
        h("div", { class: "sidebar-brand" }, [t("console")]),
        h("nav", { class: "sidebar-nav" }, [
          h("button", { id: "nav-apps", class: `nav-item ${currentView === "apps" ? "active" : ""}` }, [t("app")]),
          h("button", { id: "nav-settings", class: `nav-item ${currentView === "settings" ? "active" : ""}` }, [t("settings")]),
        ]),
      ]),
      h("main", { class: "main" }, [
        h("div", { id: "view-apps", class: `view ${currentView === "apps" ? "" : "hidden"}` }),
        h("div", { id: "view-settings", class: `view ${currentView === "settings" ? "" : "hidden"}` }),
      ]),
    ]),
    h("div", { id: "modal", class: "modal hidden" }),
  );

  // 侧边栏导航：切换视图后整体重渲染
  document.getElementById("nav-apps")!.onclick = () => { currentView = "apps"; render(); };
  document.getElementById("nav-settings")!.onclick = () => { currentView = "settings"; render(); };

  if (currentView === "apps") renderApps();
  else renderSettings();
}

// ---------- 应用视图 ----------

/**
 * 渲染应用列表视图：工具栏（刷新/添加）、状态栏与卡片网格容器，
 * 并触发一次数据刷新。
 */
function renderApps(): void {
  const view = document.getElementById("view-apps")!;
  view.innerHTML = "";
  view.append(
    h("div", { class: "toolbar" }, [
      h("h1", {}, [t("app")]),
      h("button", { id: "btn-refresh" }, [t("refresh")]),
      h("button", { id: "btn-new" }, [t("addApp")]),
    ]),
    h("div", { id: "status-bar", class: "status-bar" }, [t("platformStatus")]),
    h("div", { id: "grid", class: "grid" }),
  );
  document.getElementById("btn-refresh")!.onclick = refreshAll;
  document.getElementById("btn-new")!.onclick = () => openModal(null);
  refreshAll();
}

/**
 * 刷新全部数据：并行拉取平台状态与应用列表，
 * 更新状态栏摘要（版本/运行数/后台数/端口/内存），失败时展示连接错误。
 */
async function refreshAll(): Promise<void> {
  const statusEl = document.getElementById("status-bar")!;
  const grid = document.getElementById("grid")!;
  grid.innerHTML = "";
  try {
    const [status, apps] = await Promise.all([api.status(), api.listApps()]);
    statusEl.textContent = `${t("version")} ${status.version} · ${t("running")} ${status.running.length} · ${t("background")} ${status.background.length} · ${t("port")} ${status.port} · ${t("memory")} ${(status.memory_kb / 1024).toFixed(1)} MB`;
    renderGrid(apps, status);
  } catch (e) {
    statusEl.textContent = `${t("connectFail")}: ${(e as Error).message}`;
  }
}

/**
 * 渲染应用卡片网格。
 *
 * 每张卡片展示图标、名称、运行状态徽标、URL 与引擎/关窗行为，
 * 操作按钮通过 data-act/data-id 标记动作，统一由 handleAction 分发。
 * 系统内置应用不显示删除按钮。
 */
function renderGrid(apps: App[], status: PlatformStatus): void {
  const grid = document.getElementById("grid")!;
  for (const app of apps) {
    const running = status.running.includes(app.id);
    const background = status.background.includes(app.id);
    const stateLabel = running ? t("running") : background ? t("background") : t("stopped");
    const favicon = faviconUrl(app.url);

    const card = h("div", { class: `card ${app.is_system ? "system" : ""}` }, [
      h("div", { class: "card-header" }, [
        h("img", { class: "app-icon", src: favicon, alt: "", loading: "lazy" }),
        h("strong", {}, [app.name]),
        h("span", { class: `badge ${stateLabel}` }, [stateLabel]),
        app.is_system ? h("span", { class: "badge system" }, [t("system")]) : h("span", {}),
      ]),
      h("div", { class: "card-body" }, [
        h("div", { class: "app-url" }, [`${app.url}`]),
        h("div", { class: "muted" }, [`${t("engine")} ${app.runtime_profile} · ${t("close")} ${app.close_action === "background" ? t("dwell") : t("quit")}`]),
      ]),
      h("div", { class: "card-actions" }, [
        // 运行中显示"激活"，否则显示"启动"
        h("button", { "data-act": "launch", "data-id": app.id }, running ? [t("activate")] : [t("launch")]),
        // 未运行且未驻留时禁用"终止"
        h("button", { "data-act": "terminate", "data-id": app.id, disabled: !running && !background ? "true" : "" }, [t("terminate")]),
        h("button", { "data-act": "edit", "data-id": app.id }, [t("edit")]),
        h("button", { "data-act": "shortcut", "data-id": app.id, title: t("shortcut") }, [t("shortcut")]),
        app.is_system ? h("span", {}) : h("button", { "data-act": "delete", "data-id": app.id, class: "danger" }, [t("delete")]),
      ]),
    ]);
    grid.append(card);
  }
  // 事件委托：为所有动作按钮统一绑定点击处理
  grid.querySelectorAll<HTMLButtonElement>("[data-act]").forEach((btn) => {
    btn.onclick = () => handleAction(btn.dataset.act!, btn.dataset.id!);
  });
}

/**
 * 从应用 URL 提取 favicon 地址（Google favicon 服务，无后端依赖）。
 * URL 解析失败时返回内置占位图标（灰色 "W" SVG）。
 */
function faviconUrl(url: string): string {
  try {
    const host = new URL(url).hostname;
    return `https://www.google.com/s2/favicons?domain=${host}&sz=32`;
  } catch {
    return "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><rect width='32' height='32' fill='%23888' rx='6'/><text x='16' y='22' text-anchor='middle' font-size='16' fill='white'>W</text></svg>";
  }
}

/**
 * 卡片动作统一分发：按 data-act 执行对应 API 调用。
 *
 * - launch：启动（或激活）应用；
 * - terminate：终止应用；
 * - edit：拉取详情后打开编辑对话框；
 * - delete / unshortcut：先 confirm 确认再执行；
 * - shortcut：打开快捷方式创建对话框（不触发列表刷新）。
 *
 * 操作完成后刷新列表；任何失败均以 error toast 提示。
 */
async function handleAction(act: string, id: string): Promise<void> {
  try {
    if (act === "launch") {
      const r = await api.launchApp(id);
      toast(r.status === "running" ? t("launched") : r.windowId ? `${t("launched")} (${r.windowId})` : t("opDone"));
    } else if (act === "terminate") {
      await api.terminateApp(id);
    } else if (act === "edit") {
      const app = await api.getApp(id);
      openModal(app);
    } else if (act === "delete") {
      if (confirm(t("deleteConfirm"))) await api.deleteApp(id);
    } else if (act === "shortcut") {
      const app = await api.getApp(id);
      shortcutDialog(app);
      return;
    } else if (act === "unshortcut") {
      if (confirm(t("unshortcutConfirm"))) await api.removeShortcut(id);
    }
    refreshAll();
  } catch (e) {
    toast(`${t("opFail")}: ${(e as Error).message}`, "error");
  }
}

// ---------- 设置视图 ----------

/**
 * 渲染设置视图：目前仅提供界面语言选择，
 * 切换后立即持久化并重渲染整个界面。
 */
function renderSettings(): void {
  const view = document.getElementById("view-settings")!;
  view.innerHTML = "";
  view.append(
    h("div", { class: "toolbar" }, [h("h1", {}, [t("settingsTitle")])]),
    h("div", { class: "settings-card" }, [
      h("h2", {}, [t("settingsTitle")]),
      h("p", { class: "muted" }, [t("settingsDesc")]),
      h("div", { class: "field" }, [
        h("label", { for: "set-lang" }, [t("language")]),
        h("select", { id: "set-lang" }, [
          opt("zh", "中文"),
          opt("en", "English"),
        ]),
        h("div", { class: "muted" }, [t("languageHint")]),
      ]),
    ]),
  );
  const sel = document.getElementById("set-lang") as HTMLSelectElement;
  sel.value = lang;
  sel.onchange = () => setLang(sel.value as Lang);
}

// ---------- 快捷方式对话框 ----------

/**
 * 打开"创建桌面快捷方式"对话框。
 *
 * 图标来源三选一：
 * - default：使用默认图标；
 * - local：手动填写本地 .ico 文件路径；
 * - auto：自动抓取网页图标（优先复用已绑定的 app.icon）。
 *
 * 选择来源时联动显示/隐藏对应输入区。
 */
function shortcutDialog(app: App): void {
  const modal = document.getElementById("modal")!;
  modal.classList.remove("hidden");
  modal.innerHTML = "";
  modal.append(
    h("div", { class: "modal-content" }, [
      h("h2", {}, [t("shortcutTitle")]),
      h("div", { class: "field" }, [
        h("label", {}, [t("iconSource")]),
        h("select", { id: "sc-icon-type" }, [
          opt("default", t("defaultIcon")),
          opt("local", t("localIco")),
          opt("auto", t("autoIcon")),
        ]),
      ]),
      h("div", { id: "sc-local-wrap", class: "field hidden" }, [
        h("label", { for: "sc-local" }, [t("icoPath")]),
        h("input", { id: "sc-local", placeholder: "C:\\path\\to\\icon.ico" }),
      ]),
      h("div", { class: "field", id: "sc-auto-hint" }, [
        h("label", {}, [t("autoHint")]),
        h("div", { class: "muted" }, [`${app.url}`]),
      ]),
      h("div", { class: "modal-actions" }, [
        h("button", { id: "sc-ok" }, [t("create")]),
        h("button", { id: "sc-cancel" }, [t("cancel")]),
      ]),
    ]),
  );

  const typeSel = document.getElementById("sc-icon-type") as HTMLSelectElement;
  const localWrap = document.getElementById("sc-local-wrap")!;
  const autoHint = document.getElementById("sc-auto-hint")!;
  // 图标来源切换：仅显示与当前选择相关的输入区
  typeSel.onchange = () => {
    localWrap.classList.toggle("hidden", typeSel.value !== "local");
    autoHint.classList.toggle("hidden", typeSel.value !== "auto");
  };
  document.getElementById("sc-cancel")!.onclick = () => modal.classList.add("hidden");
  document.getElementById("sc-ok")!.onclick = async () => {
    let icon: string | undefined;
    if (typeSel.value === "local") {
      // 本地图标：取输入框内容，去空白后为空则视为未指定
      icon = (document.getElementById("sc-local") as HTMLInputElement).value.trim() || undefined;
    } else if (typeSel.value === "auto") {
      // 优先复用已绑定的 app.icon；未绑定才传 url 触发抓取
      icon = app.icon || app.url;
    }
    try {
      const r = await api.createShortcut(app.id, icon);
      modal.classList.add("hidden");
      toast(r.created ? t("created") : t("createFail"), r.created ? "info" : "error");
    } catch (e) {
      toast(`${t("createFail2")}: ${(e as Error).message}`, "error");
    }
  };
}

// ---------- 应用编辑对话框 ----------

/**
 * 打开应用编辑/新建对话框。
 *
 * @param app 待编辑的应用；传 null 表示新建。
 * 保存时收集表单值组装 Partial<App> 载荷：
 * 钩子按行拆分（非空才提交），注入时机固定为 document_idle。
 */
function openModal(app: App | null): void {
  const modal = document.getElementById("modal")!;
  modal.classList.remove("hidden");
  modal.innerHTML = "";
  modal.append(
    h("div", { class: "modal-content" }, [
      h("h2", {}, [app ? `${t("editApp")} ${app.name}` : t("addAppTitle")]),
      field(t("name"), "f-name", app?.name || ""),
      field(t("url"), "f-url", app?.url || "https://"),
      field(t("closeAction"), "f-close", app?.close_action || "background", [
        opt("background", t("dwell")),
        opt("quit", t("quit")),
      ]),
      field(t("preHook"), "f-pre", (app?.hooks.pre_launch || []).join("\n"), [], true),
      field(t("postHook"), "f-post", (app?.hooks.post_exit || []).join("\n"), [], true),
      field(t("injectCss"), "f-css", app?.injections.css || "", [], true),
      field(t("injectJs"), "f-js", app?.injections.js || "", [], true),
      h("div", { class: "modal-actions" }, [
        h("button", { id: "modal-save" }, [t("save")]),
        h("button", { id: "modal-cancel" }, [t("cancel")]),
      ]),
    ]),
  );
  document.getElementById("modal-cancel")!.onclick = () => modal.classList.add("hidden");
  document.getElementById("modal-save")!.onclick = async () => {
    const payload: Partial<App> = {
      name: val("f-name"),
      url: val("f-url"),
      close_action: val("f-close") as "background" | "quit",
      hooks: {
        // 钩子输入框按行拆分：整块非空才作为单条命令提交
        pre_launch: (val("f-pre") || "").trim() ? [val("f-pre")] : [],
        post_exit: (val("f-post") || "").trim() ? [val("f-post")] : [],
      },
      injections: { css: val("f-css"), js: val("f-js"), timing: "document_idle" },
    };
    try {
      if (app) await api.updateApp(app.id, payload);
      else await api.createApp(payload);
      modal.classList.add("hidden");
      refreshAll();
      toast(t("saved"));
    } catch (e) {
      toast(`${t("saveFail")}: ${(e as Error).message}`, "error");
    }
  };
}

/**
 * 读取表单输入框的当前值。
 */
function val(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}

/**
 * 构建一个表单字段（label + 控件）。
 *
 * @param label 字段标签
 * @param id 控件 id（同时用于 label 的 for 关联）
 * @param value 初始值
 * @param options 非空时渲染为下拉框
 * @param textarea 为 true 时渲染为多行文本域
 */
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

/**
 * 构建一个下拉选项。
 */
function opt(value: string, label: string): HTMLOptionElement {
  const o = el("option");
  o.value = value;
  o.textContent = label;
  return o;
}

// ---------- 启动 ----------

/**
 * 应用入口：初始化 API 客户端后渲染首屏。
 */
(async function bootstrap() {
  await initApi();
  render();
})();
