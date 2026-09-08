import { h, t, icon, initials, tint, number, tokens, modelName, emptyState, date } from "./ui.js";
import { pageHeader } from "./shell.js";

const RECENT_SESSION_LIMIT = 3;
const PROVIDER_OVERVIEW_LIMIT = 4;

function stat(label, value, name, note, color = "") {
  return `<article class="stat-card"><div class="stat-heading"><span>${label}</span>${icon(name)}</div><div class="stat-value ${color}">${value}</div><div class="stat-note">${note}</div></article>`;
}

function defaultSelector(snapshot) {
  const providers = snapshot.providers.filter((provider) => provider.inPi && provider.models.length);
  const validDefault = providers.some((provider) => provider.id === snapshot.defaultProvider && provider.models.some((model) => model.id === snapshot.defaultModel));
  return `<label class="default-selector" for="default-model-select"><span>${t("切换默认模型", "Switch default model")}</span>
    <select id="default-model-select" data-action="choose-default" ${providers.length ? "" : "disabled"}>
      <option value="" disabled ${validDefault ? "" : "selected"}>${t("选择已同步的模型…", "Choose a synced model…")}</option>
      ${providers.map((provider) => `<optgroup label="${h(provider.id)}">${provider.models.map((model) => `<option value="${h(JSON.stringify([provider.id, model.id]))}" ${snapshot.defaultProvider === provider.id && snapshot.defaultModel === model.id ? "selected" : ""}>${h(modelName(model))}</option>`).join("")}</optgroup>`).join("")}
    </select>
  </label>`;
}

function defaultCard(snapshot) {
  const provider = snapshot.providers.find((item) => item.id === snapshot.defaultProvider);
  const model = provider?.models.find((item) => item.id === snapshot.defaultModel);
  const available = Boolean(provider?.inPi && model);
  const invalid = !available && Boolean(snapshot.defaultProvider || snapshot.defaultModel);
  return `<section class="panel default-card">
    <div class="panel-header"><h2>${icon("star")}${t("当前默认模型", "Default model")}</h2>
      <span class="badge ${available ? "success" : invalid ? "warning" : ""}">${available ? icon("check") + t("已同步到 Pi", "Synced to Pi") : invalid ? t("需要检查", "Needs attention") : t("未设置", "Not set")}</span>
    </div>
    ${available ? `<div class="default-content"><div class="default-provider">${h(provider.id)}</div><div class="default-name">${h(modelName(model))}</div><div class="default-id">${h(model.id)}</div>
      <div class="model-facts"><span class="model-fact">${icon("cpu")}${tokens(model.contextWindow)} ${t("上下文", "context")}</span><span class="model-fact">${icon("bolt")}${tokens(model.maxTokens)} ${t("输出", "output")}</span>${model.reasoning ? `<span class="model-fact">${icon("brain")}${t("推理", "Reasoning")}</span>` : ""}${model.input.includes("image") ? `<span class="model-fact">${icon("image")}${t("视觉", "Vision")}</span>` : ""}</div>
    </div>` : `<div class="default-empty"><h3>${invalid ? t("默认模型当前不可用", "The default model is unavailable") : t("尚未选择默认模型", "No default model selected")}</h3><p>${invalid ? t("当前默认配置不在 Pi 的同步列表中，请重新选择或检查配置。", "The default is outside the synced configuration. Choose another model or validate your configuration.") : t("从下方选择模型，设置下一次 Pi 对话使用的默认值。", "Choose the model Pi should use for your next conversation.")}</p></div>`}
    <div class="default-bottom">${defaultSelector(snapshot)}<a class="text-button" href="#profiles${provider ? "?provider=" + encodeURIComponent(provider.id) : ""}">${t("管理模型", "Manage models")}${icon("arrow")}</a></div>
  </section>`;
}

function recentSessions(state) {
  const recent = [...state.sessions].sort((left, right) => new Date(right.modifiedAt) - new Date(left.modifiedAt)).slice(0, RECENT_SESSION_LIMIT);
  let content;
  if (state.sessionsError) {
    content = '<div class="panel-body"><div class="banner error" role="alert">' + h(state.sessionsError) + "</div></div>";
  } else if (!state.sessionsLoaded) {
    content = '<div class="loading-state" role="status"><span class="spinner"></span>' + t("正在读取会话…", "Loading sessions…") + "</div>";
  } else if (!recent.length) {
    content = emptyState(t("还没有会话记录", "No sessions yet"), t("使用 Pi 开始对话后，会话会出现在这里。", "Your conversations will appear here after using Pi."), "messages");
  } else {
    content = recent.map((session) => `<a class="recent-session" href="#sessions?session=${encodeURIComponent(session.id)}">
      <span class="recent-session-icon">${icon("messages")}</span><span class="recent-session-content"><span class="item-title">${h(session.title)}</span><span class="recent-session-meta"><span class="recent-workspace" title="${h(session.cwd)}">${icon("folder")}${h(session.cwd.split(/[/\\]/).filter(Boolean).at(-1) || session.cwd)}</span><span>${session.messageCount} ${t("条消息", "messages")}</span><span>${date(session.modifiedAt, true)}</span></span></span>${icon("chevron")}
    </a>`).join("");
  }
  return `<section class="panel recent-card"><div class="panel-header"><h2>${icon("messages")}${t("最近会话", "Recent sessions")}</h2><a class="text-button" href="#sessions">${t("查看全部", "View all")}${icon("arrow")}</a></div>${content}</section>`;
}

export function overview(state) {
  const snapshot = state.snapshot;
  const providers = snapshot.providers;
  const synced = providers.filter((provider) => provider.inPi);
  const modelCount = providers.reduce((sum, provider) => sum + provider.models.length, 0);
  const syncedCount = synced.reduce((sum, provider) => sum + provider.models.length, 0);
  const headerActions = `<button class="btn" data-action="doctor">${icon("shield")}${t("检查配置", "Validate")}</button><button class="btn primary" data-action="new-provider">${icon("plus")}${t("新建 Provider", "New provider")}</button>`;
  return pageHeader("", t("工作台", "Overview"), t("查看当前配置，切换默认模型，浏览最近的会话。", "Review your configuration, switch the default model, and browse recent sessions."), headerActions)
    + `<div class="stats-grid">
      ${stat("Providers", number(providers.length), "box", `${number(synced.length)} ${t("个已同步到 Pi", "synced to Pi")}`)}
      ${stat(t("本地模型", "Local models"), number(modelCount), "cpu", t("所有 provider 的模型", "Across all providers"), "mauve")}
      ${stat(t("Pi 可用模型", "Models in Pi"), number(syncedCount), "bolt", t("来自已同步的 provider", "From synced providers"), "green")}
      ${stat(t("本地会话", "Local sessions"), state.sessionsError ? "—" : state.sessionsLoaded ? number(state.sessions.length) : "…", "messages", t("按工作目录整理", "Organized by workspace"))}
    </div>
    <div class="overview-grid">
      ${defaultCard(snapshot)}
      ${recentSessions(state)}
      <section class="panel"><div class="panel-header"><h2>${t("Provider 概览", "Your providers")}<span class="count">${providers.length}</span></h2><a href="#profiles" class="text-button">${t("管理配置", "Manage")}${icon("arrow")}</a></div>
        ${providers.length ? providers.slice(0, PROVIDER_OVERVIEW_LIMIT).map((provider) => `<a class="provider-summary" href="#profiles?provider=${encodeURIComponent(provider.id)}">
          <span class="avatar ${tint(provider.id)}">${initials(provider.id)}</span><div><div class="item-title">${h(provider.id)}</div><div class="item-subtitle">${h(provider.baseUrl || provider.api)}</div></div><span class="provider-count">${provider.models.length} models</span><span class="badge ${provider.inPi ? "success" : ""}">${provider.inPi ? icon("check") + t("已同步", "Synced") : t("仅本地", "Local only")}</span>
        </a>`).join("") : emptyState(t("添加你的第一个 Provider", "Add your first provider"), t("手动添加，或从已有的 OpenCode 配置导入。", "Add a provider or import an existing OpenCode configuration."), "box", `<button class="btn" data-action="opencode">${icon("download")}${t("从 OpenCode 导入", "Import from OpenCode")}</button>`)}
      </section>
      <section class="panel"><div class="panel-header"><h2>${t("常用操作", "Quick actions")}</h2></div><div class="quick-actions">
        <button class="quick-action" data-action="opencode" aria-label="${t("从 OpenCode 导入", "Import from OpenCode")}">${icon("download")}<div><h3>${t("从 OpenCode 导入", "Import from OpenCode")}</h3><p>${t("选择已有配置并同步到 Pi", "Import selected providers and sync to Pi")}</p></div>${icon("chevron")}</button>
        <button class="quick-action" data-action="backups" aria-label="${t("浏览配置备份", "Browse backups")}">${icon("history")}<div><h3>${t("浏览配置备份", "Browse backups")}</h3><p>${t("查看和恢复之前的配置", "View and restore saved configurations")}</p></div>${icon("chevron")}</button>
        <button class="quick-action" data-action="reload" aria-label="${t("重新读取配置", "Reload configuration")}">${icon("refresh")}<div><h3>${t("重新读取配置", "Reload configuration")}</h3><p>${t("获取在 Pi 或 TUI 中的最新修改", "Read the latest changes made in Pi or the TUI")}</p></div>${icon("chevron")}</button>
      </div></section>
    </div>`;
}
