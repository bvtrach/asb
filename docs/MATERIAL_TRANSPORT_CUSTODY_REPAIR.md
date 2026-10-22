# Class50: host material and Docker transport custody

## Status and scope

Design and RED-fixture plan only, based on actual main
`66fb5fda` (PR309 merge). No production, fixture, version, wire, or preparation
change is implemented here. No new failure has been reproduced by this
checkpoint. The source findings below are not claims of an observed Docker leak.
Root must approve both architecture and fixtures before implementation or RED
execution. Parked class46 `ede2ffc7` and its source/cache/evidence are untouched.

AGENTS.md, DEVELOPMENT.md, ARCHITECTURE.md and all four MULTITRACK documents are
byte-identical to the previously fully read qualified `551fe3c` baseline. The
material/Docker/task-server/resource and Rust remote/controller sources cited
below are also unchanged between those checkpoints. Documentation's existing
one-second direct-reap bound is not evidence of ownership after that bound.

## Source trace and existing evidence

1. `assets/benchmarks/asb_material_root.py:35-52` launches the selected Docker
   CLI against the exact owned container. `asb_material_environment.py:105-106`
   and `:157` use the same private helper transport for service observations.
2. `asb_material_exec.py:15-25` checks input/output limits and deadline before
   spawning. Lines29-57 create two daemon readers and a daemon stdin writer.
   Lines61-67 watch both child and workers, including inherited-pipe lifetime.
   Lines72-89 kill the direct child, wait at most one second, and join each
   worker for 0.1s. Still-live workers retain buffered streams and can outlive
   the call. Unreapable children produce an explicit error but are not handed
   to an established owner. A kill/wait/worker-start exception can interrupt
   this cleanup before all independent resources are handled.
3. SWE `asb_swebench_task.py:103-138` and Harbor `asb_harbor.py:425-433`
   normalize material errors to ExecutionError without copying helper output.
   `asb_task_server.py:155-165` emits ERROR. It does not itself end a connected
   session for every operation error; the Rust caller's cancellation is
   important. A continuing caller must never interpret that error as material.
4. `src/workloads/remote.rs:504-539` requests initial material during prepared
   admission. Failure calls `controller.cancel(CANCELLATION_TIMEOUT)` and
   composes cleanup errors. No provider/agent admission follows failed material.
5. `asb_task_server.py:51-58,66-84` marks EOF/cancel/malformed input interrupted;
   only explicit CLOSE clears that flag at124-126. The task context exits
   before CLOSED is emitted at177-181. SWE exit141-158 and Harbor exit577-611
   preserve interrupted resources through their existing journals.
6. `asb_resources.py:216-232,244-280` validates daemon identity, whole declared
   population, exact resource labels/names, stops each owned container, reloads
   and requires Running=false. This contains guest helper processes on
   confirmed stopped preservation. It is not host Docker CLI ownership, a grade,
   a retry receipt, or proof of an uncertain pending daemon-side create.
7. `asb_swebench.py:883-892` and `asb_harbor.py:630-631` enclose serve in
   DockerSession. Thus CLOSED acknowledges task-resource cleanup before outer
   DockerSession teardown. `asb_docker.py:212-220` closes SDK then connections,
   including when client.close throws. Entry failure208-210 also runs teardown.
8. `asb_docker.py:68-85` creates a socketpair and separate-session CLI child per
   connection. Close87-105 detaches the whole child list, skips reaped leaders,
   TERM-signals unreaped leaders' groups, waits2s, then KILL and unconditional
   wait. A first unexpected exception skips remaining children. The timeout is
   per child, not aggregate; no maximum retained child population is enforced.
9. `controller/unix.rs:294-308` waits for controller exit after CLOSED. An outer
   cleanup deadline does not falsely return confirmed successful controller
   exit while Python is stuck closing transports. On timeout/poll uncertainty,
   the Rust keeper retains the exact Python child (370-388,420-439). It does
   not own Python's separate-session Docker CLI children. Killing Python can
   terminate daemon workers while leaving those host children outside custody.
   Cancellation's wait deliberately does not require a success exit; resource
   acknowledgement and transport uncertainty must not silently be conflated.

Existing offline tests reviewed, not rerun here:

- `tests/benchmark_material_exec.py:94-105,118-153`: overflow, blocked input,
  cancellation, direct-child scope, inherited output, explicit unreapable error.
  The inherited descendant self-exits after0.5s. These prove bounded caller
  return and safe errors, not final host-worker/child settlement.
- `tests/benchmark_docker.py:49-68,96-129`: failed SDK close still closes
  connections; cooperative direct CLI children reap; socketpair failure closes;
  unrelated child survives. No first-child signal denial/stalled wait matrix.
- `tests/benchmark_task_server.py:196-207`: only explicit CLOSE authorizes
  normal teardown, otherwise stopped-preserved. Resource-journal tests validate
  exact ownership and observed stopped states independently.
- Rust controller cancellation tests validate in-flight draining, finite
  deadline, disposition and child reap, but do not prove grandchildren custody.

The concrete defect candidates are abandoned host owners after failure and
unbounded/partial connection teardown. Ordinary healthy paths are not alleged
broken. Guest-container containment must be preserved, not replaced by claims
that stopping docker exec kills its guest.

## Proposed cohesive ownership boundary (requires review)

### Bounded exchange, not detached pumps

On qualified Unix hosts use selector-driven nonblocking owned pipe/socket IO
for material input/output. Preserve1MiB input, existing per-stream4KiB default
and64KiB maximum, exact schema and source limits. Bound buffers while reading,
use finite per-iteration read/write quanta, retry Interrupted, distinguish read
failure from EOF, and keep one absolute operation deadline plus finite cleanup
allowance. Close only owned descriptors; no host stdout/TTY flag mutation.
No reader/writer threads and no blocking stream-close locks remain after return.
Malformed output never earns fallback material or a fresh budget.

### Prefer existing gate custody; broker is an alternative, not a decision

The smallest candidate is to wrap the exact opaque controller Command in the
existing typed launch/ManagedGate mechanism on Linux, not introduce a bespoke
Python broker. `process_gate/managed.rs:24-89` establishes receipt, optional
control and OwnedReaper before spawn. `process_gate.rs:162,218-245` establishes
an exclusive subreaper before launching its principal, retains supervision on
descendant cleanup failure, and emits QUIESCENT only after actual settlement.
`managed.rs:305-354` hands the still-supervising gate to wait-only custody on
uncertainty; it must not be converted to controller's current kill-Python path.
`process_cleanup.rs:61-76` already distinguishes terminate from wait-only.

This gate's principal would be the existing Python task controller. During
normal operation Python owns its direct CLI children and existing SDK socket
lifetimes. On Python exit, the exclusive gate adopts orphaned host transports,
including separate-session children; no cached PGID discovery is needed.
DescendantScope uses exact kernel child checks/pidfds, not shared-runner global
subreaping (`process_gate/descendants.rs:1-26,61-103`). Signal denial leaves the
gate alive without QUIESCENT; it cannot promise privileged termination. The
Rust Controller owns ManagedGate, and its pre-established wait-only reaper owns
that exact gate after a bounded caller failure. On actual ASB process death
there is no crash-persistent Rust thread; the independent gate remains the
kernel subreaper/owner and its own owner-loss path continues descendant cleanup.
Do not promise a surviving Rust waiter after ASB exits. The fixture guardian
must explicitly adopt/reap the gate in owner-loss tests and observe eventual
termination after real descendant release.

Required integration, if approved after RED: typed opaque gate command wrapping
(no argv/env reconstruction), startup receipt/stopped/resume before controller
protocol readiness; nonblocking Python exchange; one cleanup deadline; separate
principal exit and tree-quiescence observations. A valid CLOSED remains only a
resource acknowledgement until Python exit AND gate receipt. On timeout/receipt
loss transfer the supervising gate wait-only, never kill it to obtain a clean
exit. The current Controller::child test seams require explicit migration, not
fabricated direct Python status. This is a real cross-seam change requiring
source review; reuse does not itself prove correct cancellation semantics.

A gate around the complete controller avoids a new broker protocol, descriptor
passing and separate broker owner. It also contains descendants of material
helper transports after interpreter exit. It does not by itself remove blocked
Python pumps during the live session: nonblocking exchange and finite sibling
cleanup remain necessary. Its existing cleanup implementation must be checked
for fair sibling processing after a single EPERM; do not assume aggregate
progress merely because a subreaper exists. If that prerequisite requires a
generic gate semantic change, submit it separately for root review before
changing a previously qualified ownership contract.

The alternative below is retained for explicit comparison, not adopted.
A broker cannot be its own unexplained persistent owner: it would need to be
launched as an owned gate child, remain supervised through caller exit and
return the same actual quiescence receipt. Without that, it only moves the
indefinite wait invisibly and is rejected. Since gate reuse can own the whole
controller tree, current source evidence does not justify that extra architecture.

### Alternative broker requirements (only if gate reuse proves insufficient)

A pre-established thread inside Python cannot own a child beyond Python exit.
The proposed strong design is one private transport broker process per owning
DockerSession, established before its first transport spawn. The broker is the
actual parent of every local material CLI and dial-stdio child from creation;
do not try to transfer wait ownership of an already-spawned child by PID.
Establish and handshake the broker before admitting work; startup failure
admits no child. Use owner-only inherited Unix IPC, bounded framing and SCM_RIGHTS
for owned connection descriptors, not a public listener or named host service.
The broker may serve only this controller/session, never arbitrary commands
from untrusted request fields: requests select already validated internal
transport operations/argv, preserving Docker selection and existing privilege.

Broker owns a bounded child population and multiplexes IO/polling, not one
thread/keeper per request. Provisional population limit must derive from actual
SDK pool/concurrency and one sequential material observer; do not arbitrarily
break previously valid ordinary reconnects. Reap settled children eagerly.
When capacity is exhausted or custody uncertain, reject new transport admission
with a bounded categorical error, never start a replacement broker. Root review
must settle the capacity policy before production implementation.

At explicit close or controller IPC EOF, stop admission, close every owned
connection, TERM eligible unreaped children, escalate within one aggregate
deadline, poll all children fairly, and report all failures without abandoning
siblings. No blocking child.wait/join on the caller. Numeric group signaling is
permitted only while this broker still owns the unreaped original leader;
never signal a cached PGID after leader reap. Prefer exact direct-child custody
and connection EOF; no process-name search or unrelated descendant enumeration.

After the finite foreground deadline the broker retains any exact unsettled
Popen/wait owners and closed transport descriptors, continues bounded polling,
and admits no more children. It survives controller exit when custody remains
uncertain. This is a deliberately retained cleanup owner, not evidence of
termination. No infinite supply of brokers/reapers is allowed. Tests must
observe this owner and its final exit after actual release, not kill it to hide
the unresolved condition. A normally settled broker exits and is reaped.

Root-elevated transports can reject an unprivileged TERM/KILL. Socket EOF may
make them exit normally; that must be observed, not assumed. EPERM must not
abandon siblings, mark the elevated child reaped, or retry via new privilege.
An unkillable child can only retain exact wait custody and report uncertainty;
there is no promise of forced privileged cleanup or bounded eventual death.
Root review must explicitly accept that retained-owner boundary versus choose
a different architecture. No sudo, root helper installation or host policy
change is authorized for this repair.

### Acknowledgements and failure precedence

Keep the existing task resource meanings: removed/not_created/stopped-preserved
remain resource observations. Preserve original material/operation error first,
compose finite transport-cleanup categories second, never raw helper diagnostics.
Transport uncertainty must not become successful prepared material or new
agent/batch admission. Existing valid grades/resource artifacts are independent.

The existing CLOSED wire alone does not certify broker settlement, and Rust
cancel currently accepts a nonzero controller exit after CLOSED. Before changing
that path, propose the exact private transport-settlement diagnostic/return
mapping to root; no new grade/status/resource-disposition or silent blanket
receipt rejection. In particular a confirmed guest stopped-preservation can
remain true while host transport custody is explicitly unresolved. No change
to normal close versus cancel authorization or preparation retry policy.

Non-Unix healthy behavior is preserved initially; no broad unsupported-host
refusal. Any legacy platform without bounded descriptor transport must retain
its old execution contract with an explicit unqualified gap, not claim this
Unix design qualified Windows. No general SDK execution/capture audit claim.

## Exact offline RED plan (not implemented or executed)

First build actual ASB from this exact baseline with a newly assigned exclusive
cache; freeze source/hash before runtime. No existing class46 cache use. Outer
fixture guardian owns all children, descendants and handshake descriptors and
has its own finite watchdog. Preserve failed logs and binary hash. All final
intended assertions occur after real owned cleanup and unrelated sentinel
survival. No real Docker daemon/SDK/provider/credentials/sudo/host configuration.

1. Actual Rust Controller::spawn/cancel launches a committed Python fixture
   entry using real task_server. Its minimal task uses actual ResourceJournal
   preservation with fake Docker client/container: real journal identity/labels,
   stop+reload+Running=false, durable interruption/quiescence files. No fake
   CLOSED bypass. Use real StdioConnections with inert Python fake Docker CLI.
   EOF and explicit cancel variants go through actual receive/error/context exit.
2. During real INITIAL_MATERIAL invoke actual material_exec.run. Fake CLI starts
   a guardian-owned indefinite pipe-holder, confirms parent/holder descriptor
   handshake, exits0. Deadline fails the observation. Observe blocked daemon
   reader count while task remains alive; then cancel, confirm journal stop
   and CLOSED, and actual controller exit. Classify workers gone at interpreter
   exit separately from host descendant settlement: do not call terminated
   daemon threads an escaped live worker. Guardian observes holder PID/FD alive
   after acknowledgement/exit, then releases/reaps it before intended assertion.
   This proves only fake host descendant behavior, not real Docker behavior.
3. Three actual socketpair connections, cooperative child, TERM-resistant
   child, and later sibling. Inject first-child signal PermissionError only in
   child-local Python test seam (no real privilege), at exact owned child
   handshake. Run actual DockerSession.__exit__/StdioConnections.close. Observe
   sibling not processed after first error, controller exit and live child
   ownership. Outer guardian reaps every fixture before asserting intended RED.
   A companion fake Process tests forever-wait behavior under outer subprocess
   watchdog, not in the CI test process itself.
4. Force material Popen poll/wait uncertainty and thread-start failure locally;
   prove no raw secret payload, original operation precedence, sibling cleanup,
   and lack of retained owner at actual call-return boundary. Separate injected
   API errors from real process evidence; do not pretend a fake wait proves an
   actual uninterruptible kernel process. No production environment fault knobs.
5. Normal controls: complete observation, caller cancel before spawn, blocked
   stdin1MiB, exact/+1 output limits, read failure not EOF, inherited stdout and
   stderr, consumer-close, normal CLOSE/removal, controller exception/unwind,
   client.close failure, no created task resource and multiple SDK reconnects.
   Assert all actual child/worker owners settled on healthy paths, exact bytes,
   no material success on failure and sentinel/foreign resource untouched.

After RED approval, implementation proof must additionally observe broker ready
before spawn, no child on keeper-start failure, finite aggregate cleanup over
multiple children, no new admission after uncertainty, root-denial simulation
with siblings still settled, broker survival after controller exit, actual
eventual child release/reap and broker exit, no unsafe PGID after leader reap,
and bounded live memory/descriptors/worker population across repeated failures.
No full suite/promotion until independently reviewed source and focused proof.
