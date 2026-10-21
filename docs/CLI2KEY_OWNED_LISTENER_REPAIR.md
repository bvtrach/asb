# Owned compatibility listener: delivery43 design and focused source proof

Actual main42 base `32329d76` (PR307), not runner-phase43. Version staged43.
The initial design checkpoint `e4e82879` contained only signed diagnostic
cherry-pick `09a9a321fbeb89a63a882b1a8a3ca31e37e85ee0` and Cargo version.
Root approved that design before implementation. Runner-phase43 remains
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

No blind CI rerun, budget increase, skip, fake-success or new authorization gate.

## Implemented source and focused evidence, pending final independent review

Signed `fec09d246744be6ca8591176ba0d1795a6b48b9f` implements the approved
contract and its first matrix. Root read its entire production diff and asked
for additional actual descriptor-fault/reservation-release tests. Signed
`a391c7f84a816f75a23dc0e5974532c32651cf6d` adds them. Its production delta is
only extraction of private spawn_reserved_proxy, called by the same spawn_proxy
entry, enabling tests to pin the actual reservation address before failures.
No implementation policy or receipt condition was relaxed.

Production files are exclusively the cli2key implementation/custody/private
proxy module, unsupported-backend signature and embedded proxy script.
All other production Rust/assets (including runner42, termination, agent,
workload, capture, publication, checkpoint and selection) stay byte-identical
to actual main42. Cargo changes only stage43. No runner-phase43 implementation
was imported. Public Bridge/ProviderSession and external/upstream behavior stay
unchanged; only the private owned proxy startup contract changes.

The main42-based historical controlled EADDRINUSE evidence above remains
immutable. Its fixture was explicitly migrated after the repair: there is no
released --port proposal now. The wrapper confirms it cannot bind the STILL
HELD inherited listener, holds a separate unrelated occupied socket instead,
execs actual Node, and requires successful model discovery and complete owned
cleanup. This is not the old contention test silently passing unchanged.

Every actual executable built FIRST, then was frozen for runtime. Own HOME
CARGO_TARGET_DIR only, locked/offline/jobs1/incremental0, no concurrent relink.
On fec09d24: build-v2 exit0,13.97s; frozen actual SHA256
`e4243cc797e179e9440ed88b45724c3eeeab4026a73d250612b37cbc4ed32c68`.
Library cli2key24/0/0,10.88s and offline cli2key7/0/0,1.83s passed, with matching
before/after actual/frozen hashes. These earlier runs remain in target logs.

Final focused source a391c7f8: `target/cli2key43-faults-build.log` exit0,13.36s.
Actual and frozen `target/qualification/cli2key43-a391c7f8-asb` SHA256:
`63c8c0a0ac80bee1affe534bca0d28670c97845dd9a61310a84ff9dd2522ddae`.
Before, after and final actual/frozen hashes agree. Post-focus libtest hash is
recorded separately; no pre-run libtest stability claim is made.

| Retained target log | Terminal result |
| --- | --- |
| cli2key43-owned-listener-lib-focus-v2.log | 26/0/0ignored;24.73s compile,11.03s runtime |
| cli2key43-owned-listener-offline-focus-v2.log | 7/0/0;1.98s runtime |
| cli2key43-layout-focus.log | 1/0/0 |
| cli2key43-owned-listener-clippy.log | Strict workspace/all-target/all-feature -Dwarnings exit0,15.61s |
| cli2key43-owned-listener-fmt.log / diffcheck.log | exit0 / exit0 |

Final total34 focused outer passes, no failures/ignored; prior runs overlap
and are not added to that count. Expected caught unwind panic text is part of
passing custody tests, not an unreported failure. All handles are terminal.
No failed compilation or runtime test occurred in the repaired-source sequence.

The actual additional cases run the bundled Node script with invalid999999 and
an inherited owned REGULAR file descriptor; both fail before readiness, preserve
the foreign sentinel listener and file bytes, and release the separately pinned
real reservation without numeric-port fallback. Another test pins each address
before reader/pre-spawn/startup-error/unwind/uncertain-reaper scenarios, requires
truthful uncertainty for the latter, awaits exact owned reaper settlement,
observes pinned child exits and reclaims the port. No process outside admission
is killed. Generic old custody cases remain intact.

Other actual tests cover four concurrent owned Node sessions with distinct
held ports, reclaim after finish (including Command pre_exec owner release),
unrelated sentinel survival, parent FD_CLOEXEC unchanged during unrelated child
execs, no listener socket in actual owned curl's /proc FDs, normal curl/request
cancellation, actual malformed/duplicate/late frames and unchanged original
CI-failing success/bridge-cleanup plus concurrent-replica cases. Parser tests
add exact schema/address/oversize matrices and bounded later-noise discard.

Scope is native Linux x86_64/local Node/Python/curl fixtures; no credentials,
live provider/SDK, privilege changes or Windows/nonLinux runtime qualification.
Existing nonUnix local refusal remains, not a new platform removal. Root must
approve source/focused evidence before full qualification or PR/push. Historical
CI cause is STILL UNKNOWN despite the independently demonstrated/fixed race.

The only new untracked source-tree artifact was exact empty
crates/asb-cli/.asb/install.log from the existing tool adapter-hook test; it is
recoverably moved into target/fixture-preservation after all runtime handles
are terminal. No user file, old evidence or unrelated directory is removed.

## Full qualification STOP and controlled high-FD RED

The earlier focused proof is NOT full qualification. Authorized full command at
b399b353 stopped exit101 in the library:1182passed/3failed/13ignored,20.96s,
`target/cli2key43-full.log`. Two assertions attempted immediate numeric port
rebind after finish and observed EADDRINUSE; those alone cannot distinguish the
original socket from a different parallel actor reacquiring that ephemeral port
or TIME_WAIT. The third failure was stronger: actual owned curl's FD table
contained the exact original listener socket inode148678075. That invalidates
the production inheritance claim. No unchanged full retry was performed.
Actual/frozen63c8c0a0 stayed unchanged, recorded after failure.

Signed TEST-ONLY `76be20424f13743476180f4cbab52a83c6d8d77c` extends the actual
bundled Node/curl fixture: acquire256 owned inert /dev/null FDs, reserve a high
listener, release those inert FDs, then admit actual Node. It observes the exact
listener in curl, but defers the intended assertion until AFTER cooperative
Node exit, pinned curl exit, connection and reservation cleanup. No foreign
descriptor, shared flag or process is closed/killed by the test.
`target/cli2key43-high-fd-red-v2.log`:exit101,0passed/1intendedfailed/0ignored,
0.19s; parent listener FD260, curl_inherited=true. This is a deterministic real
Linux defect, not speculation about the historical PR308 CI failure (still
unknown). The first short-name --exact selector accidentally ran0tests; its
`high-fd-red.log` is preserved as selector failure, NOT proof. The corrected
selector executed the named1test. Actual/frozen63c8 remained unchanged; the
post-run libtest hash is recorded with no pre-run libtest claim.

Local Nodev26.3.0 reports libuv1.52.1. Primary libuv source shows inherited TCP
adoption sets nonblocking but not CLOEXEC, and startup's inheritance scan stops
at the first invalid descriptor above15. These explain why low-FD focused
cases passed while a high descriptor above a hole can escape to curl:
https://github.com/libuv/libuv/blob/v1.52.1/src/unix/core.c and
https://github.com/libuv/libuv/blob/v1.52.1/src/unix/tcp.c . This source explanation
is not a promise that undocumented startup scanning is a safe handoff API.

## Revised handoff proposal BEFORE implementation

Do NOT clobber fixedFD3 in pre_exec (it may be Rust's exec-error transport),
close guessed ranges, rely on Node's startup scan, add an addon, or change global
host FD limits/settings. Native parent CLOEXEC remains correct; the missing
boundary is the Node→curl exec AFTER Node has adopted a deliberately inheritable
listener. Node has no documented JavaScript fcntl API to set that flag directly.

Preferred generic private utility: `__asb-owned-fd-exec` receives at most a
small bounded list of `(fd,dev,ino,kind)` plus delimiter and ORIGINAL command/
argv. Node obtains full-width identity using fstatSync with bigint:true and
decimal strings, never Number inode conversion. In the same tracked child PID,
the helper validates bounded decimal syntax, fd>=3, nonduplicate FDs and known
object kind; fstat compares exact dev/inode/kind. EBADF or a mismatching reused
FD is NOT closed and does not become a new authentication/permission refusal.
Only an exact admitted owned object receives CLOEXEC (or exact close) before
Unix exec of the original raw command/argv with inherited cwd/env/stdio. Other
I/O errors/malformed control data fail with fixed bounded diagnostics, no raw
payload/key/program echo. No retry or foreign-object cleanup occurs. Same PID
preserves Node's existing exact child cancellation and close accounting.

The cli2key adapter supplies this launch prefix; a generic OPTIONAL launch
specification in network-fetch's curl path selects program+prefix. All other
users keep existing direct curl behavior, default fetch behavior, stdin private
config, scratch ownership, children Set, cancellation and redirects unchanged.
Redirect recursion must retain the exact supplied launch context. The generic
helper and shared HTTP module contain no provider-ID branch or provider policy.
The listener is not delegated to curl; the helper exists only to remove its
exact inherited reference before curl executes.

Runtime identity needs explicit review, not `/proc/self/exe` evaluated from
Node (that is Node, not ASB). Proposed normal Linux path: Rust opens/pins its
actual /proc/self/exe ELF, inherits that exact file into Node, and Node executes
/proc/self/fd/N. Include that owned regular image FD in the helper's bounded
identity list so curl cannot retain it either. Executing the ELF opens its image
before CLOEXEC takes effect; helper safely tolerates the image FD already gone.
Parent image/listener refs remain CLOEXEC; only the intended Node child inherits
them. Node holds them until its normal/uncertain cleanup, not until a guessed
pathname is deleted. Existing loader-bundled ASB_RUNTIME_EXECUTABLE is a script
that must retain its explicit launcher contract: raw /proc/self/exe may be the
loader and a CLOEXEC fd script path can disappear before the interpreter reads
it. Proposal is to preserve that existing explicit launcher path/ownership
contract rather than invent a stronger immutable copy/authentication policy.
NonLinux Unix retains existing current-executable/explicit launcher behavior
with honest source-only scope. Root must approve that runtime distinction before
new production code; no silent raw-binary fallback for bundled loader runs.

Additional mandatory checks: exact socket match removed in helper child,
already-CLOEXEC/EBADF tolerated, reused unrelated socket/file preserved, huge
bigint identities and malformed/duplicate/std-FD arguments rejected without
secret echo; actual high-FD Node→helper→curl preserves tracked PID, bytes and
cancel/quiescence; pinned ASB image and explicit launcher paths execute helper
correctly; normal low-FD paths and all existing proxy consumers remain intact.
Replace only ambiguous immediate numeric rebind assertions with exact original
socket-inode disappearance/custody proof plus sentinel survival. Do NOT weaken
the actual curl FD-table inheritance assertion. Any future full requires fresh
source review and actual build/freeze/focused evidence first.
