#!/usr/bin/env node

// Provider-neutral compatibility hop for the project-local cli2key bridge.
// Codex can include host-only `additional_tools` input items which the bridge
// intentionally does not expose.  Strip only that metadata before forwarding;
// all actual prompts and function tools remain unchanged.

import http from "node:http";
import { Readable } from "node:stream";
import { cancelNetworkRequests, networkFetch } from "./network-fetch.mjs";

function usage(message) {
  if (message) console.error(`asb cli2key proxy: ${message}`);
  console.error("usage: cli2key-proxy --port PORT --upstream URL");
  process.exit(2);
}

const args = process.argv.slice(2);
const value = (name) => {
  const index = args.indexOf(name);
  if (index < 0 || !args[index + 1] || args[index + 1].startsWith("--")) {
    usage(`${name} requires a value`);
  }
  return args[index + 1];
};
const port = Number(value("--port"));
const upstream = value("--upstream").replace(/\/$/, "");
if (!Number.isInteger(port) || port < 1 || port > 65535) usage("--port must be a valid TCP port");

function normalizeBody(path, body) {
  if (!body || !path.includes("/responses")) return body;
  try {
    const parsed = JSON.parse(body);
    let changed = false;
    if (Array.isArray(parsed.input)) {
      const filtered = parsed.input.filter((item) => item?.type !== "additional_tools");
      changed = filtered.length !== parsed.input.length;
      parsed.input = filtered;
    }
    if (parsed.reasoning && typeof parsed.reasoning === "object" &&
        Object.hasOwn(parsed.reasoning, "context")) {
      delete parsed.reasoning.context;
      changed = true;
    }
    return changed ? JSON.stringify(parsed) : body;
  } catch {
    return body;
  }
}

const server = http.createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(Buffer.from(chunk));
  const raw = Buffer.concat(chunks);
  const path = request.url || "/";
  const body = normalizeBody(path, raw.length ? raw.toString("utf8") : "");
  const target = new URL(path, `${upstream}/`);
  const headers = { ...request.headers };
  delete headers.host;
  delete headers.connection;
  delete headers["content-length"];
  try {
    const result = await networkFetch(target, {
      method: request.method,
      headers,
      body: body || undefined,
    });
    response.writeHead(result.status, Object.fromEntries(result.headers));
    if (result.body) {
      const bodyStream = Readable.fromWeb(result.body);
      bodyStream.on("error", () => response.destroy());
      response.on("close", () => bodyStream.destroy());
      bodyStream.pipe(response);
    }
    else response.end();
  } catch (error) {
    response.writeHead(502, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: { message: `cli2key upstream unavailable: ${error.message}` } }));
  }
});

server.on("error", (error) => {
  console.error(`asb cli2key proxy: ${error.message}`);
  process.exit(1);
});
server.listen(port, "127.0.0.1", () => {
  console.log("ASB cli2key proxy ready");
});
const shutdown = () => { cancelNetworkRequests(); server.close(() => process.exit(0)); };
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
