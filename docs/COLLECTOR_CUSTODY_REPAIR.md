# Collector producer custody and completeness repair

This is the class-35 design and red-test checkpoint, based on publication source
`f8139463`. No production repair, runtime qualification, PR or release is claimed.
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
and only fixture-owned process/FD/thread handles are used. These tests have not
yet been built or executed: build capacity is intentionally reserved for the
current publication qualification. Do not call an unexecuted test a reproduced
runtime failure.

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
