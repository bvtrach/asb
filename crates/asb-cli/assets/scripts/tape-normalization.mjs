// Preserve legacy workspace keys without retrying path searches at every byte.
export function normalizeWorkspacePaths(body) {
  const runs = /\/?[A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*/g;
  return body
    .replace(runs, (run) => run.replace(/^\/?(?:[A-Za-z0-9_.-]+\/)+workspace-concurrency-\d+(?:\.asb-agent-runtime)?/, "<asb-workspace>"))
    .replace(runs, (run, offset, source) => {
      const following = source[offset + run.length];
      const suffix = following === "/" || following === '"' || following === "\\" ? following : "";
      const normalized = (run + suffix).replace(/^\/?(?:[A-Za-z0-9_.-]+\/)+workspace(?=\/|["\\])/, "<asb-workspace>");
      return suffix ? normalized.slice(0, -1) : normalized;
    });
}
