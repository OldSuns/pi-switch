const { existsSync } = require("node:fs");
const { resolve } = require("node:path");

function linuxLibc() {
  if (process.platform !== "linux") return "";
  const report = process.report?.getReport?.();
  if (report?.header?.glibcVersionRuntime) return "gnu";
  if (report?.sharedObjects?.some((path) => path.includes("musl"))) return "musl";
  throw new Error("Unable to determine Linux libc (glibc or musl)");
}

function targetSuffix() {
  const { platform, arch } = process;
  if (platform === "win32" && arch === "x64") return "win32-x64-msvc";
  if (platform === "darwin" && ["x64", "arm64"].includes(arch)) return `darwin-${arch}`;
  if (platform === "linux" && arch === "x64") return `linux-x64-${linuxLibc()}`;
  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

const filename = `pi-switch-native.${targetSuffix()}.node`;
const binary = resolve(__dirname, filename);
if (!existsSync(binary)) {
  throw new Error(`Native binding not found for ${process.platform}-${process.arch}: ${binary}`);
}

module.exports = require(binary);
