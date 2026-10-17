import { spawn } from "node:child_process";
import { createNativeClient } from "./native-client.js";
import { createWebServer, listen } from "./server.js";

export function parseWebOptions(args) {
  const options = { port: 0, open: true };
  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    if (argument === "--no-open") {
      options.open = false;
    } else if (argument === "--port") {
      const value = args[++index];
      if (!/^\d+$/.test(value ?? "") || Number(value) < 1 || Number(value) > 65535) {
        throw new Error("--port must be an integer between 1 and 65535.");
      }
      options.port = Number(value);
    } else {
      throw new Error("Unknown Web option: " + argument);
    }
  }
  return options;
}

function openBrowser(url) {
  const [command, args] = process.platform === "win32"
    ? ["rundll32.exe", ["url.dll,FileProtocolHandler", url]]
    : process.platform === "darwin" ? ["open", [url]] : ["xdg-open", [url]];
  const child = spawn(command, args, { detached: true, stdio: "ignore", windowsHide: true });
  child.on("error", (error) => console.error("Could not open the browser: " + error.message + "\nOpen " + url + " manually."));
  child.on("exit", (code) => {
    if (code) console.error("The browser launcher exited with " + code + ". Open " + url + " manually.");
  });
  child.unref();
}

export function createShutdownController({ close, exit = (code) => process.exit(code), report = (text) => console.log(text) } = {}) {
  if (typeof close !== "function") throw new TypeError("createShutdownController requires a close function.");
  let state = "running";
  let forced = false;
  return {
    get state() { return state; },
    handleSignal(signal) {
      if (state === "closed") return;
      if (state === "closing") {
        if (signal !== "SIGINT") return;
        forced = true;
        state = "closed";
        report("\n  Forcing exit…\n");
        exit(130);
        return;
      }
      state = "closing";
      report("\n  Stopping pi-switch Web… Press Ctrl+C again to force exit.\n");
      Promise.resolve().then(() => close(signal)).then(() => {
        if (forced) return;
        state = "closed";
        report("  pi-switch Web stopped.");
        exit(0);
      }, (error) => {
        if (forced) return;
        state = "closed";
        report("  Shutdown error: " + (error?.message ?? String(error)) + "\n  Exiting.");
        exit(1);
      });
    },
  };
}

export async function startWeb(options = {}) {
  const client = await createNativeClient();
  const server = createWebServer({ request: (payload) => client.request(payload) });
  let url;
  try {
    url = await listen(server, options.port ?? 0);
  } catch (error) {
    await client.close();
    throw error;
  }
  console.log("\n  pi-switch · Web\n\n  " + url + "\n\n  Local access only. Press Ctrl+C to stop.\n");
  if (options.open !== false) openBrowser(url);
  let closing = false;
  async function close() {
    if (closing) return;
    closing = true;
    server.closeAllConnections();
    await Promise.all([
      new Promise((resolve) => server.close(resolve)),
      client.close(),
    ]);
  }
  const controller = createShutdownController({ close });
  const onSignal = (signal) => controller.handleSignal(signal);
  const signals = process.platform === "win32"
    ? ["SIGINT", "SIGTERM", "SIGBREAK", "SIGHUP"]
    : ["SIGINT", "SIGTERM"];
  for (const signal of signals) process.on(signal, onSignal);
  return {
    server,
    url,
    close: async () => {
      for (const signal of signals) process.off(signal, onSignal);
      await close();
    },
  };
}
