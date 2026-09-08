import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { isAbsolute, relative } from "node:path";

const BODY_LIMIT = 512 * 1024;
const PUBLIC_DIR = new URL("./public/", import.meta.url);
const MIME_TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
};
const HEADERS = {
  "Cache-Control": "no-store",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
  "Content-Security-Policy": "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
};

class HttpError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

async function readJson(request) {
  if (request.headers["content-type"]?.split(";")[0].trim() !== "application/json") {
    throw new HttpError(415, "Expected application/json.");
  }
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > BODY_LIMIT) throw new HttpError(413, "Request body is too large.");
    chunks.push(chunk);
  }
  let value;
  try {
    value = JSON.parse(Buffer.concat(chunks).toString("utf8"), (_key, item) => {
      if (typeof item === "number" && !Number.isFinite(item)) {
        throw new HttpError(400, "JSON numbers must be finite.");
      }
      return item;
    });
  } catch (error) {
    if (error instanceof HttpError) throw error;
    throw new HttpError(400, "Request body must be valid JSON.");
  }
  if (!value || Array.isArray(value) || typeof value !== "object" || typeof value.action !== "string") {
    throw new HttpError(400, "Expected a JSON object with an action.");
  }
  return value;
}

function sendJson(response, status, value) {
  response.writeHead(status, { ...HEADERS, "Content-Type": "application/json; charset=utf-8" });
  response.end(JSON.stringify(value));
}

async function serveAsset(request, response, pathname) {
  if (request.method !== "GET" && request.method !== "HEAD") {
    throw new HttpError(405, "Method not allowed.");
  }
  const relativePath = pathname === "/" ? "index.html" : pathname.slice(1);
  // Only the packaged UI is public; configuration paths never become asset paths.
  if (!/^[a-zA-Z0-9_-]+(?:\/[a-zA-Z0-9_-]+)*\.(html|css|js|svg)$/.test(relativePath)) {
    throw new HttpError(404, "Not found.");
  }
  const assetPath = fileURLToPath(new URL(relativePath, PUBLIC_DIR));
  const withinPublic = relative(fileURLToPath(PUBLIC_DIR), assetPath);
  if (withinPublic.startsWith("..") || isAbsolute(withinPublic)) throw new HttpError(404, "Not found.");
  let body;
  try {
    body = await readFile(assetPath);
  } catch (error) {
    if (error.code === "ENOENT" || error.code === "ENOTDIR") throw new HttpError(404, "Not found.");
    throw error;
  }
  const extension = relativePath.slice(relativePath.lastIndexOf("."));
  response.writeHead(200, { ...HEADERS, "Content-Type": MIME_TYPES[extension] });
  response.end(request.method === "HEAD" ? undefined : body);
}

/**
 * The browser is another client of the native core. Injecting request keeps
 * HTTP validation separate from document operations and makes both testable.
 */
export function createWebServer({ request: dispatch }) {
  if (typeof dispatch !== "function") throw new TypeError("A native request handler is required.");

  const server = createServer(async (request, response) => {
    try {
      const address = server.address();
      const authority = "127.0.0.1:" + address.port;
      if (request.headers.host !== authority) throw new HttpError(403, "Use the local URL printed by pi-switch.");
      const origin = "http://" + authority;
      if (request.headers.origin && request.headers.origin !== origin) {
        throw new HttpError(403, "Cross-origin requests are not allowed.");
      }
      const url = new URL(request.url, origin);
      if (url.pathname !== "/api") {
        await serveAsset(request, response, url.pathname);
        return;
      }
      if (request.method !== "POST") throw new HttpError(405, "Use POST for API requests.");
      if (request.headers.origin !== origin) throw new HttpError(403, "A same-origin browser request is required.");
      const payload = await readJson(request);
      let result;
      try {
        result = await dispatch(payload);
      } catch (error) {
        throw new HttpError(422, error.message);
      }
      sendJson(response, 200, result);
    } catch (error) {
      if (!response.headersSent && !response.destroyed) {
        sendJson(response, error.status ?? 500, { error: error.message });
      }
    }
  });
  server.requestTimeout = 30_000;
  server.headersTimeout = 10_000;
  return server;
}

export async function listen(server, port = 0) {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => {
      server.off("error", reject);
      resolve();
    });
  });
  return "http://127.0.0.1:" + server.address().port;
}
