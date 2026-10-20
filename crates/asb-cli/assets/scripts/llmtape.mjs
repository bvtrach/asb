#!/usr/bin/env node

// Small project-local HTTP recorder/replayer embedded in ASB. It supplies the
// bounded loopback protocol that ASB's tape module invokes.

import { appendFileSync, existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import http from "node:http";
import { performance } from "node:perf_hooks";
import { cancelNetworkRequests, networkFetch } from "./network-fetch.mjs";

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
const readyFile = process.env.ASB_TAPE_READY_FILE;
if (!Number.isInteger(port) || port < (readyFile ? 0 : 1) || port > 65535) usage("--port must be a valid TCP port");
const cassette = mode === "record" ? option(args, "--out") : option(args, "--cassette");
const upstream = mode === "record" ? option(args, "--upstream") : undefined;
const respectRecordTimings = args.includes("--respect-record-timings");
if (respectRecordTimings && mode !== "replay") {
  usage("--respect-record-timings is only valid with replay");
}

function headersObject(headers) {
  const result = {};
  for (const [name, value] of headers) result[name] = value;
  return result;
}

function requestPath(raw) {
  try {
    const parsed = new URL(raw || "/", "http://asb-tape.invalid");
    return `${parsed.pathname}${parsed.search}`;
  } catch {
    return raw || "/";
  }
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

// Preserve server-sent event boundaries during replay.  Several agent CLIs
// dispatch tool calls as soon as the terminating SSE event arrives; sending
// the complete cassette body as one write can make those clients finish the
// turn before their tool dispatcher observes the event sequence.
async function replayResponse(response, interaction) {
  // Provider responses can contain agent tool-call arguments with the
  // recording workspace path. Materialize the current run's canonical path
  // before replay so agents never execute commands in the record directory.
  const workspace = process.env.ASB_TAPE_WORKSPACE;
  const body = workspace
    ? normalizeSseWorkspaceFragments(
        normalizeCassetteBody(interaction.response.body || ""),
        "<asb-workspace>",
        workspace,
      ).replaceAll("<asb-workspace>", workspace)
    : interaction.response.body || "";
  // Request matching intentionally redacts provider-generated tool-call IDs,
  // but Claude requires IDs to be unique across turns. Give each replayed
  // response a fresh local ID without changing the cassette evidence.
  const replayToolId = `asb-replay-${process.pid}-${Date.now()}`;
  const replayBody = body.replaceAll("<asb-call>", replayToolId);
  const contentType = Object.entries(interaction.response.headers || {})
    .find(([name]) => name.toLowerCase() === "content-type")?.[1] || "";
  response.writeHead(interaction.response.status, interaction.response.headers || {});
  if (!/text\/event-stream/i.test(contentType) || !replayBody.includes("\n\n")) {
    response.end(replayBody);
    return;
  }
  const events = replayBody.match(/[\s\S]*?(?:\r?\n\r?\n|$)/g)?.filter(Boolean) || [replayBody];
  for (const event of events) {
    response.write(event);
    await new Promise((resolve) => setImmediate(resolve));
  }
  response.end();
}

// Some agent CLIs emit Anthropic server-tool descriptors without the
// input_schema required by the Anthropic wire format.  The loopback bridge is
// deliberately provider-neutral, so make that descriptor valid while keeping
// the request deterministic for recording and replay.  This does not invent
// tool calls; it only supplies the empty schema required for validation.
function normalizeRequestBody(url, body) {
  if (!body || (!url.includes("/messages") && !url.includes("/responses"))) return body;
  try {
    const parsed = JSON.parse(body);
    let changed = false;
    // Codex 0.162 emits an internal Responses input item named
    // `additional_tools`. The local bridge intentionally does not expose that
    // internal server-tool type. It is optional metadata, so omit it at the
    // tape boundary while retaining the actual agent tools and prompt. The
    // normalized body is used for both forwarding and cassette keys, making
    // record/replay deterministic.
    if (url.includes("/responses") && Array.isArray(parsed.input)) {
      const filtered = parsed.input.filter((item) => item?.type !== "additional_tools");
      if (filtered.length !== parsed.input.length) {
        parsed.input = filtered;
        changed = true;
      }
    }
    if (url.includes("/responses") && parsed.reasoning && typeof parsed.reasoning === "object") {
      // The bridge accepts only effort and summary. Newer Codex clients add a
      // local `context` field; it is not provider input and must not make a
      // recorded request fail at the compatibility boundary.
      if (Object.hasOwn(parsed.reasoning, "context")) {
        delete parsed.reasoning.context;
        changed = true;
      }
    }
    if (url.includes("/messages") && parsed.temperature === undefined) {
      // Keep record/replay agent trajectories stable when a CLI leaves the
      // provider's sampling default unspecified.
      parsed.temperature = 0;
      changed = true;
    }
    if (!Array.isArray(parsed.tools)) return changed ? JSON.stringify(parsed) : body;
    parsed.tools = parsed.tools
      .filter((tool) => {
        // The local bridge supports function tools.  Cline's Responses
        // adapter may add native web/image tools that the bridge cannot
        // execute; omit those optional tools while retaining agent tools.
        if (url.includes("/responses") && tool &&
            (tool.type === "web_search" || tool.type === "image_generation")) {
          changed = true;
          return false;
        }
        return true;
      })
      .map((tool) => {
      if (tool && tool.type === "web_search_20250305" && !tool.input_schema) {
        changed = true;
        return { ...tool, input_schema: { type: "object", properties: {} } };
      }
      return tool;
      });
    return changed ? JSON.stringify(parsed) : body;
  } catch {
    return body;
  }
}

// Result workspaces are intentionally different for record and replay.  Keep
// absolute project paths out of cassette keys while forwarding the normalized
// request body unchanged to the live provider during recording.
function normalizeCassetteBody(body) {
  if (!body) return body;
  if (typeof body !== "string") body = JSON.stringify(body);
  const workspace = process.env.ASB_TAPE_WORKSPACE;
  const normalized = normalizeSseWorkspaceFragments(body, workspace)
    .replaceAll(workspace || "\u0000", "<asb-workspace>")
    // Sweep isolation may serialize either an absolute or a relative result
    // path. Normalize the whole path so the result-root prefix cannot leak
    // into replay keys (for example replay-sweep-results/.../workspace-*).
    .replace(/\/?(?:[A-Za-z0-9_.-]+\/)+workspace-concurrency-\d+(?:\.asb-agent-runtime)?/g, "<asb-workspace>")
    // Older recordings may contain an absolute result-root/workspace path
    // rather than the normalized marker. Canonicalize that form as well.
    .replace(/\/?(?:[A-Za-z0-9_.-]+\/)+workspace(?=\/|["\\])/g, "<asb-workspace>")
    // Agent SDKs generate fresh message/session IDs for every invocation;
    // those identifiers are not provider semantics and must not prevent a
    // cassette recorded in one process from replaying in another.
    .replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, "<asb-uuid>")
    .replace(/\b1[0-9]{12,}\b/g, "<asb-timestamp>")
    // Shell directory listings include filesystem mtimes. They are not
    // provider input and naturally differ between record and replay.
    .replace(/\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) \d{1,2} \d{2}:\d{2}\b/g, "<asb-file-mtime>")
    .replace(/\b20\d{2}-\d{2}-\d{2}[T ]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?\b/g, "<asb-file-mtime>")
    // WorkBuddy derives a memory-project slug from the absolute workspace;
    // canonicalize that generated slug when record and replay roots differ.
    .replace(/srv-data-projects-[A-Za-z0-9_.-]+-(?:sweep-(?:record|replay)-[A-Za-z0-9_.-]+|workspaces(?:-replay)?-[A-Za-z0-9_.-]+)/g, "<asb-project-workspace>")
    // Codex includes a diagnostic token count in shell-tool output. It is
    // derived from the rendered transcript and can change during replay; it
    // is not provider request semantics.
    .replace(/Original token count: \d+/g, "Original token count: <asb-token-count>")
    .replace(/original token count: \d+/g, "original token count: <asb-token-count>")
    .replace(/Total output lines: \d+/g, "Total output lines: <asb-line-count>")
    .replace(/codex-arg[A-Za-z0-9_-]+/g, "codex-arg<asb-random>")
    // Normalize generated call identifiers, but never the JSON field name
    // `call_id` itself. Replacing that key corrupts Responses tool events.
    // Restore the malformed key emitted by older recordings before replacing
    // their value marker.
    .replace(/"call_<asb-call>":/g, '"call_id":')
    .replace(/"(call_[A-Za-z0-9_-]+)"(?=\s*[,}\]])/g, '"call_<asb-call>"')
    .replace(/\bmsg_resp_[A-Za-z0-9_-]+/g, "msg_resp_<asb-response>")
    .replace(/Chunk ID: [0-9a-f]+/gi, "Chunk ID: <asb-chunk>");
  try {
    const sort = (value, key = "", path = []) => {
      // Claude Code embeds a fresh device identifier in metadata.user_id on
      // every process. It is client telemetry, not provider input, and must
      // not make an otherwise identical recording unreplayable. Keep this
      // exception path-scoped so same-named fields in prompts and tool input
      // remain request-significant.
      if (path.length === 2 && path[0] === "metadata" && path[1] === "user_id" && key === "user_id") {
        return "<asb-user-id>";
      }
      if (typeof value === "string") {
        const hadParentGuidance = value.includes("../") && value.includes("AGENTS.md");
        const cleaned = value.replace(/\n?\.\.\/.*AGENTS\.md\n?/gm, "");
        const withoutGuidance = hadParentGuidance ? cleaned.replace(/\n+$/g, "") : cleaned;
        if (withoutGuidance.includes("Output:\n")) {
          return withoutGuidance.replace(/(Output:\n)([\s\S]*?)(?=\nfatal:|$)/g, (match, prefix, listing) => {
            const marker = "<asb-workspace>\n";
            const markerIndex = listing.indexOf(marker);
            if (markerIndex >= 0) {
              const header = listing.slice(0, markerIndex);
              const lines = listing.slice(markerIndex + marker.length).split("\n");
              if (lines.every((line) => line === "" || line.startsWith("external-workload/"))) {
                return prefix + header + marker + "<asb-workspace-listing>\n";
              }
            }
            return match;
          });
        }
        return withoutGuidance;
      }
      if (Array.isArray(value)) return value.map((item) => sort(item));
      if (value && typeof value === "object") {
        return Object.fromEntries(Object.keys(value).sort().map((child) => [child, sort(value[child], child, [...path, child])]));
      }
      return value;
    };
    return JSON.stringify(sort(JSON.parse(normalized))).replace(/\.\.\/[^"\\]*AGENTS\.md(?:\\n)?/g, "");
  } catch {
    return normalized;
  }
}

// Tool-call arguments in Anthropic SSE responses are split across many
// `input_json_delta` events. A workspace path can therefore be divided across
// event boundaries and cannot be normalized by a regex over the raw SSE text.
// Reassemble each contiguous content block, normalize it, and emit the result
// in the first fragment while retaining the event boundaries for the client.
function normalizeSseWorkspaceFragments(body, workspace, replacement = "<asb-workspace>") {
  if (!body.includes("partial_json")) return body;
  const lines = body.split(/(?<=\n)/);
  const groups = [];
  const active = new Map();
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^(data:\s+)(\{.*\})(\r?\n)?$/);
    if (!match) continue;
    let event;
    try { event = JSON.parse(match[2]); } catch { continue; }
    const partial = event?.delta?.partial_json;
    const block = event?.index;
    if (typeof partial === "string" && Number.isInteger(block)) {
      const group = active.get(block) || { indices: [], parts: [] };
      group.indices.push(index);
      group.parts.push(partial);
      active.set(block, group);
    }
    if (event?.type === "content_block_stop" && Number.isInteger(block)) {
      const group = active.get(block);
      if (group) { groups.push(group); active.delete(block); }
    }
  }
  for (const group of groups) {
    const value = normalizeWorkspaceText(group.parts.join(""), workspace, replacement);
    let cursor = 0;
    for (let offset = 0; offset < group.indices.length; offset += 1) {
      const lineIndex = group.indices[offset];
      const match = lines[lineIndex].match(/^(data:\s+)(\{.*\})(\r?\n)?$/);
      if (!match) continue;
      let event;
      try { event = JSON.parse(match[2]); } catch { continue; }
      const length = group.parts[offset].length;
      const end = offset === group.indices.length - 1 ? value.length : Math.min(value.length, cursor + length);
      event.delta.partial_json = value.slice(cursor, end);
      cursor = end;
      lines[lineIndex] = `${match[1]}${JSON.stringify(event)}${match[3] || ""}`;
    }
  }
  return lines.join("");
}

function normalizeWorkspaceText(value, workspace, replacement = "<asb-workspace>") {
  return (workspace ? value.replaceAll(workspace, replacement) : value)
    .replace(/\/?(?:[A-Za-z0-9_.-]+\/)+workspace-concurrency-\d+(?:\.asb-agent-runtime)?/g, replacement)
    .replace(/\/?(?:[A-Za-z0-9_.-]+\/)+workspace(?=\/|["\\])/g, replacement);
}

function requestKey(request) {
  // Match the normalized full request, not just model and endpoint: a changed
  // prompt or tool state must not silently consume an unrelated response.
  return JSON.stringify([request.method, request.url, normalizeCassetteBody(request.body)]);
}

function sameEndpoint(actual, expected) {
  if (actual.method !== expected.method) return false;
  try {
    return new URL(actual.url, "http://asb-tape.invalid").pathname
      === new URL(expected.url, "http://asb-tape.invalid").pathname;
  } catch {
    return actual.url === expected.url;
  }
}

function requestPrefixMatches(actual, expected) {
  if (!sameEndpoint(actual, expected)) return false;
  try {
    const a = JSON.parse(actual.body || "null");
    const e = JSON.parse(expected.body || "null");
    if (!a || !e) return false;
    const field = Array.isArray(a.messages) && Array.isArray(e.messages) ? "messages"
      : Array.isArray(a.input) && Array.isArray(e.input) ? "input" : null;
    if (!field || a[field].length > e[field].length) return false;
    const comparable = (value) => {
      if (Array.isArray(value)) return value.map(comparable);
      if (value && typeof value === "object") {
        return Object.fromEntries(Object.entries(value)
          .filter(([key]) => key !== "cache_control")
          .map(([key, item]) => [key, comparable(item)]));
      }
      return value;
    };
    const withoutMessages = (value) => {
      const copy = comparable(value);
      delete copy[field];
      return copy;
    };
    if (JSON.stringify(withoutMessages(a)) !== JSON.stringify(withoutMessages(e))) return false;
    return a[field].every((message, index) => JSON.stringify(comparable(message)) === JSON.stringify(comparable(e[field][index])));
  } catch {
    return false;
  }
}

// Some clients omit tool-result turns when retrying a streamed response. Keep
// replay deterministic by accepting only the same endpoint, model, and user
// prompt for the next recorded interaction; arbitrary prompts still reject.
function requestReplayCompatible(actual, expected) {
  if (!sameEndpoint(actual, expected)) return false;
  try {
    const a = JSON.parse(actual.body || "null");
    const e = JSON.parse(expected.body || "null");
    if (!a || !e || a.model !== e.model) return false;
    const comparable = (value) => {
      if (Array.isArray(value)) return value.map(comparable);
      if (value && typeof value === "object") {
        return Object.fromEntries(Object.entries(value)
          .filter(([key]) => key !== "cache_control")
          .map(([key, item]) => [key, comparable(item)]));
      }
      return value;
    };
    // Chat-completions clients expose the initial prompt in `messages`, while
    // Responses clients (Codex, Cline, Pi, Qwen and WorkBuddy) use `input`.
    // Compare only the first user item; tool-result turns and generated IDs
    // may legitimately differ between record and replay, but a changed user
    // prompt must remain a hard mismatch.
    const firstUser = (value) => {
      const items = Array.isArray(value?.messages) ? value.messages
        : Array.isArray(value?.input) ? value.input : [];
      return items.find((item) => item?.role === "user"
        || (typeof item?.type === "string" && item.type === "message" && item.role === "user"))
        || items[0];
    };
    const actualPrompt = firstUser(a);
    const expectedPrompt = firstUser(e);
    if (!actualPrompt || !expectedPrompt) return false;
    return JSON.stringify(comparable(actualPrompt)) === JSON.stringify(comparable(expectedPrompt));
  } catch {
    return false;
  }
}

const interactions = mode === "replay"
  ? readFileSync(cassette, "utf8")
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => {
        const interaction = JSON.parse(line);
        if (interaction.request) {
          interaction.request.url = requestPath(interaction.request.url);
          interaction.request.body = normalizeCassetteBody(interaction.request.body);
        }
        if (interaction.response?.body) {
          interaction.response.body = normalizeCassetteBody(interaction.response.body);
        }
        return interaction;
      })
  : [];
let cursor = 0;
let rejected = 0;
const progressPath = `${cassette}.progress.json`;
function writeProgress() {
  if (mode === "replay") {
    writeFileSync(progressPath, JSON.stringify({ schema_version: 1, consumed: cursor, total: interactions.length, rejected }));
  }
}

function isReplayProbe(request) {
  if (!request.body) {
    return (request.method === "HEAD" || request.method === "GET")
      && /(?:\/api\/hello|\/health(?:z)?|\/models|\/api\/tags)(?:[/?]|$)/.test(request.url || "");
  }
  try {
    const parsed = JSON.parse(request.body);
    if (request.url?.includes("/embeddings") && parsed.model === "text-embedding-3-small") {
      return true;
    }
  } catch {
    // Keep arbitrary malformed bodies fail-closed below.
  }
  // OpenJiuwen performs this local vision-capability check before its first
  // workload request. It has no benchmark semantics and can be omitted when
  // replaying on a provider/agent path that does not repeat the probe.
  return typeof request.body === "string"
    && request.body.includes("What color is this image? Reply with one word.");
}
writeProgress();

async function handle(request, response) {
  const startedAt = performance.now();
  const chunks = [];
  let bodySize = 0;
  for await (const chunk of request.iterator({ destroyOnReturn: false })) {
    const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    bodySize += bytes.length;
    if (bodySize > MAX_BODY) {
      if (mode === "replay") {
        rejected += 1;
        writeProgress();
      }
      request.resume();
      response.writeHead(413, { "content-type": "application/json", connection: "close" });
      response.end(JSON.stringify({ error: "request body exceeds 16 MiB" }));
      return;
    }
    chunks.push(bytes);
  }
  const body = Buffer.concat(chunks, bodySize);
  const normalizedBody = normalizeRequestBody(request.url || "/", body.toString("utf8"));
  const recordedRequest = {
    method: request.method || "GET",
    // Some SDKs send an absolute URL even when configured for a proxy. Keep
    // only the path in the cassette and always route the request to the
    // selected upstream; this prevents a client from bypassing the loopback
    // recorder and makes separate-directory replay path-stable.
    url: requestPath(request.url),
    headers: redactHeaders(Object.fromEntries(Object.entries(request.headers))),
    body: body.length ? normalizeCassetteBody(normalizedBody) : null,
  };

  if (mode === "replay") {
    // Capability probes may be issued by the client even when the replayed
    // run would otherwise omit them.  If the cassette has the corresponding
    // known probe at the cursor, consume it and return its recorded response
    // instead of comparing the probe with the first workload request.
    if (isReplayProbe(recordedRequest)
      && interactions[cursor]?.request
      && isReplayProbe(interactions[cursor].request)) {
      const probe = interactions[cursor];
      cursor += 1;
      writeProgress();
      await replayResponse(response, probe);
      return;
    }
    // A provider bridge may record a health/model probe that the agent omits
    // on replay. Consume only these known, credential-free probes when the
    // next request carries an LLM body; arbitrary mismatches stay fail-closed.
    while (interactions[cursor]?.request
      && body.length > 0
      && isReplayProbe(interactions[cursor].request)) {
      cursor += 1;
      writeProgress();
    }
    const interaction = interactions[cursor];
    if (!interaction) {
      rejected += 1;
      writeProgress();
      response.writeHead(409, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: "cassette has no more recorded requests" }));
      return;
    }
    if (requestKey(recordedRequest) !== requestKey(interaction.request)
      && !requestPrefixMatches(recordedRequest, interaction.request)
      && !requestReplayCompatible(recordedRequest, interaction.request)) {
      // A client may retry a request after receiving a valid streamed
      // response (for example when it loses the final SSE marker). Replaying
      // the already-consumed exact request is safe and keeps the cursor on
      // the next interaction; any other mismatch remains fail-closed.
      let retry;
      for (let index = cursor - 1; index >= 0; index -= 1) {
        if (requestKey(recordedRequest) === requestKey(interactions[index].request)) {
          retry = interactions[index];
          break;
        }
      }
      if (retry) {
        await replayResponse(response, retry);
        return;
      }
      rejected += 1;
      writeProgress();
      if (process.env.ASB_TAPE_DEBUG === "1") {
        writeFileSync("/tmp/asb-tape-expected.json", interaction.request.body || "");
        writeFileSync("/tmp/asb-tape-actual.json", recordedRequest.body || "");
      }
      response.writeHead(409, { "content-type": "application/json" });
      response.end(JSON.stringify({
        error: "request does not match cassette",
        ...(process.env.ASB_TAPE_DEBUG === "1"
          ? {
              expected: {
                method: interaction.request.method,
                url: interaction.request.url,
                body_length: interaction.request.body?.length ?? 0,
                body_prefix: interaction.request.body?.slice(0, 500) ?? null,
              },
              actual: {
                method: recordedRequest.method,
                url: recordedRequest.url,
                body_length: recordedRequest.body?.length ?? 0,
                body_prefix: recordedRequest.body?.slice(0, 500) ?? null,
              },
            }
          : {}),
      }));
      return;
    }
    cursor += 1;
    writeProgress();
    const delayMs = Number(interaction.delay_ms);
    if (respectRecordTimings && Number.isFinite(delayMs) && delayMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, Math.min(delayMs, MAX_DELAY_MS)));
    }
    await replayResponse(response, interaction);
    return;
  }

  // Preserve provider-specific prefixes (e.g. OpenRouter's /api) while
  // keeping the selected upstream host, even for absolute-form requests.
  const target = new URL(`${upstream.replace(/\/+$/, "")}${recordedRequest.url}`);
  const forwardedHeaders = { ...request.headers };
  delete forwardedHeaders.host;
  delete forwardedHeaders.connection;
  delete forwardedHeaders["content-length"];
  const upstreamResponse = await networkFetch(target, {
    method: recordedRequest.method,
    headers: forwardedHeaders,
    body: body.length ? Buffer.from(normalizedBody) : undefined,
  });
  const responseChunks = [];
  let responseSize = 0;
  if (upstreamResponse.body) {
    for await (const chunk of upstreamResponse.body) {
      responseSize += chunk.byteLength;
      if (responseSize > MAX_BODY) throw new Error("upstream response body exceeds 16 MiB");
      responseChunks.push(chunk);
    }
  }
  const responseBody = Buffer.concat(responseChunks, responseSize).toString("utf8");
  const recordedResponse = {
    status: upstreamResponse.status,
    headers: responseHeaders(upstreamResponse.headers),
    body: normalizeCassetteBody(responseBody),
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
server.listen(port, "127.0.0.1", () => {
  if (readyFile) {
    const temporary = `${readyFile}.tmp`;
    writeFileSync(temporary, JSON.stringify({
      schema_version: 1, pid: process.pid, host: "127.0.0.1", port: server.address().port,
    }), { mode: 0o600, flag: "wx" });
    renameSync(temporary, readyFile);
  }
});
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => { cancelNetworkRequests(); server.close(() => process.exit(0)); });
}
