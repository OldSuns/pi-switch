const indexes = new WeakMap();
export const VISIBLE_TREE_LANES = 4;

export function isBranchPoint(node) {
  return node?.children.length > 1;
}

export function isBranchStart(node) {
  return node?.parentId != null && node.siblings > 1;
}

export function canFoldBranch(node) {
  return isBranchStart(node) && node.children.length > 0;
}

function summary(text) {
  const line = text.split(/\r?\n/).find((line) => line.trim() && !line.trim().startsWith("```"));
  return (line ?? text).trim().replace(/^#{1,6}\s+/, "").replace(/^[-*]\s+/, "").replace(/\s+/g, " ").slice(0, 220);
}

export function sessionTree(preview) {
  if (indexes.has(preview)) return indexes.get(preview);
  const nodes = new Map(preview.messages.map((message, index) => [message.id, {
    id: message.id, message, index, parentId: message.tree.parentId, children: [],
    depth: 0, lane: 0, position: 1, siblings: 1, descendants: 0,
    summary: summary(message.text),
  }]));
  if (nodes.size !== preview.messages.length) throw new Error("Session tree contains duplicate message IDs.");
  const roots = [];
  for (const node of nodes.values()) {
    if (node.parentId == null) roots.push(node.id);
    else {
      const parent = nodes.get(node.parentId);
      if (!parent) throw new Error("Session tree references a missing parent: " + node.parentId);
      parent.children.push(node.id);
    }
  }
  const ordered = [];
  const pending = roots.toReversed();
  roots.forEach((id, index) => { nodes.get(id).position = index + 1; nodes.get(id).siblings = roots.length; });
  while (pending.length) {
    const node = nodes.get(pending.pop());
    ordered.push(node);
    node.children.forEach((id, index) => {
      const child = nodes.get(id);
      child.depth = node.depth + 1;
      // Only forks add visual indentation; a continuous conversation stays aligned.
      child.lane = node.lane + Number(isBranchPoint(node));
      child.position = index + 1;
      child.siblings = node.children.length;
    });
    for (let index = node.children.length - 1; index >= 0; index--) pending.push(node.children[index]);
  }
  if (ordered.length !== nodes.size) throw new Error("Session tree contains a parent cycle.");
  for (const node of ordered.toReversed()) {
    node.descendants = node.children.reduce((count, id) => count + 1 + nodes.get(id).descendants, 0);
  }
  const tree = { nodes, roots, ordered };
  // Preview responses are immutable; a new response receives its own index.
  indexes.set(preview, tree);
  return tree;
}

export function visibleTreeNodes(preview, folded) {
  if (!preview) return [];
  const tree = sessionTree(preview);
  const visible = [];
  const pending = tree.roots.toReversed();
  while (pending.length) {
    const node = tree.nodes.get(pending.pop());
    visible.push(node);
    if (!canFoldBranch(node) || !folded.has(node.id)) {
      for (let index = node.children.length - 1; index >= 0; index--) pending.push(node.children[index]);
    }
  }
  return visible;
}

export function reconcilePreview(state, preview) {
  const tree = sessionTree(preview);
  const folded = new Set([...state.folded].filter((id) => canFoldBranch(tree.nodes.get(id))));
  const visible = visibleTreeNodes(preview, folded);
  const visibleIds = new Set(visible.map((node) => node.id));
  const ancestry = new Map([...(state.preview?.messages ?? []), ...preview.messages].map((message) => [message.id, message.tree.parentId]));
  function visibleAncestor(id) {
    const visited = new Set();
    while (id && !visibleIds.has(id) && !visited.has(id)) {
      visited.add(id);
      id = ancestry.get(id);
    }
    return visibleIds.has(id) ? id : null;
  }
  return {
    preview, folded,
    messageId: visibleAncestor(state.messageId) ?? visibleAncestor(preview.activeMessageId) ?? visible[0]?.id ?? null,
  };
}

export function expandedMessagePath(preview, folded, id) {
  const tree = sessionTree(preview);
  if (!tree.nodes.has(id)) throw new Error("The message is not in this session tree.");
  const next = new Set(folded);
  let parentId = tree.nodes.get(id)?.parentId;
  while (parentId != null) {
    next.delete(parentId);
    parentId = tree.nodes.get(parentId).parentId;
  }
  return next;
}

export function messagePath(preview, id) {
  const tree = sessionTree(preview);
  const path = [];
  let node = tree.nodes.get(id);
  while (node) {
    if (node.parentId == null || isBranchStart(node)) path.push(node);
    node = tree.nodes.get(node.parentId);
  }
  return path.reverse();
}
