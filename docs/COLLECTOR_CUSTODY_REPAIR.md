# Collector producer custody and completeness repair

This is the class-35 design, observed-red evidence and partial implementation,
based on publication source `f8139463`. No complete qualification, PR or release is claimed.
The retained version remains 0.2.32 until its later reviewed stack integration.

## Confirmed gaps at the original design baseline

- `ProcessTreeSnapshot` calls `Command::output`, with no deadline or output bound,
  then unconditionally joins that worker. A stopped benchmark cannot interrupt
  an active command or a pipe writer inherited after its principal exits.
- Snapshot stdout/stderr writes and command exit failures are discarded. A
  readable, valid first sample may therefore masquerade as a complete capture
  after a later ENOSPC, WriteZero, read error, failed command or worker panic.
- `Collector` normal stop/Drop still invokes external `kill` and `pkill -P`, then
  an unconditional `Child::wait`. A PID-parent scan is not exact owned-tree
  custody; direct-child exit does not prove its capture writers are quiescent.
- Perfetto readiness starts a detached blocking reader and uses kill/wait on
  failure. Preserve its actual ready byte and marker ordering, not only startup.

## Proposed ownership boundary

1. A generic per-session producer owns either a supervised process or a snapshot
   worker, its admitted captures, a sticky completion observation and an explicit
   settlement state. Replace the `sleep 86400` placeholder rather than pretending
   a sampler is an unrelated sleeping process. Adapter IDs, arguments, metric
   formats, observation limits and graceful-flush requirements stay in adapters.
2. Establish fallback custody fallibly before starting any producer. Use the
   existing launch descriptor and native `ManagedGate` for exact descendant
   supervision where available. A successful gate exit plus matching quiescence
   receipt, not a direct tool/wrapper exit or EOF, permits freezing capture.
   Unsupported native supervision remains explicit; never silently use parent
   PID scans or numeric process-group killing. Preserve non-Linux scope honestly.
3. Graceful shutdown needs a new private bounded gate control operation. Current
   gate `Owner::bind` maps SIGINT, SIGTERM and SIGHUP to *cancellation*, followed
   by descendant TERM/KILL. Sending SIGINT to that guardian therefore does NOT
   deliver the adapter's required SIGINT to its actual collector. A typed private
   request must instead ask the guardian to interrupt its pinned owned principal
   while keeping supervision and the final quiescence protocol alive. Validate
   protocol compatibility and command ownership; do not invent authorization or
   signing requirements. An expired graceful window escalates through existing
   proof-aware cancellation, retaining uncertainty rather than killing guardian.
4. All request, readiness, observation, graceful drain and fallback waits use
   purpose-specific absolute deadlines. No unbounded caller wait or unfinished
   thread join; uncertain direct children retain pre-created reaper ownership,
   and uncertain guardians retain wait-only custody. Shared private control frames
   and capture pumps remain bounded. Finalize every sibling independently.
5. Snapshot cancellation crosses into the active owned command. The owner can
   stop it without waiting for its sampling interval or inherited stream EOF.
   Adapters supply bounded observation policy; retain the exact identity checks
   around complete samples and mark generation loss separately from I/O failure.
6. Every producer read/write/flush/exit/panic failure becomes a sticky typed
   incomplete-capture fact. Retry Interrupted; WriteZero is failure. Never infer
   success from a nonempty prefix or a signal-only placeholder. A failed producer
   clears derived metrics/series, retains raw evidence and diagnostics, and cannot
   authorize capture cleanup or completed evidence. Independent official grades,
   provider/workload settlement and other collector outputs remain intact.
7. Preserve publication32's admitted raw handles, independent read offsets,
   confirmed-result-before-consume and per-file durable error classifications.
   Trace flush must finish before artifact parsing; whole-run Perfetto continues
   through lifecycle finishing, retaining its shared service/tracefs leases.

## Offline qualification order

First execute the three initially-red `measurements::collector_custody_tests`
fixtures: an active 1.5-second command must cancel promptly; a retained duplicate
stdout writer after command exit must not extend stop; `/dev/full` ENOSPC must
be reported rather than discarded. Fixture safety bounds avoid indefinite hangs,
and only fixture-owned process/FD/thread handles are used.

All three failures were actually observed at `727000e5` using the exclusive
diagnostics cache with `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2`:

- Active-command stop blocked **1.501913556 s**, exceeding its 600 ms assertion.
- The retained duplicate writer extended stop by **1.495494427 s**, likewise
  exceeding its 600 ms assertion after the principal exited.
- Actual `/dev/full` ENOSPC was discarded: stop returned `()` and stderr was empty.

The focused suite exited 101: zero passed, three failed, zero skipped, in 1.53 s.
The built baseline executable stayed unchanged before/after those actual cases:
`57c123c847991f707c06d2a74eb7d65c516e6882baec7543aa2aa0fdcbe12ab3`.
Local transcripts are `target/collector35-baseline-{build,red}.log`, with matching
`collector35-baseline-binary-{before,after}.sha256` records. This establishes the
three specific defects, not full tree-cleanup, graceful-flush or SDK qualification.

Before production implementation, retain actual red transcripts and independently
review this design. Then add deterministic private WriteZero, Interrupted, read
failure, worker creation/panic, pre-spawn keeper failure, readiness failure,
graceful flush and forced-cancel fixtures. Actual owned double-fork writers and
an unrelated sentinel must prove exact tree cleanup and no late output; a held
writer fixture alone is not descendant-quiescence qualification. Verify malformed,
duplicate, late and unsupported control requests without starting an unowned
target. Perfetto fixtures use fake processes and regular marker files, never a
live service, collector, kernel tracing, sudo or host setting.

Actual replay-only runner cases must preserve true/false/native/pending grades,
single evaluation/provider/workload cleanup, independent result publication and
resource-disposition facts. Incomplete producer evidence must withhold matching
completion receipts and later repeated admission without retrying any client.
After source review, run focused tests, the full offline suite, strict Clippy,
formatting and sequential documentation/build gates. Exact-head hosted/package
CI, independent review, merge, both post-merge jobs and a patch release remain
separate delivery obligations; the removed 90% coverage gate stays removed.

## Approved guardian-control seam

The parent independently approved this optional protocol after inspecting the
observed-red evidence; implementation still requires independent source review.
The preferred seam is an optional dedicated nonblocking control socket created
before admission alongside the existing keeper/receipt. Ordinary benchmark
gates retain their existing receipt ABI and cancellation behavior. A collector
requires an explicit bounded capability-ready frame before CONT, so an older
runtime cannot silently ignore a graceful-control request and start anyway.
All control descriptors are CLOEXEC for the target and closed in the wake-only
child; no environment lookup or allocation is added to post-fork callbacks.

A small fixed versioned request may ask only for interruption of the guardian's
already pinned principal: no caller-supplied PID, process group, signal number,
collector ID or executable. The guardian's wait loop polls this socket together
with its owner/cancellation/principal pidfds. It performs identity-pinned SIGINT
without setting the cancellation flag, acknowledges delivery separately and
continues supervision through natural principal exit and existing tree quiescence.
Owner loss and normal cancellation keep their present semantics. A malformed,
duplicate, unsupported or expired control exchange cannot grant successful flush.

Record exact principal exit code/signal as a separate bounded observation when
this control mode is enabled; retain the existing gate-exit/quiescence contract.
An interrupt acknowledgement, nonempty output or principal exit is never itself
proof of a complete collector. Adapter-owned policy decides whether the actual
termination is expected; generic settlement additionally requires confirmed
quiescence, completed capture I/O and successful flush/publication obligations.
On grace expiry, use proof-aware guardian cancellation and wait-only fallback,
not KILL of an admitted guardian. This candidate requires independent review of
framing, partial writes, late replies, descriptor inheritance and all races before
final qualification; no new authorization/signing policy is proposed.

## Partial foundation implementation (not class-35 completion)

The optional control channel uses fixed 16-byte versioned frames, at most three
bounded replies, and a fixed interruption request with no caller PID or signal.
Capability readiness precedes resume. The guardian interrupts only its pinned
principal with SIGINT, separately acknowledges delivery and reports exact
principal exit; its existing quiescence receipt and cancellation remain distinct.
Control descriptors are removed from the target environment and CLOEXEC; the
wake-only child closes its copy. Request writes use an absolute caller deadline;
guardian replies have a fixed 100 ms ceiling per tiny frame. Ordinary gates do
not negotiate or inherit the optional channel.

The first production caller is the process-tree snapshot worker. Adapters supply
observation time/output limits; generic runtime uses guarded admission, bounded
nonblocking capture and checked output writes/flush. Intentional cancellation
discards its in-flight sample, not a partial sample. The caller waits at most six
seconds for worker settlement; a still-running worker retains its existing
captures and guardian, returns explicit uncertainty and is not joined/replaced.
Potentially blocking regular-file I/O cannot honestly be declared quiescent just
because this caller wait expired. Sticky producer errors prevent freezing,
parsing or consuming matching raw capture and clear derived values. Error
messages retain the underlying write/flush reason.

The three initially-red tests now pass. The ENOSPC fixture additionally waits
for the actual write attempt to finish before stopping, so it does not race a
valid intentional cancellation that discards an unfinished sample. The original
red checkpoint and transcript remain unchanged. Actual process control tests
separate acknowledgement from flush/exit, check a nonzero exact principal exit,
and preserve an unrelated sentinel. Codec tests cover fragmentation, duplicates,
owner disconnect, malformed/truncated frames and exhausted request deadlines.

Remaining mandatory work: replace the sleeping snapshot lifecycle placeholder;
migrate ordinary collector child ownership and external kill/pkill/unbounded
wait; bounded Perfetto readiness and proper flush-before-parse; deterministic
WriteZero/read-error/worker-failure and controlled-owner-loss fixtures; all
sibling, independent grade, provider/resource, artifact and receipt integration
cases. These are not covered by the first three green regressions or the gate
tests. No full offline suite or final qualification is authorized at this stage.
Main has independently advanced to `cabaa5766` with bundled recording/replay;
later integration must preserve its cleanup and recording completeness, not
replace those functions wholesale from this older baseline.

Focused foundation evidence (not full qualification): actual ASB build succeeded
before final tests; `measurements::` passed 76/0/0 and `process_gate::` passed
23/0/0. The actual binary remained
`f6ea8797c7e81ec9b921fd4e9243c164bcbddcb9b9ca3d209af1735cecdb2a30`
before/after both groups. Offline locked all-features Clippy for lib/tests passed
with `-D warnings`; formatting and diff whitespace checks passed. Logs are
`target/collector35-foundation-build-second.log`,
`target/collector35-foundation-final-{measurements,control}.log`,
`target/collector35-foundation-final-binary-{before,after}.sha256`, and
`target/collector35-foundation-clippy-second.log`. Earlier compile attempts and
the initial Clippy type-complexity failure are retained separately, not replaced.
Only the exclusive diagnostics cache was used, with incremental compilation off
and two build jobs. No host permission, service, provider, real collector or
live tracing change was made. Independent source review is next; this partial
foundation must not be published as the complete collector migration.

## Independent foundation review follow-up: source-only pending execution

Review of immutable `96032b4a` found that the inner read loop retried Interrupted
without rechecking cancellation/deadline, while its byte counter never advanced.
The follow-up gives each read turn at most 32 attempts as well as 64 KiB and
checks cancellation plus the original command/drain deadline before every read.
Interrupted consumes an attempt; permanent read errors preserve their actual
reason and cannot count as EOF. Intentional cancellation still discards only
the in-flight snapshot, not an ordinary graceful collector's flush output.

New private deterministic source fixtures cover Interrupted fairness,
cancellation during retry, an expired shared budget without another read,
permanent read failure after a valid prefix, actual WriteZero, failed worker
creation before command admission and worker panic before producer launch.
The worker factory is a private typed seam, not an environment/configuration
fault switch. Checked observation writes are extracted unchanged for the
WriteZero fixture; potentially blocking file writes retain the explicitly
bounded caller wait/uncertain worker-custody policy, not an invented hard kernel
I/O deadline. Source is formatted but has not been compiled or executed yet:
publication34 qualification exclusively owned compiler resources while these
changes were prepared. Fresh focused execution, strict lint and independent
follow-up review remain required. No normal collector/Perfetto migration or
full class-35 qualification is claimed.

### Observed focused follow-up at immutable `325b3bd9`

The actual ASB build completed before runtime cases. All **81 measurement** and
**23 process-gate** tests passed, zero failures/zero skips, including the new
deterministic retry/I/O/start/panic fixtures and the original actual local
process/descriptor regressions. The deliberately panicking worker is observed
and reported as a failed producer; its expected panic text is not a failed test.
Strict locked/offline/all-target/all-feature Clippy (`-D warnings`), formatting
and diff whitespace checks passed. The actual executable SHA-256 stayed
`8621cdef57d71c3079c704e75dc0e233e358725f138ac2d9c598d2f928bee41a`
before/after focused tests. Evidence is retained in
`target/collector35-followup-{build,measurements,control,clippy,fmt}.log` and
`target/collector35-followup-binary-{before,after}.sha256`.

The parent independently source-approved this corrected partial foundation.
Only the exclusive diagnostics cache was used (offline, two jobs, incremental
off), with all test handles now terminal. This is not full class qualification:
ordinary producer ownership/graceful concurrent draining, snapshot placeholder
removal and bounded Perfetto readiness still require coherent implementation,
new focused source review and later full/hosted/package/post-merge gates.

## Ordinary producer migration checkpoint (not full qualification)

The preserved WIP `33833221` is followed by an owned process producer using the
typed launch route, optional principal-control guardian, simultaneous bounded
stdout/stderr pumping and adapter-supplied stream/grace limits. Snapshots are
actual producer variants, not sleeping placeholder processes or invented exit
statuses. Session stop broadcasts cancellation to every selected sibling before
joining under one shared absolute budget. Unknown cleanup retains the existing
worker rather than creating another one or releasing its guardian.

Raw files, pipe handles, guardian and optional generic dependency lease live
outside the pump unwind scope. Perfetto supplies its service lease through this
generic interface; no collector ID is inspected centrally. On read/write/panic
failure, the worker requests fixed guardian cancellation and retains these
resources until actual matching quiescence, with bounded backoff. Failure-only
pipe draining prevents guardian diagnostics from hitting prematurely closed
pipes; it never validates a discarded prefix. Parent timeout remains unknown
even after later worker settlement. Incomplete captures are never frozen,
parsed or consumed; independent grades and sibling publication remain intact.

Perfetto startup observes retained readiness bytes with bounded polling instead
of a detached blocking reader. Existing service/tracefs policy is unchanged.
Typed pre-admission executable lookup preserves optional missing-tool behavior;
failed readiness or an exec race after lookup remain real failures. An exec
failure has no fabricated principal exit and retains its bounded diagnostics.

Actual offline focused evidence at this checkpoint: measurements **87/0/0**,
collectors **123/0/5** (the five existing opt-in fixture skips), guardian protocol
**23/0/0**, strict all-target/all-feature Clippy passed. The actual ASB binary was
built first and remained at SHA-256
`647cf4bb06628a8613bd37a4ca28bf6081c087cd4a91e5dbcbeb112ea0d8f8e2`.
Logs use `target/collector35-migration-*`: actual build eighth, measurements
fourth, collectors third, gate second and Clippy first. Earlier failed build,
producer, adapter and protocol transcripts are preserved. New actual cases
include 192 KiB flush per pipe, dev-full/read/panic dependency retention past a
30 ms parent wait, actual grace expiry without fault hooks, invalid admission,
exec removal after precheck and unrelated-process survival. The duplicate-control
fixture now waits under a fixed deadline for actual BrokenPipe after owner drop,
instead of assuming immediate EOF despite concurrent fork/exec descriptor copies.

This is an intermediate source checkpoint: normal deliberate perf SIGINT exit
compatibility still needs the owning adapter's explicit validated-data contract.
Upstream perf can flush and then restore/re-raise SIGINT; simply rejecting all
signal exits would regress normal use. No full suite, PR, push, release or final
class claim is authorized yet. Further preflight/postprocessor subprocesses,
platform qualification and integration with newer main remain separate review
obligations. The file-syscall policy bounds caller settlement and retains an
uncertain worker; it does not invent a hard deadline on kernel file writes.

## Explicit perf deliberate-stop compatibility follow-up

The intermediate ordinary-producer checkpoint is immutable `0fe173e0`. Its
remaining perf compatibility seam is addressed in the next source checkpoint,
not silently treated as already qualified. Linux v6.12 primary source confirms
that both [perf stat's signal exit](https://github.com/torvalds/linux/blob/v6.12/tools/perf/builtin-stat.c)
and [perf record's signal exit](https://github.com/torvalds/linux/blob/v6.12/tools/perf/builtin-record.c)
can restore/re-raise SIGINT after flushing. Thus exit signal 2 or wrapper code
130 is eligible for owning-adapter validation only with an actual requested and
acknowledged principal interruption, matching guardian proof and complete I/O.
The generic runtime carries these separate facts; it neither names perf nor
converts a signal exit into exit zero. Collector JSON explicitly retains
`exit_signal` alongside nullable `exit_code`.

The perf adapter validates the complete bounded stat counter source before
marking it completed. Record data must pass the installed selected perf binary's
built-in offline `script -i <explicit path>` decoder, the same decoding operation
used for flamegraphs, under a 300-second/256-MiB-per-stream owning policy. Its
process uses the generic exact guardian, concurrent capture pumps and confirmed
cleanup. No partial ad-hoc perf.data parser or live sample is substituted. The
decoder receives a cleared environment, disabled debuginfod and disabled global
and system perf configuration; these config controls are documented in upstream
[perf configuration implementation](https://github.com/torvalds/linux/blob/v6.12/tools/perf/util/config.c).
No caller script, sudo or provider is selected. A successful decoder may
legitimately produce no samples: this is explicit zero-sample validity, not an
invented metric. Nonzero, timeout and output overflow retain validation captures
and cannot upgrade record status. An owning-adapter policy retains original raw
capture when its data validator fails; legacy adapters' existing cleanup policy
is unchanged. Sibling outputs and independent official grades still finalize.

Actual synthetic Python processes reproduce requested/acknowledged SIGINT with
complete numeric counters, truncated counters, and spontaneous SIGINT. The exit
remains signal 2 in all three. Private inert project-local decoder fixtures
exercise the actual record-adapter entry point exactly once on valid/failed
decode, plus bounded timeout and stream overflow. They check explicit argv and
cleared/configured environment, raw retention and honest empty output. These
fixtures prove lifecycle/dispatch contracts, not any live kernel perf facility.
A generic publication matrix additionally checks truthful signal serialization,
raw retention for rejected or incomplete evidence, a surviving sibling result,
and unchanged false official grade/run/summary.

Fresh focused evidence: **88 measurement tests, 126 collector tests, 23 guardian
tests, zero failures**; collector group retains the same **five opt-in skips**.
The final owning-perf group passed **10/0/0**; strict locked/offline all-target /
all-feature Clippy (`-D warnings`), fmt and diff whitespace checks passed. Actual
ASB was built before runtime tests and stayed SHA-256
`e4c4072dd221e49c38da1c222f97f451729eae971c2644124546fbcf767e41a1`.
Evidence is `target/collector35-perf-contract-{build-third,focus-third,
measurements-first,collectors-final,gate-first,clippy-first,fmt}.log` and binary
before/after SHA files. The first decoder fixture's expected pathname was not
canonicalized while production correctly supplied the canonical explicit path;
its **9-pass/1-fail** transcript is preserved, then only the fixture expectation
was corrected. All runtime handles are terminal. Independent source review,
current-main semantic integration and full/hosted/post-merge gates remain
required; there is still no full-class, merge or release claim.
