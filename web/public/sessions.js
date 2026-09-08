import { h, t, icon, iconButton, emptyState, searchInput, date } from "./ui.js";
import { pageHeader, contextHelp } from "./shell.js";

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

function sessionList(state) {
  const sessions = visibleSessions(state);
  const groups = groupSessions(sessions);
  return `<section class="panel session-sidebar"><div class="panel-header"><h2>${t("本地会话", "Local sessions")}<span class="count">${sessions.length}</span></h2>${iconButton("refresh-sessions", "refresh", t("重新扫描会话", "Rescan sessions"))}</div>
    <div class="provider-filters">${searchInput("session-search", state.sessionQuery, t("搜索会话、工作目录…", "Search sessions, workspaces…"))}<label class="checkbox-label"><input type="checkbox" data-action="named-only" ${state.namedOnly ? "checked" : ""}>${t("仅显示已命名会话", "Only named sessions")}</label></div>
    <div class="session-list">
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

function messageContent(message) {
  return `<div class="message-header"><span class="avatar ${message.role === "user" ? "blue" : "mauve"}">${icon(message.role === "user" ? "user" : "terminal")}</span><span>${message.role === "user" ? t("你", "You") : "Pi"}</span>${message.label ? `<span class="badge">${h(message.label)}</span>` : ""}${iconButton("copy-message", "copy", t("复制消息", "Copy message"), 'data-message="' + h(message.id) + '"')}</div>
    <div class="markdown">${message.html}</div>`;
}

function previewContent(state) {
  if (state.previewLoading) return `<div class="loading-state" role="status"><span class="spinner"></span>${t("正在读取会话…", "Loading conversation…")}</div>`;
  if (state.previewError) return `<div class="panel-body"><div class="banner error" role="alert">${icon("warning")}<span>${h(state.previewError)}</span></div><button class="btn" data-action="reload-preview">${icon("refresh")}${t("重试", "Retry")}</button></div>`;
  const preview = state.preview;
  if (!preview?.messages.length) return emptyState(t("没有可显示的消息", "No messages to display"), t("该会话没有符合当前筛选条件的文本消息。", "This session has no text messages matching the current filter."), "messages");
  const active = preview.messages.find((message) => message.id === state.messageId);
  if (state.previewMode === "reading") {
    return '<div class="reading-view">' + preview.messages.map((message) => '<article class="reading-message" data-message-id="' + h(message.id) + '">' + messageContent(message) + "</article>").join("") + "</div>";
  }
  return `<div class="tree-list" role="tree" aria-label="${t("会话分支", "Conversation tree")}">
    ${visibleMessages(preview, state.folded).map((message) => {
      const connector = "  ".repeat(Math.min(message.tree.indent, 12)) + (message.tree.showConnector ? (message.tree.isLast ? "└─ " : "├─ ") : "");
      return `<button class="tree-item" role="treeitem" aria-level="${message.tree.level + 1}" aria-selected="${message.id === state.messageId}" ${message.tree.hasChildren ? 'aria-expanded="' + !state.folded.has(message.id) + '"' : ""} tabindex="${message.id === state.messageId ? "0" : "-1"}" data-action="select-message" data-message="${h(message.id)}">
        <span class="tree-connector" aria-hidden="true">${connector}</span>${message.tree.hasChildren ? icon(state.folded.has(message.id) ? "chevron" : "chevronDown") : icon(message.role === "user" ? "user" : "terminal")}<span class="tree-role ${message.role === "user" ? "user" : ""}">${message.role === "user" ? "You" : "Pi"}</span><span class="tree-text">${h(message.text.replace(/\s+/g, " ").slice(0, 240))}</span>${message.tree.activePath ? '<span class="tree-active" title="' + t("当前活动分支", "Active branch") + '"></span>' : ""}
      </button>`;
    }).join("")}
  </div>${active ? `<article class="message-preview">${messageContent(active)}</article>` : ""}`;
}

function sessionPreview(state) {
  const session = state.sessions.find((entry) => entry.id === state.sessionId);
  if (!session) return `<section class="panel">${emptyState(t("回到对话发生的地方", "Pick up the thread"), t("选择左侧会话，浏览完整消息和分支历史。", "Select a session to explore its messages and branches."), "messages")}</section>`;
  const active = state.preview?.messages.find((message) => message.id === state.messageId);
  return `<section class="panel session-preview"><div class="panel-header"><div><h2>${h(session.title)}</h2><div class="item-subtitle" title="${h(session.cwd)}">${h(session.cwd)}</div></div>${iconButton("delete-session", "trash", t("删除会话", "Delete session"), "", "danger")}</div>
    <div class="preview-toolbar"><div class="segments" aria-label="${t("预览模式", "Preview mode")}"><button data-action="preview-mode" data-mode="tree" aria-pressed="${state.previewMode === "tree"}">${icon("branch")}Tree</button><button data-action="preview-mode" data-mode="reading" aria-pressed="${state.previewMode === "reading"}">${icon("book")}${t("阅读", "Read")}</button></div><div class="inline-actions"><label class="checkbox-label"><input type="checkbox" data-action="user-only" ${state.userOnly ? "checked" : ""}>${t("仅用户", "User only")}</label>${state.previewMode === "tree" ? iconButton("toggle-branch", "branch", t("折叠 / 展开当前分支", "Collapse / expand selected branch"), active?.tree.hasChildren ? "" : "disabled") : ""}</div></div>
    ${previewContent(state)}
    <div class="models-foot"><span>${session.messageCount} ${t("条消息", "messages")}${state.preview ? ' <span class="subtle-divider">·</span> ' + state.preview.branchPoints + " " + t("个分叉", "branches") : ""}</span><span>${date(session.modifiedAt)}</span></div>
  </section>`;
}

export function sessions(state) {
  const header = pageHeader("CONVERSATIONS & BRANCHES", t("会话记录", "Session history"), t("按工作目录整理对话，沿着每一条分支回顾思路。", "Conversations, organized by workspace. Follow every branch of thought."), `<button class="btn" data-action="refresh-sessions">${icon("refresh")}${t("重新扫描", "Rescan sessions")}</button>`);
  if (state.sessionsError) return header + `<div class="banner error" role="alert">${icon("warning")}<span>${h(state.sessionsError)}</span></div>`;
  if (!state.sessionsLoaded) return header + `<div class="loading-state" role="status"><span class="spinner"></span>${t("正在扫描会话…", "Scanning sessions…")}</div>`;
  return header + `<div class="sessions-layout">${sessionList(state)}${sessionPreview(state)}</div>`
    + contextHelp([["↑ ↓", t("浏览消息", "Browse messages")], ["← →", t("父级 / 子级", "Parent / child")], ["Space", t("折叠分支", "Toggle branch")], ["v", t("切换阅读视图", "Toggle reading")], ["Ctrl C", t("复制当前消息", "Copy message")], ["/", t("筛选会话", "Filter sessions")]]);
}
