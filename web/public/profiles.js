import { h, t, icon, initials, tint, tokens, price, modelName, capabilities, emptyState, listSearch, iconButton } from "./ui.js";
import { pageHeader, contextHelp } from "./shell.js";

const API_KEY_EDGE_LENGTH = 4;

function sortControl(scope, ordering, providerId = "") {
  const id = scope === "providers" ? "provider-sort" : "model-sort";
  const options = [
    ["custom", t("自定义顺序", "Custom order")],
    ["name-asc", t("名称 · A → Z", "Name · A → Z")],
    ["name-desc", t("名称 · Z → A", "Name · Z → A")],
    ["added-asc", t("添加时间升序", "Added ↑")],
    ["added-desc", t("添加时间降序", "Added ↓")],
  ];
  return `<label class="profile-sort" for="${id}"><span ${scope === "providers" ? 'class="sr-only"' : ""}>${t("排序", "Sort")}</span><select id="${id}" data-action="profile-sort" data-scope="${scope}" data-provider="${h(providerId)}">${options.map(([value, label]) => `<option value="${value}" ${ordering.sort === value ? "selected" : ""}>${label}</option>`).join("")}</select></label>`;
}

function orderHandle({ scope, item, providerId = "" }) {
  const attributes = `data-scope="${scope}" data-item="${h(item.id)}" data-provider="${h(providerId)}"`;
  const name = modelName(item);
  return `<button type="button" class="icon-button order-handle" draggable="true" data-order-handle ${attributes} aria-label="${h(t("拖动排序；Alt + ↑ ↓ 调整顺序", "Drag to reorder; Alt + arrows to move") + " · " + name)}" title="${t("拖动排序 · Alt + ↑ ↓", "Drag to reorder · Alt + ↑ ↓")}">${icon("grip")}</button>`;
}

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

function providerSyncButton(provider) {
  const label = t("同步到 Pi", "Sync to Pi") + " · " + provider.id;
  const hint = provider.inPi
    ? t("已同步到 Pi，点击取消同步", "Synced to Pi; click to remove")
    : t("仅保存在本地，点击同步到 Pi", "Local only; click to sync to Pi");
  return `<button type="button" class="icon-button provider-sync-button" data-action="sync-provider" data-provider="${h(provider.id)}" aria-pressed="${provider.inPi}" aria-label="${h(label)}" title="${h(hint)}">${icon(provider.inPi ? "check" : "unlink")}</button>`;
}

function providerList(state) {
  const providers = visibleProviders(state);
  const ordering = state.snapshot.ordering.providers;
  const custom = ordering.sort === "custom";
  const search = listSearch({ scope: "provider", query: state.providerQuery, open: state.providerSearchOpen, label: t("搜索 Provider", "Search providers") });
  return `<section class="panel provider-panel" aria-label="${t("Provider 列表", "Provider list")}">
    <div class="panel-header"><h2>PROVIDERS<span class="count">${state.snapshot.providers.length}</span></h2><div class="inline-actions">${search.button}${iconButton("new-provider", "plus", t("新建 Provider", "New provider"))}</div></div>
    ${search.field}
    <div class="provider-filters profile-filter-row">
      <label class="profile-filter" for="provider-filter"><span class="sr-only">${t("同步状态筛选", "Filter by sync status")}</span><select id="provider-filter" data-action="provider-filter">
        ${[["all", t("全部", "All")], ["synced", t("已同步", "Synced")], ["local", t("未同步", "Not synced")]].map(([value, label]) => `<option value="${value}" ${state.providerFilter === value ? "selected" : ""}>${label}</option>`).join("")}
      </select></label>
      ${sortControl("providers", ordering)}
    </div>
    <div class="provider-list" data-session-scroll="providers" data-order-list data-order-scope="providers" data-order-provider="" aria-label="${t("选择 Provider", "Select provider")}">
      ${providers.length ? providers.map((provider) => `<div class="provider-row" data-order-item data-order-scope="providers" data-order-id="${h(provider.id)}" data-order-provider="">${providerSyncButton(provider)}<button class="provider-option" data-action="select-provider" data-provider="${h(provider.id)}" aria-current="${provider.id === state.providerId}">
        <span class="item-title">${h(provider.id)}</span>
        ${provider.id === state.snapshot.defaultProvider ? icon("star") : ""}<span class="provider-count">${provider.models.length}</span>
      </button>${custom ? orderHandle({ scope: "providers", item: provider }) : ""}</div>`).join("") : emptyState(t("没有匹配的 Provider", "No matching providers"), t("试试其他关键词或同步状态。", "Try another search or sync filter."), "search")}
    </div>
  </section>`;
}

function apiKeyPreview(value) {
  if (!value) return "auth.json / CLI";
  if (/^[$!]/.test(value)) return value;
  const characters = Array.from(value);
  const mask = "••••••••";
  if (characters.length <= API_KEY_EDGE_LENGTH * 2) return mask;
  return characters.slice(0, API_KEY_EDGE_LENGTH).join("") + mask + characters.slice(-API_KEY_EDGE_LENGTH).join("");
}

function providerInfo(provider) {
  const apiKey = apiKeyPreview(provider.apiKey);
  return `<section class="panel">
    <div class="provider-info"><div class="provider-title-row">
      <span class="avatar ${tint(provider.id)}">${initials(provider.id)}</span>
      <div><h2>${h(provider.id)}</h2><div class="provider-title-label">${h(provider.api || t("按模型设置 API", "API set per model"))}</div></div>
      <div class="inline-actions">
        <button class="btn small" data-action="edit-provider">${icon("edit")}${t("编辑", "Edit")}</button>${iconButton("duplicate-provider", "copy", t("复制 Provider", "Duplicate provider"))}${iconButton("remove-provider", "trash", t("删除 Provider", "Delete provider"), "", "danger")}
      </div>
    </div><div class="provider-meta">
      <div><div class="meta-label">BASE URL</div><div class="meta-value">${icon("globe")}${h(provider.baseUrl || "—")}</div></div>
      <div><div class="meta-label">API KEY</div><div class="meta-value">${icon("key")}${h(apiKey)}${provider.apiKeySource ? `<span class="meta-hint"> · ${h(provider.apiKeySource)}</span>` : ""}</div></div>
    </div></div>
  </section>`;
}

function modelTable(state, provider) {
  const models = visibleModels(state);
  const ordering = state.snapshot.ordering.models[provider.id];
  const custom = ordering.sort === "custom";
  const search = listSearch({ scope: "model", query: state.modelQuery, open: state.modelSearchOpen, label: t("搜索模型", "Search models") });
  return `<section class="panel models-panel" aria-label="${t("模型列表", "Models")}">
    <div class="panel-header"><h2>${icon("cpu")}${t("模型", "Models")}<span class="count">${provider.models.length}</span></h2><div class="inline-actions">
      <button class="btn small" data-action="import-models">${icon("download")}${t("在线导入", "Import models")}</button>${sortControl("models", ordering, provider.id)}${search.button}${iconButton("new-model", "plus", t("新建模型", "New model"))}
    </div></div>
    ${search.field}
    ${models.length ? `<div class="table-wrap"><table><thead><tr><th>${t("模型名称", "MODEL")}</th><th>${t("能力", "CAPABILITIES")}</th><th>${t("上下文 / 输出", "CONTEXT / OUTPUT")}</th><th class="optional-column">${t("输入 / 输出价格", "INPUT / OUTPUT")}</th><th><span class="sr-only">${t("操作", "Actions")}</span></th></tr></thead><tbody data-order-list data-order-scope="models" data-order-provider="${h(provider.id)}">
      ${models.map((model) => {
        const isDefault = state.snapshot.defaultProvider === provider.id && state.snapshot.defaultModel === model.id;
        return `<tr class="model-row ${state.modelId === model.id ? "selected" : ""}" tabindex="0" data-action="select-model" data-model="${h(model.id)}" data-order-item data-order-scope="models" data-order-id="${h(model.id)}" data-order-provider="${h(provider.id)}" aria-label="${h(modelName(model))}${isDefault ? " · " + t("默认模型", "Default model") : ""}">
          <td><div class="model-title">${h(modelName(model))}${isDefault ? icon("star") : ""}</div><div class="model-id">${h(model.id)}</div></td>
          <td><div class="capabilities">${capabilities(model)}</div></td>
          <td class="number-cell">${tokens(model.contextWindow)}<span class="subtle-divider">/</span><span class="muted">${tokens(model.maxTokens)}</span></td>
          <td class="number-cell optional-column">${price(model.inputCost)}<span class="subtle-divider">/</span>${price(model.outputCost)}</td>
          <td><div class="model-actions">${custom ? orderHandle({ scope: "models", item: model, providerId: provider.id }) : ""}<span class="model-main-actions">
            ${iconButton("default-model", "star", isDefault ? t("当前默认模型", "Current default model") : provider.inPi ? t("设为默认模型", "Set as default") + " · " + modelName(model) : t("先同步 Provider，再设为默认", "Sync this provider before setting a default"), 'data-model="' + h(model.id) + '" ' + (!provider.inPi || isDefault ? "disabled" : ""), isDefault ? "active" : "")}
            ${iconButton("edit-model", "edit", t("编辑模型", "Edit model") + " · " + modelName(model), 'data-model="' + h(model.id) + '"')}
            ${iconButton("remove-model", "trash", t("删除模型", "Delete model") + " · " + modelName(model), 'data-provider="' + h(provider.id) + '" data-model="' + h(model.id) + '"', "danger")}
          </span></div></td>
        </tr>`;
      }).join("")}
    </tbody></table></div>` : emptyState(provider.models.length ? t("没有匹配的模型", "No matching models") : t("给这个 Provider 添加模型", "Add models to this provider"), provider.models.length ? t("试试其他模型名称或 ID。", "Try another model name or ID.") : t("从服务商在线获取模型，或手动填写模型配置。", "Fetch available models from the provider, or add one manually."), "cpu", provider.models.length ? "" : `<button class="btn primary" data-action="import-models">${icon("download")}${t("在线导入模型", "Import models")}</button>`)}
    <div class="models-foot"><span>${t("显示", "Showing")} ${models.length} / ${provider.models.length} ${t("个模型", "models")}</span><span>${t("价格单位：USD / 1M tokens", "Prices in USD / 1M tokens")}<span class="subtle-divider">·</span>${icon("star")} ${t("默认模型", "Default")}</span></div>
  </section>`;
}

export function profiles(state) {
  const provider = selectedProvider(state);
  const header = pageHeader(t("模型配置", "Model configuration"), "", `<button class="btn" data-action="opencode">${icon("download")}${t("从 OpenCode 导入", "Import OpenCode")}</button><button class="btn primary" data-action="new-provider">${icon("plus")}${t("新建 Provider", "New provider")}</button>`);
  if (!state.snapshot.providers.length) {
    return header + `<section class="panel">${emptyState(t("添加你的第一个 Provider", "Add your first provider"), t("支持 OpenAI、Anthropic、Google 兼容 API，也可以连接自己的模型网关。", "Connect OpenAI, Anthropic, Google compatible APIs, or your own model gateway."), "sliders", `<div class="inline-actions"><button class="btn primary" data-action="new-provider">${icon("plus")}${t("新建 Provider", "New provider")}</button><button class="btn" data-action="opencode">${icon("download")}${t("从 OpenCode 导入", "Import OpenCode")}</button></div>`)}</section>`;
  }
  return header + `<div class="profiles-layout">${providerList(state)}<div class="provider-detail">${provider ? providerInfo(provider) + modelTable(state, provider) : `<section class="panel">${emptyState(t("选择一个 Provider", "Select a provider"), t("在左侧选择 provider 来管理模型。", "Select a provider to manage its models."), "sliders")}</section>`}</div></div>`
    + contextHelp([["↑ ↓", t("选择", "Select")], ["Alt ↑ ↓", t("自定义排序", "Move in custom order")], ["n", t("新建", "New")], ["e", t("编辑", "Edit")], ["Space", t("同步 / 设为默认", "Sync / set default")], ["/", t("筛选", "Filter")], ["?", t("更多快捷键", "More shortcuts")]]);
}
