# Installer subprocess boundary: design and observed-red plan

Design/test-only stage 44, based on independently approved official-engine source
`31070b9a7c61c1e0a3c15180496c153ed4a9030a`. The design/red manifest stayed
0.2.41; the authorized partial implementation below stages 0.2.44.
No production process repair, full qualification, PR, merge or release is claimed.
Root must review this design and actual red evidence before implementation.

## Evidence and ownership scope

The original 52-line code-smell report was re-read. Contributor instructions
were read; development/architecture documents are byte-identical to `24d7a880`,
where both were read completely. The source boundary was inspected against
actual remote/main `1a88ed8547270bdb4bb70cc88183fe2c449be0d5` (0.2.37).

`LoggedStatus::logged_status` calls raw `Child::wait`; `command_output` calls raw
`Command::output`, without parent-enforced byte or pipe-lifetime bounds.
`source::DownloadChild` establishes an owned reaper before admission and bounds
the direct-child wait, but its successful direct wait is not descendant
quiescence. Its direct-child kill/reap cannot prove that a background writer
has stopped touching source staging. A five-second cleanup grace does not turn
that into tree proof. These are real source-level gaps, not evidence that every
normal installation fails. Actual regressions below must establish their scope.

All LoggedStatus callers were inspected: npm/pip/venv/dependencies; workload and
language-runtime extraction; perf package download/extraction/build; bpftrace
download/extraction/configuration/build; cli2key local-key post-install; original
SWE and Terminal/Harbor identity/catalog/source helpers. `source::command_status`
and version/download helpers share DownloadChild. Direct custom perf probes
also use raw output/status. Conversion must cover these call sites explicitly,
not leave an old unsafe trait as a fallback under the new implementation.

## Materialize intent before configuration

Add a private installation-process module with an immutable `Step`/`Purpose`,
explicit project/log context, budgets and an owned prepared command. This is
installer infrastructure, not another tool registry or an adapter-ID dispatcher.
The owning adapter retains program/argv, package/SDK flags, expected format,
network/source policy and acceptable known-negative fallback conditions.

The required construction order is:

1. Build explicit `ProcessLaunch` intent (including intentional argv[0]).
2. Materialize `ProcessLaunch::native(Some(gate_executable))` **once**.
3. Configure that original Command: cwd, env_clear/env removal, runtime PATH,
   proxy, stdin, stdout/stderr and pre-exec/credential settings as appropriate.
4. Consume its `CommandRoute::validate_gate` and establish fallible admission,
   pipe/log/custody ownership before spawning. Retain exclusive command access
   through validation/admission; no late replacement or spurious spawn.

Use existing `process_gate::executable` for loader-bundled/self-image handling;
tests pass the actual built ASB gate, never a libtest binary pretending to be it.
No wrapper assembled after configuration; no Command getter reconstruction,
env_clear guess, shell quoting, transmute or lost pre-exec/I/O state. Getters in
the existing framing validation or metadata view are not reconstruction.

The old `&mut Command` extension trait cannot safely retrofit this behavior.
Migrate callers to the explicit prepared boundary, then remove the raw trait.
Configuration-preservation tests must cover cwd, env_clear/removal, PATH/proxy,
binary stdin/stdout/stderr, argv[0] and a once-only pre-exec marker. Missing
dependencies and setup/custody failures must occur before target admission.

## Typed completion and non-swallowable uncertainty

Do not erase outcomes to Error::Io or Boolean probes inside the installer.
Introduce private types along these lines (names subject to source review):

- `SettledStep`: privately constructed only after actual exit, matching guardian
  quiescence and required stream/capture completion. Preserve actual status and
  optional termination observation separately; do not invent exit zero.
- `KnownNegative`: a complete, settled observation the owner explicitly permits
  as unavailable/unusable (missing optional dependency, known nonzero package
  response, or complete invalid candidate version, as appropriate).
- `AbortInstallation`: typed cause plus independent cleanup/capture state and
  retained evidence locations. Cancellation, shared deadline expiry, lost
  custody, incomplete required capture and unknown cleanup propagate here.
- A probe returns `Result<UsableOrKnownNegative, AbortInstallation>`, not bool.
  Package attempts similarly distinguish a settled candidate failure from an
  abort that must stop the operation. Only the outer command/presentation layer
  converts the private typed failure to the public Error representation.

Custom-install callback signatures and internal recipe/source helper results
must preserve this type until the final operation boundary; merely typing the
lowest function and immediately mapping everything back to Error::Io is not a
repair. Audit every current `if let Ok`, `.is_ok_and`, `unwrap_or(false)` and
fallback match in perf, bpftrace and package/dependency preparation. A known
settled package miss may continue to the next declared candidate or source
build. A timeout/unknown cleanup **must not** become a package miss, trigger a
second build while the first writer lives, call a Boolean runnable probe, parse
a prefix, publish an artifact or remove staging. Use `?` on the explicit abort
channel, with no string parsing, hidden global flag or tool-ID condition.
`#[must_use]` assists review but does not replace actual propagation tests.

Preserve status, cleanup, required-capture completeness, ancillary log problems
and installation publication as separate facts. An optional exact-termination
diagnostic must not acquire a new success/receipt policy. A mere known nonzero
exit is not unknown cleanup; it remains a meaningful owner-controlled negative.

## Deadlines, cancellation and resource custody

An operation context owns a fixed absolute overall deadline, explicit
cancellation and diagnostic context. Each step gets the earlier of its declared
purpose deadline and the remaining operation budget; output/progress never
resets either. Preserve existing download 300s/version 10s semantics initially.
Inspect, extraction/configuration, package installation and source compilation
need different **documented, configurable** budgets. A 300s download constant is
not an appropriate hidden compiler limit.

Concrete proposed configuration surface for review, not implemented policy:
optional `[installation]` in the existing project-local configuration, consumed
by both CLI installation and the TUI operation context. Absence uses defaults;
existing project files are not rewritten. `asb init` can emit the documented
section when the implementation is approved. No separate user-home config or
production environment timeout override is proposed.

```toml
[installation]
operation_timeout_seconds = 10800
probe_timeout_seconds = 30
version_timeout_seconds = 10
download_timeout_seconds = 300
extract_timeout_seconds = 300
configure_timeout_seconds = 900
package_timeout_seconds = 2700
build_timeout_seconds = 7200
```

An operator can raise both operation and build budgets for a slower valid
build. These are configurable defaults, not hidden hard upper caps. Validate
nonzero integers, Duration conversion and checked Instant arithmetic; reject
unrepresentable values explicitly rather than clamp them. A step extending
beyond the operation budget receives the remaining budget and the diagnostic
must say which limit expired. Timeouts identify phase, elapsed time, configured
limit and project config path/key to change. The separate existing six-second
native cleanup grace is not extra execution allowance and may end in retained
uncertainty, not false success. Tests inject short typed budgets directly, not
production sleeps or environment-based production fault switches. Root review
must approve the schema/defaults before implementation; none exists in product
configuration at this checkpoint.

Load and validate this section once at outer install admission through the same
project-local reader for CLI and TUI; freeze the typed values for that operation.
An edit during a build cannot reset deadlines; the next invocation reloads it.
Missing keys use defaults. Invalid values fail before spawn or staging mutation,
with the config path/key. Initialization without a config uses identical
defaults. No credentials, authorization or authenticated-runtime gate is added.
Proposed inspection bounds are 1 MiB per stream and 2 MiB aggregate per step;
owners can declare smaller format-specific limits explicitly.

Before spawn establish guardian admission, custody worker, pipes/log handles and
any producer-dependent staging lease. The supervisor owns them outside an
unwind scope. Resume only after required readiness. On cancellation, timeout,
read/write failure or panic request the existing fixed protocol on the exact
admitted guardian; never KILL it or target guessed process groups, parent-PID
scans, names, or unrelated host processes. Continue draining output during
bounded shutdown. Principal exit alone never means streams complete or a
producer tree quiescent.

After a bounded parent wait, an unconfirmed operation must leave the already
established worker holding its gate, pipes/logs and staging dependencies until
actual matching quiescence and stream settlement. Use existing wait-only reaper
custody where appropriate; do not transfer a gate while dropping the staging
lease or captures it protects. No new thread in Drop, unbounded caller join,
forced detach that discards errors, placeholder sleep process or busy spin.
Back off while custody remains uncertain. Retain a locatable stage on unknown
cleanup; no Staging Drop/validation/publication/removal before the appropriate
producer-completion permission. Blocking kernel/file operations cannot receive
an absolute hard termination guarantee; truthful retained uncertainty is required.

Concretely, a private shared `ProducerLease` owns the actual Staging guard, log
sink and pipe pumps, not merely pathnames. The operation and each admitted step
hold references. The pre-established worker retains its reference until matching
quiescence and required stream settlement. Abort transfers ownership before
returning a typed diagnostic. DownloadChild replacements take this lease
explicitly. Custom recipes/fallback scopes must not keep a producer-exposed
stack-local Staging guard that destructively drops on `?`: move it into the
lease before admission. Recover/drop it only with a settled token and no active
producer references. Unknown cleanup marks the lease retained; its final Drop
must not call today's destructive Staging settlement, but preserve the pinned
stage/evidence location. Readiness, spawn, panic and pump-error exits follow the
same transition; a never-exposed stage may be released on pre-spawn failure.
OwnedReaper alone is insufficient: it owns Child, not these stage/log/pipe leases.

The TUI's outer installation gate still owns its worker. Inner step deadlines
are not a replacement for owner-loss cancellation; nested guardians must settle
without abandoning children when their outer owner exits. Synchronous CLI and
TUI use the same step implementation. Linux provides the current owned native
gate backend. Native Windows/other Unix need an explicit platform decision and
eventual backend qualification; do not silently label direct-child cleanup as
equivalent or claim Linux tests qualify them. Preserve documented behavior until
that decision is independently reviewed; do not add an accidental platform-wide
unsupported-install regression during a Linux-only repair.

Proposed explicit compatibility decision for root approval: scope the repaired
backend to Linux. Keep existing non-Linux behavior in a separately named legacy
backend returning honest `DirectChildOnly` facts, never `SettledStep` or a
manufactured tree receipt. Do not silently alter its existing publication policy
or claim the new custody guarantee there. Make the platform capability visible
in typed routing. A qualified non-Linux tree backend (for example Windows Job
Objects) is separate work. If root instead requires that backend in this class,
implementation remains held until its design is approved; no blanket platform
rejection or direct-child-as-tree shortcut is proposed.

## Output and log policy

Drain stdout and stderr concurrently with bounded fair turns and backoff.
Inspection has explicit per-stream and aggregate byte limits, fixed deadline,
strict required encoding/shape checks and required EOF/settlement. Reject
overflow, held pipes and incomplete prefixes as inputs, even if the principal
exits zero. Sanitize error reasons; no provider bodies, credentials or arbitrary
helper output in ordinary errors.

Status-only steps stream to a prepared owned project log rather than accumulating
unbounded output in memory. Explicitly bound logging policy/storage and report
log-open/write/flush failures; continued discard/drain after an ancillary log
failure must not block the child or fabricate complete diagnostic logs. An
ancillary log failure is not automatically an invalid installed product; carry
it separately and decide presentation/operation policy explicitly. Required
captured metadata remains unusable when incomplete. Keep filesystem write
failure, process failure, timeout, cancellation and unconfirmed cleanup distinct.

Concrete ancillary policy proposed for approval: one owned log per step, at
most 16 MiB combined stdout/stderr, keeping the first bytes. After the limit,
continue bounded drain/discard and return `Truncated { retained, discarded }`;
print one concise path-bearing warning at the operation boundary. This is a
diagnostic-retention cap, not an execution or required-metadata cap. Log
open/write/flush failures retain the distinct error/path, switch to drain/discard
and warn once. They do not alone invalidate a product whose exit, custody,
required capture and owner validation succeeded. Required capture instead
aborts on overflow/read error and never parses a successful prefix. Any log
also used as adapter input must be declared required, without the ancillary
relaxation. Freeze these policies in the step context and test both categories.

### Approved pre-implementation log/cancellation invariants

Regular-file writes and flushes can block. The supervisor retains independent,
fixed-identity ManagedGate cancellation authority: neither the sole gate nor a
mutex required to request cancellation may be held by a blocked pipe/log pump.
A pre-established log writer consumes a bounded queue; pressure or an ancillary
writer failure switches producers to bounded discard plus an honest diagnostic.
The blocked writer retains its lease and file until it actually settles, without
preventing target cancellation or fabricating completed logging. An offline
fixture deliberately holds the writer, observes actual child/grandchild
cancellation and matching guardian quiescence with the unrelated sentinel alive,
and only then releases the writer. Never wait for that release to cancel.

New Linux raw per-step logs are create-new mode 0600 inside an owned mode-0700
attempt namespace, reached without following symlinks. Existing install.log or
foreign entries are neither reused, overwritten nor chmod'ed. Creation races
fail or choose a fresh private attempt; they never relax ownership. Ordinary
warnings contain only static reason classes and safe paths, never stdout/stderr
payloads or credentials. Tests use umask 022, pre-existing foreign regular files
and symlinks, and inert secret text to verify modes, preservation and absence
from human-readable output. These safeguards add no authorization gate.

## Actual red fixtures before implementation

The test-only module calls the **old actual** LoggedStatus/command_output APIs
inside a separately admitted real ASB watchdog gate. A distinct sentinel gate
is a sibling outside that owned subtree. Handshake files prove helpers started
before observation windows; the principal-exit/writer case requires actual raw
status success, then deliberately releases the orphan writer **after** return.
The outer test keeps its principal alive so normal outer cleanup cannot hide
the race. Every failure assertion follows positive gate exit/quiescence cleanup
and sentinel-survival checks. Fixture directories/logs are retained.

Four required red cases:

1. Stuck raw status step does not return within the injected short fixture
   observation budget. The exact watchdog then settles it, not a PID-name kill.
2. Actual raw principal exit zero returns success while a controlled descendant
   can subsequently change staging. The release/mutation handshake proves order.
3. Raw output collection waits for a descendant-held pipe after the finite
   principal exits; it has no bounded stream-drain contract.
4. Raw output accepts 64KiB despite a requested fixture contract of 4KiB. The old
   API cannot express that budget; after repair migrate the owning test entry to
   the explicit typed budget without weakening the refusal assertion.

The 500ms watchdog observation and 4KiB fixture budget are test contracts, **not
proposed global installer defaults**. Finite fixture admission/cleanup bounds are
separate. No live provider, package registry, SDK, Docker, compiler build,
privilege, network or host-setting change is used. Test-only child selection is
scoped to isolated one-test processes; it is not a production environment hook.

Later implementation tests also need pre-spawn worker failure; owner-loss/cancel;
read/WriteZero/ENOSPC/pump panic; simultaneous pipe pressure; exhausted shared
budgets; meaningful build-vs-inspection limits; no candidate fallback/publication
after typed abort; unchanged unrelated sentinels/old installed bytes; and all
existing source-publication/recovery/installer/TUI fixtures. Independent review
must precede production changes and any full-suite authorization.

## Separate residual: custom binary/metadata atomicity

Custom perf/bpftrace publish into live tool roots and bypass the generic pinned
source-publication mechanism. In particular bpftrace copies the binary before
writing its compatibility metadata. Generic producer quiescence does not make
that multi-artifact publication atomic. Keep it a separately tracked residual:
private pinned staging, actual validation, exact identity-aware commit/rollback
or retained diagnosis, metadata pairing, foreign replacement/symlink and ENOSPC
tests need their own coherent design. This repair must not claim 0.2.37, or stage
44, fixed all custom recipe publication merely by bounding the subprocesses.

## Observed red checkpoint

Own source tree/branch:
`/home/martin/projects/asb-installer-process-boundary`,
`codex/installer-process-boundary`. Root explicitly approved copying only the
verified-idle official41 target into this worktree's previously absent target.
Old cache, source, frozen binaries and evidence were unchanged. The owning
source actual ASB build ran first, locked/offline, one job, incremental disabled:
`target/installer44-baseline-build.log`, exit zero, 13.89s, version 0.2.41.

Actual ASB SHA256 before and after the real red run:
`e924e17a349265b8c4c80976a9354a041e8d77bbda91e828b851f56626001bbf`.
See `target/installer44-{before,after}-red.sha256`; the exact actual is preserved
as `target/qualification/installer44-red-asb`.

The first test compilation referenced private `ManagedGate::stop_before` and
failed with three E0624 fixture errors. That transcript remains in
`target/installer44-observed-red.log`. The fixture was corrected to call the
existing public bounded `stop` method; no production visibility or gate behavior
was changed. The result handshake was also published by private-file rename to
avoid observing a partially written JSON file. This is a diagnosed test-harness
correction, not a product fix or an unmodified retry.

Fresh actual run `target/installer44-observed-red-v2.log` compiled in 24.17s,
then **0 passed / 4 failed / 0 ignored**, exit 101, runtime 1.33s. Each failure
was the intended assertion, not admission or quiescence failure. Its sentinel
liveness check preceded main cleanup, a test weakness corrected below:

| Case | Actual observed failure | Preserved fixture suffix |
| --- | --- | --- |
| Inherited pipes | raw output call did not return in observation window | `18e0ab3ec8979d5f-60389-0` |
| Output budget | accepted 65536 bytes; no 4096-byte limit could be expressed | `18e0ab3eeac2f8b8-60389-1` |
| Staging writer | raw exit-zero success returned, then released descendant mutated staging | `18e0ab3eef546285-60389-2` |
| Stuck status | raw wait did not return in observation window | `18e0ab3ef431bd44-60389-3` |

All four directories under `target/installer-process-red/` retain a
`watchdog-settlement` marker, written only after matching guardian exit and
quiescence for the test tree and its sentinel, with sentinel survival checked
before stopping that separate owned gate. V2 checked liveness before main
cleanup, so only the corrected V3 proves post-cleanup sentinel survival.
The writer fixture retains actual
`returned.json` success, the release marker, changed `stage` and `writer-mutated`
marker. The pipe fixture retains its finite principal's exit-zero marker; its
raw output call never returned, so no direct wait result is fabricated there.
The byte fixture retains the actual accepted length. No fixture process remains
live; all actual target children were stopped/reaped through their explicit
owned guardians. No raw PID/name/group cleanup or host setting was used.

Self-review corrected only fixture ordering after immutable initial checkpoint
`e1be4b72377138bd8bb153e9d28582603dbab7be`: main gate stop/settlement first,
then sentinel `try_wait`, then the sentinel's own stop. Production was unchanged.
Fresh `target/installer44-observed-red-v3.log` compiled in 23.82s and returned
exit 101: **0 passed / 4 failed / 0 ignored**, runtime 1.35s. All four failed at
the intended final assertions after positive main/sentinel settlement and
post-main-cleanup sentinel-survival checks. The respective retained fixture
suffixes are `18e0ab6460f5f92c-705a5-0` (pipes),
`18e0ab64836db4c1-705a5-1` (overflow),
`18e0ab64885046b3-705a5-2` (writer), and
`18e0ab648d726175-705a5-3` (stuck). Each contains `watchdog-settlement`.
The exact actual and frozen-copy hashes remained the above `e924e17a...` after
V3. The V3 libtest's post-run SHA256 is
`23b7809d3f4eeae595b84f221ed781510e518bbebf642d5eb0df9ed881a85187`;
no pre-run libtest hash was measured and none is claimed. All handles are
terminal; read-only process inspection found no remaining fixture process.

Only three cfg(test) module-declaration lines were added to production source;
the complete implementation of LoggedStatus, command_output, DownloadChild,
adapters and publication remains unchanged. No version bump, ordinary full
suite, production repair, external installation or release is claimed. The
intentional failing tests are preserved for implementation after independent
design/red review; do not skip, weaken or silently regenerate them.

## Approved implementation: configuration foundation (partial)

The pre-code independent-cancellation/private-log invariants were signed in
`a1655ade`; signed merge `137c8e7c` brought qualified43 `e71a3b62` without
conflicts. Against that parent the integration had only this design, the red
fixture and its three cfg-test inclusion lines: runner43 and termination42
production paths were exact. Version is now staged 0.2.44, not released.

`ProjectConfig.installation` now deserializes absent sections/keys with approved
defaults, is emitted by normal project serialization, and validates nonzero,
checked-Instant durations with precise config path/key in the existing reader.
Three focused tests cover defaults, all malformed/zero fields plus direct
overflow, and fresh reads after edits while retaining prior loaded values.
The existing exhaustive remote host-config fixture needed only the default new
field; its four-field guest projection and all old host fields are unchanged.

Owning actual build first: `target/installer44-config-build.log`, exit0.
Initial libtest compile failed E0063 on that exhaustive fixture, preserved in
`target/installer44-config-tests.log`; no test ran. After the narrow fixture
correction v2 passed2/0, then the added reload/path-key case passed in fresh v3
`target/installer44-config-tests-v3.log`:3/0/0ignored,24.27s compile and0.00s
runtime. Actual ASB before/after focused tests remained SHA256
`db2f725fe7708d51b1679de14e9332c28ac4a6f2ca9db1c0d408d5d4597c6060`.
This is configuration-only partial proof: operation deadline freezing,
supervision/leases, typed all-caller propagation and the approved fault matrix
remain implementation work. The old process APIs remain unchanged and the four
observed-red tests have not been weakened or declared repaired. No full/PR/push.

## Intermediate worker-core checkpoint: TEST-SCOPED, not caller migration

Root explicitly authorized a Linux cfg(test)-only worker proof before broad
callback migration. The new `install/process.rs` and `process/native.rs` are
included only under `cfg(all(test, target_os = "linux"))`. Actual production
LoggedStatus, command_output, DownloadChild, source/custom callbacks and fallback
behavior remain unchanged; the original four observed-red fixtures are intact.
This is not a usable repaired installer or full class-44 source qualification.

The prototype provides typed `InstallFailure::{Ordinary(crate::Error), Abort}`,
with From only in the ordinary-to-install direction and an explicit outer render
method, not a general conversion erasing Abort. Immutable OperationContext clones
freeze project budgets and the shared absolute deadline. PreparedStep materializes
intent before original-Command configuration. The pre-established supervisor
retains the actual generic dependency Arc, exact gate and nonblocking pipe pumps
across bounded parent return. Ancillary logging uses a separate bounded queue
and private file-owning writer, never the supervisor's cancellation authority.
Accepted queue bytes are not claimed as written: actual positive writes advance
retained bytes; loss, pending settlement and write/flush faults remain separate.

Actual owner build ran first, `target/installer44-core-build.log`, exit0,14.70s.
Actual guardian ASB SHA256 before/final focused/lint checks:
`558cf2b6d44c892791af1a6e9f04346cfa82b7bb558c6df45b05830d94729ff0`.
The actual new supervisor itself executes in the libtest; the separately built
actual ASB supplies its real native guardian. No claim is made that the user CLI
already invokes this new test-scoped supervisor.

| Evidence | Actual result |
| --- | --- |
| `target/installer44-core-focus.log` | Initial4/0,0.10s |
| `target/installer44-core-focus-v2.log` | Expanded10/0/0ignored,6.48s |
| `target/installer44-core-clippy.log` | Failed4 style findings; preserved |
| `target/installer44-core-clippy-v2.log` | Strict workspace/all-targets/all-features `-D warnings` PASS,8.77s |
| `target/installer44-core-focus-v3.log` | Fresh final10/0/0ignored,23.99s compile,6.48s runtime |

The style-only corrections extracted the native test module to its own file,
removed two needless borrows and replaced checked unwrap with equivalent if-let;
no lint suppressions or weakened assertions were added. Final10 includes the
three native log tests; the initial4 are overlapping, not14 distinct cases.
The nested isolated umask child is part of its owning test, not an extra outer
success count. Final libtest post-run SHA256:
`a2fe90b32d7122be17074110933927e486d84f4cd387ff5dd9b3e40f3ed1c824`.
There is no claimed pre-run final libtest hash. Exact actual and libtest copies
are preserved in `target/qualification/installer44-core-{asb,libtests}`.

The held-writer fixture pins the real helper and grandchild using read-only
pidfds. It confirms their exits and matching base guardian quiescence, verifies
the separately owned sentinel is still live, and only then releases the writer.
Another fixture withholds observation (not cancellation authority), observes
bounded unknown-cleanup return while the real dependency remains held, then
allows matching settlement and verifies that dependency is finally released.
These are generic dependency-dropcounter tests, not yet proof of real source
Staging integration. Further actual cases cover deadline cancellation, writer
refusal after principal success, capture overflow rejection, original argv[0]/
cwd/env preservation, isolated-process umask022/private0600+0700 modes,
foreign-file/symlink preservation, and honest WriteZero/ENOSPC log-byte accounting.

Remaining before production integration/source approval: carry explicit context,
typed Abort and actual Staging leases through every source/custom/adapter hook;
bind and consume a settled publication permission to the SAME actual stage/lease
(the generic SettledStep currently is NOT that final stage-specific receipt);
render elapsed/selected numeric budget and deduplicated ancillary warnings at
the correct outer boundary; and complete required-read-error, pump-panic,
pre-admission/worker-start failure, inherited-pipe/aggregate-pressure, binary
stdin/pre-exec preservation, and actual no-fallback/no-publication fault cases.
Class45 must not treat this intermediate outcome as its final publication
permission. No additional full suite, live installation, push or PR was run.
All runtime handles are terminal at this checkpoint. Root requested a pause
before broad migration for independent review of this narrow core and evidence.

## Core review: independently found gaps and observed RED before repair

Root found three concrete gaps in immutable `b7f819e4`: preparation/readiness
can outlast cancellation/deadline and still reach unconditional CONT; queue
pressure drops bytes but permits later chunks, violating first-bytes prefix
policy; and per-step apply_network_proxy reloads project configuration instead
of using the operation snapshot, including a potentially foreign network scope.
Independent first review approved the limited core but did not find these gaps;
neither review constitutes production-caller or full-class qualification.

Four added tests plus private test-only admission hooks reproduced the old core
before any repair. `target/installer44-review-red.log`: compile23.54s,
**0 passed /4 failed /0ignored**, exit101,12.70s. All failures were intended:
the log contained late bytes after a forced lost chunk; both cancelled and
expired stopped guardians subsequently ran their real client marker AFTER the
parent had returned Abort; and preparation rejected a newly invalid config that
the existing operation should not reload. The late-start cases first observed
actual stopped readiness, held the worker until parent Abort, released it,
observed actual guardian quiescence, and verified the unrelated sentinel alive
before failing on the forbidden client marker. The post-CONT observation hook
only gives an actually resumed program scheduling time; it does not create the
marker or manufacture status. The forced-pressure writer was fully settled
before byte comparison. Markers/log directories remain under the owned target.

Actual ASB remained `558cf2b6d44c892791af1a6e9f04346cfa82b7bb558c6df45b05830d94729ff0`,
matching the preserved b7 core actual. The new red libtest is preserved separately
as `target/qualification/installer44-review-red-libtests`; its post-run hash is
recorded in `target/installer44-review-red.sha256`. This checkpoint contains
tests/hooks/evidence only, not the fixes. Original legacy-API red tests remain
unchanged. No broad caller migration, full suite, live tool installation or PR.

## Narrow core-review repair and focused evidence

Observed-red source `6f8ee9a796b3044c620f305c97974083834a1074` stays immutable.
The test-scoped supervisor now checks cancel/deadline after log preparation and
before admission. Private ManagedGate.resume_when checks its operation condition
AFTER actual wait_stopped, immediately before existing CONT. False retains the
stopped guardian for normal proof-aware cancellation; ordinary resume delegates
with constant true, preserving signature/protocol/behavior. A private test-only
readiness delay has no production representation. Its actual-native test delays
readiness300ms across a100ms deadline, refuses execution, then observes actual
guardian exit/quiescence without inventing an application exit status. Existing
route validation and admission ownership are unchanged.

Log truncation is sticky after the first lost byte: queue pressure, partial cap
admission or writer loss cannot be followed by accepted later chunks. Earlier
queued prefix bytes may finish. The held-writer test establishes accepted and
retained17 bytes (`A`+16`B`), discarded8 (`LOST`+`LATE`), and exact contiguous
contents after writer settlement. Independent held-writer cancellation remains
green. OperationContext snapshots the SAME explicit ProjectConfig.proxy with
its budgets; Step uses existing NetworkProxy.apply without project load or
NETWORK_PROJECT. Its original environment/loopback/CGI/Node rules remain intact.
A new invalid file does not affect the current operation; fresh public load
still rejects it. A foreign invalid network scope cannot replace the explicitly
frozen configuration. No proxy algorithm was copied or new policy added.

| Repair evidence | Actual result |
| --- | --- |
| `target/installer44-core-repair-build.log` | Owning actual build FIRST,13.95s,exit0 |
| `target/installer44-core-repair-focus.log` | Core15/0/0ignored,24.50s compile,19.15s runtime |
| `target/installer44-core-repair-managed.log` | Actual ManagedGate14/0/0ignored,6.43s |
| `target/installer44-core-repair-proxy.log` | Existing proxy/CGI/Node/scope/legacy-config6/0,0.00s |
| `target/installer44-core-repair-clippy.log` | Strict workspace/all-targets/all-features `-D warnings` PASS,15.94s |
| `target/installer44-core-repair-snapshot-final.log` | Refined foreign-scope case1/0,23.29s compile,0.00s runtime |

These are35 distinct focused tests; the final case repeats a core15 member,
not a36th distinct test. After15, only test initialization became a struct
literal and its comparison failure became a static message protecting possible
environment proxy credentials. That exact case passed again; no worker/guard
logic changed after15/14/6. No new failing repair run or blind retry occurred.
Original red transcripts and earlier frozen actual/libtests remain preserved.

Actual ASB before/after all checks:
`0f151fbb18460b5c05a047150b17631e0d58345bbd5de14171f6e6a061801b54`,
preserved in `target/qualification/installer44-core-repair-asb`.
Final libtest post-run hash:
`2ac699f16f07070b0fa4af2d23150777f9b3de406a1cff3a917aab42eb57156d`,
preserved in `target/qualification/installer44-core-repair-libtests`.
See `target/installer44-core-repair-final.sha256`; no pre-run final libtest hash
is claimed. Formatting/diff checks passed; all handles are terminal. Broad
migration is paused for root's independent source/proof review.

This repairs prototype-core gaps, NOT the production installer class. The
supervisor stays cfg(test)-only; legacy APIs and their four red fixtures are
unchanged. Actual Staging-bound consumed permission, typed all-caller abort/
fallback migration, remaining offline faults and outer diagnostic rendering
remain required. No full suite, external installation, PR, push or release.

Root independently reviewed repaired checkpoint `1a2eb7f2`, the complete core
and protected gate delta, all four observed-red regressions, added delayed-
readiness and foreign-scope tests, exact 35-distinct focused results and frozen
hash records. The late continuation, non-prefix logging and configuration
reload findings are corrected. The existing gate resume path delegates with
constant true; its protocol and ordinary behavior are unchanged. This review
does not turn the cfg(test) core into a production installer qualification.

Broad typed caller migration is now authorized under the approved design:
wire the real Staging owner/retained-on-uncertainty lease, bind consumed producer
permission to that same owner, and preserve Abort through every callback and
fallback. Remove unsafe legacy installer APIs only after all owners migrate.
Complete the listed remaining faults and meaningful timeout/log diagnostics;
never call pending writer state a complete log. Signed production source plus
focused cross-seam evidence requires independent review before a full suite.
Do not publish a PR or claim this class complete from prototype tests.

Local version44 is staging, not final release allocation. A separately
reproduced compatibility-proxy startup repair must precede the queued runner
delivery; root will coordinate monotonic patch versions during final ancestry
integration. Preserve all current source/binary checkpoints rather than amend.

## Actual staging owner and generation-bound permission: focused checkpoint

Root's intermediate WIP review correctly rejected pointer identity plus a
cloneable keepalive as sufficient settlement authority: a second scope could
otherwise mint a never-admitted token while another scope exposed a producer.
The prototype now uses actual owner-side private state, not that rejected design.
Its generation, active admission, actual settlement proof, operation identity,
consumed flag and sticky uncertainty are all revalidated on permission consume.
Before every attempted admission, the actual owner disables destructive Drop,
the old generation's proof is invalidated, and a private worker ticket is issued.
Only explicit never-admitted or matching guardian-quiescent completion settles
that ticket. Parent disconnect/deadline uncertainty permanently denies permission
even if the retained worker later observes quiescence. No process exit is invented.

ProcessScope returns status/captures separately from its non-cloneable final
ProducerPermission. Another scope cannot finish while an actual producer is
active; an already-issued token is stale after a later admission. Same-owner
permission is consumed BEFORE restoring quiet staging or destructive publication.
The worker owns the real Arc<Staging>, not a pathname surrogate; on an uncertain
parent return, that owner and its opened directory/file identities remain alive.
After later quiescence the guard drops without deleting raw evidence because its
quiet flag was never restored. Ordinary pre-admission failure settles only the
new never-admitted fact; it does not restore an old generation's permission.

This remains an intermediate cfg(test)/Linux prototype and actual-Staging seam,
NOT the broad production caller migration. The real Staging constructor,
identity-bound snapshot/publication and cleanup code are exercised by a private
test driver; producer fields/guards are test-scoped until all owning callers
carry the typed operation and permission. Legacy LoggedStatus, command_output,
DownloadChild and callback/fallback APIs remain unchanged, as do their four
original observed-red cases. Non-Linux qualification is not claimed.

| Evidence | Actual result |
| --- | --- |
| `target/installer44-staging-build.log` | Actual ASB build FIRST, exit0,13.58s |
| `target/installer44-staging-focus.log` | Core19/0/0ignored,25.40s compile,25.25s runtime |
| `target/installer44-staging-clippy.log` | Strict workspace/all-targets/all-features `-D warnings` PASS,15.24s |
| `target/installer44-staging-unwind-guard.log` | Exact real-writer case repeated1/0,24.53s compile,0.03s runtime |
| `target/installer44-staging-clippy-final.log` | Exact final source strict lint PASS,9.64s |
| `target/installer44-staging-fmt.log` | Formatting PASS; git diff whitespace check also PASS |

The four added distinct cases cover stale never-admitted/second-scope denial
while a real helper writes the actual Stage, matching quiescent publication,
wrong-owner proof preserving old destination and raw stage, one-use consume,
and actual guard retention across bounded parent uncertainty then raw retention
after late quiescence. The repeated case adds only an unwind release guard for
its handshake file so assertion failures cannot strand the waiting fixture.
It is not a twentieth distinct test. All tests are synthetic/offline; no live
tool install, provider, SDK, Docker, sudo or host setting is involved.

Actual ASB before/after and frozen copy:
`b518297ec9662e0fde38286bf170c88365186bd036b7a92b01911117de1f473d`.
Final libtest post-run/frozen copy:
`6c72fa1b2a6e931a820303d3ff586d65f9be0935bd7c9e2d461aad3747bd94ab`.
See `target/installer44-staging-before.sha256` and
`target/installer44-staging-final.sha256`, and preserved copies
`target/qualification/installer44-staging-{asb,libtests}`. No pre-run final libtest
hash is claimed. No failure/retry occurred in this matrix; all earlier red and
qualification evidence is preserved. All handles are terminal. No full suite,
PR, push or release; independent source review precedes broad caller dependency
on this owner-authority API. Remaining read/pump/start faults and complete typed
all-caller Abort propagation are still required, not inferred from these19 tests.

### Further actual-owner constructor gap: observed before correction

Root's review of baaf805e found that ProducerLease::new still allocated a fresh
state even for the same actual Stage, and with_state accepted arbitrary state.
An old Stage-issued permission therefore stayed valid while an alternate lease
admitted a writer into that SAME actual Stage. Generation checks alone did not
close this constructor path. The added real-Stage regression records rejection
while an actual helper is stopped at its own ready/release handshake, then
releases and settles the helper, verifies an unrelated sentinel survived and
stops that sentinel independently BEFORE its final assertion.

`target/installer44-owner-binding-red.log` records the intended0PASS/1FAIL in
0.06s, exit101; the old actual-owner permission was accepted during admission.
The owning actual build ran FIRST (0.13s, exit0); its hash remains the preserved
baaf staging actual b518297ec9662e0fde38286bf170c88365186bd036b7a92b01911117de1f473d.
See `installer44-owner-binding-red-before.sha256`. The fixture writes
`alternate-owner-cleanup-confirmed` only after actual settlement and sentinel
cleanup. This is an observed prototype authority gap; no production installer
repair, full qualification or historical user failure is inferred. This
checkpoint adds the regression/evidence ONLY, before repairing the constructor.

### Intrinsic actual-owner state correction

The observed-red checkpoint03812155 remains immutable. ProducerOwner now exposes
a borrowed reference to its own fixed ProducerState; the owning Stage returns its
private stored state. ProducerLease::new derives its state ONLY from that owner.
The arbitrary-state with_state constructor has been removed, and Stage's scope
constructor uses the same sole owner-bearing lease constructor as every other
caller. Test dependency owners likewise contain a fixed state instead of asking
the lease to allocate one. Pointer identity is still only one necessary check,
not a substitute for the actual owner's admission/settlement generation.

The SAME actual Stage alternate-constructor regression now rejects its old
permission during admission. Actual helper settlement, unrelated sentinel
survival/separate stop, preserved old destination and retained source are checked
before the final rejection assertion. All previous19 focused cases remain green.
This is the narrow constructor correction, not broad caller migration approval.

| Fresh evidence | Actual result |
| --- | --- |
| `target/installer44-owner-binding-repair-build.log` | Actual build FIRST,14.34s,exit0 |
| `target/installer44-owner-binding-repair-focus.log` | Core20/0/0ignored,25.80s compile,25.27s runtime |
| `target/installer44-owner-binding-repair-clippy.log` | Strict workspace/all-targets/all-features `-D warnings` PASS,16.77s |
| `target/installer44-owner-binding-repair-fmt.log` | Formatting PASS; git diff check also PASS |

Actual binary before/after the complete matrix and frozen copy:
`35f8fb9200b77cab02ed09943b49885a86027c68350a4fb080a09fb3705762b1`.
Final libtest post-run/frozen copy:
`9b81fbc91cbc8a8008443b3af3e27e411aeefefda3d6495417e4f120a9dc815e`.
See `target/installer44-owner-binding-repair-before.sha256` and
`target/installer44-owner-binding-repair-final.sha256`, plus preserved
`target/qualification/installer44-owner-binding-{asb,libtests}`. Original baaf
and earlier frozen binaries/logs remain untouched. No pre-run final libtest hash,
full suite, production installer qualification, PR, push or release is claimed.
All handles are terminal; the API awaits independent review before broad callers
depend on it. Prior read/panic/start/fallback and all-caller obligations still hold.

## Broad production graph and independent required-resource observer: partial

Root independently approved056dc8bd and its intrinsic-owner construction after
reading the complete delta, actual alternate-constructor regression,20-pass
focus, strict lint and matching actual/frozen35f8 hashes. Broad migration resumed.

Production tool/dependency/recipe/custom/post-install APIs now carry the frozen
OperationContext and typed InstallResult. Npm/pip/venv, Node version probing,
Python/uv discovery/bootstrap, perf package/extraction/build/runnable fallback,
bpftrace configure/build/version, runtime archives, Perfetto and FlameGraph,
and SWE/Terminal evaluator/catalog/source subprocesses use prepared commands.
Project-init distinguishes optional ordinary Python setup failure from a typed
Abort; the latter stops initialization rather than becoming a warning/fallback.
Nested installers use the same context instead of reloading configuration.
Pure SHA256 cache inspection reuses the existing pinned Rust checksum helper,
not another external inspection child. The existing non-Linux direct-child
backend is explicitly compatibility-only; no tree/pipe/deadline qualification
for it is inferred from Linux fixtures. No new authorization policy is added.

Actual source preparation carries the SAME Arc<Staging> through helper callbacks,
requires its intrinsic permission before existing identity-bound publication,
and returns typed Abort without destructive cleanup. Generic directory owners
hold opened real resource directories rather than only evidence path strings.
Perf Abort bypasses candidate/source fallback, runnable=false classification,
publication and scratch removal; bpftrace propagates it without help-text erasure.
Custom perf/bpf binary/metadata atomicity remains the separately assigned class45
residual, not something this subprocess migration claims to repair.

Required source size/identity inspection now has its OWN pre-established worker,
not a callback running on the cancellation pump. The owner supplies its policy;
the generic worker carries a clone of the actual owner and emits at most Ready
and Finished through a two-slot channel. First readiness precedes admission;
cancel/deadline is checked while waiting and again before actual continuation.
The supervisor retains independent gate/pipe/cancellation authority throughout
blocked observer IO. It requires observer completion separately from principal
exit, matching guardian quiescence and complete pipe capture. A pending reserve
expiry/failure poisons permission and keeps raw staging. No thread join occurs.
Both observer closure and actual-owner keepalive drop BEFORE Finished is sent,
avoiding a positive completion signal while that worker still holds the stage.
Ancillary log diagnostics are bounded snapshots, explicitly reporting pending
writers and retained/accepted/discarded counts without raw payloads.

`target/installer44-observer-focus.log` records21PASS/0FAIL/0ignored in31.36s
after23.17s libtest compilation. This includes all prior20 core tests plus actual
blocked-resource-observer ordering: a real principal and grandchild are observed
exited through pinned descriptors, and the actual guardian/pipe settlement hook
is positive BEFORE the deliberately held observer is released. The unrelated
sentinel remains alive and is stopped independently. The parent returns bounded
Abort; the real Stage stays alive until observer return, then raw evidence still
remains. The final fixture marker is blocked-observer-cleanup-confirmed.
The actual ASB built FIRST in13.43s; before/after/frozen hash is
`d29c8a077720ac420786aa2cf11209b58f7c97b49eef0ce8dce3a08c39da2f11`,
preserved at target/qualification/installer44-observer-focused-asb.

Intermediate diagnostics are preserved, not hidden by retries:
production-source-build (unused not-yet-wired APIs), broad-migration-build-v1
(four import/type/borrow issues), broad-migration-build-v2 (obsolete production
API dead code), and successful broad-migration-build-v3 (14.26s). Fixture
compile-v1 exposed71 signature/import errors; compile-v2 left four obsolete
unused test wrappers, which were removed. No runtime was claimed from those
compile failures. Strict broad-observer-clippy-v1 then found three needless
borrows; those were corrected without suppression before the next baseline.

### Newly observed operation-liveness gaps, not yet corrected here

Auditing the pure-copy path exposed missing operation-live checks in permission
current/consume: operation Arc identity alone does not reject its cancelled or
expired state. A local source copy with no external helper can publish anyway.
Also a physical required-capture Abort does not yet mark the shared context
cancelled, so a different actual owner lane can still admit a new producer.

Three new actual regressions were run BEFORE correction. They show both
cancelled and expired local copies overwrote their isolated old destinations,
a previously issued permission survived cancellation, and a real capture abort
was followed by a different lane actually writing its forbidden marker. The
native helpers settled through their owned gate before final assertions; the
pure-copy and permission cases start no external producer. Retained markers are
local-copy-operation-review-confirmed and later-lane-operation-review-confirmed.

`target/installer44-operation-review-red.log`:0PASS/3intendedFAIL/0ignored in0.06s,
exit101,26.87s compilation. Owning actual build FIRST13.27s, with unchanged
before/after/frozen SHA256
`5993d07542c3bb3fb4af138b21a865a9167cd76d51ce00b711f84cdfb12dad7d` at
target/qualification/installer44-operation-review-red-asb. This checkpoint
preserves the failures; no operation-liveness correction is claimed yet.

This is a partial production graph/core proof, NOT final class clearance. Legacy
LoggedStatus/command_output/DownloadChild definitions temporarily remain cfg(test)
ONLY while historical fixture entries are modernized; they are no longer used
by production callers and are not a final dead-code retention strategy. Required
read/panic/start/pre-exec/binary-stdin/aggregate-log faults, all migrated owning
CLI/fallback cases, meaningful final timeout diagnostics and the new operation
regressions still need completion. No full suite, PR, push, merge or release.
Actual delivered main ancestry will be merged normally before combined final
qualification; immutable prior source, failures and frozen binaries are preserved.

### Review checkpoint: operation liveness and typed final commit admission

This SOURCE-ONLY follow-up preserves the three observed failures above. The
operation-liveness correction alone built the actual ASB successfully in13.22s
(`target/installer44-operation-live-repair-build.log`); no post-fix runtime result
is claimed yet. Physical native Abort now cancels the shared operation, while a
stale/different authority rejection does not cancel a legitimate original lane.
Pure-copy entry, permission current/consume and scope finish check operation
liveness, not just Arc identity. Rejected authority never becomes a receipt.

Root requested a minimal protected publication seam before further runtime.
`publish_copy_checked<E>` returns distinct CorePublication(Error) and Check(E).
The old test compatibility wrapper supplies an infallible check and preserves
its prior ordinary publication and cleanup semantics. A nonclone consumed
permission retains the actual intrinsic owner's state, generation, proof and
original frozen operation. Its final constant-time check validates consumed,
inactive, unpoisoned matching state plus operation liveness; it performs no
callback IO. No external caller can supply an arbitrary owner state.

The check runs AFTER the existing CommitRename fault hook and immediately
BEFORE either replacement rename or no-clobber hard link. This is validation at
commit admission, NOT an atomic clock-plus-filesystem operation or a promise
that a potentially blocking rename completes before the operation deadline.
Typed Check rejection disarms both actual staging owners, leaves detached raw
and quiet candidate namespaces retained, skips quiet.finish/commit.finish, and
does not fall through ordinary cleanup, fallback or public-error conversion.
Ordinary core failures retain the existing settlement/error behavior.

New unexecuted owning tests cover late cancel/expiry at the real fault hook for
both commit variants (old target bytes unchanged or no new target, both raw and
candidate payloads retained), and a same-owner post-consumption admission
attempt invalidating the guard without executing its forbidden client. These
are test source, not qualification evidence. Source review is requested BEFORE
building/running this protected seam.

An additional independently identified ordinary-pre-admission observer issue
remains explicitly open in this checkpoint: an observer may keep the actual
Stage after an ordinary spawn/pre_exec failure, while early permission acceptance
re-enables quiet Drop before Arc::try_unwrap fails. The required deterministic
old-path regression must prove eventual Stage Drop retains raw after an Abort,
not just inspect before releasing that observer. The proposed minimal repair
acquires the uniquely owned Stage BEFORE consuming its intrinsic permission;
Arc uniqueness is resource acquisition, never substituted for producer proof.
That repair is deliberately NOT included before observing the requested RED.
No full suite, PR, push, merge or release is authorized or claimed here.

### Observed final-owner Drop retention failures (99736eb4)

The observer fixture now uses an actual Stage-owned last-field completion
receipt, published only after its custom Drop body and all earlier resource
fields have dropped. Weak strong-count zero is NOT used as destructor proof.
The local cases likewise inspect raw only after that same explicit receipt.

Actual ASB built FIRST in13.29s (`installer44-retention-red-build.log`). The
observer regression compiled in23.35s and then FAILED0/1/0 in0.05s, exit101,
at its intended final assertion: raw was present at the custody Abort, but
deleted after observer release and completed Stage destruction. The real
pre_exec callback failed with fixed EIO after an owned descriptor handshake;
the forbidden application was never admitted. The unrelated sentinel stayed
alive through observer release and was separately stopped. Its final marker
is ordinary-observer-review-confirmed. Transcript:
`target/installer44-observer-ordinary-red.log`.

Separately, `target/installer44-local-abort-red.log` FAILED0/1/0 in0.00s, exit101:
BOTH validate-cancel and explicit typed Abort deleted raw after completed Stage
destruction. Old installation bytes survived. Its final marker is
local-abort-drop-confirmed. No external producer runs in these local cases.
The actual binary was unchanged before/after both executions and its frozen
copy, SHA256
`8af09ebf7238994f094184b9f5de2874b563b3131ed5d2cabefe53cedf94ff7c`.
Baseline binary/libtest copies remain under target/qualification with prefix
installer44-retention-red; both failed transcripts and markers are preserved.
This records observed failures BEFORE their production retention correction.

### Focused retention and commit-admission correction (8939bcbe)

After independent root source approval, the actual ASB rebuilt FIRST in13.50s.
Typed Abort exits from validation, scope completion and permission consumption
now sticky-retain the actual owner. Intrinsic state poisoning never erases the
separate active/proof facts; a later actual settle does not clear poison.
Source ownership acquisition precedes quiet permission consumption. Failed
Arc::try_unwrap retains the real owner; uniqueness never substitutes for the
intrinsic generation/proof/operation check.

The unchanged observer and local regressions now PASS, with actual completed
Stage Drop receipts before final raw inspection. Original three operation
liveness regressions, late cancel/expiry for BOTH commit variants, same-owner
guard invalidation and prior custody/cancellation tests also PASS.

| Transcript under target/ | Result |
| --- | --- |
| installer44-retention-fix-build.log | actual build13.50s |
| installer44-retention-fix-process.log |24PASS/0FAIL/0ignored,32.56s |
| installer44-retention-fix-source.log |23PASS/1FAIL,2.04s; preserved fixture mismatch |
| installer44-retention-fix-source-v2.log |24PASS/0FAIL/0ignored,1.99s |
| installer44-retention-fix-source-v3.log |24PASS/0FAIL/0ignored,1.51s |
| installer44-retention-fix-native.log |4PASS/0FAIL/0ignored,0.04s |
| installer44-retention-fix-clippy.log | strict workspace/alltargets/allfeatures,-Dwarnings,17.01s |
| installer44-retention-fix-fmt.log | rustfmt check PASS |

The first source-family failure was an obsolete fake-curl assertion demanding
legacy --max-time, not a downloader timeout failure. The operation deadline is
now the authority; unchanged actual managed timeout coverage remains. Test-only
d02ccbc1 replaced that assertion with --connect-timeout;49941554 strengthened
the following value to10 and requires positive numeric --max-filesize, leaving
the existing output-path, URL and payload checks intact. These are fixture
changes only. The source family still includes explicitly historical wrappers;
its24 passes are NOT proof that all migrated production callbacks are covered.
No live provider, network source, SDK or privileged host collector was used.

Actual binary/frozen copy and before/after/final hashes all equal
`cda77047ae2fa77411c85c94c7e350712ae9a65c94ed12cbe45816ca60ee8525`.
The exact actual binary and final libtest remain frozen under
target/qualification/installer44-retention-fix-{asb,libtests}. Process/source/
native filter sets are disjoint52 top-level passes, not an exhaustive class or
full-stack claim. All handles are terminal. Diagnostic consolidation and the
remaining production-callback/fault matrix remain open. No full suite, PR,
push, merge or release is claimed or authorized by this focused evidence.

### First migrated callback fault matrix and consolidated boundary diagnostics

Source c08d7353 built actual ASB FIRST in16.03s, frozen unchanged through the
following focused executions and strict lint. Migrated install_input_in tests
now invoke the REAL callback's same-owner ProcessScope: synthetic pipe EIO,
caught pump panic, actual capture overflow and actual source-name replacement
all return typed Abort, never run the callback continuation/publish, positively
settle the guardian tree and preserve the named evidence/foreign replacement
through completed Stage Drop. The unrelated sentinel survives each case.
The owner-loss case preserves the arriving foreign bytes; it does NOT claim to
reconstruct the original file the fixture deliberately unlinked.

The separate worker-start fault exercises the shared spawn-error path before
any process admission. It remains an Ordinary known-negative and supports quiet
scratch settlement only with the intrinsic never-admitted permission. No
fabricated exit is used. Private cfg(test)-only factories inject failures;
there is no production fault environment variable or caller-supplied PID/signal.

`installer44-migrated-source-faults.log`:2PASS/0FAIL/0ignored,0.21s after24.77s
libtest compilation; the deliberately caught pump panic prints its static test
message but its owning test PASSes after cleanup. `installer44-duplex-binary.log`:
1PASS,0.05s; actual Python copies131072 binary stdin bytes, with262144 extra
stdout bytes and262144 stderr bytes, preserving invalid UTF8, env_remove and
configured cwd without pipe deadlock. No provider/network/SDK is involved.

`installer44-diagnostic-summary.log`:1PASS,0.00s;130 synthetic anomalous log
snapshots emit ONE warning heading,128 bounded per-path details and truthful
two-omitted count; a second report is empty and pending is not called complete.
`installer44-migrated-fault-clippy.log`: strict workspace/alltargets/allfeatures
-Dwarnings PASS16.02s; fmt/diffcheck PASS. Actual/frozen before/final SHA256:
`5a81608f194e4cc5b4ac08b8fb2bb9b795e28c9ffb2a51b6abcb0e927528f052`.

This is four additional focused tests (one covers four real callback failures),
not a full or live-host qualification. Log fault phase/kind classification is
still pending: the generic previous write-failed wording cannot distinguish
namespace/open, writer-start, queue, write and flush failures. Real adapter
fallback/CLI paths, remaining fault/budget coverage and legacy test-API removal
also remain open. All handles terminal; no full suite/PR/push/release claim.

### Typed diagnostic phases: approved focused qualification

18e8506b distinguishes Namespace/Open/WorkerStart/Queue/Write/Flush faults using
private phase+ErrorKind, with first fault sticky. Setup failure includes the
intended private log path and correctly reports a never-started writer as
settled-with-fault, not pending. Queue fullness records bounded loss and keeps
the first-bytes-only policy. Required capture read/configuration ErrorKind is
instead surfaced in the fatal Abort detail; ancillary loss never replaces
capture failure or changes it into success.

Actual ASB built FIRST in13.44s. Initial libtest compile failed only for two
unnecessary mut bindings in new tests; installer44-diagnostic-phases-focus.log
is preserved. Signed test-only3a78edd0 removes those bindings without changing
production. Fresh installer44-diagnostic-phases-focus-v2.log records34PASS,
0FAIL,0ignored in32.94s after compilation. This includes true namespace/open
regular+symlink collisions, never-started writer, actual flush ENOSPC, zero/
ENOSPC write failure, sticky queue/prefix loss, actual healthy Source publication
despite ancillary write/flush failure, and all earlier process/custody cases.
No foreign file or permissions were changed, no raw payload was rendered.

Strict workspace/alltargets/allfeatures Clippy -Dwarnings passed16.01s
(installer44-diagnostic-phases-clippy.log); fmt/diffcheck passed. Actual/frozen
before/final SHA256 all equal
`3e8e90411f2a614d22707f7b40722fe5026802847ddd84f16a71a58422369e60`.
All handles terminal; this remains focused Linux qualification, not complete
adapter/CLI/aggregate-budget coverage or non-Linux qualification. No full suite,
PR, push or release is claimed.

### Approved aggregate ancillary policy and prerequisite ancestry (source)

Normal signed merge e125d2fb brings delivered main66fb and the independently
approved minimal reaper prerequisite73e, NOT the unfinished class50 guardian
branch. Only Cargo.toml/lock version conflicts occurred;44 remains a local
staging value pending root release allocation. Process cleanup/helper exchange/
checkpoint source matches73e; our gate/installer/runner/collector protections
were unchanged by the merge. No qualification is inherited merely from merging.

Root approved a private operation-owned diagnostic budget: four live ancillary
writers and64MiB total accepted bytes, with16MiB per-step unchanged. Writer
permission is nonblocking and acquired BEFORE namespace/thread admission. The
actual worker holds it through write, flush and file close; taking a pending
snapshot does not return it. Denial is settled-with-fault Budget/WouldBlock,
not a fake pending writer and never a capture/status/product failure. Byte quota
is charged for accepted prefix bytes and not replenished when a writer exits or
fails. Only an actual SyncSender rejection proves bytes unaccepted and returns
that reservation. No uncertain accepted bytes are refunded. This policy adds no
public configuration/authentication requirement; required capture budgets stay
independent. Tiny-budget/held-writer and healthy-product tests are SOURCE ONLY
until the new combined actual is built and focused cases are observed.

The new combined bc5d8ad2 actual built FIRST in13.35s and was frozen through
all following focused tests. `installer44-aggregate-budget-focus.log` records
37PASS/0FAIL/0ignored,32.86s, including the actual held-writer/accepted-byte
quota cases and successful required capture/publication with no diagnostic
admission capacity. Four pending snapshots retain all four slots; the fifth
request creates no namespace; slots return only after actual write/flush/close
settlement. Accepted bytes remain charged across steps after worker exit.

Combined prerequisite filters are disjoint: reaper5PASS/0.04s,
checkpoint transport7PASS/2.42s, runtime inputs8PASS/2.12s, actual managed
gate14PASS/6.48s. Logs are target/installer44-combined-{process_cleanup-tests,
sweep-checkpoint-transport_tests,runtime_inputs-tests,managed-gate}.log.
The initial process_gate::tests filter matched ZERO tests and is preserved in
installer44-combined-process_gate-tests.log; it is NOT qualification evidence.
The corrected managed::tests filter above executed all14 actual cases.
Total71 disjoint top-level focused passes; no full suite was run.

Strict workspace/alltargets/allfeatures Clippy -Dwarnings passed22.67s
(installer44-aggregate-budget-clippy.log), fmt/diffcheck passed. Before/final/
frozen actual SHA256 all equal
`f9886a273216f4f28c5d2faac2282b8b7c4ff9f48aa9d6b5659968f6967a616f`.
All handles terminal. This proves the stated combined focused seam, not final
adapter/CLI fault coverage, legacy-wrapper removal or full class completion.

### Actual public CLI / owning Perf and BPF adapter abort paths

TEST-only fd974714 runs the frozen combined f988 actual public command, not a
direct helper clone. `installer44-public-cli-faults.log` records2PASS/0FAIL,
0ignored in3.81s after24.60s libtest compilation. One test executes four Perf
CLI scenarios: catalog inspection overflow, package download deadline and
runnable version overflow/deadline. Real registry dispatch propagates Abort,
preserves the prior executable and actual private package evidence, and neither
tries another package nor enters source fallback/publication. A second test
executes two BPF version overflow/deadline scenarios: old binary and metadata
remain unchanged, uname is not reached, and the typed class is not replaced by
generic build/host-install help. These are six synthetic offline CLI executions,
not six independently named tests or real host collector qualification.

The fixture gives each actual CLI a matching owned guardian, an independent
sentinel, bounded file-backed ordinary output and a final matching cleanup
receipt before assertions. Every package/tool command is an inert local script;
curl/tar/make/cmake fallback stubs only record a forbidden call and exit, never
accessing a network/provider/SDK or privileged host collector. All six outer
CLI trees settle and each sentinel remains alive until its separate stop.

`installer44-public-cli-clippy.log`: strict workspace/alltargets/allfeatures
-Dwarnings PASS20.98s; fmt/diffcheck PASS. No production changes or binary
relink occurred: actual/frozen final SHA remains
`f9886a273216f4f28c5d2faac2282b8b7c4ff9f48aa9d6b5659968f6967a616f`.
All handles terminal. Original four RED fixtures and other historical raw test
helpers still require migration/removal before any full-suite claim.

Independent review found fd974's parent script writes could expose executable
writable descriptions to unrelated parallel libtest forks. Its focused result
is real, but NOT evidence excluding that hazard. Signed TEST-only77d7b4fc keeps
all six cases/assertions/bytes/modes while moving generation into one exact-case
child before its first local spawn. Outer libtest creates only directories,
uses the existing bounded ProcessScope/Inspect, and consumes actual matching
directory proof BEFORE interpreting child status. Only the exact one-test
child receives --test-threads=1; the suite stays parallel. e5130a8b forwards
optional LLVM_PROFILE_FILE through both env_clear boundaries, never credentials
or other inherited settings. This does not restore any coverage gate.

Actual rebuilt FIRST13.41s, then installer44-isolated-cli-focus.log records
2PASS/0FAIL/0ignored in2.90s after23.50s compilation, outer default parallel.
Both original six-case assertions and nested CLI/sentinel cleanup survive;
outer exact-child tree settlement has its own retained marker. Strict
workspace/alltargets/allfeatures Clippy -Dwarnings passed9.12s, fmt/diffcheck
passed. Before/final/frozen actual SHA is
`179fa45a7e855e29c07d9612eea22a94f7014788641cc65ad63a427da1c99565`.
The first docs-only07d38236 accidentally repeated the older f988 checksum;
the preserved isolated-cli before/final hash transcripts both show179fa45a,
matching the frozen isolated-cli copy. No binary mutation occurred during tests.
All handles terminal. Other existing historical parent-generated fixtures
remain separate migration/shared-writer work; no full-suite clearance is claimed.

### Original obligations and primary-error RED (22 October)

037eecaa migrated the original four observed-red obligations to actual typed
ProcessScope; 27da62a8 corrected one remaining coverage-only raw command_output
caller after removing that raw wrapper and LoggedStatus.logged_status. The
initial E0425 compile transcript remains original-obligations-focus.log. The
next original-obligations-focus-v2.log records2PASS/2FAIL in1.08s: its two
normal-principal fixtures incorrectly required Deadline/nullstatus. Actual
existing guardian behavior quiesces adopted descendants BEFORE its receipt;
both returned exit0 with consumed matching proof. Staging remained initial
after releasing the writer, with no writer-mutated marker, and the separately
owned sentinel survived. Failed libtest is frozen as
qualification/installer44-original-obligations-failed-libtests, SHA
`6a189f63a67a09b043241573dc8386cec03a4a38388f8547e67274f02c65507c`.

Root approved TEST-only0ab2f410 to accept truthful exit0 after that proof,
while retaining unchanged-stage/no-late-writer assertions and checking exact
valid-prefix bytes, not only length. After actual-FIRST rebuild,
original-obligations-focus-v3.log records4PASS/0FAIL/0ignored in1.08s.
Stuck and overflow still require Deadline/CaptureOverflow respectively,
confirmed cleanup and null process status. Actual/frozen before/after SHA is
`aad9b505b09fc06aaf5f8056c9b3552e8c850fbebdc8fbe1e51a2ff510c1698e`.

Separate approved TEST-onlyf34eb7f1 proves a remaining diagnostic defect through
actual install_input_in: a local validator cancels its operation and returns
Ordinary("specific original validation reason"). After actual Stage Drop
completion, retained raw/unchanged destination and Abort classification all
pass; the final assertion fails because later Cancelled Abort erased that
original reason. primary-error-red.log records0PASS/1FAIL/0ignored in0.00s,
after23.77s compilation; primary-error-red-build.log actual-FIRST passed.
The same aad9 actual/frozen hash was unchanged. Failed libtest is preserved
as qualification/installer44-primary-error-red-libtests. No production repair
is included in this observed-red checkpoint. All handles terminal; no full
qualification claim. Remaining Source/DownloadChild test-shadow migration is
explicitly unfinished.

Root reviewed and approved5aae0bd4's minimal typed repair: only a later Abort
can append the preceding Ordinary reason, retaining its existing detail and all
cause/cleanup/capture/evidence fields. Actual owner unwrap and permission
acceptance failures use the same composition as scope.finish; neither can
reclassify uncertainty as a fallback. TEST-only73d7b6e4 covers existing/absent
Abort detail, present/absent prior error, unchanged typed facts, exactly one
prior occurrence and unchanged Ordinary behavior. The original actualSource
RED is unchanged and now passes.

Actual-FIRST primary-error-fix-build.log passed14.40s. Frozen executable
qualification/installer44-primary-error-fix-asb matches before/after/final SHA
`a6723742ea9e454cd10de18bab53ace88411c8798ba6736f5901930e4ed839f6`.
All primary-error-fix logs are preserved under target/:

| Focus | Passing / failed / ignored | Runtime |
| --- | --- | --- |
| process | 32 / 0 / 0 | 6.58s |
| native | 7 / 0 / 0 | 0.02s |
| original | 4 / 0 / 0 | 1.09s |
| cli | 2 / 0 / 0 | 2.78s |
| source (historical shadows, not migrated-wiring proof) | 24 / 0 / 0 | 0.38s |

These are69 disjoint top-level passing executions, not a full suite or blanket
installer acceptance. Strict workspace/alltargets/allfeatures Clippy -Dwarnings
passed17.97s; fmt/diffcheck passed. Every handle is terminal; actual binary was
unchanged during all runtime tests. Source/DownloadChild test-shadow retirement
remains the next explicit unfinished task; all original failure transcripts
and frozen binaries remain intact.

### Source-shadow retirement candidate (source only; not yet executed)

After approved main merge986360d6, repair fixtures319783fc and publication
fixtures8bb21aea call actual install_input_in with explicit project-root
configuration and the SAME validation ProcessScope. Bounded harness5b8a4667
and b5e0e560 require matching consumed directory proof and an exact one-test
libtest footer before status acceptance. Script writes/namespace cleanup use
the reviewed upstream shared fixture writer, not a second writer implementation.

The next source candidate removes test-only DownloadChild, raw logged_spawn,
duplicate source copy/download/publication implementations, and unused old
verified/executable conveniences. Remaining coverage-only conveniences now
load the explicit root and delegate to actual install_input_in; they contain
no alternate subprocess or publication algorithm. The oversized-copy test
creates valid project configuration and checks the actual size reason, not
merely any error. Full original red/old lifecycle sources remain immutable in
Git; removal does not erase historical failed evidence.

The Linux lifecycle replacement has five actual Source cases, not twelve
invented raw-poll simulations: remote known-never-admitted Download worker
failure, caught Abort returned as Ok, caught Abort returned as a specific
Ordinary error, unwind after uncertain return, and normal validated success.
The operation-local test-only one-shot fault matches typed Purpose::Download,
records actual Stage destructor completion and reuses the existing native
worker-start fault; it cannot affect another operation/outer helper. It is
neither an environment switch nor production policy. Unknown cases keep the
actual observation/lease held until AFTER bounded Source return, release it,
then require the Stage LAST-field completion receipt before examining retained
raw. A separate real guardian sentinel survives until explicitly settled.

Historical obligation mapping (new actual Source cases complement, not replace,
the independently owned generic gate/reaper tests):

| Historical raw case | Current obligation / owning evidence |
| --- | --- |
| download-worker-creation | actual remote Source Download fault, zero downloader/validator marker, Ordinary, completed Stage Drop and legitimate quiet cleanup |
| probe-worker-creation | migrated_source_worker_start_failure_is_known_never_admitted_not_unknown_success, actual Source callback and Stage completion |
| spawn-failure | ordinary_preexec_failure_must_retain_actual_stage_after_observer_release; real pre_exec failure plus blocked observer custody |
| download-pending | actual remote configured timeout plus generic pending-custody tests and Source late-settlement cases; not a claim of every cross-product fault/purpose combination |
| download-poll-error | owned reaper controlled_poll_error/controlled_wait_error real identity tests plus actual Source fault/unknown settlement propagation; old raw Child polling branch no longer exists |
| probe-pending | actual Source caught-abort-ok with held matching observation and late actual Stage completion |
| probe-poll-error | generic real observation-error custody, migrated Source capture/pump/owner fault matrix and sticky-Abort composition; no invented ExitStatus |
| drop-pending | actual Source unwind-after-abort retains the Stage until real matching worker release, then retains raw after its destructor |
| unwind-poll-error | same actual unwind case plus owned reaper observation-error custody; no fake permanent poll result |
| finish-success-pending | actual Source caught-abort-ok cannot publish despite validator returning Ok |
| finish-error-poll-error | actual Source caught-abort-ordinary preserves specific primary reason exactly once while retaining Abort and raw |
| normal-success | actual Source normal-success validates through same scope, publishes complete bytes and completes quiet Stage cleanup |

Current c725 ancestry contains the original five owned-reaper tests. The two
separately reviewed fairness cases are not claimed integrated/qualified here;
later combined ancestry must retain them when delivered. No production change,
new runtime evidence, full-suite clearance or final release allocation is
claimed by this source-only migration candidate.
