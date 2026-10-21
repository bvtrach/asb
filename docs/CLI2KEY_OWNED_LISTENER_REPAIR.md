# Owned compatibility listener: delivery43 design, awaiting source approval

Actual main42 base `32329d76` (PR307), not runner-phase43. Version staged43.
Only signed diagnostic cherry-pick `09a9a321fbeb89a63a882b1a8a3ca31e37e85ee0`
and Cargo version differ so far; NO production repair. Runner-phase43 remains
held separately and will receive a later delivery slot. Class44/45/47 untouched.
AGENTS/original report read; development/architecture identity checked against
already fully read qualified source. All new build/evidence lives in HOME.

## Baseline and honest cause boundary

PR308 CI39690369033's compatibility EOF/not-ready failure is historical fact;
its compatibility stderr was discarded, so its exact cause remains UNKNOWN.
Signed diagnostic `fc86aab0` on qualified43 and the exact test-only cherry-pick
on main42 prove a real release-free-port→Node-bind race reproduces that same
failure signature with actual EADDRINUSE. It is not proof of historical cause.
The original CI log remains preserved and was copied to the diagnostic tree.

Main42-based actual built FIRST (own HOME target, copied verified idle diagnostic
debug cache, old cache unchanged): `target/cli2key43-main42-baseline-build.log`,
exit0,13.04s. Actual/frozen `target/qualification/cli2key43-main42-baseline-asb`
SHA256 `46366485b34b1f2b05f7159e3799ea70b34ee4f606e06c0e0b5a1b1a13c63a6e`.
The controlled diagnostic passed1/0/0 in0.47s, including healthy and occupied
compatibility-port cases, both exact owned children gone/listeners closed before
assertions. No production edit or build/test overlap. This baseline does not
qualify the future repaired source. Copied old artifacts are not evidence.

## Root-selected mechanism B: one owned reservation, inherited by Node

Keep upstream codex-bridge's nonzero CODEX_BRIDGE_PORT/free-port/external-endpoint
behavior unchanged. It cannot use port0; ASB's separate compatibility hop can.
Rust binds a fresh TcpListener to exactly127.0.0.1:0 and owns the resulting
listening socket. Never drop it and ask Node to rebind its numeric port.

Proposed private `ProxyReservation` stores the actual TcpListener and expected
SocketAddrV4. Its original FD stays CLOEXEC in the parent, including while other
threads fork. Configure the ORIGINAL Node Command, with private `--listen-fd`
argument, then install a minimal child-only pre_exec closure holding the exact
listener owner. The closure only checks F_GETFD and clears FD_CLOEXEC with
F_SETFD on that child's descriptor table; it returns errno on failure. No global
parent flag clear, guessed FD, parent-pid scan, shell reconstruction or broad
pass-fds list. Every unrelated child retains normal CLOEXEC behavior.

The bundled Node hop accepts ONLY this explicitly inherited socket for ASB
startup, calling server.listen({fd}). No address-based rebind/fallback or foreign
listener adoption on invalid FD. It reads server.address() after successful
listen and emits the actual address. Rust compares that address to its STILL
OWNED reservation; a matching number alone is not ownership evidence. Custody,
the inherited object and private receipt TOGETHER establish this contract.
The authoritative public base_url is constructed locally from the verified
loopback address, never copied from an arbitrary emitted URL.

Node's documented listen(handle) adopts an already bound FD and is unsupported
on Windows; this matches the existing local cli2key Unix-only backend, whose
nonUnix explicit refusal/external-endpoint behavior remains unchanged. Official
API: https://nodejs.org/api/net.html#serverlistenhandle-backlog-callback . Do not
assume adoption alone fixes descriptor inheritance: runtime tests must show
Node-spawned curl/other children cannot retain this listening FD. Node children
must use only explicit stdio; no new native addon or global host setting.

## Exact typed receipt and bounded observer

Proposed private `ProxyReady` (serde deny_unknown_fields):
`{"schema_version":1,"host":"127.0.0.1","port":12345}` on one line prefixed
`ASB cli2key proxy ready `, newline terminated. Exact schema1, literal IPv4
loopback host, integer u16 port1..65535, and agreement with the owned reservation
are mandatory. Reject malformed JSON/types/trailing payload, zero/out-of-range,
wrong host, mismatched port and duplicate reserved frames (even identical).
Do not recognize the old bare ready line as usable address data. Both binary
and embedded script ship together; no public user API or provider auth changes.

Replace the proxy bool/string path with explicit private `spawn_bridge` and
`spawn_proxy` entry points (or an equally typed internal expectation/result).
Upstream key parser, trimming/override/redaction and arbitrary upstream output
behavior remain unchanged. Proxy state is Pending→Ready→Fault, or Pending→Fault;
first fault is sticky. A bounded shared observation stores only typed ready or
fixed error category, not raw output/key. Check state during startup, final
confirm_ready and finish. No EOF-as-ready or first-good-line bypass of later
reserved duplicate/malformed facts. Early process exit still fails admission.

Keep original5s startup and64KiB/64messages/4KiB line limits. Before first ready,
all existing startup output bounds apply. After ready, continue bounded-line
recognition of the reserved proxy prefix; arbitrary other output stays
drain/discard, not retained or newly interpreted as provider errors. Overlong
reserved lines and excessive reserved protocol frames fault boundedly; overlong
nonreserved later lines are discarded with bounded framing memory. Reader state
cannot be erased by a full single-result channel. Use shared bounded state plus
notification, not an unbounded event queue or blocked writer.

Finish must observe proxy protocol output through owned child exit and final
nonblocking drain within the EXISTING cleanup deadline before declaring the
observer settled. Do not set stop before examining already buffered final
reserved facts. No wait-for-EOF forever if a duplicated descriptor survives;
unknown observer settlement remains an explicit bounded cleanup diagnostic.
This is a local protocol correctness check, not a new authenticated-runtime
policy. A duplicate arriving after an earlier snapshot is surfaced by finish,
not claimed impossible; no finite admission check promises future behavior.

## Resource transitions and cleanup

Reserve before proxy spawn; failed preparation/spawn closes only our parent
reservation and never targets another listener. The Command closure keeps its
exact owner alive through fork/exec. After spawn the Node child owns its own FD;
the Local session retains the parent reservation through startup and session.
Close the parent reservation on finish after cooperative Node shutdown/exit
checks; verify the actual port is then closed, not merely that Node exited.
On startup error/unwind, existing Local cleanup still attempts all exact owned
children/readers; release parent reservation explicitly without pretending its
close stopped an unconfirmed child. That child's FD remains in its own process
until exit, under existing pre-established OwnedReaper custody if confirmation
fails. No new Drop worker, unbounded join, kill-by-name or retry loop.

Unconfirmed cleanup remains unconfirmed; no fabricated cooperative success.
The reservation is an owned socket, not a foreign pathname to delete. Retaining
an uncertain child cannot cause a parent-held reservation to be presented as a
healthy service. On normal close all duplicate parent/Command handles must be
gone, with actual port reuse/connection-refusal evidence. Private raw proxy
stderr collection remains TEST-LOCAL; do not expose sensitive production output.

## Files and owning tests to migrate after approval

Production scope: implementation.rs proxy reservation/verified URL handoff;
custody.rs typed proxy observer/reservation lifetime; custody_unsupported.rs
signature-only agreement without changing refusal; embedded cli2key-proxy.mjs
inherited listen/receipt. Existing upstream key/custody/public Bridge and
ProviderSession APIs stay unchanged. No generic runner, phase43 or installer
callback modifications. Preserve configured original Command semantics.

- Parser matrix: malformed JSON, absent/bare marker, schema, zero/out-of-range,
  wrong host/port, duplicate identical/conflicting, early EOF/exit, oversized
  reserved line/message/byte flood, ordinary later-output drain policy.
- Actual built-in Node server adopts owned FD, reports matching bound address,
  serves fixture models and closes on normal/startup-error finish. Real parallel
  sessions have distinct still-held sockets. Other occupied sockets/sentinel
  remain live. Invalid FD never falls back to an arbitrary numeric listener.
- Parent FD_CLOEXEC unchanged during concurrent spawn. A child probe and actual
  network-fetch curl cannot retain the listening socket. Confirm port closure
  after normal finish, startup fault, reader fault and reaper handoff scenarios
  with honest bounded uncertainty; no outside-process kill.
- Migrate existing Python fake-proxy receipts deliberately; preserve upstream
  fixtures/key parser tests and actual lifecycle/custody/request cancellation.
- Old controlled `--port` contention is preserved as historical evidence, NOT
  silently reused as if it still targets the repaired interface. Adapt the
  owning test to assert no free-port proposal/rebind: a separately occupied
  socket is irrelevant to the pinned inherited listener; startup succeeds,
  occupied sentinel survives and all owned children/socket handles settle.
- Run unchanged original success/bridge-cleanup offline test and full cli2key
  families, strict lint/fmt, then independent source review BEFORE fresh full.

No production implementation until root approves this exact design checkpoint.
No blind CI rerun, budget increase, skip, fake-success or new authorization gate.
