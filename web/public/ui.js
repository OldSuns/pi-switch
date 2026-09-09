let language = "zh-CN";

export function setLanguage(value) {
  language = value;
  document.documentElement.lang = value;
}

export const t = (zh, en) => language === "zh-CN" ? zh : en;
export const h = (value) => String(value ?? "").replace(/[&<>"']/g, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character]);

const paths = {
  home: '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  sliders: '<path d="M4 7h8m5 0h3M4 17h3m5 0h8"/><rect x="12" y="4" width="5" height="6" rx="1"/><rect x="7" y="14" width="5" height="6" rx="1"/>',
  messages: '<path d="M21 11a8 8 0 0 1-8 8H7l-5 3 2-6a8 8 0 0 1-1-5 8 8 0 0 1 8-8h2a8 8 0 0 1 8 8Z"/><path d="M8 9h8M8 13h5"/>',
  settings: '<path d="m9 3-.6 2.3-2 .9-2.2-.6-2 3.4 1.6 1.7v2.5l-1.6 1.7 2 3.4 2.2-.6 2 .9L9 21h4l.6-2.4 2-.9 2.2.6 2-3.4-1.6-1.7v-2.5L19.8 9l-2-3.4-2.2.6-2-.9L13 3Z"/><circle cx="11" cy="12" r="3"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="3"/><path d="m7 9 3 3-3 3m6 0h4"/>',
  search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4 4"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  arrow: '<path d="M5 12h14m-5-5 5 5-5 5"/>',
  arrowDown: '<path d="M12 5v14m-5-5 5 5 5-5"/>',
  grip: '<circle cx="9" cy="5" r="1"/><circle cx="15" cy="5" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="9" cy="19" r="1"/><circle cx="15" cy="19" r="1"/>',
  target: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/><path d="M12 2v3m0 14v3M2 12h3m14 0h3"/>',
  chevron: '<path d="m9 5 7 7-7 7"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  chevronsUp: '<path d="m7 11 5-5 5 5m-10 7 5-5 5 5"/>',
  chevronsDown: '<path d="m7 6 5 5 5-5m-10 7 5 5 5-5"/>',
  refresh: '<path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.1 6.1a8 8 0 0 1 13.2 3M4.7 14.9a8 8 0 0 0 13.2 3"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  checkCircle: '<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>',
  shield: '<path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6Z"/><path d="m8 12 3 3 5-6"/>',
  star: '<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9Z"/>',
  cpu: '<rect x="6" y="6" width="12" height="12" rx="3"/><rect x="9" y="9" width="6" height="6" rx="1"/><path d="M9 3v3m6-3v3M9 18v3m6-3v3M3 9h3m-3 6h3m12-6h3m-3 6h3"/>',
  box: '<path d="m12 3 9 5-9 5-9-5 9-5ZM3 8v9l9 5 9-5V8M12 13v9m-5-16 9 5"/>',
  globe: '<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/>',
  link: '<path d="m10 13 4-4m-6 6-1 1a3.5 3.5 0 0 1-5-5l4-4a3.5 3.5 0 0 1 5 0m2 2 1-1a3.5 3.5 0 0 1 5 5l-4 4a3.5 3.5 0 0 1-5 0"/>',
  folder: '<path d="M3 8V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/>',
  file: '<path d="M14 3H5v18h14V8l-5-5Zm0 0v6h5M8 13h8m-8 4h6"/>',
  edit: '<path d="m15 5 4 4M4 20l5-1L21 7a2.8 2.8 0 0 0-4-4L5 15l-1 5Z"/>',
  copy: '<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/>',
  trash: '<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7m4-7v7"/>',
  download: '<path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5"/>',
  upload: '<path d="M12 16V4m-5 5 5-5 5 5M4 16v5h16v-5"/>',
  history: '<path d="M3 3v6h6"/><path d="M3.7 9a9 9 0 1 1 .3 7M12 7v5l3 2"/>',
  help: '<circle cx="12" cy="12" r="9"/><path d="M9 9a3 3 0 0 1 6 0c0 2-3 2-3 4m0 3h.01"/>',
  close: '<path d="m6 6 12 12M18 6 6 18"/>',
  more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  eye: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>',
  key: '<circle cx="8" cy="9" r="5"/><path d="m12 13 8 8m-2-2 3-3m-6 0 3-3"/>',
  bolt: '<path d="m13 2-9 12h7l-1 8 10-12h-8Z"/>',
  brain: '<path d="M12 5c-3-5-8 0-6 3-5 1-4 7-1 7-1 5 5 7 7 3m0-13c3-5 8 0 6 3 5 1 4 7 1 7 1 5-5 7-7 3V5Zm-6 3 2 2m-3 5 4-1m9-6-2 2m3 5-4-1"/>',
  image: '<rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8" cy="8" r="1.5"/><path d="m3 17 5-5 4 4 4-7 5 8"/>',
  branch: '<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="6" r="2"/><path d="M6 7v10m0-5h7a5 5 0 0 0 5-4"/>',
  book: '<path d="M12 5c-4-3-8-1-9-1v15c3-1 6-1 9 1m0-15c4-3 8-1 9-1v15c-3-1-6-1-9 1V5Z"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21v-2a8 8 0 0 1 16 0v2"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  warning: '<path d="m12 3 10 18H2L12 3Z"/><path d="M12 9v5m0 3h.01"/>',
  external: '<path d="M14 3h7v7m0-7L10 14M10 3H3v18h18v-7"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>',
  moon: '<path d="M20.5 13.1A8.7 8.7 0 0 1 10.9 3.5 8.8 8.8 0 1 0 20.5 13.1Z"/>',
};

export function icon(name, className = "") {
  return '<svg class="icon ' + className + '" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + (paths[name] ?? paths.box) + "</svg>";
}

export function initials(value) {
  return h(value.slice(0, 2).toUpperCase());
}

export function tint(value) {
  return ["mauve", "blue", "peach", "teal", "pink"][Array.from(value).reduce((sum, c) => sum + c.charCodeAt(0), 0) % 5];
}

export function number(value) {
  return new Intl.NumberFormat(language).format(value);
}

export function tokens(value) {
  if (value == null) return "—";
  if (value >= 1_000_000) return Number((value / 1_000_000).toFixed(2)) + "M";
  if (value >= 1_000) return Number((value / 1_000).toFixed(1)) + "k";
  return number(value);
}

export function price(value) {
  return value == null ? "—" : "$" + new Intl.NumberFormat("en", { maximumFractionDigits: 4 }).format(value);
}

export function date(value, short = false) {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.valueOf())) return h(value);
  return new Intl.DateTimeFormat(language, {
    month: "short", day: "numeric", ...(short ? {} : { hour: "2-digit", minute: "2-digit" }),
  }).format(parsed);
}

export function modelName(model) {
  return model.name || model.id;
}

export function capabilities(model) {
  return (model.reasoning ? '<span class="capability mauve" title="' + t("推理", "Reasoning") + '">' + icon("brain") + t("推理", "Reasoning") + "</span>" : "")
    + (model.input.includes("image") ? '<span class="capability blue" title="' + t("图像输入", "Vision") + '">' + icon("image") + t("视觉", "Vision") + "</span>" : "")
    + (!model.reasoning && !model.input.includes("image") ? '<span class="capability muted">' + t("文本", "Text") + "</span>" : "");
}

export function emptyState(title, description, name = "box", action = "") {
  return '<div class="empty-state"><span class="empty-icon">' + icon(name) + '</span><h3>' + h(title) + '</h3><p>' + h(description) + "</p>" + action + "</div>";
}

export function searchInput(id, value, placeholder) {
  return '<label class="search-box" for="' + id + '">' + icon("search")
    + '<input id="' + id + '" type="search" autocomplete="off" value="' + h(value) + '" placeholder="' + h(placeholder) + '" aria-label="' + h(placeholder) + '"><kbd>/</kbd></label>';
}

export function toggle(action, checked, label, attributes = "") {
  return '<label class="switch" title="' + h(label) + '"><input type="checkbox" role="switch" data-action="' + action + '" aria-label="' + h(label) + '" ' + (checked ? "checked " : "") + attributes + '><span class="switch-track"></span></label>';
}

export function iconButton(action, name, label, attributes = "", className = "") {
  return '<button type="button" class="icon-button ' + className + '" data-action="' + action + '" aria-label="' + h(label) + '" title="' + h(label) + '" ' + attributes + ">" + icon(name) + "</button>";
}
