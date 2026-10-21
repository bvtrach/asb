# Benchmark outcomes and metric reporting

`WorkloadSession::evaluate_outcome` supplies a versioned evaluation contract.
Its default wraps the current binary evaluators, preserving false grades and
their diagnostics. Modules can return completed named numeric rewards with
nullable resolution, pending evaluation, or evaluator error. Numeric rewards
are never converted to a Boolean resolution. `metric_policy` declares the
policy identity/version, primary named reward and attempt-mean or task-mean
aggregation. Other named rewards remain available in generic metric charts.

The runner captures `module` from the adapter at execution time and writes it
into the run log, JSONL summary, measurements and completion summary. Reports
use only recorded provenance or a saved selection, never live registry lookup.
The runner writes `benchmark_outcome` into the run log, JSONL summary,
measurement record and private completion summary. It carries independent
execution success, evaluation, cleanup confirmation, evaluator evidence paths,
policy, execution-attempt identity, evaluation identity and optional superseding
evaluation reference. Execution errors do not erase confirmed evaluation.
Legacy fields remain readable; a record without the new object keeps its
historical binary semantics. Explicit malformed objects fail closed.
Grade-only recovery does not invent execution, timing or token observations.
Completed native reward outcomes can be sealed and reused without inventing
resolution; pending/error outcomes cannot earn a completed-outcome receipt.

Comparison preserves the structured outcome in every available report row.
Metric groups bind agent/provider/model, workload, dataset, module descriptor,
evaluation scope and versioned policy. Missing attempts remain in observed
policy coverage. The JSON and HTML reports show completed reward coverage,
observed repetitions by task, means and task-cluster standard error. An
incomplete estimate is explicitly not an official population score. Uncertainty
requires at least two independently treated tasks; attempt means with unequal
observed cluster sizes do not receive a guessed standard error. This is a
descriptive standard error, not a calibrated confidence interval or pass@k.
Non-binary policies do not enter legacy binary-resolution score tables.

## Remaining work beyond this implementation

- Repetition counts describe observed attempts, not a new scheduler repetition
  plan or seed policy. Step 4 owns planning.
- Only attempt means and task means are implemented. Custom upstream metric
  scripts, weighted policies, pass@k, bootstrap intervals and official exports
  require separate validated policy implementations.
- Evaluation lineage is recorded and checked against the execution attempt.
  There is no new regrading/async worker command or lineage-graph reconciliation
  API. Existing official-grade recovery retains its legacy path.
- Current family adapters continue their binary evaluator protocols. Their
  harness transport must explicitly supply non-binary rewards before any
  upstream suite can claim runtime support for them.
- Historical records without recorded module provenance or a saved selection
  retain unknown module identity. They cannot retroactively acquire a profile.
- The legacy measurements `success` flag remains a binary-resolution field;
  consumers of native rewards must read `benchmark_outcome` and metric policy.
