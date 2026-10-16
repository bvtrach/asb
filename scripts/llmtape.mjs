#!/usr/bin/env node

// Small project-local HTTP recorder/replayer used by ASB.  The npm llmtape
// package is a library, not a command-line proxy, so this wrapper supplies the
// bounded loopback protocol that ASB's tape module invokes.

import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import http from "node:http";
import { performance } from "node:perf_hooks";

const MAX_BODY = 16 * 1024 * 1024;
const MAX_DELAY_MS = 60 * 60 * 1000;

function usage(message) {
  if (message) console.error(`llmtape: ${message}`);
  console.error("usage: llmtape record --port PORT --upstream URL --out FILE");
  console.error("       llmtape replay --port PORT --cassette FILE");
  process.exit(2);
}

function option(args, name, required = true) {
  const index = args.indexOf(name);
  if (index < 0) {
    if (required) usage(`missing ${name}`);
    return undefined;
  }
  const value = args[index + 1];
  if (!value || value.startsWith("--")) usage(`${name} requires a value`);
  return value;
}

const args = process.argv.slice(2);
const mode = args.shift();
if (mode !== "record" && mode !== "replay") usage("mode must be record or replay");
const port = Number(option(args, "--port"));
if (!Number.isInteger(port) || port < 1 || port > 65535) usage("--port must be a valid TCP port");
const cassette = mode === "record" ? option(args, "--out") : option(args, "--cassette");
const upstream = mode === "record" ? option(args, "--upstream") : undefined;
const respectRecordTimings = args.includes("--respect-record-timings");
if (respectRecordTimings && mode !== "replay") {
  usage("--respect-record-timings is only valid with replay");
}

function boundedBody(chunks) {
  const body = Buffer.concat(chunks);
  if (body.length > MAX_BODY) throw new Error("request body exceeds 16 MiB");
  return body;
}

function headersObject(headers) {
  const result = {};
  for (const [name, value] of headers) result[name] = value;
  return result;
}

function redactHeaders(headers) {
  return Object.fromEntries(
    Object.entries(headers).map(([name, value]) =>
      /key|auth|token|secret|credential/i.test(name) ? [name, "[redacted]"] : [name, value],
    ),
  );
}

function responseHeaders(headers) {
  const result = {};
  for (const [name, value] of headers) {
    if (!/^(connection|content-length|transfer-encoding)$/i.test(name)) result[name] = value;
  }
  return result;
}

function requestKey(request) {
  return JSON.stringify([request.method, request.url, request.body]);
}

const interactions = mode === "replay"
  ? readFileSync(cassette, "utf8")
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => JSON.parse(line))
  : [];
let cursor = 0;

async function handle(request, response) {
  const startedAt = performance.now();
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  const body = boundedBody(chunks);
  const recordedRequest = {
    method: request.method || "GET",
    url: request.url || "/",
    headers: redactHeaders(Object.fromEntries(Object.entries(request.headers))),
    body: body.length ? body.toString("utf8") : null,
  };

  if (mode === "replay") {
    const interaction = interactions[cursor];
    if (!interaction) {
      response.writeHead(409, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: "cassette has no more recorded requests" }));
      return;
    }
    if (requestKey(recordedRequest) !== requestKey(interaction.request)) {
      response.writeHead(409, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: "request does not match cassette" }));
      return;
    }
    cursor += 1;
    const delayMs = Number(interaction.delay_ms);
    if (respectRecordTimings && Number.isFinite(delayMs) && delayMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, Math.min(delayMs, MAX_DELAY_MS)));
    }
    response.writeHead(interaction.response.status, interaction.response.headers || {});
    response.end(interaction.response.body || "");
    return;
  }

  const target = new URL(request.url || "/", upstream);
  const forwardedHeaders = { ...request.headers };
  delete forwardedHeaders.host;
  delete forwardedHeaders.connection;
  delete forwardedHeaders["content-length"];
  const upstreamResponse = await fetch(target, {
    method: recordedRequest.method,
    headers: forwardedHeaders,
    body: body.length ? body : undefined,
  });
  const responseBody = await upstreamResponse.text();
  const recordedResponse = {
    status: upstreamResponse.status,
    headers: responseHeaders(upstreamResponse.headers),
    body: responseBody,
  };
  const delayMs = Math.min(Math.max(0, Math.round(performance.now() - startedAt)), MAX_DELAY_MS);
  appendFileSync(cassette, `${JSON.stringify({ request: recordedRequest, response: recordedResponse, delay_ms: delayMs })}\n`);
  response.writeHead(recordedResponse.status, recordedResponse.headers);
  response.end(responseBody);
}

const server = http.createServer((request, response) => {
  handle(request, response).catch((error) => {
    response.writeHead(502, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: String(error.message || error) }));
  });
});

if (mode === "record") {
  if (existsSync(cassette)) writeFileSync(cassette, "");
}
server.listen(port, "127.0.0.1");
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
