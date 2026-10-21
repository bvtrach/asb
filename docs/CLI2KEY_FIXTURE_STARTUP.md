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
only absence of one PID. Endpoint refusal corroborates the owned-child checks;
it is not ownership proof for arbitrary listeners.

This is a fixture-only repair. It does not change production bridge port
semantics or claim to repair the separate production free-port allocation
protocol. The success fixture also retains bounded diagnostics on failure.
Focused source verification and independent review precede broad qualification.

## Signed source checkpoint

The implementation follows this reservation design for both original
explicit-port fixtures. Parent descriptor flags are checked unchanged, with the
assertion deferred until after ASB finishes. The parent listener closes before
output collection. A permanent regression retains both the occupied-port
negative (real EADDRINUSE) and exact-FD-adoption positive, including bridge
reaping and final endpoint refusal. A second regression writes 16 KiB of actual
fixture stderr and proves only 8192 bytes are retained without swallowing its
failure. The successful catalog fixture also writes and checks a real diagnostic.

The helper intentionally retains existing std `wait_with_output` semantics:
an output-collection error can still lose direct-child cleanup confirmation.
There is no new assertion or fallible callback between spawn, dropping the
parent listener and that collection primitive. This narrow repair does not
claim to solve generic test-harness unwind/stream custody or import a private
production reaper to address it opportunistically.

Six focused cli2key tests passed with zero failures/ignored tests in
`target/cli2key-fixture-focused.log`. The immutable red source is `a01dde7a`;
`target/cli2key-fixture-red.log` retains its deliberate failure and EADDRINUSE
evidence. Actual main-30 ASB was built first; its SHA-256 remained unchanged:
`169aa0af5b52021b82f12fd0eaa0145fe615c5df42b3108e1219168ff22b79f7`.
At that historical checkpoint, production source, Cargo files and version
0.2.30 were unchanged. Entire
offline target and strict lint verification follow independent source review;
hosted CI, main repair and release remain pending.

## Reviewed offline-target qualification

Root independently approved source `7b723b0a` and the terminal focused/broad
evidence. The entire `offline_cli` target passed 94 tests, failed zero and kept
5 existing optional tests ignored; those opt-ins are not qualified. Strict
offline all-target/all-feature Clippy and formatting passed. Logs are
`target/cli2key-fixture-offline-cli.log`, `target/cli2key-fixture-clippy.log`
and `target/cli2key-fixture-fmt.log`. The before/after actual CLI hash files
`target/cli2key-fixture-qualified-{before,after}.sha256` match the hash above.

The source checkpoint is unchanged; this qualification addition is docs-only.
No local full-crate suite, hosted/package CI, PR publication, merge or release
is claimed by that historical checkpoint. The then-planned main-30 repair was
subsequently superseded by the peer main-31 integration below.

## Main-31 integration and coordinated patch 32

Actual main advanced to `cabaa576` through peer PR294 (version 0.2.31).
The signed integration `01e3af38` retained that exact production, Cargo, workflow
and version tree; its only delta was this document and the fixture repair in
`offline_cli.rs`.
The incoming cassette compatibility changes remain: fresh fake clients repair
their own task material, and recorded output is read as a bounded cassette
archive with complete-track validation rather than assumed plain JSONL. CI's
new capture-session and tape-normalization script packaging checks are retained.

The preceding version-30 test counts/hash are historical evidence, not proof of
this integrated head. Its version-31 build finished without starting tests.
Root then authorized coordinated patch 0.2.32: only the package version in
Cargo.toml and Cargo.lock changes alongside this fixture/document repair.
The old version-30 delivery is superseded; the intended repair release includes
peer main-31 without tagging version 30 or replacing/reusing the peer release.
A fresh actual patch-32 CLI build, complete offline target and strict lint/format
qualification are required; earlier hashes/results are not substituted.
Hosted CI, PR, merge and release decisions remain with root after exact review.

## Current patch-32 offline qualification

Root independently approved integrated/versioned source `bec846d1`: all
production/workflow bytes match actual main `cabaa576`, apart from the two
authorized Cargo package-version lines. The fixture patch's stable ID remains
`ce3d48b6f87c643726c491d91996ea1b19c3324e`, identical to the reviewed repair.

A fresh actual 0.2.32 CLI build and new complete `offline_cli` run passed
94 tests, failed zero and ignored 5 existing optional tests (not qualified).
Strict offline all-target/all-feature Clippy and formatting passed on this
integrated source. These are new results, not reuse of the historical version-30
qualification. Logs are `target/cli2key-fixture-v032-{build,offline-cli,clippy,fmt}.log`.
Actual CLI hashes before and after the terminal sequence matched:
`4bad6979166c39b8c00f9dc38b5820def40239fb8bf5312ae30ff195f9bfd243`.
The hash files are `target/cli2key-fixture-v032-{before,after}.sha256`.

This qualification addition changes only this document. No local full-crate
suite or hosted/package CI is claimed; PR publication, merge and any release
remain root-owned pending steps. Version-30 source/evidence and runtime34's
separate source checkpoint remain immutable.
