const STORAGE_KEY = "pi-switch.web-theme";

export function getTheme() {
  return document.documentElement.dataset.theme;
}

export function themeName() {
  return getTheme() === "light" ? "Catppuccin Latte" : "Catppuccin Mocha";
}

function applyTheme(theme) {
  document.documentElement.dataset.theme = theme;
  document.querySelector('meta[name="color-scheme"]').content = theme;
  document.querySelector('meta[name="theme-color"]').content =
    getComputedStyle(document.documentElement).getPropertyValue("--base").trim();
}

export function setTheme(theme) {
  if (theme !== "light" && theme !== "dark") throw new Error("Unsupported interface theme: " + theme);
  localStorage.setItem(STORAGE_KEY, theme);
  applyTheme(theme);
}

applyTheme(localStorage.getItem(STORAGE_KEY) ?? getTheme());
