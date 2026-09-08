import { h, t, icon, iconButton, emptyState, searchInput, date } from "./ui.js";
import { pageHeader, contextHelp } from "./shell.js";

const MAX_TREE_INDENT = 12;

export function visibleSessions(state) {
  const query = state.sessionQuery.toLowerCase();
  return state.sessions.filter((session) => (!state.namedOnly || session.name)
    && [session.title, session.cwd, session.searchText].some((value) => value?.toLowerCase().includes(query)));
}

export function visibleMessages(preview, folded) {
  if (!preview) return [];
  const messages = new Map(preview.messages.map((message) => [message.id, message]));
  return preview.messages.filter((message) => {
    let parent = messages.get(message.tree.parentId);
    const visited = new Set();
    while (parent && !visited.has(parent.id)) {
      if (folded.has(parent.id)) return false;
      visited.add(parent.id);
      parent = messages.get(parent.tree.parentId);
    }
    return true;
  });
}

export function reconcilePreview(state, preview) {
  const current = new Map(preview.messages.map((message) => [message.id, message]));
  const folded = new Set([...state.folded].filter((id) => current.get(id)?.tree.hasChildren));
  const visible = visibleMessages(preview, folded);
  const visibleIds = new Set(visible.map((message) => message.id));
  const ancestry = new Map([...(state.preview?.messages ?? []), ...preview.messages].map((message) => [message.id, message]));
  function visibleAncestor(id) {
    const visited = new Set();
    while (id && !visibleIds.has(id) && !visited.has(id)) {
      visited.add(id);
      id = ancestry.get(id)?.tree.parentId;
    }
    return visibleIds.has(id) ? id : null;
  }
  return {
    preview, folded,
    messageId: visibleAncestor(state.messageId) ?? visibleAncestor(preview.activeMessageId) ?? visible[0]?.id ?? null,
  };
}

function sessionList(state) {
  const sessions = visibleSessions(state);
  const groups = groupSessions(sessions);
  return `<section class="panel session-sidebar" aria-busy="${state.sessionsLoading}"><div class="panel-header"><h2>${t("本地会话", "Local sessions")}<span class="count">${sessions.length}</span></h2>${iconButton("refresh-sessions", "refresh", t("重新扫描会话", "Rescan sessions"))}</div>
    <div class="provider-filters">${searchInput("session-search", state.sessionQuery, t("搜索会话、工作目录…", "Search sessions, workspaces…"))}<label class="checkbox-label"><input type="checkbox" data-action="named-only" ${state.namedOnly ? "checked" : ""}>${t("仅显示已命名会话", "Only named sessions")}</label></div>
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

function messageContent(message, folded) {
  const collapsed = folded?.has(message.id);
  const branchControl = folded && message.tree.hasChildren
    ? iconButton("toggle-branch", collapsed ? "chevron" : "chevronDown", collapsed ? t("展开分支", "Expand branch") : t("折叠分支", "Collapse branch"), 'data-message="' + h(message.id) + '" aria-expanded="' + !collapsed + '"') : "";
  return `<div class="message-header"><span class="avatar ${message.role === "user" ? "blue" : "mauve"}">${icon(message.role === "user" ? "user" : "terminal")}</span><span>${message.role === "user" ? t("你", "You") : "Pi"}</span>${message.label ? `<span class="badge">${h(message.label)}</span>` : ""}${folded && message.tree.activePath ? `<span class="badge active-path">${t("活动分支", "Active branch")}</span>` : ""}${collapsed ? `<span class="badge">${t("已折叠", "Collapsed")}</span>` : ""}<span class="message-actions">${branchControl}${iconButton("copy-message", "copy", t("复制消息", "Copy message"), 'data-message="' + h(message.id) + '"')}</span></div>
    <div class="markdown">${message.html}</div>`;
}

function treePrefix(tree) {
  const start = Math.max(0, tree.indent - MAX_TREE_INDENT);
  let prefix = start ? "… " : "";
  for (let level = start; level < tree.indent; level++) {
    if (tree.showConnector && level === tree.indent - 1) prefix += tree.isLast ? "└─ " : "├─ ";
    else prefix += tree.gutters.some((gutter) => gutter.position === level && gutter.show) ? "│  " : "   ";
  }
  return prefix;
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
  const active = preview.messages.find((message) => message.id === state.messageId);
  const messages = visibleMessages(preview, state.folded);
  if (state.previewMode === "reading") {
    return notice + `<div class="reading-view" data-session-scroll="reading" data-session-id="${h(preview.id)}" aria-label="${t("会话阅读", "Conversation reading")}">` + messages.map((message) => `<article class="reading-message message-node" tabindex="${message.id === state.messageId ? "0" : "-1"}" aria-current="${message.id === state.messageId}" aria-label="${h((message.role === "user" ? t("你", "You") : "Pi") + ": " + message.text.slice(0, 100))}" data-action="select-message" data-message="${h(message.id)}"><span class="reading-tree tree-prefix" aria-hidden="true"><span class="tree-connector">${treePrefix(message.tree)}</span></span><div class="reading-body">${messageContent(message, state.folded)}</div></article>`).join("") + "</div>";
  }
  return notice + `<div class="tree-list" data-session-scroll="tree" data-session-id="${h(preview.id)}" role="tree" aria-label="${t("会话分支", "Conversation tree")}">
    ${messages.map((message) => {
      return `<button class="tree-item message-node" role="treeitem" aria-level="${message.tree.level + 1}" aria-selected="${message.id === state.messageId}" ${message.tree.hasChildren ? 'aria-expanded="' + !state.folded.has(message.id) + '"' : ""} tabindex="${message.id === state.messageId ? "0" : "-1"}" data-action="select-message" data-message="${h(message.id)}">
        <span class="tree-prefix" aria-hidden="true"><span class="tree-connector">${treePrefix(message.tree)}</span></span>${message.tree.hasChildren ? icon(state.folded.has(message.id) ? "chevron" : "chevronDown") : icon(message.role === "user" ? "user" : "terminal")}<span class="tree-role ${message.role === "user" ? "user" : ""}">${message.role === "user" ? "You" : "Pi"}</span><span class="tree-text">${h(message.text.replace(/\s+/g, " ").slice(0, 240))}</span>${message.tree.activePath ? '<span class="tree-active" title="' + t("当前活动分支", "Active branch") + '"></span>' : ""}
      </button>`;
    }).join("")}
  </div>${active ? `<article class="message-preview">${messageContent(active)}</article>` : ""}`;
}

function sessionPreview(state) {
  const session = state.sessions.find((entry) => entry.id === state.sessionId);
  if (state.sessionId && !session) {
    if (state.sessionsLoading) return `<section class="panel"><div class="loading-state" role="status"><span class="spinner"></span>${t("正在查找会话…", "Looking for the session…")}</div></section>`;
    return `<section class="panel"><div class="panel-body"><div class="banner error" role="alert">${icon("warning")}<span>${state.sessionsError ? t("会话列表读取失败，请重新扫描。", "Could not load the session list. Rescan to try again.") : t("会话不存在或已删除。", "This session does not exist or has been deleted.")}</span></div><p class="missing-session-id">${h(state.sessionId)}</p><div class="inline-actions"><button class="btn" data-action="refresh-sessions">${icon("refresh")}${t("重新扫描", "Rescan sessions")}</button><button class="btn" data-action="clear-session">${t("返回列表", "Back to list")}</button></div></div></section>`;
  }
  if (!session) return `<section class="panel">${emptyState(t("回到对话发生的地方", "Pick up the thread"), t("选择左侧会话，浏览完整消息和分支历史。", "Select a session to explore its messages and branches."), "messages")}</section>`;
  const active = state.preview?.messages.find((message) => message.id === state.messageId);
  return `<section class="panel session-preview" aria-busy="${state.previewLoading}"><div class="panel-header"><div><h2>${h(session.title)}</h2><div class="item-subtitle" title="${h(session.cwd)}">${h(session.cwd)}</div></div>${iconButton("delete-session", "trash", t("删除会话", "Delete session"), "", "danger")}</div>
    <div class="preview-toolbar"><div class="segments" aria-label="${t("预览模式", "Preview mode")}"><button data-action="preview-mode" data-mode="tree" aria-pressed="${state.previewMode === "tree"}">${icon("branch")}Tree</button><button data-action="preview-mode" data-mode="reading" aria-pressed="${state.previewMode === "reading"}">${icon("book")}${t("阅读", "Read")}</button></div><div class="inline-actions"><label class="checkbox-label"><input type="checkbox" data-action="user-only" ${state.userOnly ? "checked" : ""}>${t("仅用户", "User only")}</label>${iconButton("toggle-branch", "branch", t("折叠 / 展开当前分支", "Collapse / expand selected branch"), active?.tree.hasChildren ? 'aria-expanded="' + !state.folded.has(active.id) + '"' : "disabled")}</div></div>
    ${previewContent(state)}
    <div class="models-foot"><span>${session.messageCount} ${t("条消息", "messages")}${state.preview ? ' <span class="subtle-divider">·</span> ' + state.preview.branchPoints + " " + t("个分叉", "branches") : ""}</span><span>${date(session.modifiedAt)}</span></div>
  </section>`;
}

export function sessions(state) {
  const header = pageHeader("CONVERSATIONS & BRANCHES", t("会话记录", "Session history"), t("按工作目录整理对话，沿着每一条分支回顾思路。", "Conversations, organized by workspace. Follow every branch of thought."), `<button class="btn" data-action="refresh-sessions">${state.sessionsLoading ? '<span class="spinner"></span>' : icon("refresh")}${state.sessionsLoading ? t("正在扫描…", "Scanning…") : t("重新扫描", "Rescan sessions")}</button>`);
  const error = state.sessionsError ? `<div class="banner error" role="alert">${icon("warning")}<span>${h(state.sessionsError)}</span></div>` : "";
  if (!state.sessionsLoaded) return header + `<div class="loading-state" role="status"><span class="spinner"></span>${t("正在扫描会话…", "Scanning sessions…")}</div>`;
  if (state.sessionsError && !state.sessions.length) return header + error;
  return header + error + `<div class="sessions-layout">${sessionList(state)}${sessionPreview(state)}</div>`
    + contextHelp([["↑ ↓", t("浏览消息", "Browse messages")], ["← →", t("父级 / 子级", "Parent / child")], ["Space", t("折叠分支", "Toggle branch")], ["v", t("切换阅读视图", "Toggle reading")], ["Ctrl C", t("复制当前消息", "Copy message")], ["/", t("筛选会话", "Filter sessions")]]);
}
