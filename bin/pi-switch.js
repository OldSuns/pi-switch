#!/usr/bin/env node
import { doctor, runTui, version } from "../index.js";

const command = process.argv[2];

if (command === "--help" || command === "-h" || command === "help") {
  console.log(`pi-switch ${version()}

Usage:
  pi-switch              open the terminal UI
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
} else {
  console.error("Usage: pi-switch [tui|doctor|--version]");
  process.exitCode = 2;
}
