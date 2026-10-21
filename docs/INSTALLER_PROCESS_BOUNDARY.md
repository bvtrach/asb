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
