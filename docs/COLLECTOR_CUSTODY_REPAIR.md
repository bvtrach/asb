# Collector producer custody and completeness repair

This is the class-35 design and observed-red checkpoint, based on publication
source `f8139463`. No production repair, passing qualification, PR or release is claimed.
The retained version remains 0.2.32 until its later reviewed stack integration.

## Confirmed current gaps

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

## Candidate guardian-control seam, awaiting further review

Do not implement this protocol merely from the first observed-red approval.
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
production implementation; no new authorization/signing policy is proposed.
