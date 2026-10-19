// HTTP client for ASB-owned Node sidecars. curl supplies HTTP, HTTPS and SOCKS
// proxy support on Node versions whose built-in fetch ignores proxy variables.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { PassThrough, Readable } from "node:stream";
import { fileURLToPath } from "node:url";

const children = new Set();
const scratchRoot = dirname(fileURLToPath(import.meta.url));

export function cancelNetworkRequests() {
  for (const child of children) child.kill("SIGTERM");
}

function quoted(value) {
  if (/[\r\n\0]/.test(value)) throw new Error("invalid outbound request metadata");
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

function anyProxy() {
  return ["http_proxy", "HTTP_PROXY", "https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"]
    .some((name) => Boolean(process.env[name]));
}

export async function networkFetch(url, options = {}, redirects = 0) {
  if (!anyProxy()) return fetch(url, options);
  const target = new URL(url);
  if (!["http:", "https:"].includes(target.protocol)) throw new Error("unsupported upstream protocol");
  const scratch = mkdtempSync(join(scratchRoot, ".network-request-"));
  const body = options.body == null ? null : Buffer.from(options.body);
  const requestFile = join(scratch, "body");
  let config;
  try {
    if (body) writeFileSync(requestFile, body, { mode: 0o600 });
    config = [
      `url = ${quoted(target.href)}`,
      `request = ${quoted(options.method || "GET")}`,
      ...Object.entries(options.headers || {}).map(([name, value]) => `header = ${quoted(`${name}: ${value}`)}`),
      ...(body ? [`data-binary = ${quoted(`@${requestFile}`)}`] : []),
    ].join("\n") + "\n";
  } catch (error) {
    rmSync(scratch, { recursive: true, force: true });
    throw error;
  }

  const result = await new Promise((resolve, reject) => {
    let settled = false;
    let finished = false;
    let pending = Buffer.alloc(0);
    const stream = new PassThrough();
    stream.on("error", () => {});
    const child = spawn("curl", ["--disable", "--config", "-", "--silent", "--show-error",
      "--include", "--suppress-connect-headers", "--no-buffer", "--compressed",
      "--connect-timeout", "10", "--proto", "=http,https"],
      { stdio: ["pipe", "pipe", "ignore"] });
    children.add(child);
    const cleanup = () => {
      children.delete(child);
      rmSync(scratch, { recursive: true, force: true });
    };
    const fail = (message) => {
      const error = new Error(message);
      if (!settled) { settled = true; reject(error); }
      else stream.destroy(error);
    };
    child.stdin.on("error", () => {});
    child.stdout.on("error", () => {});
    child.on("error", () => { fail("cannot start curl for the outbound proxy request"); cleanup(); });
    child.on("close", (code) => {
      finished = true;
      const reasons = {
        5: "could not resolve the proxy host",
        6: "could not resolve the provider host",
        7: "could not connect to the proxy or provider",
        28: "the connection timed out",
        35: "the TLS handshake failed",
        60: "TLS certificate verification failed",
        97: "the proxy handshake failed",
      };
      if (code !== 0) fail(`outbound proxy request failed: ${reasons[code] || "check the proxy and upstream connection"}`);
      else if (!settled) fail("outbound proxy returned no HTTP response");
      else stream.end();
      cleanup();
    });
    stream.on("close", () => { if (!finished) child.kill("SIGTERM"); });
    stream.on("drain", () => child.stdout.resume());
    child.stdout.on("data", (chunk) => {
      if (settled) {
        if (!stream.write(chunk)) child.stdout.pause();
        return;
      }
      pending = Buffer.concat([pending, chunk]);
      while (!settled) {
        const end = pending.indexOf("\r\n\r\n");
        if (end < 0) {
          if (pending.length > 65536) { fail("outbound response headers exceed 64 KiB"); child.kill(); }
          return;
        }
        const lines = pending.subarray(0, end).toString("latin1").split("\r\n");
        pending = pending.subarray(end + 4);
        const status = Number(/^HTTP\/\S+\s+(\d{3})/.exec(lines.shift() || "")?.[1]);
        if (!status) { fail("outbound proxy returned invalid HTTP headers"); child.kill(); return; }
        if (status >= 100 && status < 200) continue;
        const headers = new Headers();
        try {
          for (const line of lines) {
            const colon = line.indexOf(":");
            if (colon <= 0) throw new Error("invalid header");
            const name = line.slice(0, colon);
            // curl decompresses bodies; these wire headers no longer apply.
            if (!/^(content-encoding|content-length|transfer-encoding|connection)$/i.test(name)) {
              headers.append(name, line.slice(colon + 1).trim());
            }
          }
        } catch { fail("outbound proxy returned invalid HTTP headers"); child.kill(); return; }
        settled = true;
        resolve({ status, headers, body: Readable.toWeb(stream) });
        if (pending.length && !stream.write(pending)) child.stdout.pause();
        pending = Buffer.alloc(0);
      }
    });
    child.stdin.end(config);
  });
  const location = result.headers.get("location");
  if (![301, 302, 303, 307, 308].includes(result.status) || !location) return result;
  // Never forward provider credentials to a different origin. The redirect
  // response is drained first so the curl process and private files are reaped.
  for await (const _ of result.body) { /* drain */ }
  const next = new URL(location, target);
  if (next.origin !== target.origin || !["http:", "https:"].includes(next.protocol)) {
    throw new Error("outbound proxy refused a cross-origin redirect");
  }
  if (redirects >= 20) throw new Error("outbound proxy exceeded the redirect limit");
  const method = (options.method || "GET").toUpperCase();
  const switchToGet = (result.status === 303 && method !== "HEAD") ||
    ([301, 302].includes(result.status) && method === "POST");
  const nextOptions = switchToGet ? {
    ...options,
    method: "GET",
    body: undefined,
    headers: Object.fromEntries(Object.entries(options.headers || {}).filter(([name]) =>
      !/^(content-type|content-length|transfer-encoding)$/i.test(name))),
  } : options;
  return networkFetch(next, nextOptions, redirects + 1);
}
