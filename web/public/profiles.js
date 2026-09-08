import { h, t, icon, initials, tint, tokens, price, modelName, capabilities, emptyState, searchInput, toggle, iconButton } from "./ui.js";
import { pageHeader, contextHelp } from "./shell.js";

export function selectedProvider(state) {
  return state.snapshot.providers.find((provider) => provider.id === state.providerId);
}

export function visibleProviders(state) {
  return state.snapshot.providers.filter((provider) =>
    provider.id.toLowerCase().includes(state.providerQuery.toLowerCase())
    && (state.providerFilter === "all" || provider.inPi === (state.providerFilter === "synced")));
}

export function visibleModels(state) {
  const query = state.modelQuery.toLowerCase();
  return (selectedProvider(state)?.models ?? []).filter((model) =>
    model.id.toLowerCase().includes(query) || model.name?.toLowerCase().includes(query));
}

function providerList(state) {
  const providers = visibleProviders(state);
  return `<section class="panel provider-panel" aria-label="${t("Provider 列表", "Provider list")}">
    <div class="panel-header"><h2>PROVIDERS<span class="count">${state.snapshot.providers.length}</span></h2>${iconButton("new-provider", "plus", t("新建 Provider", "New provider"))}</div>
    <div class="provider-filters">${searchInput("provider-search", state.providerQuery, t("搜索 provider…", "Search providers…"))}
      <div class="segments" aria-label="${t("同步状态筛选", "Filter by sync status")}">
        ${[["all", t("全部", "All")], ["synced", t("已同步", "Synced")], ["local", t("仅本地", "Local")]].map(([value, label]) => `<button data-action="provider-filter" data-value="${value}" aria-pressed="${state.providerFilter === value}">${label}</button>`).join("")}
      </div>
    </div>
    <div class="provider-list" aria-label="${t("选择 Provider", "Select provider")}">
      ${providers.length ? providers.map((provider) => `<button class="provider-option" data-action="select-provider" data-provider="${h(provider.id)}" aria-current="${provider.id === state.providerId}">
        <span class="avatar ${tint(provider.id)}">${initials(provider.id)}</span>
        <span class="provider-option-content"><span class="item-title">${h(provider.id)}</span><span class="item-subtitle"><span class="status-dot ${provider.inPi ? "" : "off"}"></span>${provider.inPi ? t("已同步到 Pi", "Synced to Pi") : t("仅保存在本地", "Local only")}</span></span>
        ${provider.id === state.snapshot.defaultProvider ? icon("star") : ""}<span class="provider-count">${provider.models.length}</span>
      </button>`).join("") : emptyState(t("没有匹配的 Provider", "No matching providers"), t("试试其他关键词或同步状态。", "Try another search or sync filter."), "search")}
    </div><div class="provider-list-foot">${icon("box")}${t("本地库始终保留完整配置", "Your full library stays local")}</div>
  </section>`;
}

function providerInfo(provider) {
  const apiKey = provider.apiKey
    ? (/^[$!]/.test(provider.apiKey) ? provider.apiKey : "••••••••••••")
    : "auth.json / CLI";
  return `<section class="panel">
    <div class="provider-info"><div class="provider-title-row">
      <span class="avatar ${tint(provider.id)}">${initials(provider.id)}</span>
      <div><h2>${h(provider.id)}</h2><div class="provider-title-label">${h(provider.api || t("按模型设置 API", "API set per model"))}</div></div>
      <div class="inline-actions"><button class="btn small" data-action="edit-provider">${icon("edit")}${t("编辑", "Edit")}</button>${iconButton("duplicate-provider", "copy", t("复制 Provider", "Duplicate provider"))}${iconButton("remove-provider", "trash", t("删除 Provider", "Delete provider"), "", "danger")}</div>
    </div><div class="provider-meta">
      <div><div class="meta-label">BASE URL</div><div class="meta-value">${icon("globe")}${h(provider.baseUrl || "—")}</div></div>
      <div><div class="meta-label">API KEY</div><div class="meta-value">${icon("key")}${h(apiKey)}</div></div>
    </div></div>
    <div class="provider-sync"><div><h3>${t("同步到 Pi", "Sync to Pi")}</h3><p>${provider.inPi ? t("此 provider 的模型可在 Pi 中使用", "This provider's models are available in Pi") : t("配置保留在本地，暂不写入 Pi", "Kept in your local library, outside Pi")}</p></div>${toggle("sync-provider", provider.inPi, t("同步到 Pi", "Sync to Pi"), 'data-provider="' + h(provider.id) + '"')}</div>
  </section>`;
}

function modelTable(state, provider) {
  const models = visibleModels(state);
  return `<section class="panel models-panel" aria-label="${t("模型列表", "Models")}">
    <div class="panel-header"><h2>${icon("cpu")}${t("模型", "Models")}<span class="count">${provider.models.length}</span></h2><div class="inline-actions">
      <button class="btn small" data-action="import-models">${icon("download")}${t("在线导入", "Import models")}</button><button class="btn small" data-action="new-model">${icon("plus")}${t("新建模型", "New model")}</button>
    </div></div>
    <div class="model-toolbar">${searchInput("model-search", state.modelQuery, t("搜索模型…", "Search models…"))}<span class="badge">${h(provider.api || "API")} </span></div>
    ${models.length ? `<div class="table-wrap"><table><thead><tr><th>${t("模型名称", "MODEL")}</th><th>${t("能力", "CAPABILITIES")}</th><th>${t("上下文 / 输出", "CONTEXT / OUTPUT")}</th><th class="optional-column">${t("输入 / 输出价格", "INPUT / OUTPUT")}</th><th><span class="sr-only">${t("操作", "Actions")}</span></th></tr></thead><tbody>
      ${models.map((model) => {
        const isDefault = state.snapshot.defaultProvider === provider.id && state.snapshot.defaultModel === model.id;
        return `<tr class="model-row ${state.modelId === model.id ? "selected" : ""}" tabindex="0" data-action="select-model" data-model="${h(model.id)}" aria-label="${h(modelName(model))}${isDefault ? " · " + t("默认模型", "Default model") : ""}">
          <td><div class="model-title">${h(modelName(model))}${isDefault ? icon("star") : ""}</div><div class="model-id">${h(model.id)}</div></td>
          <td><div class="capabilities">${capabilities(model)}</div></td>
          <td class="number-cell">${tokens(model.contextWindow)}<span class="subtle-divider">/</span><span class="muted">${tokens(model.maxTokens)}</span></td>
          <td class="number-cell optional-column">${price(model.inputCost)}<span class="subtle-divider">/</span>${price(model.outputCost)}</td>
          <td><div class="model-actions">${iconButton("default-model", "star", isDefault ? t("当前默认模型", "Current default model") : provider.inPi ? t("设为默认模型", "Set as default") + " · " + modelName(model) : t("先同步 Provider，再设为默认", "Sync this provider before setting a default"), 'data-model="' + h(model.id) + '" ' + (!provider.inPi || isDefault ? "disabled" : ""), isDefault ? "active" : "")}${iconButton("edit-model", "edit", t("编辑模型", "Edit model") + " · " + modelName(model), 'data-model="' + h(model.id) + '"')}</div></td>
        </tr>`;
      }).join("")}
    </tbody></table></div>` : emptyState(provider.models.length ? t("没有匹配的模型", "No matching models") : t("给这个 Provider 添加模型", "Add models to this provider"), provider.models.length ? t("试试其他模型名称或 ID。", "Try another model name or ID.") : t("从服务商在线获取模型，或手动填写模型配置。", "Fetch available models from the provider, or add one manually."), "cpu", provider.models.length ? "" : `<button class="btn primary" data-action="import-models">${icon("download")}${t("在线导入模型", "Import models")}</button>`)}
    <div class="models-foot"><span>${t("显示", "Showing")} ${models.length} / ${provider.models.length} ${t("个模型", "models")}</span><span>${t("价格单位：USD / 1M tokens", "Prices in USD / 1M tokens")}<span class="subtle-divider">·</span>${icon("star")} ${t("默认模型", "Default")}</span></div>
  </section>`;
}

export function profiles(state) {
  const provider = selectedProvider(state);
  const header = pageHeader("PROVIDERS & MODELS", t("模型配置", "Model configuration"), t("一处管理所有 provider，只将需要的模型同步到 Pi。", "All your providers in one place. Sync what you need to Pi."), `<button class="btn" data-action="opencode">${icon("download")}${t("从 OpenCode 导入", "Import OpenCode")}</button><button class="btn primary" data-action="new-provider">${icon("plus")}${t("新建 Provider", "New provider")}</button>`);
  if (!state.snapshot.providers.length) {
    return header + `<section class="panel">${emptyState(t("添加你的第一个 Provider", "Add your first provider"), t("支持 OpenAI、Anthropic、Google 兼容 API，也可以连接自己的模型网关。", "Connect OpenAI, Anthropic, Google compatible APIs, or your own model gateway."), "sliders", `<div class="inline-actions"><button class="btn primary" data-action="new-provider">${icon("plus")}${t("新建 Provider", "New provider")}</button><button class="btn" data-action="opencode">${icon("download")}${t("从 OpenCode 导入", "Import OpenCode")}</button></div>`)}</section>`;
  }
  return header + `<div class="profiles-layout">${providerList(state)}<div class="provider-detail">${provider ? providerInfo(provider) + modelTable(state, provider) : `<section class="panel">${emptyState(t("选择一个 Provider", "Select a provider"), t("在左侧选择 provider 来管理模型。", "Select a provider to manage its models."), "sliders")}</section>`}</div></div>`
    + contextHelp([["↑ ↓", t("选择", "Select")], ["n", t("新建", "New")], ["e", t("编辑", "Edit")], ["Space", t("同步 / 设为默认", "Sync / set default")], ["/", t("筛选", "Filter")], ["?", t("更多快捷键", "More shortcuts")]]);
}
