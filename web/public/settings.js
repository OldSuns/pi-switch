import { h, t, icon, toggle, iconButton } from "./ui.js";
import { pageHeader } from "./shell.js";
import { getTheme } from "./appearance.js";

function setting(title, description, control) {
  return `<div class="setting-row"><div><h3>${title}</h3><p>${description}</p></div>${control}</div>`;
}

export function settings(state) {
  const snapshot = state.snapshot;
  const paths = [
    [t("Provider 本地库", "Provider library"), "providers"],
    ["Pi models.json", "piModels"],
    ["Pi settings.json", "piSettings"],
    [t("pi-switch 设置", "pi-switch settings"), "appSettings"],
    [t("会话目录", "Session directory"), "sessions"],
    [t("备份目录", "Backup directory"), "backups"],
  ];
  return pageHeader("", t("设置", "Settings"), t("管理界面偏好、配置备份与程序更新。", "Manage interface preferences, configuration backups, and updates."))
    + `<div class="settings-layout"><div>
      <section class="panel settings-group"><div class="panel-header"><h2>${icon("sliders")}${t("偏好设置", "Preferences")}</h2></div>
        ${setting(t("界面语言", "Language"), t("Web 和 TUI 共享这项设置。", "Shared between the Web interface and TUI."), `<select data-action="language" aria-label="${t("界面语言", "Interface language")}"><option value="zh-CN" ${snapshot.language === "zh-CN" ? "selected" : ""}>简体中文</option><option value="en" ${snapshot.language === "en" ? "selected" : ""}>English</option></select>`)}
        ${setting(t("获取模型元数据", "Fetch model metadata"), t("导入时从 models.dev 获取上下文、价格与模型能力信息。", "Get context limits, pricing, and capabilities from models.dev during import."), toggle("metadata", snapshot.fetchModelMetadata, t("获取模型元数据", "Fetch model metadata")))}
        ${!snapshot.fetchModelMetadata ? setting(t("默认模型参数", "Default model parameters"), t("未使用在线元数据时，导入模型采用这些缺省值。", "Defaults used when importing without online metadata."), `<button class="btn" data-action="model-defaults">${icon("edit")}${t("编辑", "Edit")}</button>`) : ""}
        ${setting(t("TUI 启动时检查更新", "Check for updates on TUI startup"), t("控制终端界面的自动检查。Web 可在下方手动检查。", "Controls automatic checks in the terminal interface. Check manually below on the Web."), toggle("auto-updates", snapshot.checkUpdates, t("TUI 启动时检查更新", "Check for updates on TUI startup")))}
      </section>
      <section class="panel settings-group"><div class="panel-header"><h2>${icon("shield")}${t("配置维护", "Configuration tools")}</h2></div>
        ${setting(t("重新读取配置", "Reload configuration"), t("读取磁盘上的最新内容，包括在 TUI 或 Pi 中的修改。", "Read the latest changes made in Pi, the TUI, or on disk."), `<button class="btn" data-action="reload">${icon("refresh")}${t("重载", "Reload")}</button>`)}
        ${setting(t("验证配置", "Validate configuration"), t("检查配置文件、默认模型及写入锁状态。", "Check configuration files, the default model, and write locks."), `<button class="btn" data-action="doctor">${icon("shield")}${t("检查", "Validate")}</button>`)}
        ${setting(t("浏览备份", "Browse backups"), t("每次写入前自动备份，最多保留最近 10 份。", "Automatically saved before writes. The last 10 backups are retained."), `<button class="btn" data-action="backups">${icon("history")}${t("浏览", "Browse")}</button>`)}
        ${setting(t("从 OpenCode 导入", "Import from OpenCode"), t("选择已有 provider 导入本地库并同步到 Pi。", "Import selected providers into your library and sync them to Pi."), `<button class="btn" data-action="opencode">${icon("download")}${t("导入", "Import")}</button>`)}
      </section>
      <section class="panel settings-group"><div class="panel-header"><h2>${icon("terminal")}${t("关于 pi-switch", "About pi-switch")}</h2><span class="version">v${h(snapshot.version)}</span></div>
        ${setting("pi-switch", t("为 Pi 打造的本地 provider 与模型管理工具。", "A local provider and model manager for Pi."), `<button class="btn" data-action="check-updates">${icon("refresh")}${t("检查更新", "Check updates")}</button>`)}
      </section>
    </div><div>
      <section class="panel"><div class="panel-header"><h2>${icon("eye")}${t("外观", "Appearance")}</h2></div><div class="appearance-settings">
        <fieldset class="theme-control"><legend class="sr-only">${t("界面主题", "Interface theme")}</legend>
          <label class="theme-choice ${getTheme() === "dark" ? "selected" : ""}"><input type="radio" name="web-theme" value="dark" data-action="theme" ${getTheme() === "dark" ? "checked" : ""}>${icon("moon")}<span><strong>${t("暗色", "Dark")}</strong><small>Catppuccin Mocha</small></span></label>
          <label class="theme-choice ${getTheme() === "light" ? "selected" : ""}"><input type="radio" name="web-theme" value="light" data-action="theme" ${getTheme() === "light" ? "checked" : ""}>${icon("sun")}<span><strong>${t("亮色", "Light")}</strong><small>Catppuccin Latte</small></span></label>
        </fieldset>
      </div></section>
      <section class="panel section-gap"><div class="panel-header"><h2>${icon("folder")}${t("配置路径", "Configuration paths")}</h2></div><div class="path-list">
        ${paths.map(([label, key]) => `<div class="path-row"><label>${label}</label><div class="path-value"><code>${h(snapshot.paths[key])}</code>${iconButton("copy-path", "copy", t("复制路径", "Copy path") + " · " + label, 'data-path="' + h(snapshot.paths[key]) + '"')}</div></div>`).join("")}
      </div></section>
    </div></div>`;
}
