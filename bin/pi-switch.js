#!/usr/bin/env node
import { doctor, runTui, version } from "../index.js";

const command = process.argv[2];

if (command === "--help" || command === "-h" || command === "help") {
  console.log(`pi-switch ${version()}

Usage:
  pi-switch              open the terminal UI
  pi-switch --web         open the local Web interface
  pi-switch --web --port 5210 --no-open
                         choose a port without opening a browser
  pi-switch doctor       validate Pi documents and defaults
  pi-switch --version    print the native module version
`);
} else if (command === "--version" || command === "-v") {
  console.log(version());
} else if (command === "doctor") {
  const checks = doctor();
  for (const check of checks) {
    console.log(`${check.ok ? "OK" : "!!"} ${check.label}: ${check.detail}`);
  }
  if (checks.some((check) => !check.ok)) process.exitCode = 1;
} else if (!command || command === "tui") {
  runTui();
} else if (command === "--web" || command === "web") {
  try {
    const { parseWebOptions, startWeb } = await import("../web/start.js");
    await startWeb(parseWebOptions(process.argv.slice(3)));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
} else {
  console.error("Usage: pi-switch [tui|--web|doctor|--version|--help]");
  process.exitCode = 2;
}
