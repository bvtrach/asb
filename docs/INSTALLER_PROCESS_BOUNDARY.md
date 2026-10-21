# Installer subprocess boundary: design and observed-red plan

Design/test-only stage 44, based on independently approved official-engine source
`31070b9a7c61c1e0a3c15180496c153ed4a9030a`. Manifest version stays 0.2.41.
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

The TUI's outer installation gate still owns its worker. Inner step deadlines
are not a replacement for owner-loss cancellation; nested guardians must settle
without abandoning children when their outer owner exits. Synchronous CLI and
TUI use the same step implementation. Linux provides the current owned native
gate backend. Native Windows/other Unix need an explicit platform decision and
eventual backend qualification; do not silently label direct-child cleanup as
equivalent or claim Linux tests qualify them. Preserve documented behavior until
that decision is independently reviewed; do not add an accidental platform-wide
unsupported-install regression during a Linux-only repair.

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
was the intended post-cleanup assertion, not admission, quiescence or sentinel
failure:

| Case | Actual observed failure | Preserved fixture suffix |
| --- | --- | --- |
| Inherited pipes | raw output call did not return in observation window | `18e0ab3ec8979d5f-60389-0` |
| Output budget | accepted 65536 bytes; no 4096-byte limit could be expressed | `18e0ab3eeac2f8b8-60389-1` |
| Staging writer | raw exit-zero success returned, then released descendant mutated staging | `18e0ab3eef546285-60389-2` |
| Stuck status | raw wait did not return in observation window | `18e0ab3ef431bd44-60389-3` |

All four directories under `target/installer-process-red/` retain a
`watchdog-settlement` marker, written only after matching guardian exit and
quiescence for the test tree and its sentinel, with sentinel survival checked
before stopping that separate owned gate. The writer fixture retains actual
`returned.json` success, the release marker, changed `stage` and `writer-mutated`
marker. The pipe fixture retains its finite principal's exit-zero marker; its
raw output call never returned, so no direct wait result is fabricated there.
The byte fixture retains the actual accepted length. No fixture process remains
live; all actual target children were stopped/reaped through their explicit
owned guardians. No raw PID/name/group cleanup or host setting was used.

Only three cfg(test) module-declaration lines were added to production source;
the complete implementation of LoggedStatus, command_output, DownloadChild,
adapters and publication remains unchanged. No version bump, ordinary full
suite, production repair, external installation or release is claimed. The
intentional failing tests are preserved for implementation after independent
design/red review; do not skip, weaken or silently regenerate them.
