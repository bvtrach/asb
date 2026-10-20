# Task success, smoke exercises, steps and tokens

## What was being measured

The external workload aliases currently materialize a tiny ASB adapter trial
beside downloaded upstream references. For `swe-bench`, this is a Python
string-normalization task under `external-workload/asb-trial`, not a real
SWE-bench issue. Four-digit token counts and a few agent iterations can be
plausible for that exercise; they do not demonstrate that an upstream issue or
the complete benchmark was solved. A run is one task attempt, never a whole
suite. Concurrency creates repeated attempts, not additional unique issues.

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
resulting task state. Process or proxy failures remain operational failures,
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
`upstream-harness` evaluator, not local fixtures. Unknown/malformed evidence
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

## Missing upstream execution

No current external adapter executes its upstream suite's official evaluator.
ASB cannot truthfully report an official SWE-bench resolution rate from the
existing smoke results. Actual support still requires selecting dataset
instance IDs and split, materializing the correct repository/base commit and
issue instructions, capturing the generated patch, running the official
isolated harness with its test patch and resource limits, and retaining
instance-bound evaluation evidence. Missing or stale reports, partial fixes,
test regressions and missing patches must not pass. Those suite-specific
operations belong in the owning workload adapter, not branches in run/sweep.
This repair corrects scoring; it does not claim to have implemented that
missing full-suite executor or retroactively evaluated previous runs.

## Steps and tokens

The old step collector searched JSON keys/text for words such as `step`,
`command` and `action`, counted streaming lifecycle events more than once,
and invented one step when plain output existed. Those counts were not a
reliable action count. Schema 2 delegates extraction to agent adapters.
Qwen Code uses its final `num_turns`; Cline uses final `run_result.iterations`.
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
