# cli2key mock startup reservation

## Observed failure and deterministic reproduction

Postmerge-30 local job 119428982853 in run 39659641538 failed only
`cli2key_success_stops_its_mock_bridge_after_model_discovery`: the bridge closed
startup stdout with exit status 1. Its Node stderr was discarded. Hosted and
premerge checks passed. The retained old log therefore does not identify the
actual Node failure; an address collision is a supported hypothesis, not a
proven attribution of that historical run.

Both explicit-port bridge fixtures select an ephemeral loopback port by binding
`:0`, then immediately drop that listener before launching Node. Another owned
listener can occupy that port during this gap. The deterministic regression
keeps the selected listener alive through launch: the old Node fixture attempts
a second bind and exits 1 with the same public startup failure. Bounded
fixture-only stderr records `EADDRINUSE`; the failed bridge is reaped before the
test asserts. This proves the TOCTOU defect without relying on probabilistic
stress, providers, credentials or host configuration.

The red source checkpoint and `target/cli2key-fixture-red.log` preserve that
failure. A maximum 8192-byte fixture diagnostic file records actual stderr
writes plus bounded uncaught Node error fields. Monitoring does not swallow the
Node exception or change its exit status. Production stderr handling is not
changed and no real provider output is retained.

## Narrow repair design

The inert Node fixtures will adopt the exact reserved listener using
`server.listen({fd: ...})`, instead of selecting a port and rebinding it.
Only the forked ASB child clears FD_CLOEXEC on this owned descriptor in an
async-signal-safe pre-exec fcntl call. Parent descriptor flags never change, so
parallel unrelated launches cannot inherit it. The parent's listener owner
closes immediately after successful spawn, before waiting for ASB.

ASB and its bridge/proxy children retain their inherited copies until their
normal owned lifetimes end. The bridge adopts its copy; ASB and any compatibility
proxy copy close on process exit. The test must verify both bridge reaping and
connection refusal on the former listening endpoint after ASB returns, not
only absence of one PID. Failure/unwind must still retain normal child custody.

This is a fixture-only repair. It does not change production bridge port
semantics or claim to repair the separate production free-port allocation
protocol. The success fixture also retains bounded diagnostics on failure.
Focused source verification and independent review precede broad qualification.
