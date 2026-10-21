# Runner phase completion: design and red checkpoint

## Historical red checkpoint dcd2467d

Baseline: independently reviewed and locally qualified setup40 `f3883ca8`.
The historical red checkpoint `dcd2467d` changed only private tests and this
document; it contained no production repair or full qualification and made no
push, PR or release claim. Version 0.2.40 was its historical baseline, not a
reserved delivery slot. The later production implementation and focused evidence
are recorded in the implementation and qualification sections below.

## Source findings and approved design

`runner::run_mode` still owns preparation, execution and result publication.
Existing consuming post-execution phases are useful but do not prevent two
post-grade early exits:

- A stopped-preserved human warning uses `emit_warning(...)?` after evaluation
  and workload finish, before provider finish, artifact handoff and both result
  publishers. Outer provider settlement still runs. Frozen artifact-owner Drop
  retains the raw source; the finding is not loss of those raw bytes.
- Result construction calls `std::path::absolute` on the measurement override
  after evaluation and resource settlement. A failed cwd lookup bypasses log
  and measurement publication, even though an official grade already exists.

The intended repair is a cohesive consuming preparation/runtime/result pipeline,
not extraction of helpers with the existing large parameter lists:

| Boundary | Custody and invariant |
| --- | --- |
| Admitted/configured | Keep arrival/intent/cohort admission order; resolve policy and necessary execution paths before allocating measurement/provider/workload resources. |
| Provider-ready | Own provider session and negotiated endpoint facts; temporary route borrows must not create self-referential ownership or duplicate credential state. |
| Workload-ready | Own workload, measurements, tape and launch admission; complete baseline/material checks before execution. Failed transitions retain custody for explicit cancellation. |
| Executed/settled/evaluated | Retain existing proxy settlement, bounded capture, artifact freezing, grading and workload finish, each once. |
| Finalized result | Finish provider and artifact handoff once; accumulate issues and seal the result without fallible path discovery or escaping presentation errors. |
| Published/presented | Recording commit precedes sealing; attempt log and measurements independently, then retain existing private-outcome, observer and final presentation semantics. |

Early failures must notify waiting material/launch peers before potentially slow
teardown. Preparation cancellation must not authorize destructive normal close.
After execution evidence exists, instrumentation/presentation/cleanup failures
cannot bypass independent grading and publication. ThroughEvaluation lifetime,
principal-exit accounting, typed launch and bounded collector/capture custody
remain unchanged.

Capture the stopped-warning write failure in `AttemptIssues.output_error` before
sealing the result; preserve I/O command classification and primary-error order.
Reporting-only outcomes and observer callbacks retain existing behavior. Such an
outcome is not itself a validated resume receipt; do not invent a new blanket
continuation policy. Late post-summary observer warnings remain presentation-only
and cannot rewrite an already sealed grade.

Early path anchoring must preserve syscall semantics: a relative measurement
override is relative to the invocation cwd, using `std::path::absolute`, not
canonicalization that requires existing destinations. Keep logical spelling
separate from resolved execution paths. Do not silently rewrite cassette/log
wire strings, configuration references or fingerprints to absolute paths.

## Actual offline red evidence

The existing isolated full-entry replay harness now has four independent outer
tests. Private root-scoped workload observation retains the actual evaluator
outcome before finish; it does not run the evaluator again. Native fixture
metadata is declared through the existing private hook before ordinary planning
and runtime, not by editing saved evidence or adding an SDK capability.

Three cases return positive `StoppedPreserved` settlement and fail the writer
only on the specific post-grade warning, leaving startup output unaffected.
They exercise real true/false bundled evaluation and a completed numeric reward
with nullable resolution. The fourth subprocess uses an absolute configuration,
starts in its owned empty `root/empty-cwd`, and selects `../measurements` outside
that directory. Workload finish removes only that empty cwd. The late absolute
conversion then fails through the real OS cwd lookup. No recursive deletion,
host cwd change, live provider, SDK, container or privileged collector is used.

After verifying the exclusive FLAME cache idle and preserving the qualified
setup40 binary, the actual baseline binary was built first with one job and
incremental compilation disabled. The focused command was:

```sh
cargo test --locked --offline -p asb-cli --lib runner_phase_completion_red -- --nocapture
```

It exited **101: zero passed, four failed, zero ignored**, all at the intended
assertion requiring log, measurements and artifact handoff after grading.
Every case already observed exactly `evaluate`, `finish`, `provider-finish`,
and one actual replay client invocation. The captured grades were true, false,
and `accuracy=0.5` with null resolution. Warning cases retained raw trajectory
bytes but had no frozen handoff, log or measurement result. The removed-cwd case
had successful frozen handoff, then `ENOENT`, with neither result publisher run.
Owned fixture directories are intentionally retained and printed in the log.

Actual binary SHA-256 before and after runtime:
`e84df17b77a3ca1a8bd6e3c7c8469b091b5e389181878f85752f69fe2ebe6c1a`.
Logs/hashes are `target/runner-phase-red-build.log`,
`target/runner-phase-red-tests.log`, `target/runner-phase-red-before.sha256`
and `target/runner-phase-red-after.sha256`. The red transcript includes the
original nested subprocess failures, retained fixture paths and resource facts.
Compilation was admitted only above 3 GiB free space. All handles are terminal.

These are intentionally failing regressions, not a passing qualification.
Assertions beyond the first failing publication check are not yet qualified.
Production implementation and its expanded failure/primary-order, repeated/sweep
validation, private-outcome, replay and preservation matrix require independent
review and fresh focused proof. The inherited full-entry subprocess helper uses
standard output collection; this checkpoint does not repair or qualify generic
test-harness cancellation/collection custody. No other original-report class is
claimed complete.

## Implementation checkpoint (coordinated staged 0.2.43)

The immutable red checkpoint is `dcd2467d`; all red logs and retained fixtures
remain available. The production implementation keeps `run_mode` as admission
and pipeline coordination. `runner/attempt.rs` has consuming configured,
provider-owned, workload-ready, completed, finalized-result and publication
states. Its context owns immutable configuration/provenance and the execution
path anchors; workload metadata is produced once before dispatch. Provider
custody and the project network scope surround every transition, including
early-error teardown. There is no new provider/agent dispatch branch or public
wire schema.

Relative measurement overrides are anchored with `std::path::absolute` before
measurement/provider/workload resource allocation, without requiring an existing
destination. Configuration publication has a separate physical anchor. Logical
configuration/cassette spelling and fingerprint/configuration inputs remain
unchanged; recording/reporting still consume the same wire contracts. This does
not claim that earlier shared dependency preflight is mutation-free.

The stopped warning's returned error becomes sticky output failure before
result sealing. Grading, workload finish, provider finish, frozen artifact handoff
and both publishers still run independently. I/O command classification stops
current repeated execution; a higher-priority official false grade stays first,
with the presentation diagnostic retained secondarily rather than discarded.
An output error already serving as the primary failure is not duplicated.

Independent draft review identified two custody hazards in the initial
extraction. Both were corrected before final qualification:

- The finished workload box now remains an explicit publication resource until
  both publishers, private outcome/observer and final presentation have returned.
  A private Drop trace and live-owner assertions cover the observer and writer.
- Prepared-only cancellation is used only before dispatch. After entering a
  workload execution method, failure notifies material peers and retains the
  backend's existing running-owner Drop/cancellation contract; it does not grant
  prepared-only teardown authority to a started execution. A fake backend
  dispatch failure asserts no grading, no normal finish and no prepared cancel.
  The real-child capture/guest/gate matrices separately preserve actual process
  custody; the fake backend is not a new descendant-cleanup qualification.

A channel-driven preparation test requires a waiting material peer to observe
abort before owned workload cancellation can return, preserves Input/Io/Run
primary variants when cancellation also fails, and forbids normal destructive
finish. Actual workload-preparation end-marker failures retain the prepared owner
for explicit cancellation, with provider finish once and no client/evaluator.

Ten phase tests cover 18 isolated actual-entry scenarios and three channel
primary-error variants. The four original red regressions pass, along with
true/false/native combinations of stopped-warning, provider, log and measurement
failure. Numeric rewards retain null resolution. Actual repeated runs stop before
batch two after output I/O. Actual sweeps separately prove the existing policy:
a completed, cleanup-confirmed grade can be validated/sealed and reused on resume
without another client, even though its presentation failed. Reporting-only
outcomes are not themselves resume receipts. No blanket status/failure-kind
rejection was added. Capture, publication or cleanup uncertainty still uses the
existing independent validation requirements.

The continuation fixtures use the existing Pi/Node recipe before normal planning
and runtime-input sealing. The MiniSWE artifact-handoff fixtures remain separate:
their fake Node executable is sufficient for the native artifact contract, not
qualification of an installed MiniSWE Python environment. An initial continuation
fixture incorrectly reused that recipe and could not claim an input seal; it was
corrected before planning, never by changing saved evidence or validation policy.
Initial compile/lint and focused failure logs remain preserved. A false-grade
test expectation was also corrected to include the existing `workload check
failed:` primary prefix; no production grade assertion was weakened.

## Fresh focused qualification, full held

The actual 0.2.43 executable was built first on the exclusive dedicated cache
`/home/martin/projects/asb-build-cache-runner43/target`, offline/locked, one job,
incremental compilation disabled. The final deduplicated matrix passed **745
tests, zero failures and 13 optional skips** across 12 groups: one library OR-filter
union (549 passed/eight skips, including the ten phase tests) and repeated run
(30), benchmark protocol (35), guest agent (14), artifact ownership (2), capture
session (2), completion diagnostics (6), offline CLI (94/five skips), parallel
sweep (2), partial sweep (2), replay sweep (2), and planning admission (7).
The library union also covers recording/publication, generic bounded capture,
collector/measurement lifecycle, workload/controller/lifetime and layout seams.
Earlier overlapping phase runs are not added to these totals. Strict workspace/
all-target/all-feature Clippy and formatting passed on this same source.

Actual executable SHA-256 stayed
`b318ba434f7b8d679a2c71f534fa8e1ebe2b3e224e744ce87962398f218697a3`
before/after the final library and integration tests. Logs are
`target/runner43-{build-final,clippy-final,fmt-final,library-preservation}.log`
and the eleven named integration logs. Hash records are
`target/runner43-before-preservation.sha256`,
`target/runner43-after-library-preservation.sha256` and
`target/runner43-after-integration-focus.sha256`. A frozen actual copy is retained
at `target/qualification/runner43-b318ba43-asb`; the red actual is separately
preserved at `target/qualification/runner43-red-dcd2467d-asb`. Compilation admission
checked at least 3 GiB available on the assigned cache filesystem. No source
changes occurred during runtime; all handles are terminal.

This is focused Linux/offline evidence, not full-stack qualification. The separate
optional process-termination diagnostics class is not merged or overwritten here.
Normal returned-error paths are repaired; arbitrary panics/OOM and a blocking
borrowed `Write` are not contained or hard-cancelled by this pipeline. Existing
bounded adapter Drop/reaper, platform and ThroughEvaluation contracts remain;
no new Windows runtime, live SDK/provider, crash-persistent cleanup or whole-audit
claim is made. Independent source/evidence review, coordinated current-stack
integration, fresh full qualification, hosted/package CI, merge, post-merge jobs
and release remain pending. The removed coverage gate stays removed. No push or
PR accompanies this source checkpoint.
