import { h, t, icon, setLanguage, emptyState } from "./ui.js";
import { shell, pages } from "./shell.js";
import { overview } from "./overview.js";
import { profiles, selectedProvider } from "./profiles.js";
import { sessions } from "./sessions.js";
import { settings } from "./settings.js";
import * as dialogs from "./dialogs.js";
import { setTheme } from "./appearance.js";

const state = {
  snapshot: null, page: "overview", providerId: null, providerQuery: "", providerFilter: "all", modelQuery: "", modelId: null,
  sessions: [], sessionsLoaded: false, sessionsError: null, sessionId: null, sessionQuery: "", namedOnly: false,
  preview: null, previewLoading: false, previewError: null, previewMode: "tree", messageId: null, userOnly: false, folded: new Set(),
  checks: null, busy: false, connected: false, navOpen: false, lastSaved: null,
};
const app = document.getElementById("app");
const dialog = document.getElementById("dialog");
let dialogContext = null;
let dialogVersion = 0;
let previewVersion = 0;
let sessionVersion = 0;
let returnFocus = null;
let returnFocusSelector = null;
let toastTimer;
const BUSY_ACTIONS = new Set(["close-toast", "toggle-nav", "close-nav", "select-model", "skip-to-content", "theme"]);
const mobileViewport = matchMedia("(max-width: 760px)");

async function api(action, payload = {}) {
  let response;
  try {
    response = await fetch("/api", {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ action, ...payload }),
    });
  } catch (error) {
    state.connected = false;
    throw new Error(t("无法连接本地服务。请确认 pi-switch --web 仍在运行。", "Cannot connect to the local service. Check that pi-switch --web is still running."), { cause: error });
  }
  state.connected = true;
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || t("请求失败", "Request failed"));
  return result;
}

function applySnapshot(snapshot) {
  state.snapshot = snapshot;
  setLanguage(snapshot.language);
  if (!snapshot.providers.some((provider) => provider.id === state.providerId)) state.providerId = snapshot.providers[0]?.id ?? null;
  if (!selectedProvider(state)?.models.some((model) => model.id === state.modelId)) state.modelId = null;
}

function render({ resetScroll = false, focusMain = false } = {}) {
  if (!state.snapshot) return;
  const main = document.getElementById("main");
  const scrollTop = resetScroll ? 0 : main?.scrollTop ?? 0;
  const active = document.activeElement;
  const activeId = app.contains(active) ? active.id : null;
  const activeSelector = app.contains(active) ? focusSelector(active) : null;
  const selectionStart = activeId && "selectionStart" in active ? active.selectionStart : null;
  const selectionEnd = activeId && "selectionEnd" in active ? active.selectionEnd : null;
  const renderer = { overview, profiles, sessions, settings }[state.page];
  app.innerHTML = shell(state, renderer(state));
  if (state.busy) {
    for (const element of app.querySelectorAll("button[data-action], input[data-action], select[data-action]")) {
      if (!BUSY_ACTIONS.has(element.dataset.action)) element.disabled = true;
    }
  }
  document.title = "pi-switch · " + t(pages[state.page].zh, pages[state.page].en);
  document.getElementById("main").scrollTop = scrollTop;
  if (activeId) {
    const next = document.getElementById(activeId);
    next?.focus({ preventScroll: true });
    if (selectionStart != null && next?.setSelectionRange) next.setSelectionRange(selectionStart, selectionEnd);
  } else if (activeSelector) {
    document.querySelector(activeSelector)?.focus({ preventScroll: true });
  }
  if (focusMain) document.getElementById("main").focus({ preventScroll: true });
  for (const anchor of document.querySelectorAll(".markdown a")) {
    anchor.target = "_blank";
    anchor.rel = "noreferrer";
  }
}

function route() {
  const [page, query] = location.hash.slice(1).split("?");
  const previousPage = state.page;
  state.page = Object.hasOwn(pages, page) ? page : "overview";
  state.navOpen = false;
  const parameters = new URLSearchParams(query);
  if (parameters.has("provider")) state.providerId = parameters.get("provider");
  if (state.snapshot && !selectedProvider(state)) state.providerId = state.snapshot.providers[0]?.id ?? null;
  const sessionId = parameters.get("session");
  if (state.page === "sessions" && sessionId && sessionId !== state.sessionId) {
    state.sessionId = sessionId;
    if (state.sessionsLoaded) void loadPreview();
  }
  render({ resetScroll: previousPage !== state.page, focusMain: previousPage !== state.page });
}

function navigate(page, key, value) {
  const next = "#" + page + (key && value ? "?" + key + "=" + encodeURIComponent(value) : "");
  if (location.hash === next) route();
  else location.hash = next;
}

function toast(message, error = false) {
  clearTimeout(toastTimer);
  document.getElementById("toast-region").innerHTML = `<div class="toast ${error ? "error" : ""}" ${error ? 'role="alert"' : ""}>${icon(error ? "warning" : "checkCircle")}<div class="toast-content">${h(message)}</div><button class="icon-button" data-action="close-toast" aria-label="${t("关闭提示", "Dismiss notification")}">${icon("close")}</button></div>`;
  if (!error) toastTimer = setTimeout(() => { document.getElementById("toast-region").textContent = ""; }, 5000);
}

function showError(error) {
  if (!dialog.open) { toast(error.message, true); render(); return; }
  const target = dialog.querySelector("#dialog-error");
  if (target) target.innerHTML = '<div class="banner error">' + icon("warning") + "<span>" + h(error.message) + "</span></div>";
  if (error instanceof dialogs.FieldError) {
    const field = dialog.querySelector('[name="' + error.field + '"]');
    const details = field?.closest("details");
    if (details) details.open = true;
    field?.setAttribute("aria-invalid", "true");
    const fieldError = dialog.querySelector('[data-field-error="' + error.field + '"]');
    if (fieldError) fieldError.textContent = error.message;
    field?.focus();
  } else {
    target?.scrollIntoView({ block: "nearest" });
  }
}

function openDialog(content, context = null, preserveFocus = false) {
  if (!dialog.open) {
    returnFocus = document.activeElement;
    returnFocusSelector = focusSelector(returnFocus);
  }
  let focusId;
  let start;
  if (preserveFocus) {
    focusId = dialog.contains(document.activeElement) ? document.activeElement.id : null;
    start = document.activeElement.selectionStart;
  }
  dialogVersion++;
  dialogContext = context;
  dialog.innerHTML = content;
  if (!dialog.open) dialog.showModal();
  const focus = focusId ? document.getElementById(focusId) : dialog.querySelector("[autofocus]");
  focus?.focus({ preventScroll: true });
  if (focusId && start != null) focus?.setSelectionRange(start, start);
}

function closeDialog() {
  if (state.busy) return;
  dialogVersion++;
  dialog.close();
}

dialog.addEventListener("cancel", (event) => {
  if (state.busy) event.preventDefault();
});
dialog.addEventListener("close", () => {
  dialogVersion++;
  dialogContext = null;
  const nextFocus = returnFocus?.isConnected ? returnFocus : returnFocusSelector && document.querySelector(returnFocusSelector);
  if (nextFocus) nextFocus.focus({ preventScroll: true });
  else document.getElementById("main")?.focus({ preventScroll: true });
});

async function execute(task) {
  if (state.busy) return;
  const previousFocus = focusSelector(document.activeElement);
  const rowFocus = focusSelector(document.activeElement.closest(".model-row,.provider-option,.session-option"));
  state.busy = true;
  const buttons = [...dialog.querySelectorAll("[data-submit]")];
  const labels = buttons.map((button) => button.innerHTML);
  for (const button of buttons) { button.disabled = true; button.innerHTML = '<span class="spinner"></span>' + t("处理中…", "Working…"); }
  render();
  let controls = [];
  let outcome;
  let failure;
  try {
    const operation = task();
    controls = [...dialog.querySelectorAll("input,select,textarea")].map((element) => ({ element, disabled: element.disabled }));
    controls.forEach(({ element }) => { element.disabled = true; });
    outcome = await operation;
  } catch (error) {
    failure = error;
  } finally {
    state.busy = false;
    controls.forEach(({ element, disabled }) => { if (element.isConnected) element.disabled = disabled; });
    buttons.forEach((button, index) => { if (button.isConnected) { button.disabled = false; button.innerHTML = labels[index]; } });
    render();
    if (!dialog.open) {
      const preferred = previousFocus && document.querySelector(previousFocus);
      const fallback = rowFocus && document.querySelector(rowFocus);
      const target = preferred && !preferred.disabled ? preferred : fallback;
      target?.focus({ preventScroll: true });
    }
  }
  if (failure) showError(failure);
  return outcome;
}

async function save(action, payload, message, options = {}) {
  const result = await api(action, payload);
  if (result.snapshot) {
    applySnapshot(result.snapshot);
    state.lastSaved = Date.now();
    state.checks = null;
  }
  if (options.close !== false && dialog.open) dialog.close();
  if (message) toast(message);
  return result;
}

function confirm(options, task) {
  openDialog(dialogs.confirmationDialog(options), { kind: "confirm", task });
}

async function refresh() {
  const snapshot = await api("snapshot");
  applySnapshot(snapshot);
  render();
  await loadSessions();
}

async function loadSessions() {
  const requestVersion = ++sessionVersion;
  try {
    const result = await api("sessions.list");
    if (requestVersion !== sessionVersion) return;
    state.sessions = result.sessions;
    state.sessionsError = null;
    if (state.sessionId && !state.sessions.some((session) => session.id === state.sessionId)) {
      state.sessionId = null;
      state.preview = null;
    }
    if (state.sessionId && state.page === "sessions") void loadPreview();
  } catch (error) {
    if (requestVersion !== sessionVersion) return;
    state.sessionsError = error.message;
  } finally {
    if (requestVersion === sessionVersion) { state.sessionsLoaded = true; render(); }
  }
}

async function loadPreview() {
  if (!state.sessionId) return;
  const requestVersion = ++previewVersion;
  const id = state.sessionId;
  state.previewLoading = true;
  state.previewError = null;
  state.preview = null;
  render();
  try {
    const preview = await api("sessions.preview", { id, userOnly: state.userOnly });
    if (requestVersion !== previewVersion || id !== state.sessionId) return;
    state.preview = preview;
    state.folded = new Set();
    state.messageId = preview.activeMessageId ?? preview.messages[0]?.id ?? null;
  } catch (error) {
    if (requestVersion === previewVersion) state.previewError = error.message;
  } finally {
    if (requestVersion === previewVersion) {
      state.previewLoading = false;
      render();
      document.querySelector('.tree-item[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
    }
  }
}

async function loadDialog(title, action, renderer, payload = {}) {
  openDialog(dialogs.loadingDialog(title), { kind: "loading" });
  const version = dialogVersion;
  try {
    const result = await api(action, payload);
    if (!dialog.open || dialogVersion !== version) return;
    openDialog(renderer(result));
  } catch (error) {
    if (dialog.open && dialogVersion === version) {
      openDialog(dialogs.frame({ title, body: "" }));
      showError(error);
    }
  }
}

function editProvider(provider = null) {
  openDialog(dialogs.providerDialog(state.snapshot, provider), { kind: "provider", provider });
}

function editModel(model = null, copy = false, provider = selectedProvider(state)) {
  if (!provider) return;
  let draft = model;
  if (copy) {
    let suffix = "-copy";
    let index = 2;
    while (provider.models.some((candidate) => candidate.id === model.id + suffix)) suffix = "-copy-" + index++;
    draft = { ...model, id: model.id + suffix };
  }
  openDialog(dialogs.modelDialog(draft, copy, state.snapshot.apiTypes), { kind: "model", providerId: provider.id, previousId: model && !copy ? model.id : null });
}

async function importModels(provider) {
  openDialog(dialogs.loadingDialog(t("在线导入模型", "Import models"), t("正在获取服务商的模型列表…", "Fetching the provider's model list…")));
  const version = dialogVersion;
  try {
    const result = await api("models.fetch", { providerId: provider.id });
    if (!dialog.open || dialogVersion !== version) return;
    const context = { kind: "models", providerId: provider.id, items: result.models, selected: new Set(result.models.filter((model) => !model.existing).map((model) => model.id)), query: "", updateExisting: false };
    openDialog(dialogs.selectionDialog(context), context);
  } catch (error) {
    if (dialog.open && version === dialogVersion) {
      openDialog(dialogs.frame({ title: t("在线导入模型", "Import models"), body: "" }));
      showError(error);
    }
  }
}

async function importOpenCode() {
  openDialog(dialogs.loadingDialog(t("从 OpenCode 导入", "Import from OpenCode")));
  const version = dialogVersion;
  try {
    const result = await api("opencode.list");
    if (!dialog.open || dialogVersion !== version) return;
    const context = { kind: "opencode", path: result.path, items: result.providerIds.map((id) => ({ id })), selected: new Set(), query: "" };
    openDialog(dialogs.selectionDialog(context), context);
  } catch (error) {
    if (dialog.open && version === dialogVersion) {
      openDialog(dialogs.frame({ title: t("从 OpenCode 导入", "Import from OpenCode"), body: "" }));
      showError(error);
    }
  }
}

function showImportResult(result) {
  applySnapshot(result.snapshot);
  state.lastSaved = Date.now();
  dialog.close();
  const summary = result.summary;
  const message = "added" in summary
    ? t("模型导入完成：新增 " + summary.added + "，更新 " + summary.updated + "。", "Models imported: " + summary.added + " added, " + summary.updated + " updated.")
    : t("已导入 " + summary.providers + " 个 Provider、" + summary.models + " 个模型，并同步到 Pi。", "Imported " + summary.providers + " providers and " + summary.models + " models, synced to Pi.");
  toast(message + (result.warning ? "\n" + result.warning : ""), Boolean(result.warning));
}

async function finishModelImport(payload) {
  const result = await api("models.import", payload);
  if (result.requiresSelection) {
    openDialog(dialogs.ambiguityDialog(result.ambiguities, result.warning), { kind: "ambiguities", action: "models.import", payload: { ...payload, selectionId: result.selectionId }, count: result.ambiguities.length });
  } else {
    showImportResult(result);
  }
}

async function submitImport() {
  const context = dialogContext;
  const ids = Array.from(context.selected);
  if (!ids.length) throw new Error(t("请至少选择一项。", "Select at least one item."));
  if (context.kind === "models") {
    await finishModelImport({ providerId: context.providerId, ids, updateExisting: context.updateExisting });
    return;
  }
  const prepared = await api("opencode.prepare", { providerIds: ids });
  if (prepared.ambiguities.length) {
    openDialog(dialogs.ambiguityDialog(prepared.ambiguities), { kind: "ambiguities", action: "opencode.import", payload: { planId: prepared.planId }, count: prepared.ambiguities.length });
  } else {
    showImportResult(await api("opencode.import", { planId: prepared.planId, candidateIndices: [] }));
  }
}

async function copyText(value) {
  await navigator.clipboard.writeText(value);
  toast(t("已复制到剪贴板", "Copied to clipboard"));
}

function focusMessage(id) {
  state.messageId = id;
  const treeScroll = document.querySelector(".tree-list")?.scrollTop ?? 0;
  render();
  const tree = document.querySelector(".tree-list");
  if (tree) tree.scrollTop = treeScroll;
  document.querySelector('.tree-item[aria-selected="true"]')?.focus({ preventScroll: true });
  document.querySelector('.tree-item[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
}

async function handleAction(action, target) {
  const providerId = target?.dataset.provider ?? (dialog.open && dialogContext?.kind === "model" ? dialogContext.providerId : state.providerId);
  const provider = state.snapshot?.providers.find((item) => item.id === providerId);
  const modelId = target?.dataset.model ?? state.modelId;
  const model = provider?.models.find((item) => item.id === modelId);
  switch (action) {
    case "theme": setTheme(target.value); render(); break;
    case "choose-default": {
      const [providerId, modelId] = JSON.parse(target.value);
      await execute(() => save("model.default", { providerId, modelId }, t("默认模型已切换", "Default model changed")));
      break;
    }
    case "skip-to-content": document.getElementById("main")?.focus(); break;
    case "toggle-nav":
      state.navOpen = !state.navOpen; render();
      document.querySelector(state.navOpen ? ".nav-link.active" : ".mobile-menu")?.focus();
      break;
    case "close-nav": state.navOpen = false; render(); document.querySelector(".mobile-menu")?.focus(); break;
    case "close-toast": document.getElementById("toast-region").textContent = ""; break;
    case "help": openDialog(dialogs.helpDialog()); break;
    case "close-dialog": closeDialog(); break;
    case "confirm": { const task = dialogContext.task; await execute(task); break; }
    case "reload": await execute(async () => { await refresh(); toast(t("已重新读取本地配置", "Local configuration reloaded")); }); break;
    case "new-provider": editProvider(); break;
    case "edit-provider": editProvider(provider); break;
    case "select-provider":
      state.modelQuery = ""; state.modelId = null;
      navigate("profiles", "provider", target.dataset.provider); break;
    case "provider-filter": state.providerFilter = target.dataset.value; render(); break;
    case "duplicate-provider":
      if (!provider) break;
      await execute(async () => {
        const result = await save("provider.duplicate", { providerId: provider.id }, t("Provider 已复制", "Provider duplicated"));
        state.providerQuery = ""; state.providerFilter = "all";
        navigate("profiles", "provider", result.providerId);
      }); break;
    case "remove-provider":
      if (!provider) break;
      confirm({ title: t("删除 Provider？", "Delete provider?"), description: t("这会从本地库删除此 Provider，并取消其 Pi 同步。关联的默认模型也会清除。", "This deletes the provider from your local library and Pi, and clears its default model if selected."), detail: provider.id, label: t("删除 Provider", "Delete provider"), danger: true },
        () => save("provider.remove", { providerId: provider.id }, t("Provider 已删除", "Provider deleted"))); break;
    case "sync-provider": {
      const inPi = target.checked;
      if (!inPi && state.snapshot.defaultProvider === provider.id) {
        target.checked = true;
        confirm({ title: t("取消同步到 Pi？", "Remove this provider from Pi?"), description: t("当前默认模型属于此 Provider。取消同步会清除默认模型，本地配置仍会保留。", "Your default model belongs to this provider. Removing it from Pi clears the default; the local configuration is kept."), detail: provider.id + " / " + state.snapshot.defaultModel, label: t("取消同步并清除默认", "Remove and clear default") },
          () => save("provider.sync", { providerId: provider.id, inPi }, t("已取消同步，本地配置已保留", "Removed from Pi. Local configuration kept.")));
      } else {
        await execute(() => save("provider.sync", { providerId: provider.id, inPi }, inPi ? t("Provider 已同步到 Pi", "Provider synced to Pi") : t("已取消同步，本地配置已保留", "Removed from Pi. Local configuration kept.")));
      } break;
    }
    case "new-model": editModel(); break;
    case "edit-model": state.modelId = modelId; editModel(model, false, provider); break;
    case "duplicate-model": if (model) editModel(model, true, provider); break;
    case "select-model": state.modelId = modelId; document.querySelectorAll(".model-row").forEach((row) => row.classList.toggle("selected", row.dataset.model === modelId)); break;
    case "default-model":
      if (!model) break;
      if (!provider.inPi) throw new Error(t("请先将 Provider 同步到 Pi。", "Sync the provider to Pi first."));
      await execute(() => save("model.default", { providerId: provider.id, modelId }, t("默认模型已切换为 ", "Default model changed to ") + (model.name || model.id))); break;
    case "remove-model":
      if (!model) break;
      confirm({ title: t("删除模型？", "Delete model?"), description: t("此模型将从 Provider 中删除，并同步更新 Pi。若它是默认模型，默认设置也会清除。", "This removes the model and updates Pi if synced. If selected as default, the default is also cleared."), detail: provider.id + " / " + model.id, label: t("删除模型", "Delete model"), danger: true },
        () => save("model.remove", { providerId: provider.id, modelId: model.id }, t("模型已删除", "Model deleted"))); break;
    case "import-models": if (provider) await importModels(provider); break;
    case "opencode": await importOpenCode(); break;
    case "select-all-import":
      for (const item of dialogContext.items.filter((item) => item.id.toLowerCase().includes(dialogContext.query.toLowerCase()))) dialogContext.selected.add(item.id);
      openDialog(dialogs.selectionDialog(dialogContext), dialogContext); break;
    case "clear-import": dialogContext.selected.clear(); openDialog(dialogs.selectionDialog(dialogContext), dialogContext); break;
    case "import-selection":
      if (target.checked) dialogContext.selected.add(target.value); else dialogContext.selected.delete(target.value);
      document.getElementById("import-selected-count").textContent = dialogContext.selected.size; break;
    case "import-overwrite": dialogContext.updateExisting = target.checked; break;
    case "reveal-key": {
      const input = dialog.querySelector('[name="apiKey"]');
      input.type = input.type === "password" ? "text" : "password";
      target.setAttribute("aria-pressed", input.type === "text"); break;
    }
    case "draft-inpi": {
      const warning = document.getElementById("unsync-confirmation");
      if (warning) { warning.hidden = target.checked; warning.querySelector("input").required = !target.checked; }
      break;
    }
    case "draft-reasoning": document.getElementById("thinking-fields").hidden = !target.checked; break;
    case "doctor": await loadDialog(t("配置检查", "Configuration checks"), "doctor", (result) => { state.checks = result.checks; render(); return dialogs.doctorDialog(result.checks); }); break;
    case "backups": await loadDialog(t("配置备份", "Configuration backups"), "backups.list", (result) => dialogs.backupsDialog(result.backups)); break;
    case "restore-backup": {
      const name = target.dataset.backup;
      confirm({ title: t("恢复这份备份？", "Restore this backup?"), description: t("当前的本地库、Pi 配置和 pi-switch 设置会被替换。恢复前会自动备份当前配置。", "This replaces the current provider library, Pi configuration, and pi-switch settings. Current files are backed up first."), detail: name, label: t("恢复配置", "Restore configuration") },
        () => save("backups.restore", { name }, t("配置已恢复", "Configuration restored"))); break;
    }
    case "language": await execute(() => save("settings.language", { value: target.value }, t("语言设置已保存", "Language preference saved"))); break;
    case "metadata": await execute(() => save("settings.metadata", { value: target.checked }, t("元数据设置已保存", "Metadata preference saved"))); break;
    case "auto-updates": await execute(() => save("settings.updates", { value: target.checked }, t("更新设置已保存", "Update preference saved"))); break;
    case "model-defaults": openDialog(dialogs.defaultsDialog(state.snapshot.modelDefaults), { kind: "defaults" }); break;
    case "copy-path": await copyText(target.dataset.path); break;
    case "refresh-sessions": await loadSessions(); break;
    case "select-session": state.sessionId = target.dataset.session; navigate("sessions", "session", state.sessionId); await loadPreview(); break;
    case "reload-preview": await loadPreview(); break;
    case "named-only": state.namedOnly = target.checked; render(); break;
    case "user-only": state.userOnly = target.checked; await loadPreview(); break;
    case "preview-mode": state.previewMode = target.dataset.mode; render(); break;
    case "select-message": focusMessage(target.dataset.message); break;
    case "toggle-branch":
      if (state.preview?.messages.find((message) => message.id === state.messageId)?.tree.hasChildren) {
        if (state.folded.has(state.messageId)) state.folded.delete(state.messageId); else state.folded.add(state.messageId);
        focusMessage(state.messageId);
      } break;
    case "copy-message": {
      const message = state.preview?.messages.find((item) => item.id === (target?.dataset.message ?? state.messageId));
      if (message) await copyText(message.text);
      break;
    }
    case "delete-session": {
      const session = state.sessions.find((item) => item.id === (target?.dataset.session ?? state.sessionId));
      if (!session) break;
      confirm({ title: t("删除这个会话？", "Delete this session?"), description: t("将优先移到系统回收站；如果回收站不可用，则永久删除该会话文件。", "The session is moved to the system trash when available; otherwise its file is permanently deleted."), detail: session.title + "\n" + session.id, label: t("删除会话", "Delete session"), danger: true }, async () => {
        const result = await save("sessions.delete", { id: session.id }, null);
        state.sessionId = null; state.preview = null; ++previewVersion;
        await loadSessions();
        toast(result.method === "trash" ? t("会话已移到回收站", "Session moved to trash") : t("会话文件已永久删除", "Session file permanently deleted"));
      }); break;
    }
    case "check-updates": {
      openDialog(dialogs.loadingDialog(t("检查更新", "Check for updates")));
      const version = dialogVersion;
      try {
        const result = await api("updates.check");
        if (!dialog.open || version !== dialogVersion) break;
        if (result.available) {
          confirm({ title: t("发现新版本", "Update available"), description: t("将通过 npm 全局安装最新版本。完成后需重启 pi-switch。", "The latest version will be installed globally with npm. Restart pi-switch after installation."), detail: result.current + " → " + result.latest, label: t("安装更新", "Install update") },
            () => save("updates.install", {}, t("更新已安装，请重启 pi-switch 生效", "Update installed. Restart pi-switch to apply.")));
        } else {
          openDialog(dialogs.frame({ title: t("已是最新版本", "You're up to date"), body: '<p class="confirm-description">pi-switch v' + h(result.current) + "</p>" }));
        }
      } catch (error) { if (dialog.open && version === dialogVersion) { openDialog(dialogs.frame({ title: t("更新检查失败", "Update check failed"), body: "" })); showError(error); } }
      break;
    }
  }
}

function dispatch(action, target) {
  if (state.busy && !BUSY_ACTIONS.has(action)) return;
  Promise.resolve(handleAction(action, target)).catch(showError);
}

document.addEventListener("click", (event) => {
  const target = event.target.closest("[data-action]");
  if (!target || target.disabled || target.matches('input,select')) return;
  if (target.dataset.action === "skip-to-content") event.preventDefault();
  dispatch(target.dataset.action, target);
});
document.addEventListener("change", (event) => {
  const target = event.target;
  if (target.dataset.action) dispatch(target.dataset.action, target);
});
document.addEventListener("input", (event) => {
  const fields = { "provider-search": "providerQuery", "model-search": "modelQuery", "session-search": "sessionQuery" };
  const key = fields[event.target.id];
  if (key) { state[key] = event.target.value; render(); }
  if (event.target.id === "import-search") {
    dialogContext.query = event.target.value;
    openDialog(dialogs.selectionDialog(dialogContext), dialogContext, true);
  }
  event.target.removeAttribute("aria-invalid");
});
document.addEventListener("submit", (event) => {
  const form = event.target;
  if (!form.dataset.form) return;
  event.preventDefault();
  if (!form.reportValidity()) return;
  const context = dialogContext;
  void execute(async () => {
    dialog.querySelectorAll(".field-error").forEach((element) => { element.textContent = ""; });
    dialog.querySelector("#dialog-error").textContent = "";
    switch (form.dataset.form) {
      case "provider": {
        const draft = dialogs.providerDraft(form, context.provider);
        await save("provider.save", { previousId: context.provider?.id ?? null, draft }, t("Provider 已保存", "Provider saved"));
        state.providerQuery = ""; state.providerFilter = "all";
        navigate("profiles", "provider", draft.id); break;
      }
      case "model": {
        const draft = dialogs.modelDraft(form);
        await save("model.save", { providerId: context.providerId, previousId: context.previousId, draft }, t("模型已保存", "Model saved"));
        state.modelId = draft.id; state.modelQuery = "";
        navigate("profiles", "provider", context.providerId); break;
      }
      case "defaults": await save("settings.defaults", { value: dialogs.numericDraft(form) }, t("默认参数已保存", "Default parameters saved")); break;
      case "import": await submitImport(); break;
      case "ambiguities": {
        const candidateIndices = Array.from({ length: context.count }, (_, index) => Number(form.elements["candidate-" + index].value));
        showImportResult(await api(context.action, { ...context.payload, candidateIndices })); break;
      }
    }
  });
});

function moveList(direction) {
  const active = document.activeElement;
  const selector = active.closest(".tree-list") ? ".tree-item" : active.closest(".session-list") ? ".session-option" : active.closest(".model-row") ? ".model-row" : state.page === "profiles" ? ".provider-option" : state.page === "sessions" ? ".session-option" : null;
  if (!selector) return false;
  const list = [...document.querySelectorAll(selector)];
  if (!list.length) return false;
  const current = list.findIndex((item) => item === active || item.contains(active));
  const next = list[Math.max(0, Math.min(list.length - 1, current + direction))];
  next.focus();
  if (selector === ".tree-item") focusMessage(next.dataset.message);
  if (selector === ".model-row") dispatch("select-model", next);
  if (selector === ".provider-option") dispatch("select-provider", next);
  if (selector === ".session-option") dispatch("select-session", next);
  return true;
}

function treeDirection(direction) {
  const current = state.preview?.messages.find((message) => message.id === state.messageId);
  if (!current) return;
  if (direction > 0 && state.folded.has(current.id)) { state.folded.delete(current.id); focusMessage(current.id); return; }
  const next = direction < 0 ? current.tree.parentId : state.preview.messages.find((message) => message.tree.parentId === current.id)?.id;
  if (next) focusMessage(next);
}

document.addEventListener("keydown", (event) => {
  if (event.defaultPrevented || event.isComposing || !state.snapshot) return;
  if (dialog.open || event.target.matches("input,textarea,select,[contenteditable=true]")) return;
  if (state.navOpen && mobileViewport.matches) {
    if (event.key === "Escape") { event.preventDefault(); dispatch("close-nav"); return; }
    if (event.key === "Tab") {
      const links = [...document.querySelectorAll("#sidebar a")];
      const edge = event.shiftKey ? links[0] : links.at(-1);
      if (document.activeElement === edge) {
        event.preventDefault();
        (event.shiftKey ? links.at(-1) : links[0]).focus();
      }
    }
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "c" && event.target.closest(".tree-list") && !window.getSelection().toString()) {
    event.preventDefault(); dispatch("copy-message"); return;
  }
  if (event.ctrlKey || event.metaKey || event.altKey || state.busy) return;
  const key = event.key;
  if (["1", "2", "3", "4"].includes(key)) { event.preventDefault(); navigate(Object.keys(pages)[Number(key) - 1]); return; }
  if (key === "?") { event.preventDefault(); dispatch("help"); return; }
  if (key === "/") {
    event.preventDefault();
    const id = state.page === "sessions" ? "session-search" : event.target.closest(".model-row") ? "model-search" : "provider-search";
    document.getElementById(id)?.focus(); return;
  }
  if (["ArrowDown", "ArrowUp", "j", "k"].includes(key)) {
    if (moveList(key === "ArrowDown" || key === "j" ? 1 : -1)) event.preventDefault();
    return;
  }
  if (["ArrowLeft", "ArrowRight", "h", "l"].includes(key) && ["profiles", "sessions"].includes(state.page)) {
    event.preventDefault();
    const direction = key === "ArrowRight" || key === "l" ? 1 : -1;
    if (event.target.closest(".tree-list")) treeDirection(direction);
    else if (state.page === "profiles") document.querySelector(direction > 0 ? ".model-row" : '.provider-option[aria-current="true"]')?.focus();
    else if (state.page === "sessions") document.querySelector(direction > 0 ? '.tree-item[aria-selected="true"]' : '.session-option[aria-current="true"]')?.focus();
    return;
  }
  if (key === "Escape") {
    state.navOpen = false; render(); document.querySelector(state.page === "profiles" ? '.provider-option[aria-current="true"]' : '.session-option[aria-current="true"]')?.focus(); return;
  }
  if (key === "r") { event.preventDefault(); dispatch(state.page === "sessions" ? "refresh-sessions" : "reload"); return; }
  if (key === "b") { event.preventDefault(); dispatch("backups"); return; }
  if (key === "v") {
    event.preventDefault();
    if (state.page === "sessions") { state.previewMode = state.previewMode === "tree" ? "reading" : "tree"; render(); }
    else dispatch("doctor");
    return;
  }
  if (state.page === "sessions") {
    if (key === "n") { event.preventDefault(); state.namedOnly = !state.namedOnly; render(); }
    if (key === "u") { event.preventDefault(); state.userOnly = !state.userOnly; void loadPreview(); }
    if (key === "d" || key === "Delete") { event.preventDefault(); dispatch("delete-session", event.target.closest(".session-option")); }
    if (key === " " && event.target.matches(".tree-item")) { event.preventDefault(); dispatch("toggle-branch"); }
    return;
  }
  if (state.page !== "profiles") return;
  const inModels = Boolean(event.target.closest(".model-row"));
  const actions = { n: inModels ? "new-model" : "new-provider", e: inModels ? "edit-model" : "edit-provider", c: inModels ? "duplicate-model" : "duplicate-provider", d: inModels ? "remove-model" : "remove-provider", Delete: inModels ? "remove-model" : "remove-provider", i: "import-models" };
  if (actions[key]) { event.preventDefault(); dispatch(actions[key], inModels ? event.target.closest(".model-row") : event.target.closest(".provider-option")); return; }
  if (key === " " && (event.target.matches(".model-row") || event.target.matches('.provider-option[aria-current="true"]'))) {
    event.preventDefault();
    if (inModels) dispatch("default-model", event.target.closest(".model-row"));
    else {
      const id = event.target.closest(".provider-option").dataset.provider;
      const provider = state.snapshot.providers.find((item) => item.id === id);
      dispatch("sync-provider", { checked: !provider.inPi, dataset: { provider: id } });
    }
  }
});

function focusSelector(element) {
  if (!element) return null;
  if (element.id) return "#" + CSS.escape(element.id);
  if (element.matches(".model-row")) return '.model-row[data-model="' + CSS.escape(element.dataset.model) + '"]';
  if (element.dataset.action) {
    let selector = '[data-action="' + CSS.escape(element.dataset.action) + '"]';
    for (const key of ["provider", "model", "session", "message", "value", "mode"]) {
      if (element.dataset[key]) selector += '[data-' + key + '="' + CSS.escape(element.dataset[key]) + '"]';
    }
    return selector;
  }
  if (element.matches("a[href]")) return 'a[href="' + CSS.escape(element.getAttribute("href")) + '"]';
  return null;
}

mobileViewport.addEventListener("change", () => { state.navOpen = false; render(); });
window.addEventListener("hashchange", route);
route();
try {
  applySnapshot(await api("snapshot"));
  render();
  void loadSessions();
} catch (error) {
  app.innerHTML = '<div class="boot-state">' + emptyState(t("无法读取本地配置", "Could not load configuration"), error.message, "warning", '<button class="btn primary" data-action="reload">' + icon("refresh") + t("重新连接", "Reconnect") + "</button>") + "</div>";
}
