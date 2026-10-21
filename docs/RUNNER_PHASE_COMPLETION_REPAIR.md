# Runner phase completion: design and red checkpoint

Baseline: independently reviewed and locally qualified setup40 `f3883ca8`.
This checkpoint changes only private tests and this document. Version 0.2.40
remains the historical baseline, not a reserved delivery slot. No production
repair, full qualification, push, PR or release is claimed.

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
