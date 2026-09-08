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
    process.off("SIGINT", close);
    process.off("SIGTERM", close);
    server.closeAllConnections();
    await Promise.all([
      new Promise((resolve) => server.close(resolve)),
      client.close(),
    ]);
  }
  process.once("SIGINT", close);
  process.once("SIGTERM", close);
  return { server, url, close };
}
