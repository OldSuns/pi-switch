import { h, t, icon, iconButton, emptyState, searchInput, tokens } from "./ui.js";

const cancel = () => '<button class="btn quiet" type="button" data-action="close-dialog">' + t("取消", "Cancel") + "</button>";
const submit = (label) => '<button class="btn primary" type="submit" data-submit>' + label + "</button>";
const jsonValue = (value) => value && Object.keys(value).length ? JSON.stringify(value, null, 2) : "";

export function frame({ title, description = "", body, footer = "", form = "" }) {
  return `${form ? '<form data-form="' + form + '">' : ""}
    <div class="dialog-header"><div><h2 id="dialog-title">${h(title)}</h2>${description ? "<p>" + h(description) + "</p>" : ""}</div>${iconButton("close-dialog", "close", t("关闭", "Close"))}</div>
    <div class="dialog-body"><div class="dialog-error" id="dialog-error" role="alert"></div>${body}</div>
    <div class="dialog-footer">${footer || '<button class="btn" type="button" data-action="close-dialog" autofocus>' + t("完成", "Done") + "</button>"}</div>
  ${form ? "</form>" : ""}`;
}

function field(name, label, value, options = {}) {
  const id = "field-" + name;
  const help = options.help ? `<span id="${id}-help" class="field-help">${h(options.help)}</span>` : "";
  const attributes = `id="${id}" name="${name}" ${options.required ? "required" : ""} ${options.autofocus ? "autofocus" : ""} ${options.help ? 'aria-describedby="' + id + '-help"' : ""}`;
  const input = options.textarea
    ? `<textarea ${attributes} spellcheck="false" placeholder="${h(options.placeholder || "")}">${h(value)}</textarea>`
    : `<input ${attributes} type="${options.type || "text"}" value="${h(value)}" placeholder="${h(options.placeholder || "")}" ${options.type === "number" ? 'min="' + (options.min ?? 0) + '" step="' + (options.step ?? "any") + '" max="9007199254740991" inputmode="decimal"' : 'autocomplete="off" spellcheck="false"'}>`;
  return `<div class="form-field ${options.full ? "full" : ""}"><label for="${id}">${h(label)}${options.required ? ' <span class="required">*</span>' : ""}</label>${options.type === "password" ? '<div class="field-with-action">' + input + iconButton("reveal-key", "eye", t("显示 / 隐藏密钥", "Show / hide key")) + "</div>" : input}${help}<span class="field-error" data-field-error="${name}"></span></div>`;
}

function apiField(value, apiTypes, provider = false) {
  return `<div class="form-field"><label for="field-api">API ${t("类型", "type")}</label><select id="field-api" name="api"><option value="">${provider ? t("由模型指定", "Set per model") : t("继承 Provider 设置", "Inherit from provider")}</option>${apiTypes.map((api) => '<option value="' + h(api) + '" ' + (value === api ? "selected" : "") + ">" + h(api) + "</option>").join("")}</select></div>`;
}

function numericFields(values) {
  return field("contextWindow", t("上下文窗口", "Context window"), values.contextWindow, { type: "number", min: 1, step: 1, placeholder: "128000", help: t("留空使用 Pi 默认值", "Leave empty for Pi's default") })
    + field("maxTokens", t("最大输出 Token", "Max output tokens"), values.maxTokens, { type: "number", min: 1, step: 1, placeholder: "16384", help: t("留空使用 Pi 默认值", "Leave empty for Pi's default") })
    + field("inputCost", t("输入价格 · USD / 1M", "Input cost · USD / 1M"), values.inputCost, { type: "number", placeholder: "0" })
    + field("outputCost", t("输出价格 · USD / 1M", "Output cost · USD / 1M"), values.outputCost, { type: "number", placeholder: "0" })
    + field("cacheReadCost", t("缓存读取 · USD / 1M", "Cache read · USD / 1M"), values.cacheReadCost, { type: "number", placeholder: "0" })
    + field("cacheWriteCost", t("缓存写入 · USD / 1M", "Cache write · USD / 1M"), values.cacheWriteCost, { type: "number", placeholder: "0" });
}

export function providerDialog(snapshot, provider, editing = Boolean(provider)) {
  const value = provider ?? { id: "", baseUrl: "", api: "openai-completions", apiKey: "", authHeader: true, inPi: true, headers: null, compat: null };
  const headers = Object.fromEntries(Object.entries(value.headers ?? {}).filter(([key]) => key.toLowerCase() !== "user-agent"));
  const userAgent = Object.entries(value.headers ?? {}).find(([key]) => key.toLowerCase() === "user-agent")?.[1] ?? "";
  const compat = Object.fromEntries(Object.entries(value.compat ?? {}).filter(([key]) => key !== "sendSessionAffinityHeaders"));
  return frame({
    title: editing ? t("编辑 Provider", "Edit provider") : t("新建 Provider", "New provider"),
    description: t("连接你的模型服务，将配置保存在本地。", "Connect a model service and keep its configuration locally."),
    form: "provider",
    body: `<div class="form-grid">
      ${field("id", "Provider ID", value.id, { required: true, autofocus: true, placeholder: "my-provider", help: t("在本地库中唯一的名称", "A unique name in your local library") })}
      ${apiField(value.api, snapshot.apiTypes, true)}
      ${field("baseUrl", "Base URL", value.baseUrl, { full: true, type: "url", placeholder: "https://api.example.com/v1", help: t("API 基础地址；留空使用内置默认地址。", "API base URL. Leave empty to use the built-in default.") })}
      ${field("apiKey", "API Key", value.apiKey, { type: "password", full: true, placeholder: "$YOUR_API_KEY", help: t("支持环境变量引用；留空使用 Pi 的 auth.json / CLI 鉴权。", "Environment variable references are supported. Leave empty for Pi auth.json / CLI authentication.") })}
    </div>
    <div class="form-options"><label class="checkbox-label"><input name="inPi" type="checkbox" data-action="draft-inpi" ${value.inPi ? "checked" : ""}>${t("同步到 Pi", "Sync to Pi")}</label><label class="checkbox-label"><input name="authHeader" type="checkbox" ${value.authHeader ? "checked" : ""}>${t("发送 Authorization 请求头", "Send Authorization header")}</label></div>
    ${snapshot.defaultProvider === provider?.id ? `<div class="banner" id="unsync-confirmation" hidden><div><p>${t("取消同步会清除当前默认模型，本地配置仍会保留。", "Removing this provider from Pi clears the current default model. The local configuration is kept.")}</p><label class="checkbox-label"><input name="confirmUnsync" type="checkbox">${t("确认清除默认模型", "I confirm clearing the default model")}</label></div></div>` : ""}
    <details class="advanced"><summary>${t("高级设置 · Headers 与兼容性", "Advanced · Headers and compatibility")}</summary><div class="advanced-content"><div class="form-grid">
      ${field("userAgent", "User-Agent", userAgent, { full: true, placeholder: t("可选", "Optional") })}
      ${field("headers", t("其他 Headers · JSON 对象", "Other headers · JSON object"), jsonValue(headers), { full: true, textarea: true, placeholder: '{"X-Custom-Header": "value"}' })}
      <div class="form-field full"><label class="checkbox-label"><input name="sessionAffinity" type="checkbox" ${value.compat?.sendSessionAffinityHeaders ? "checked" : ""}>${t("Session affinity · 会话亲和性", "Session affinity headers")}</label><span class="field-help">sendSessionAffinityHeaders</span></div>
      ${field("compat", t("其他兼容配置 · JSON 对象", "Other compatibility options · JSON"), jsonValue(compat), { full: true, textarea: true, placeholder: "{}" })}
    </div></div></details>`,
    footer: cancel() + submit(t("保存 Provider", "Save provider")),
  });
}

export function modelDialog(model, copy, apiTypes) {
  const value = model ?? { id: "", name: "", api: "", reasoning: false, input: ["text"] };
  return frame({
    title: copy ? t("复制模型", "Duplicate model") : model ? t("编辑模型", "Edit model") : t("新建模型", "New model"),
    description: t("配置模型标识、能力及参数。未修改的扩展字段会保留。", "Configure model identity, capabilities, and limits. Other extension fields are preserved."),
    form: "model",
    body: `<div class="form-grid">${field("id", "Model ID", value.id, { required: true, autofocus: true, placeholder: "model-id" })}${field("name", t("显示名称", "Display name"), value.name, { placeholder: t("可选，默认显示 Model ID", "Optional, defaults to model ID") })}${apiField(value.api, apiTypes)}<div class="form-field"><label>${t("模型能力", "Capabilities")}</label><label class="checkbox-label"><input name="reasoning" type="checkbox" data-action="draft-reasoning" ${value.reasoning ? "checked" : ""}>${t("支持推理", "Reasoning")}</label></div>
      <div class="form-field full"><label>${t("输入类型", "Input types")}</label><div class="form-options"><label class="checkbox-label"><input name="input" type="checkbox" value="text" ${value.input.includes("text") ? "checked" : ""}>${t("文本", "Text")}</label><label class="checkbox-label"><input name="input" type="checkbox" value="image" ${value.input.includes("image") ? "checked" : ""}>${t("图像", "Image")}</label></div><span class="field-error" data-field-error="input"></span></div>
    </div><details class="advanced"><summary>${t("上下文、输出限制与价格", "Context, output limits, and pricing")}</summary><div class="advanced-content"><div class="form-grid">${numericFields(value)}</div></div></details>
    <div id="thinking-fields" ${value.reasoning ? "" : "hidden"}><details class="advanced"><summary>${t("思考等级映射", "Thinking level mapping")}</summary><div class="advanced-content">${field("thinkingLevelMap", "thinkingLevelMap · JSON", jsonValue(value.thinkingLevelMap), { textarea: true, help: t("将 Pi 思考等级映射到服务商参数，留空使用默认设置。", "Map Pi thinking levels to provider values. Leave empty for defaults."), placeholder: '{"low":"low","medium":"medium","high":"high"}' })}</div></details></div>`,
    footer: `${model && !copy ? '<div class="form-extra-actions">' + iconButton("duplicate-model", "copy", t("复制模型", "Duplicate model"), 'data-model="' + h(model.id) + '"') + iconButton("remove-model", "trash", t("删除模型", "Delete model"), 'data-model="' + h(model.id) + '"', "danger") + "</div>" : ""}${cancel()}${submit(t("保存模型", "Save model"))}`,
  });
}

export function defaultsDialog(values) {
  return frame({ title: t("默认模型参数", "Default model parameters"), description: t("关闭 models.dev 元数据后，这些参数用于模型导入。", "Used for model imports when models.dev metadata is disabled."), form: "defaults", body: '<div class="form-grid">' + numericFields(values) + "</div>", footer: cancel() + submit(t("保存参数", "Save parameters")) });
}

export function confirmationDialog({ title, description, detail, label, danger = false, checkbox = null }) {
  const option = checkbox ? `<p class="confirm-description"><label class="checkbox-label"><input type="checkbox" data-confirm-option ${checkbox.checked ? "checked" : ""}>${h(checkbox.label)}</label></p>` : "";
  return frame({ title, body: `<p class="confirm-description">${h(description)}</p>${detail ? '<div class="confirm-detail">' + h(detail) + "</div>" : ""}${option}`, footer: `<button class="btn quiet" type="button" data-action="close-dialog" autofocus>${t("取消", "Cancel")}</button><button class="btn ${danger ? "danger" : "primary"}" type="button" data-action="confirm" data-submit>${h(label || t("确认", "Confirm"))}</button>` });
}

export function loadingDialog(title, description) {
  return frame({ title, body: `<div class="loading-state" role="status"><span class="spinner"></span>${h(description || t("正在读取…", "Loading…"))}</div>` });
}

export function doctorDialog(checks) {
  return frame({ title: t("配置检查", "Configuration checks"), description: checks.every((check) => check.ok) ? t("所有检查已通过。", "All checks passed.") : t("以下项目需要处理。", "Some items need your attention."), body: '<div class="check-list">' + checks.map((check) => `<div class="check-row">${icon(check.ok ? "checkCircle" : "warning", check.ok ? "green" : "red")}<div><h3>${h(check.label)}</h3><p>${h(check.detail)}</p></div></div>`).join("") + "</div>" });
}

export function backupsDialog(backups) {
  return frame({ title: t("配置备份", "Configuration backups"), description: t("恢复前会先备份当前配置。最多保留最近 10 份备份。", "Your current configuration is backed up before restoration. Up to 10 backups are kept."), body: backups.length ? backups.map((backup) => `<div class="backup-row">${icon("file")}<code>${h(backup.name)}</code><button type="button" class="btn small" data-action="restore-backup" data-backup="${h(backup.name)}">${icon("history")}${t("恢复", "Restore")}</button></div>`).join("") : emptyState(t("还没有备份", "No backups yet"), t("保存配置时，会自动为你创建备份。", "A backup will be created when you save configuration."), "history") });
}

export function selectionDialog(context) {
  const isModels = context.kind === "models";
  const visible = context.items.filter((item) => item.id.toLowerCase().includes(context.query.toLowerCase()));
  return frame({
    title: isModels ? t("在线导入模型", "Import models") : t("从 OpenCode 导入", "Import from OpenCode"),
    description: isModels ? context.providerId : t("选中的 provider 将导入本地库并同步到 Pi。OpenCode 原文件不会修改。", "Selected providers will be imported locally and synced to Pi. Your OpenCode file is kept intact."),
    form: "import",
    body: `<div class="import-tools">${searchInput("import-search", context.query, isModels ? t("搜索模型 ID…", "Search model IDs…") : t("搜索 Provider…", "Search providers…"))}<div class="inline-actions"><button type="button" class="text-button" data-action="select-all-import">${t("全选", "Select all")}</button><span class="subtle-divider">/</span><button type="button" class="text-button" data-action="clear-import">${t("清空", "Clear")}</button></div></div>
      ${!isModels ? '<div class="field-help mono">' + h(context.path) + "</div>" : ""}
      <div class="import-list">${visible.length ? visible.map((item) => `<label class="import-row"><input type="checkbox" data-action="import-selection" value="${h(item.id)}" ${context.selected.has(item.id) ? "checked" : ""}><span class="mono">${h(item.id)}</span>${item.existing ? '<span class="badge">' + t("已存在", "Existing") + "</span>" : isModels ? '<span class="badge accent">' + t("新模型", "New") + "</span>" : ""}</label>`).join("") : emptyState(t("没有匹配项", "No matches"), t("试试其他搜索关键词。", "Try another search."), "search")}</div>
      <div class="import-count">${t("已选择", "Selected")} <span id="import-selected-count">${context.selected.size}</span> / ${context.items.length}</div>
      ${isModels ? '<label class="checkbox-label"><input type="checkbox" data-action="import-overwrite" ' + (context.updateExisting ? "checked" : "") + ">" + t("更新已存在模型的元数据", "Update metadata for existing models") + "</label>" : ""}
    `, footer: cancel() + submit(isModels ? t("导入选中模型", "Import selected models") : t("导入并同步到 Pi", "Import and sync to Pi")),
  });
}

export function ambiguityDialog(ambiguities, warning) {
  return frame({
    title: t("选择模型元数据来源", "Choose model metadata"),
    description: t("同一模型在不同来源中有不同配置，请选择要使用的版本。", "Sources describe this model differently. Choose the metadata to use."),
    form: "ambiguities",
    body: (warning ? '<div class="banner">' + h(warning) + "</div>" : "") + ambiguities.map((ambiguity, index) => `<div class="form-field ambiguity-field"><label for="candidate-${index}">${h(ambiguity.providerId)} / ${h(ambiguity.modelId)}</label><select id="candidate-${index}" name="candidate-${index}" required><option value="">${t("请选择来源…", "Select a source…")}</option>${ambiguity.candidates.map((candidate, candidateIndex) => '<option value="' + candidateIndex + '">' + h(candidate.providerId) + " · " + h(candidate.id) + " · " + tokens(candidate.config.contextWindow) + " context</option>").join("")}</select></div>`).join(""),
    footer: cancel() + submit(t("确认并导入", "Confirm import")),
  });
}

export function helpDialog() {
  const rows = [
    ["1 / 2 / 3 / 4", t("主页 / 配置 / 会话 / 设置", "Overview / Profiles / Sessions / Settings")],
    ["/", t("聚焦当前列表搜索", "Focus the current search")],
    ["↑ ↓ / j k", t("在当前列表中移动", "Move through the current list")],
    ["← → / h l", t("切换 provider / 模型焦点，浏览会话树", "Move between providers / models, browse tree")],
    ["n / e / d / c", t("新建 / 编辑 / 删除 / 复制配置", "New / edit / delete / duplicate configuration")],
    ["Space", t("同步 provider / 设为默认 / 折叠分支", "Sync provider / set default / toggle branch")],
    ["i", t("在线导入模型", "Import models")],
    ["r / b", t("重载 / 浏览备份", "Reload / browse backups")],
    ["v", t("检查配置；会话页切换阅读模式", "Validate; toggle reading on Sessions")],
    ["n / u", t("会话页：仅命名 / 仅用户消息", "Sessions: named only / user messages only")],
    ["Ctrl C", t("会话树中复制选中消息", "Copy the selected message in the tree")],
    ["Esc", t("关闭对话框 / 返回列表", "Close dialog / return to list")],
  ];
  return frame({ title: t("熟悉的快捷键", "Familiar keyboard shortcuts"), description: t("保留 TUI 的操作习惯。输入文字时不会触发快捷键。", "Your TUI habits carry over. Shortcuts pause while typing."), body: '<div class="help-grid">' + rows.map(([keys, label]) => '<div class="help-row"><span>' + label + "</span><span><kbd>" + keys + "</kbd></span></div>").join("") + "</div>" });
}

export class FieldError extends Error {
  constructor(field, message) { super(message); this.field = field; }
}

function objectValue(form, name) {
  const value = form.elements[name].value.trim();
  if (!value) return null;
  let parsed;
  try {
    parsed = JSON.parse(value, (_key, item) => {
      if (typeof item === "number" && !Number.isFinite(item)) throw new Error("Non-finite number");
      return item;
    });
  } catch {
    throw new FieldError(name, t("请填写有效的 JSON 对象。", "Enter a valid JSON object."));
  }
  if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") throw new FieldError(name, t("这里需要 JSON 对象，不能是数组或其他值。", "Expected a JSON object, not an array or another value."));
  return parsed;
}

export function providerDraft(form, original) {
  const values = new FormData(form);
  const headers = objectValue(form, "headers") ?? {};
  if (Object.keys(headers).some((key) => key.toLowerCase() === "user-agent")) throw new FieldError("headers", t("请在上方独立的 User-Agent 字段填写此请求头。", "Use the dedicated User-Agent field above."));
  const userAgent = values.get("userAgent").trim();
  if (userAgent) headers["User-Agent"] = userAgent;
  const compat = objectValue(form, "compat") ?? {};
  if (Object.hasOwn(compat, "sendSessionAffinityHeaders")) throw new FieldError("compat", t("请使用上方 Session affinity 开关。", "Use the Session affinity checkbox above."));
  if (values.has("sessionAffinity") || Object.hasOwn(original?.compat ?? {}, "sendSessionAffinityHeaders")) compat.sendSessionAffinityHeaders = values.has("sessionAffinity");
  return { id: values.get("id").trim(), inPi: values.has("inPi"), baseUrl: values.get("baseUrl").trim(), api: values.get("api") || null, apiKey: values.get("apiKey"), authHeader: values.has("authHeader"), headers: Object.keys(headers).length ? headers : null, compat: Object.keys(compat).length ? compat : null };
}

export function numericDraft(form) {
  return Object.fromEntries(["contextWindow", "maxTokens", "inputCost", "outputCost", "cacheReadCost", "cacheWriteCost"].map((name) => {
    const input = form.elements[name];
    if (!input.value.trim()) return [name, null];
    const value = Number(input.value);
    if (!Number.isFinite(value) || value < 0 || (["contextWindow", "maxTokens"].includes(name) && (!Number.isSafeInteger(value) || value === 0))) throw new FieldError(name, t("请填写有效的数值。", "Enter a valid number."));
    return [name, value];
  }));
}

export function modelDraft(form) {
  const values = new FormData(form);
  const input = values.getAll("input");
  if (!input.length) throw new FieldError("input", t("至少选择一种输入类型。", "Select at least one input type."));
  return { id: values.get("id").trim(), name: values.get("name").trim() || null, api: values.get("api") || null, reasoning: values.has("reasoning"), input, ...numericDraft(form), thinkingLevelMap: objectValue(form, "thinkingLevelMap") };
}
