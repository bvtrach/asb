# Task success, smoke exercises, steps and tokens

## What was being measured

The legacy external workload aliases materialized a tiny ASB adapter trial
beside downloaded upstream references. Historically, for `swe-bench`, this was a Python
string-normalization task under `external-workload/asb-trial`, not a real
SWE-bench issue. Four-digit token counts and a few agent iterations can be
plausible for that exercise; they do not demonstrate that an upstream issue or
the complete benchmark was solved. A run is one task attempt, never a whole
suite. Concurrency creates repeated attempts, not additional unique issues.
The development integration now gives the three original SWE-bench variants
real task catalogs and official evaluation, and adds original Terminal-Bench
1.0 and 2.1 task execution. These paths must not be confused with the legacy smoke
records or with completed full-suite support; see the integration status below.

Before schema version 2, these exercises could produce `success_rate = 1.0`.
That number was an adapter smoke pass, not a valid upstream benchmark score.
The original evidence remains unchanged. Regenerating with `asb compare
SWEEP.toml` now excludes those historical scores and untrustworthy old step
counts from graphs and tables. Historical records with unknown grading
provenance are unscored too. Do not compare old raw success numbers directly
with schema-2 task-resolution results.

## Definition and primary sources

SWE-bench's official [harness grading implementation](https://github.com/SWE-bench/SWE-bench/blob/main/swebench/harness/grading.py)
requires full resolution: fail-to-pass tests pass and pass-to-pass tests are
preserved. Partial fixes are not fully resolved. Its evaluator distinguishes
patch application, missing test output, and infrastructure faults; an agent's
exit status or claim of success is not the score. Use the official
[evaluation harness](https://www.swebench.com/SWE-bench/guides/evaluation/) for
the specific dataset and split when producing a SWE-bench score.

The [tau-bench paper](https://arxiv.org/abs/2406.12045) independently evaluates
the final database against the task's annotated target state. Its repeated
trial reliability measure `pass^k` is not the same as pass@k or a single-run
success fraction. This supports the common principle: evaluate the resulting
task state using the workload's own evaluator, not an LLM's self-assessment,
token budget, or successful termination.

ASB's task-attempt resolution fraction is:

```text
resolved eligible task attempts / all selected eligible task attempts
```

Only an evaluator-confirmed complete pass earns numerator credit. A failed
agent process does not discard a usable submission: ASB still evaluates the
resulting task state. Process, proxy or owned-resource cleanup failures remain operational failures,
but a separately confirmed task resolution still earns task credit. Grading
failures and absent evaluation do not earn credit; known
eligible attempts remain in the comparison denominator even when measurements
are missing. Summary files cover recorded attempts; comparison groups cover
the selected sweep configurations and replicas. Counts are grouped by agent,
provider, model, workload and evaluation scope in `comparison.json`.
This is not pass@k, pass^k, an estimate for an entire dataset, or an average
over only successfully completed processes.

An empty eligible denominator gives `null`, never 0% or 100%. Smoke exercises
and unknown grading provenance contribute only to `unscored_count`.
ASB also checks the initial task state. A workspace that already passes (or
whose baseline cannot be verified) cannot earn new task-resolution credit:
it is `invalid-baseline`, with status `unscored` when execution otherwise
completes. Existing files are never reset or deleted. Choose a fresh workspace
for an independent attempt. This matters for reruns: preparation intentionally
preserves previous edits, and historical logs can include agents finding an
already-fixed task. Old files without baseline evidence cannot establish that
a fix was made during that execution.
New records explicitly identify baseline provenance in `workload_baseline`:
`measured` retains a measured pass/fail/unknown result, while
`upstream-task-definition` identifies an official clean base commit without
claiming ASB observed failing tests. The latter is eligible only for an
`upstream-harness` evaluator, not local fixtures.
`upstream-environment-definition` identifies a pinned original task source
revision and definition hash for environment-based tasks such as Terminal-Bench;
it likewise does not claim a measured test baseline or invent a Git base.
Unknown/malformed evidence
cannot earn credit. Legacy records without this field retain their existing
`workload_baseline_passed` interpretation; malformed explicit evidence cannot
fall back to a legacy flag. The official session must validate dataset and
repository identity before publishing this evidence.
`process_success` is independently reported. `workload_success` is the actual
task evaluator result where available. `adapter_smoke_success` is separate;
a passing smoke has status `smoke-passed`, measured `success = false`, and
no task-resolution rate. Its command can exit zero to indicate that the smoke
operation itself completed, not that an upstream benchmark was resolved.
If process execution and evaluation both fail, `failure_kind`/`failure_reason`
retain the operational failure and `workload_failure` independently retains
the evaluator diagnostic. Measurements and comparison use the evaluator's
outcome for task-resolution rates rather than requiring a successful process.

Bundled fixtures use bounded evaluator-owned accepted source expectations;
repository navigation checks the final answer's required references. These
are ASB-local tasks, not upstream benchmark equivalents. External structural
smoke grading is deliberately conservative: arbitrary equivalent code may be
unrecognized, and agent-controlled tests are never executed as trusted grading.

## Upstream integration status

### Selection snapshots and population coverage

A sweep saves requested task/dataset selections in `plan.json` before any
environment or provider starts. Public catalog metadata is read once per
installed workload, with no prompts, credentials or hidden evaluator rows in
the plan. No execution attempt or measured baseline is inferred from selection.
Configuration changes, mismatched task/dataset/grader records and duplicate
actual attempt IDs cannot be silently combined into a new score.

`comparison.json` separates `evaluation_coverage` from `resolution_groups`.
Known upstream selections stay in the selected denominator even if their
environment never prepared or grading failed. `population_coverage` adds
unique selected/evaluated/resolved task counts and the installed population
size, separately from repeated attempt counts. Its
`selected_resolution_lower_bound` is resolved attempts divided by selected
attempts; it is not presented as a complete-population score when evaluations
are missing. `full_population_resolution_rate` exists only when the entire
pinned population was selected, every requested attempt received a compatible
evaluation, and exactly one attempt was selected per unique task. Repeated
attempts are not silently converted into pass@1 or pass@k.

HTML reports show evaluation coverage and execution mode. Replay grading can
be valid task evidence, but a replay run is not a live-provider timing result.
Legacy reports without a pre-run selection snapshot cannot establish complete
dataset coverage. Fresh sweep attempts preserve old results/reports rather
than mixing old workspace edits or measurement files into a new population.

The development SWE-bench adapters select real pinned dataset instances,
prepare their base repositories in owned containers, capture candidate patches,
and invoke the official SDK evaluator. Original Terminal-Bench 1.0 instead
retains its in-place task environment and delegates test injection, execution
and result parsing to its original SDK. Terminal-Bench 2.1 delegates environment
construction and verifier execution/reward parsing to Harbor. Its binary
reward is checked for a complete task outcome; stale pre-verifier reward files
are removed and missing/invalid rewards remain evaluation errors.
These are adapter-owned sessions; the runner does not select a benchmark-specific
grader. Missing/stale reports,
partial fixes, regressions and missing patches cannot earn resolution credit.
An empty parsed Terminal-Bench test map cannot count as success.

Original source builds publish per-attempt image ownership before building.
Normal cleanup removes only the recorded owned tags, not upstream images or
shared build caches. Recovery instead retains build images and reports their
immutable identity alongside stopped task containers; preservation is not a
grade or retry authorization. Retained-outcome reconciliation requires matching
image-cleanup evidence for those build outputs.

SWE-bench still uses the official grader and final report formatter. ASB omits
the formatter's optional daemon-wide image/container inventory: unrelated image
removal can race that inventory after grading and prevent summary publication.
Task outcomes, test classifications and denominators are unchanged. Per-attempt
ASB journals retain actual ownership/cleanup evidence without scanning unrelated
daemon resources.

Representative correct/incorrect fixture submissions have qualified these
normal CLI paths without live LLM providers. This does not mean the full
populations have been benchmarked. Representative endpoint forwarding,
container measurement, real single-task sweep/replay and selection/coverage
paths are qualified. Bounded distinct-configuration scheduling (separate from
replicas), mixed original-task overlap and SWE original source-image fallback
are also qualified. Scheduling choices are recorded because simultaneous
configurations share host resources; an active-attempt count budget is not a
host memory reservation. Content-bound resume, scalable population presentation
and abrupt-crash recovery remain unfinished. See the exact requirements and evidence in
[the full integration ledger](FULL_BENCHMARK_INTEGRATION.md).
No old smoke result is retroactively interpreted as an upstream grade.

Fresh-project installation of all five complete catalogs and three SDKs is
qualified separately from task execution. A subsequent representative normal
ASB sweep has also passed using that newly installed project and Pi. These
checks use a synthetic provider for known repairs, reuse host prerequisites
and existing Docker images, and are not a full-population agent benchmark or
evidence about a completely cold Docker/OS installation. See the integration
ledger for the exact retained artifacts and qualification scope.

### Reporting without optional measurements

Task evaluation does not require runtime measurement collection. Comparison
loads the actual per-replica `.asb-completions` summaries when `run.json` is
absent, preserves task/dataset/attempt identity, grades, process/evaluator
failures, token/step counters and observed execution time, and marks the row
`result_source: "task-outcome"`. Missing outcomes remain missing; temporary
unpublished records earn no credit. No host counters, collector samples or
component timings are invented. If both measurement and outcome records exist,
their common result fields must agree before they can be reported together.

An independently confirmed grade remains visible even if the agent exited
unsuccessfully or owned-resource cleanup was not confirmed; those operational
failures are disclosed separately. This is reporting of local recorded results,
not verification of resume eligibility or historical attestation. Unknown
grading scope, unsupported baseline evidence and adapter-smoke outcomes remain
unscored. Legacy multi-replica outcomes require explicit slots or a unique match
to the original measured attempt; filenames/order never decide their slots.

## Dispatch context and resumed measurements

New run summaries, logs and measurement records include `replica_index` and
`execution_cohort`: an opaque dispatch ID, the selected replica count and the
ordered slots admitted in that invocation. A partial resume preserves cached
records and gives newly executed slots a new cohort containing only those
slots. Selected concurrency is not evidence that cached and fresh measurements
were collected under the same load. Even membership in one dispatch cohort
does not prove exact temporal overlap; preparation can stagger execution.

Comparison JSON preserves this provenance. HTML labels distinct dispatch
groups and their admitted slots; legacy records remain explicitly unknown.
Dispatch identities and slot counts are metadata, not numeric performance
metrics. Malformed or contradictory declarations are rejected. Resume receipts
bind the recorded context to measurement outputs and the pre-start intent;
changing that history invalidates reuse rather than relabeling old data.

### Explicit reconciliation of already-graded outcomes

If a controller owner stopped after grading and cleanup but before publishing
completion receipts, use `asb sweep SWEEP.toml --resume --reconcile-completed`.
This is opt-in: ordinary resume still refuses unsealed outcomes. The original
input identity, selected task, replica start intent and workload allocation
must match. Only schema-2 outcomes with boolean grades and confirmed cleanup
qualify. Owning adapters check the retained task, submission and official
grader artifacts read-only; no agent/provider, container or evaluator runs.
All candidates are checked before any missing receipt is published, and
existing receipts are never replaced. Missing or inconsistent artifacts fail
without converting a failed/ungraded attempt into success.

Repaired receipts record current local artifact consistency, **not historical
attestation** that those previously unsealed files were never changed. Reused
JSONL summaries and comparison reports disclose `reconciled: true`; its marker
is bound into the new receipt. False task grades and failed process exits stay
false/failed. This operation does not recover genuinely unfinished evaluation
or authorize rerunning already-graded failures.

### Grade-only recovery when execution summaries are missing

`asb sweep SWEEP.toml --import-grades` can import an already completed official
grade from its retained artifacts, bound to the original saved selection,
configuration, replica start and workload allocation. It never starts an agent,
provider, container or evaluator, and cannot grade an unfinished submission.
All candidates are checked before publication. Existing outcomes/measurements
are not replaced, and repeated identical imports are unchanged.

Run `asb compare SWEEP.toml` afterwards. Imported rows identify their source as
`imported-official-grade`. Their validated true/false outcomes contribute to
task-resolution/coverage, but process success, timing, tokens, steps and
execution cohort remain unknown. The original slot is reporting identity, not
proof of simultaneous execution. Compare rechecks retained grader evidence and
exact inspected artifact bytes; changed bindings/grades or a conflicting normal
outcome/measurement fail instead of being silently merged. This is current
local consistency, not historical attestation or a completion/resume receipt.
It does not authorize replaying the agent or retrying an unresolved task.

## Steps and tokens

The old step collector searched JSON keys/text for words such as `step`,
`command` and `action`, counted streaming lifecycle events more than once,
and invented one step when plain output existed. Those counts were not a
reliable action count. Schema 2 delegates extraction to agent adapters.
Qwen Code uses its final `num_turns`; Cline uses final `run_result.iterations`.
Claude Code and Workbuddy use their final native `num_turns` counters.
mini-SWE uses its trajectory's `info.model_stats.api_calls`, the counter used
for its step limit; attempted failed model calls count too. Pi counts terminal
main-agent `turn_end` events across complete, balanced agent cycles, including
failed/retried turns; duplicated messages in `agent_end` add no steps.
Compaction and other auxiliary model calls are outside Pi's main-agent turn count.
Intermediate snapshots are not summed. Unsupported, incomplete or malformed
output gives `null` (human output: `unknown`), not an invented zero or one.
Goose and Continue currently lack a verified complete-run step counter here
and therefore report unknown. Agent-reported turns/iterations remain
client-defined units, not uniformly comparable reasoning or tool-call counts.

Tokens remain provider/agent reported, never estimated from text length or
raised to an expected minimum. Complete recorded cassette usage takes
precedence over client summaries; per-turn response usage is summed once,
not cumulative snapshots multiple times. The `token_usage.source` identifies
the evidence. Missing usage stays unknown. A small genuine total neither
proves task success nor implies accounting failure; inspect evaluator scope
and task outcome independently. Offline synthetic usage tests cover all
registered agents and do not require provider credentials.
