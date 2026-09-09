import { h, t, icon, iconButton, emptyState, listSearch, date } from "./ui.js";
import { pageHeader, contextHelp } from "./shell.js";
import { canFoldBranch, isBranchPoint, isBranchStart, messagePath, sessionTree, visibleTreeNodes, VISIBLE_TREE_LANES } from "./session-tree.js";

const MESSAGE_ROLES = {
  user: { zh: "用户", en: "User", icon: "user", tone: "blue" },
  assistant: { zh: "Pi", en: "Pi", icon: "terminal", tone: "mauve" },
  branchSummary: { zh: "分支摘要", en: "Branch summary", icon: "branch", tone: "teal" },
  compaction: { zh: "压缩摘要", en: "Compaction", icon: "box", tone: "peach" },
  custom: { zh: "自定义消息", en: "Custom message", icon: "file", tone: "muted" },
};

function messageRole(message) {
  const role = Object.hasOwn(MESSAGE_ROLES, message.role) ? MESSAGE_ROLES[message.role] : { zh: message.role, en: message.role, icon: "file", tone: "muted" };
  return { ...role, name: t(role.zh, role.en) };
}

export function visibleSessions(state) {
  const query = state.sessionQuery.toLowerCase();
  return state.sessions.filter((session) => (!state.namedOnly || session.name)
    && [session.title, session.cwd, session.searchText].some((value) => value?.toLowerCase().includes(query)));
}

function sessionList(state) {
  const sessions = visibleSessions(state);
  const groups = groupSessions(sessions);
  const search = listSearch({ scope: "session", query: state.sessionQuery, open: state.sessionSearchOpen, label: t("搜索会话", "Search sessions"), placeholder: t("搜索会话、工作目录…", "Search sessions, workspaces…") });
  return `<section class="panel session-sidebar" aria-busy="${state.sessionsLoading}"><div class="panel-header"><h2>${t("本地会话", "Local sessions")}<span class="count">${sessions.length}</span></h2><div class="inline-actions">${search.button}${iconButton("refresh-sessions", "refresh", t("重新扫描会话", "Rescan sessions"))}</div></div>
    ${search.field}
    <div class="provider-filters"><label class="checkbox-label"><input type="checkbox" data-action="named-only" ${state.namedOnly ? "checked" : ""}>${t("仅显示已命名会话", "Only named sessions")}</label></div>
    <div class="session-list" data-session-scroll="list">
      ${sessions.length ? Array.from(groups, ([cwd, entries]) => `<div class="session-group" title="${h(cwd)}">${icon("folder")}<span>${h(cwd || t("未知工作目录", "Unknown workspace"))}</span></div>
        ${entries.map((session) => `<button class="session-option ${session.name ? "named" : ""}" data-action="select-session" data-session="${h(session.id)}" aria-current="${state.sessionId === session.id}">
          <div class="item-title" title="${h(session.title)}">${h(session.title)}</div><div class="session-meta"><span>${icon("messages")}${session.messageCount}</span><span>${date(session.modifiedAt)}</span></div>
        </button>`).join("")}`).join("") : emptyState(t("没有找到会话", "No sessions found"), state.sessionQuery || state.namedOnly ? t("调整搜索条件，再试一次。", "Try changing your filters.") : t("使用 Pi 开始对话后，会话会出现在这里。", "Your conversations will appear here after using Pi."), "messages")}
    </div></section>`;
}

function groupSessions(sessions) {
  const groups = new Map();
  for (const session of sessions) {
    if (!groups.has(session.cwd)) groups.set(session.cwd, []);
    groups.get(session.cwd).push(session);
  }
  return groups;
}

function foldLabel(node, collapsed) {
  return collapsed ? t("展开此分支的 " + node.descendants + " 条后续消息", "Expand " + node.descendants + " following messages in this branch")
    : t("折叠此分支的 " + node.descendants + " 条后续消息", "Collapse " + node.descendants + " following messages in this branch");
}

function branchLabel(node) {
  return isBranchStart(node) ? t("分支 " + node.position + "/" + node.siblings, "Branch " + node.position + "/" + node.siblings) : "";
}

function branchDescription(node, tree) {
  const label = branchLabel(node);
  if (!label) return "";
  const parentNumber = tree.nodes.get(node.parentId).index + 1;
  return label + t(" · 接续 #" + parentNumber, " · From #" + parentNumber);
}

function messageHeader(node, state, view) {
  const { message } = node;
  const role = messageRole(message);
  const collapsed = state.folded.has(node.id);
  const branch = branchDescription(node, sessionTree(state.preview));
  const fold = view === "reading" && canFoldBranch(node)
    ? iconButton("toggle-branch", collapsed ? "chevron" : "chevronDown", foldLabel(node, collapsed), `data-message="${h(node.id)}" data-view="reading" aria-expanded="${!collapsed}"`) : "";
  return `<div class="message-header"><span class="avatar ${role.tone}">${icon(role.icon)}</span><strong>${h(role.name)}</strong><span class="message-number">#${node.index + 1}</span>${branch ? `<span class="badge">${h(branch)}</span>` : ""}${message.label ? `<span class="badge message-label" title="${h(message.label)}">${h(message.label)}</span>` : ""}${node.id === state.preview.activeMessageId ? `<span class="badge">${t("当前节点", "Current node")}</span>` : ""}<span class="message-actions">${fold}${iconButton("copy-message", "copy", t("复制消息", "Copy message"), `data-message="${h(node.id)}" data-view="${view}"`)}</span></div>`;
}

function treeIndent(node, next) {
  const depth = Math.min(node.lane, VISIBLE_TREE_LANES);
  if (!depth) return "";
  const continuingDepth = next ? Math.min(next.lane - Number(isBranchStart(next)), VISIBLE_TREE_LANES) : 0;
  const guides = Array.from({ length: depth }, (_, index) => {
    const level = index + 1;
    const start = isBranchStart(node) && level === node.lane;
    const end = level > continuingDepth;
    return `<span class="tree-indent-guide${start ? " starts-branch" : ""}${end ? " ends-branch" : ""}"></span>`;
  });
  return `<span class="tree-indent" aria-hidden="true">${guides.join("")}</span>`;
}

function treeRow(node, state, next) {
  const role = messageRole(node.message);
  const collapsed = state.folded.has(node.id);
  const selected = node.id === state.messageId;
  const canFold = canFoldBranch(node);
  const branch = branchDescription(node, sessionTree(state.preview));
  const description = [role.name, "#" + (node.index + 1), branch, node.summary].filter(Boolean).join(" · ");
  const expander = canFold
    ? `<button class="tree-expander" type="button" tabindex="-1" data-action="toggle-branch" data-message="${h(node.id)}" data-view="tree" aria-expanded="${!collapsed}" aria-label="${h(foldLabel(node, collapsed))}" title="${h(foldLabel(node, collapsed))}">${icon(collapsed ? "chevron" : "chevronDown")}</button>`
    : '<span class="tree-expander-space" aria-hidden="true"></span>';
  return `<div class="tree-item message-node" role="treeitem" aria-level="${node.depth + 1}" aria-posinset="${node.position}" aria-setsize="${node.siblings}" aria-selected="${selected}" ${canFold ? `aria-expanded="${!collapsed}"` : ""} tabindex="${selected ? "0" : "-1"}" data-action="select-message" data-message="${h(node.id)}" aria-label="${h(description)}">
    ${treeIndent(node, next)}${expander}<div class="tree-row-content">
      ${branch ? `<div class="tree-row-branch" title="${h(branch)}">${h(branch)}</div>` : ""}
      <div class="tree-row-meta"><span class="tree-role ${role.tone}">${icon(role.icon)}${h(role.name)}</span><span class="tree-node-number">#${node.index + 1}</span>${isBranchPoint(node) ? `<span class="tree-fork-count">${node.children.length} ${t("分支", "branches")}</span>` : ""}${node.id === state.preview.activeMessageId ? `<span class="tree-current">${t("当前", "Current")}</span>` : ""}</div>
      <div class="tree-text" title="${h(node.summary)}">${h(node.summary || t("无文本内容", "No text content"))}</div>
      <div class="tree-row-tags">${collapsed ? `<span class="tree-fold-count">+${node.descendants} ${t("条已折叠", "collapsed")}</span>` : ""}${node.lane > VISIBLE_TREE_LANES ? `<span>${t("分支层级 ", "Branch depth ")}${node.lane}</span>` : ""}${node.message.label ? `<span class="tree-message-label" title="${h(node.message.label)}">${h(node.message.label)}</span>` : ""}</div>
    </div>
  </div>`;
}

export function messageReader(state) {
  const node = sessionTree(state.preview).nodes.get(state.messageId);
  if (!node) return `<section id="session-message-reader" class="message-reader">${emptyState(t("选择一条消息", "Select a message"), t("点击树中的消息，在这里阅读内容。", "Select a tree node to read its content here."), "messages")}</section>`;
  const path = messagePath(state.preview, node.id);
  const crumbs = path.slice(-4).map((entry) => `<button class="reader-crumb" data-action="select-message" data-message="${h(entry.id)}" title="${h(entry.summary)}">${h(branchLabel(entry) || t("起点", "Start"))}</button>`).join('<span aria-hidden="true">/</span>');
  const scrollKey = JSON.stringify([state.preview.id, node.id]);
  return `<section id="session-message-reader" class="message-reader" data-message-context="${h(node.id)}" aria-label="${t("消息内容", "Message content")}"><div class="reader-heading"><div class="reader-breadcrumb" aria-label="${t("分支路径", "Branch path")}">${path.length > 4 ? '<span aria-hidden="true">… /</span>' : ""}${crumbs}</div>${messageHeader(node, state, "reader")}</div><div class="reader-body" data-session-scroll="message" data-scroll-key="${h(scrollKey)}"><div class="markdown">${node.message.html}</div></div></section>`;
}

function previewNotice(state) {
  if (state.previewError) return `<div class="panel-body preview-notice"><div class="banner error" role="alert">${icon("warning")}<div>${h(state.previewError)}${state.preview ? `<p>${t("仍显示上次成功读取的内容。", "Showing the last successfully loaded content.")}</p>` : ""}</div></div><button class="btn" data-action="reload-preview">${icon("refresh")}${t("重试", "Retry")}</button></div>`;
  if (state.previewLoading) return `<div class="${state.preview ? "preview-notice" : "loading-state"}" role="status"><span class="spinner"></span>${state.preview ? t("正在刷新会话…", "Refreshing conversation…") : t("正在读取会话…", "Loading conversation…")}</div>`;
  return "";
}

function previewContent(state) {
  const notice = previewNotice(state);
  const preview = state.preview;
  if (!preview) return notice;
  if (!preview.messages.length) return notice + emptyState(t("没有可显示的消息", "No messages to display"), t("该会话没有符合当前筛选条件的文本消息。", "This session has no text messages matching the current filter."), "messages");
  const tree = sessionTree(preview);
  const nodes = visibleTreeNodes(preview, state.folded);
  if (state.previewMode === "reading") {
    return notice + `<div class="reading-view" data-session-scroll="reading" data-session-id="${h(preview.id)}" aria-label="${t("会话阅读", "Conversation reading")}">` + nodes.map((node) => `<article class="reading-message message-node ${node.message.tree.activePath ? "on-active-path" : ""}" data-depth="${Math.min(node.lane, VISIBLE_TREE_LANES)}" tabindex="${node.id === state.messageId ? "0" : "-1"}" aria-current="${node.id === state.messageId}" aria-label="${h(messageRole(node.message).name + ": " + node.summary)}" data-action="select-message" data-message="${h(node.id)}">${messageHeader(node, state, "reading")}<div class="markdown">${node.message.html}</div>${state.folded.has(node.id) ? `<div class="reading-fold-note">${icon("branch")}${t("已折叠 ", "Collapsed ")}${node.descendants} ${t("条后续消息", "following messages")}</div>` : ""}</article>`).join("") + "</div>";
  }
  return notice + `<div class="tree-workspace"><section class="tree-panel" aria-label="${t("消息树", "Message tree")}"><div class="tree-panel-heading"><strong>${t("消息树", "Message tree")}</strong><span>${nodes.length} / ${tree.ordered.length}</span></div><div class="tree-list" data-session-scroll="tree" data-session-id="${h(preview.id)}" role="tree" aria-label="${t("会话分支", "Conversation tree")}">${nodes.map((node, index) => treeRow(node, state, nodes[index + 1])).join("")}</div></section>${messageReader(state)}</div>`;
}

function sessionPreview(state) {
  const session = state.sessions.find((entry) => entry.id === state.sessionId);
  if (state.sessionId && !session) {
    if (state.sessionsLoading) return `<section class="panel"><div class="loading-state" role="status"><span class="spinner"></span>${t("正在查找会话…", "Looking for the session…")}</div></section>`;
    return `<section class="panel"><div class="panel-body"><div class="banner error" role="alert">${icon("warning")}<span>${state.sessionsError ? t("会话列表读取失败，请重新扫描。", "Could not load the session list. Rescan to try again.") : t("会话不存在或已删除。", "This session does not exist or has been deleted.")}</span></div><p class="missing-session-id">${h(state.sessionId)}</p><div class="inline-actions"><button class="btn" data-action="refresh-sessions">${icon("refresh")}${t("重新扫描", "Rescan sessions")}</button><button class="btn" data-action="clear-session">${t("返回列表", "Back to list")}</button></div></div></section>`;
  }
  if (!session) return `<section class="panel">${emptyState(t("回到对话发生的地方", "Pick up the thread"), t("选择左侧会话，浏览完整消息和分支历史。", "Select a session to explore its messages and branches."), "messages")}</section>`;
  const tree = state.preview && sessionTree(state.preview);
  const canFold = tree?.ordered.some(canFoldBranch);
  return `<section class="panel session-preview" aria-busy="${state.previewLoading}"><div class="panel-header"><div><h2>${h(session.title)}</h2><div class="item-subtitle" title="${h(session.cwd)}">${h(session.cwd)}</div></div>${iconButton("delete-session", "trash", t("删除会话", "Delete session"), "", "danger")}</div>
    <div class="preview-toolbar"><div class="segments" aria-label="${t("预览模式", "Preview mode")}"><button data-action="preview-mode" data-mode="tree" aria-pressed="${state.previewMode === "tree"}">${icon("branch")}${t("树状", "Tree")}</button><button data-action="preview-mode" data-mode="reading" aria-pressed="${state.previewMode === "reading"}">${icon("book")}${t("阅读", "Read")}</button></div>
      <div class="preview-controls"><div class="preview-tree-actions" role="group" aria-label="${t("分支操作", "Branch actions")}">
        ${iconButton("collapse-tree", "chevronsUp", t("全部折叠", "Collapse all"), canFold ? "" : "disabled")}
        ${iconButton("expand-tree", "chevronsDown", t("全部展开", "Expand all"), state.folded.size ? "" : "disabled")}
        ${iconButton("active-message", "target", t("定位当前", "Locate current"), state.preview?.activeMessageId ? "" : "disabled")}
      </div><label class="checkbox-label"><input type="checkbox" data-action="user-only" ${state.userOnly ? "checked" : ""}>${t("仅用户", "User only")}</label></div>
    </div>
    ${previewContent(state)}
    <div class="models-foot"><span>${session.messageCount} ${t("条消息", "messages")}${state.preview ? ' <span class="subtle-divider">·</span> ' + state.preview.branchPoints + " " + t("个分叉", "branches") : ""}</span><span>${date(session.modifiedAt)}</span></div>
  </section>`;
}

export function sessions(state) {
  const header = pageHeader(t("会话记录", "Session history"), "", `<button class="btn" data-action="refresh-sessions">${state.sessionsLoading ? '<span class="spinner"></span>' : icon("refresh")}${state.sessionsLoading ? t("正在扫描…", "Scanning…") : t("重新扫描", "Rescan sessions")}</button>`);
  const error = state.sessionsError ? `<div class="banner error" role="alert">${icon("warning")}<span>${h(state.sessionsError)}</span></div>` : "";
  if (!state.sessionsLoaded) return header + `<div class="loading-state" role="status"><span class="spinner"></span>${t("正在扫描会话…", "Scanning sessions…")}</div>`;
  if (state.sessionsError && !state.sessions.length) return header + error;
  return header + error + `<div class="sessions-layout">${sessionList(state)}${sessionPreview(state)}</div>`
    + contextHelp([["↑ ↓", t("浏览消息", "Browse messages")], ["← →", t("折叠 / 展开与父子导航", "Fold / expand and navigate")], ["Space", t("折叠 / 展开分支", "Toggle branches")], ["Home / End", t("首条 / 末条", "First / last")], ["v", t("切换阅读视图", "Toggle reading")], ["Ctrl C", t("复制当前消息", "Copy message")], ["/", t("筛选会话", "Filter sessions")]]);
}
