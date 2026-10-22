# Class50: host material and Docker transport custody

## Status and scope

Current continuation after approved `39efb7b7`: controller/Python integration
and revised fixture guardian are WIP SOURCE ONLY, pending root's guardian review
before ANY further build/runtime. The prerequisite proof below remains historical
and valid, not qualification of this new integration. No full/promotion claim.

This document began as design-only `bdbd290c`, based on actual main
`66fb5fda` (PR309 merge). The historical design and observed RED sections below
record that progression, not completion of host transport custody. Root then
approved RED `c0cfae8e` and ONLY the narrow generic-gate prerequisite. Its current
implementation/focused proof is recorded at the end. No controller/Python
integration, version, public wire, or preparation-policy change is implemented.
Parked class46 `ede2ffc7` and its source/cache/evidence are untouched.

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

The prerequisite is now source-confirmed at `descendants.rs:93`: `?` on the
first signal error stops the whole sibling sweep; reap_exited has the same
early-return structure. Proposed narrow change, still NOT implemented: keep
the existing finite population and five-second cleanup allowance; accumulate
only the first safe io::Error for return, continue every independently owned
child in the finite snapshot, and spend the remaining original deadline on
fair TERM/grace/KILL/poll sweeps. Do not allocate an error per child or extend
the budget per failure. reap_exited likewise visits all children without
signalling any live child. At the finite unresolved boundary return the first
error, retain unknown custody, and issue no QUIESCENT receipt. An actual later
empty owned-child set is a new positive settlement observation, not evidence
that denied signaling succeeded. Root reviews this generic gate prerequisite
separately before controller integration; the RED below does not modify it.

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

After RED approval, implementation proof must additionally observe actual gate
admission receipt before controller launch/resume, no child on keeper-start failure, finite aggregate cleanup over
multiple children, no new admission after uncertainty, root-denial simulation
with siblings still settled, actual gate custody after controller exit, actual
eventual child release/reap and QUIESCENT receipt/gate exit, wait-only handoff
to the pre-established Rust owner while ASB lives and exclusive guardian
adoption on actual ASB owner loss, no unsafe PGID after leader reap,
and bounded live memory/descriptors/worker population across repeated failures.
No full suite/promotion until independently reviewed source and focused proof.

## Observed baseline RED checkpoint

Actual main49 baseline binary was built FIRST on the new exclusive HOME50 cache
`/home/martin/projects/asb-build-cache-material50/target`, copied only from the
verified-idle HOME43 cache as a dependency seed. Copied products are not claimed
as qualification. Fresh build terminal0; actual/frozen SHA256:
`80c37cf6edb8809f846462dd8030ad023004a10b0b711089a25c0ce6a1bd7a17`.
Frozen binary is `target/qualification/material50-baseline-asb`. No version bump
or production repair; the only production-file edits are cfg(test) test-module
registration and the exact-owned-pid/thread-local error hook AFTER the real
pidfd/kernel wait ownership check. No production environment fault setting.

At immutable `c0cfae8e`, three outer RED tests failed at their intended assertions:

- Real Controller/task_server/ResourceJournal/DockerSession path, inherited
  private output pipe: material call returned with TWO live daemon readers;
  subsequent cancellation confirmed fake exact-container stopped preservation,
  Rust observed controller exit, and the outer guardian adopted ONE still-live
  inert host holder. It actually killed/reaped that holder and its own sentinel
  before returning observations; sentinel survived to teardown. The intended
  failure is live-session worker retention. Later no-host-child assertion is
  not reached, but actual guardian JSON independently records the host holder.
- Same complete cancellation/preservation/exit path, THREE actual separate-
  session inert connection children, child-local first-PID PermissionError:
  DockerSession reached its real close, all THREE children remained alive after
  controller exit and were adopted, killed and reaped by the guardian before
  the intended final assertion. This models signal denial, not actual sudo/root.
  Rust cancellation still returned the existing stopped-preserved resource
  disposition after that controller's nonzero exit; no protocol was changed.
- Isolated gate DescendantScope path: injected EPERM after real ownership check
  of the first exact child prevented a later actual owned sibling receiving any
  cleanup signal. The sibling remained alive through a bounded200ms exit poll.
  BOTH children were actually killed/reaped and the unrelated guardian sentinel
  survived before the intended assertion. No fake uninterruptible kernel claim.

All child reexec cases exit0 after recording observations and doing their
required cleanup; only the three outer contract assertions fail. Nested libtest
rows are not additional passes. Current final logs:

- `target/material50-baseline-build.log` — actual build0.
- `target/material50-wholepath-red-final.log` — EXIT101,0pass/2intendedfail.
- `target/material50-gate-sibling-red-final.log` — EXIT101,0pass/1intendedfail.
- `target/material50-red-before.sha256` and `...-after.sha256` — actual ASB AND
  reexec libtest binary byte-identical around final runtime; immutable raw JSON,
  journal files and child stdout/stderr under `target/material50-fixtures/`.

The earlier sibling v1 failed BEFORE the intended assertion: libtest spawns its
test worker on a nonleader thread, unlike the real exclusive gate. `/proc`
leader children was empty. Guardian still adopted and reaped both real children.
Preserved `target/material50-gate-sibling-red-v1.log` and its raw files document
that fixture error. The test-only correction exits the spawning thread so real
children reparent to the process leader before production's actual observation.
v2 and final then reach the intended EPERM/sibling assertion. Original wholepath
v1 and gate-v2 logs also remain retained; no blind retry or production change.

Existing offline positive controls also ran on this baseline: material8,
Docker6, task-server13, resource-journal23 = **50passing/0failing/0skips** in four
Python outer invocations. Logs `target/material50-baseline-{material,docker,
task,resource}-controls.log`. These are existing controls, not successful REDs
or full qualification, and prove no real Docker/provider/privilege behavior.
No live Docker, SDK, credentials, sudo, host mutation, full suite, PR or push.
Strict offline locked workspace/all-target/all-feature Clippy `-D warnings`,
fmt-check and diff-check all terminal0 (`target/material50-red-{clippy,fmt,
diffcheck}.log`). Lint success does not qualify the intentionally failing REDs.
The final reexec libtest binary SHA256 is
`8ee0fb031d145a702dc370f64b8bab39c94f633b901e765868a45ec37b33d32d`,
retained readonly at `target/qualification/material50-red-libtests`.

Coordinator reports checkpoint49 delivered at actual signed66fb5fda: exact
PR309 run39695483387 and both postmerge jobs39696944017 succeeded; signed tag
4810664778dda385d666aa1e943f9fe1410870dc points to66fb, release v0.2.43 published
00:10:42UTC. This supplied delivery evidence is distinct from this class50
RED proof. Parked class46 source/cache remain unchanged. That RED checkpoint was
submitted for independent review before any production implementation authority.

## Narrow generic-gate prerequisite implementation and focused proof

After independent review of complete RED source and logs, root authorized ONLY
this prerequisite, before controller integration. Production changes are
limited to `src/process_gate/descendants.rs`:

- quiesce keeps the first io::Error, continues separately owned siblings and
  fair polling/escalation within the original five-second allowance. The
  deadline is also checked inside a finite snapshot; per-child failures do not
  restart it. Population/churn limits remain. On unresolved deadline the first
  error is returned, not replaced by a later timeout or sibling error.
- reap_exited records the first error and visits remaining children without
  signalling live children.
- WNOWAIT's actual exited-child observation skips unnecessary signalling and
  directly reaps through the pinned pidfd. Signal denial cannot prevent reaping
  a child already known exited. Kernel wait ownership checks remain before any
  signal or test hook. No cached PID/PGID signalling after reap.
- An actually empty owned-child population still returns positive quiescence;
  an inaccessible live child still returns an error and the unchanged gate
  retaining loop cannot issue QUIESCENT. No grade/resource/authorization or
  ManagedGate/receipt semantics were altered.

The former gate RED now passes with actual pinned sibling exit AND waitid
ECHILD proving production Scope reaped it (not fixture Child::try_wait).
The first EPERM remains returned at5004ms, while sibling_alive=false and
sibling_reaped_by_scope=true. The still-denied child is then actually killed
and reaped before outer assertions; unrelated sentinel survives. An additional
isolated EIO reap test proves later exited siblings reap while a live child is
not signalled; all real cleanup precedes assertions. A direct exited-child test
sets the signal-denial hook but observes successful production reap and ECHILD
from both the pidfd and original Child::wait. No blanket lint suppression.

Actual ASB built FIRST and frozen readonly:
`5e10539a39042b273b7ea89ad2ab53c083ad9a75bcd82b975a9a247b70612fa8`,
at `target/qualification/material50-gate-prerequisite-asb`. Before/after-runtime
hash files match. Fresh complete gate focus: **30passing/0failing/0skips**, one
outer libtest target, nested fixture rows not double-counted. Log
`target/material50-gate-focused-final.log`; initial all-green focus-v1 remains.
The actual binary changed only for this narrow production prerequisite; original
baseline/RED binaries and source remain immutable and separately named.

Strict lint initially reported zombie_processes on a test's conditional
Child::wait after actual pidfd reaping; preserved
`target/material50-gate-clippy-v1.log`. The test now always observes original
Child::wait and requires ECHILD after positive production reap, safely collecting
the already-exited child before assertions if the implementation regresses.
No production fix was made for that lint and final30-test focus was rerun.
Final strict offline/locked workspace/all-target/all-feature Clippy -Dwarnings,
fmt-check and diff-check are terminal0; logs
`target/material50-gate-{clippy,fmt,diffcheck}-final.log`. Actual and frozen
binary hashes still match after all gates, recorded at
`target/material50-gate-final-frozen.sha256`. No source or relink during runtime.

The two host-material/controller REDs are deliberately STILL UNFIXED; their
historical observations are not replaced by this gate-only qualification. Root
must review signed prerequisite proof before any controller/Python integration.
No full suite, live Docker/privilege, new broker, version bump, PR or push.

## Unqualified guardian migration and caller inventory

The first development integration ran30passing/1fixturefailure, not a qualified
checkpoint (`target/material50-controller-dev-tests.log`). The old RED guardian
was safe only for the raw-controller baseline; after ManagedGate insertion its
blanket adopted-child KILL was WRONG. A3s test cancellation budget raced the new
aggregate3s Python close, the Rust fixture panicked, and the old guardian killed
adopted actual gate PID1731528. Its JSON proves that gate was reaped, NOT that
its descendants were safely settled. Later ps absence is NOT reaping evidence.
All raw per-PID/connection/journal files remain under the logged fixture root
`target/material50-fixtures/18e0b28a134f722c-1a6b58-0`; no successful ownership
qualification is claimed from that development run. Development actual binary
`e49c363b3f81a13f755d4e09d33f89bee55d093717e74e5067efa346da14ec3e`
is separately frozen at `target/qualification/material50-dev-e49c363b-before-guardian-fix-asb`.

Root stopped further runtime. The revised guardian SOURCE ONLY:

- records actual admitted gate PID plus executable device/inode while live,
  opens pidfds and verifies direct kernel wait ownership on every adoption;
- accepts an owned actor/fault release-request/ack while the Rust caller is
  alive; always releases actors after caller death, then repeatedly adopts and
  waits/reaps all actual descendants and the gate. NO adopted gate/child KILL;
- verifies ECHILD after actual pidfd reap; never treats a single empty/initial
  PID snapshot as proof that all later adoption is complete while a live known
  gate remains. A35s cleanup watchdog covers the inert actors'30s natural exit
  fallback, not a fake kernel hang. Only the direct test owner on its15s timeout
  and the unrelated direct sentinel are eligible for fixture kill;
- uses6s only for normal finite3s aggregate-close plus real gate quiescence,
  well inside production's existing240s cancellation allowance. Separate40ms
  cases require genuinely live retained gate/unknown return, then actual actor
  release and keeper exit/ECHILD. An actual owner-exit case bypasses destructor
  and thread completion, then requires outer adoption and reap of that exact
  actual gate. Missing-receipt case must remain unconfirmed even if real exit
  later occurs. Resource preservation is separately observed through real
  protocol cancel and durable journal before starting the40ms host budget.

Root also found duplicate concurrent close ownership in the development Python
copy. Current WIP detaches under an exclusive single-closer claim; a concurrent
closer cannot poll/reap/signal or claim settlement. Unknown completed close is
sticky and cannot restart a budget or signal a cached reaped PID. Added source-
only tests use a real owned child/sentinel, a finite close-owner barrier and
actual reap before assertions; no test run is claimed yet.

Direct source caller inventory (not an audit of external SDK side effects):

| DockerSession/material transport entry | Rust/product owner in chosen integration |
| --- | --- |
| SWE session `asb_swebench.py:890`, original terminal session `asb_terminal_bench_1.py:473`, Harbor session `asb_harbor.py:630` | official adapters → `remote::launch` → ControllerCommand/ManagedGate before Python starts |
| Retained grading DockerSession: SWE588, original terminal155, Harbor374 | `remote::retained_grade_request` → gated `asb_retained.py` → selected module.grade_retained; same outer gate owns nested helper children |
| SWE official harness755 | task.evaluate (`asb_swebench_task.py:195`) or retained grader (`asb_swebench.py:623`) → evaluate_submission/run_harness; nested process remains descendant of same gated controller even with start_new_session |
| Recovery `asb_resources.py:409` | `remote::recover` → gated controller one-shot; retained nested quiesce uses already-owned session client |
| material_exec imports: material_root, material_environment, material_runtime | task.initial_material → task_server → same gated official session; service observations are sequential |
| material_exec imports: staging.root_execute, terminal_session.target | task_server AGENT preparation/original Terminal session path → same gated official session |
| `remote::verify_retained` | gated one-shot private verifier; no direct DockerSession in normal verify path, but same process custody before dynamic selected-module invocation |

All FOUR production Controller::spawn producers in remote.rs construct the
typed ProcessLaunch carrier BEFORE opaque std::Command configuration; env_clear,
removals, cwd, stdio and pre_exec are kept on the same owned Command, not rebuilt
from getters. Principal status is exposed only after actual gate exit+QUIESCENT;
resource CLOSED acknowledgement and original principal status remain distinct.

Raw helper routes NOT cleared by this class: distributed Python main
`asb_swebench.py evaluate/harness` can be manually invoked outside ASB's Rust
controller; direct external use of DockerSession/material_exec also lacks the
outer gate and is not newly qualified. Current Rust catalog/install producers
(`swe_bench/catalog.rs:152,162,201`, `terminal_bench/catalog.rs:209,266`) still
use raw logged_status. Their direct operations are identity/verify/catalog or
terminal_catalog runtime/install/verify, not the DockerSession/material_exec
functions inventoried above. External SDK import/process side effects are NOT
proven absent. General installer-helper subprocess custody remains class44's
separate dependency/scope; do not infer universal cross-process custody merely
because these helpers share a publication directory. No blanket command_output,
Python subprocess.run, SDK, non-Linux or elevated-runtime qualification.

## Revised guardian and close-lock source checkpoint (not runtime-qualified)

The 35s fixture cleanup allowance is now only a finite uncertainty-reporting
boundary. On timeout or observation error the Python guardian retains its exact
Popen/pidfds and namespace and continues nonblocking adoption/reaping until actual
settlement. It never kills an admitted ASB guardian, exits on a still-live known
gate, or closes all ownership handles merely because a diagnostic was published.
The direct inert test owner alone may be terminated after its separate 15s
watchdog. Actor release remains real; no fixture fabricates gate quiescence.

The Rust outer caller pre-establishes a wait-only keeper before spawning that
guardian and installs an immediate Child guard. It polls with a finite caller
deadline rather than using unconditional status/wait; error, unwind and timeout
handoff the exact guardian wait-only and disclose the retained namespace. The
explicit actual ASB path, admission device/inode and pinned live executable are
used for identity, not a guessed parent.parent layout of a frozen libtest image.
Both the generic sibling fixtures and controller whole-path fixtures use this
same owned caller. These changes are source-only pending independent review.

StdioConnections now starts its absolute 3s close allowance before acquiring any
lock. A close-request Event excludes new admission even while an in-flight Popen
holds the ordinary lock. One nonblocking one-shot cleanup claim owns all
poll/reap/signal rights; competing close calls cannot report success while that
claim is pending or restart an unknown cleanup budget. Initial state-lock
acquisition is bounded. If it times out, the owner record retains the shared
actual Child list, including a subsequently returned in-flight spawn. Once
detached, the exclusive close record needs no fallible final lock acquisition to
retain pending owners. Controlled blocked-spawn and concurrent-close source tests
require bounded caller return, eventual actual child reap and sentinel survival;
they have not yet been run on this revision.

The shared OwnedReaper prerequisite is isolated independently from this dirty
integration ancestry: clean branch codex/reaper-observation-custody at signed
73e2f43b (d475a58d, 2fbee55d, 73e2f43b), based on actual main66fb. Its five
standalone tests, strict workspace/all-target/all-feature Clippy and fmt passed;
actual/frozen hash is
8ad11f9d46b91758bf55d0c5111b702be8d862d23c404c061e606619cf848f34.
Observation errors retain the actual Child on the pre-established worker;
WaitOnly never kills, and worker completion requires actual exit/reap. This is
not whole-path controller qualification or crash-persistent custody.

No class50 runtime has been rerun since the documented development failure.
Root source review is required before any revised whole-path runtime, build or
qualification claim. No version, public protocol, grade or authorization policy
is changed by these fixture ownership revisions.

## Revised focused qualification at source 3edcd8ca

After independent source review of 2b702edc, a narrow library-boundary correction
3edcd8ca places the guardian only in cfg(test, Linux) lib.rs. Shared support/mod
is also used by integration crates and must not import private library owners.
No private guardian module is compiled into those independent harness roots.

Actual ASB was built FIRST, frozen read-only at
`target/qualification/material50-revised-3edcd8ca-asb`, and selected explicitly
for every guardian/controller admission. Before/after actual and frozen SHA256:
`838bea92eea9a199fae32fa258e70c323e98e18b1574b773383ab4248db3b3ea`.
No source edit or actual-binary relink occurred during runtime.

Fresh focused results are **117 passed / 0 failed / 0 skipped**, without counting
reexecuted child rows twice: controller34, gate30, Python material8, Docker9,
task-server13 and resources23. The five whole-path controller cases include
normal preserving cancellation, first transport signal denial, original short
caller deadline, missing proof and real owner exit. The owner-exit fixture's
actual gate2007717 was pinned, adopted and reaped by the outer guardian; every
guardian reports zero adopted gate KILLs and sentinel survival. The independent
short/missing-proof cases retain their existing wait-only keeper observations.
The controlled first EPERM remains the returned error at5009ms while the later
sibling has actual pinned exit and ECHILD proving Scope reap. No actual Docker,
sudo, elevated-child, external SDK or provider qualification is implied.

All handles are terminal. Strict offline/locked workspace/all-target/all-feature
Clippy -Dwarnings, fmt-check and diff-check passed. Evidence remains under
`target/material50-revised-{build,controller,gate,material,docker,task,resources,clippy,fmt}.log`
and `target/material50-revised-{before,after}.sha256`; raw fixture JSON and all
historical failing development/RED logs remain intact. This is a focused source
checkpoint, NOT a full-suite, current-stack integration, release or delivery
claim. No version bump, push or PR; independent final review remains required.

## Guardian error-branch fixtures (source-only, awaiting review)

The 117 passing cases above did not execute outer Rust Guardian::retain or the
Python finite cleanup-report path. Three additional private Linux library tests
now reuse the actual owner-loss scenario, explicit ASB identity, real stopped
resource journal and positive direct adoption of the live admitted gate. An
explicit script-only control defers actor release until the parent writes the
owned marker; normal existing fixture behavior is unchanged. Only the reporting
case shortens the private post-owner-exit reporting allowance to 40ms, not actual
custody lifetime or production timeouts. Other cases exercise a 40ms Rust caller
timeout and controlled unwind/Drop separately. No synthetic kernel error is
claimed.

An owned release safeguard precedes spawn and outlives the controlled unwind;
all ordinary error paths run it. The tests pin the exact outer guardian and
live adopted gate, require an incomplete keeper receipt before release, then
explicitly release real actors and observe actual keeper reap/ECHILD for the
outer guardian, actual Python gate reap, sentinel survival and zero admitted
guardian KILLs before intended assertions. On failed release/observation the
existing wait-only owner and namespace are retained, never killed for a test
deadline. These new fixtures have NOT been built or run; source review is
required before qualification. The fair multi-child reaper follow-up remains
separate and unimplemented.

The first new-three run at c1cca0c3 failed all three during fixture preparation,
before any intended guardian error branch: unlike the existing whole_path helper,
the new attempt root used default mode0755, which the actual ResourceJournal
correctly refused. The raw inner traceback, outer settlement/sentinel receipts
and full transcript remain in `target/material50-errorpaths-new3.log` and its
logged namespaces. This is not a production regression or passing error-path
proof. A test-only correction sets mode0700 before spawn, matching the existing
fixture contract. No tests have been rerun on that correction; source review
precedes another actual-first/frozen attempt. The four Python owning families
passed53/0/0 separately; their success does not qualify the three new branches.

## Fresh error-path and regression proof at source 901d4ff1

After the narrow source review/correction, actual ASB was built FIRST and frozen
read-only at `target/qualification/material50-errorpaths-901d4ff1-asb`. Its
before/after/live/frozen SHA256 remains
`838bea92eea9a199fae32fa258e70c323e98e18b1574b773383ab4248db3b3ea`;
these test-only changes do not change the production executable. The selected
actual image was explicit; no source edit/relink occurred during runtime.

The new three error-branch tests passed3/0/0 in0.40s. Each observed live exact
outer guardian and adopted gate with an incomplete keeper receipt before real
release, then actual outer exit/reap/ECHILD, positive Python gate reap, sentinel
survival and zero admitted gate KILLs. The reporting fixture actually reached
CleanupDeadlineUnknown with gate2234048, retained custody after its finite report
and settled only after release. Separate timeout and controlled unwind/Drop
cases retained no false Python cleanup error. These are controlled short fixture
deadlines, not privileged Docker or kernel-hang qualifications.

Fresh existing families also passed: controller34, gate30, Python Docker9,
material8, task-server13 and resources23. Total **120 passed / 0 failed / 0
skipped**, counting only outer rows, not nested reexec output. Strict locked/
offline workspace/all-target/all-feature Clippy -Dwarnings, fmt and diff-check
are terminal0. All handles are terminal; logs are
`target/material50-errorpaths-v2-{build,new3,controller,gate,docker,material,task,resources,clippy,fmt}.log`
and matching `before/after.sha256` files. Original failed setup/RED/development
logs and raw namespaces remain intact. Source is frozen for independent final
review; no full-suite/current-stack/release claim, version bump, push or PR.
