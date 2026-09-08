import { h, icon, iconButton, t } from "./ui.js";
import { getTheme, themeName } from "./appearance.js";

export const pages = {
  overview: { zh: "主页", en: "Overview", icon: "home", index: "01" },
  profiles: { zh: "配置", en: "Profiles", icon: "sliders", index: "02" },
  sessions: { zh: "会话", en: "Sessions", icon: "messages", index: "03" },
  settings: { zh: "设置", en: "Settings", icon: "settings", index: "04" },
};

export function shell(state, content) {
  const page = pages[state.page];
  return `<div class="app-shell">
    <button type="button" class="nav-scrim ${state.navOpen ? "visible" : ""}" data-action="close-nav" aria-label="${t("关闭导航", "Close navigation")}" tabindex="-1"></button>
    <aside id="sidebar" class="sidebar ${state.navOpen ? "open" : ""}" aria-label="${t("主导航", "Main navigation")}" ${matchMedia("(max-width: 760px)").matches && !state.navOpen ? "inert" : ""}>
      <a class="brand" href="#overview" aria-label="pi-switch · ${t("主页", "Overview")}">
        <span class="brand-mark">π</span><span><span class="brand-name">pi-switch</span><small>MODEL WORKSPACE</small></span>
      </a>
      <div class="nav-label">WORKSPACE</div>
      <nav class="nav-links">
        ${Object.entries(pages).map(([key, item]) => `<a class="nav-link ${state.page === key ? "active" : ""}" href="#${key}" ${state.page === key ? 'aria-current="page"' : ""}>
          ${icon(item.icon)}<span>${t(item.zh, item.en)}</span><span class="nav-index">${item.index}</span>
        </a>`).join("")}
      </nav>
      <div class="sidebar-bottom">
        <div class="local-card"><div class="local-card-heading">${icon("terminal")}Pi Agent<span class="status-dot"></span></div><code title="${h(state.snapshot.paths.piModels)}">${h(state.snapshot.paths.piModels.replace(/[/\\]models\.json$/, ""))}</code></div>
        <div class="sidebar-footer"><span class="version">v${h(state.snapshot.version)}</span><a class="icon-button" href="https://github.com/OldSuns/pi-switch" target="_blank" rel="noreferrer" aria-label="GitHub" title="GitHub">${icon("external")}</a></div>
      </div>
    </aside>
    <header class="topbar">
      <div class="breadcrumb">${iconButton("toggle-nav", "menu", t("打开导航", "Open navigation"), 'aria-controls="sidebar" aria-expanded="' + state.navOpen + '"', "mobile-menu")}${icon("terminal")}<span>workspace</span><span class="separator">/</span><strong>${page.en.toLowerCase()}</strong></div>
      <div class="topbar-actions"><span class="connection"><span class="status-dot ${state.connected ? "" : "off"}"></span>${state.connected ? t("本地服务已连接", "Connected locally") : t("本地连接已断开", "Disconnected")}</span><span class="topbar-divider"></span>${iconButton("reload", "refresh", t("重新读取配置 · R", "Reload configuration · R"))}${iconButton("help", "help", t("快捷键 · ?", "Keyboard shortcuts · ?"))}</div>
    </header>
    <main id="main" class="workspace" tabindex="-1" aria-busy="${state.busy}"><div class="page">
      ${state.snapshot.warning ? `<div class="banner" role="alert">${icon("warning")}<span>${h(state.snapshot.warning)}</span></div>` : ""}
      ${content}
    </div></main>
    <footer class="statusbar"><span class="statusbar-left">${state.busy ? '<span class="spinner"></span>' : icon("checkCircle")}<span>${state.busy ? t("正在处理…", "Working…") : state.lastSaved ? t("更改已保存", "Changes saved") : t("本地配置已加载", "Local configuration loaded")}</span><code title="${h(state.snapshot.paths.providers)}">${h(state.snapshot.paths.providers)}</code></span><span class="statusbar-right">${icon(getTheme() === "light" ? "sun" : "moon")}${themeName()}</span></footer>
  </div>`;
}

export function pageHeader(eyebrow, title, description, actions = "") {
  return `<div class="page-header"><div>${eyebrow ? '<div class="eyebrow">' + eyebrow + "</div>" : ""}<h1>${title}</h1><p>${description}</p></div>${actions ? `<div class="header-actions">${actions}</div>` : ""}</div>`;
}

export function contextHelp(items) {
  return '<div class="context-help">' + items.map(([key, label]) => "<span><kbd>" + h(key) + "</kbd>" + label + "</span>").join("") + "</div>";
}
