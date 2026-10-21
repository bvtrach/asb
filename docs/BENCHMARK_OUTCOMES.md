# Benchmark outcomes and metric reporting

`WorkloadSession::evaluate_outcome` supplies a versioned evaluation contract.
Its default wraps the current binary evaluators, preserving false grades and
their diagnostics. Modules can return completed named numeric rewards with
nullable resolution, pending evaluation, or evaluator error. Numeric rewards
are never converted to a Boolean resolution. `metric_policy` declares the
policy identity/version, primary named reward and attempt-mean or task-mean
aggregation. Other named rewards remain available in generic metric charts.
Completed evaluations must contain the primary requested reward; empty or
secondary-only rewards cannot earn completed-outcome receipts. Pending/error
states remain unevaluated. `missing_rewards` is explicit policy: `zero` adds
zero to aggregation for missing selected rewards, while `exclude` computes an
observed-only estimate. Binary defaults to `zero`, retaining its historical
selected-attempt denominator. Zero imputation never changes evaluator status
or invents a false grade, and coverage remains incomplete. Reports expose both
rewarded and contributing attempt counts and label observed-only estimates.
Each reward declares a unit and optional finite inclusive range. Names and
units use bounded portable identifiers; undeclared, nonfinite and out-of-range
rewards become evaluation errors. Native scoring requires evaluator evidence.
The descriptor's scoring protocol binds custom policies as
`metric:<policy-id>@<version>`. `task-resolution` retains the original binary
default; `unscored` profiles cannot acquire benchmark score credit.

The runner captures `module` from the adapter at execution time and writes it
into the run log, JSONL summary, measurements and completion summary. Reports
use only recorded provenance or a saved selection, never live registry lookup.
`RunOptions::execution_plan_fingerprint` carries an optional opaque resolved
planning-policy identity into the same records and all score/coverage grouping
keys. Different known identities and unknown legacy identities are separate
comparison groups. Step 4 must supply this fingerprint from its immutable plan,
covering backend, effective budgets, resource/network policy and overrides;
reports never derive it from current configuration or runtime defaults.
Missing selected slots remain in compatible observed cohorts' coverage,
preferring the same configuration's observed replicas. When a missing slot has
no unique cohort, coverage is conservative in each compatible cohort; this is
not evidence that execution happened under any of them. Actual legacy records
retain their unknown fingerprint and are never relabeled. Structured rewards
require an attributed execution task, and completion receipts bind the selected
metric policy even if a result omits its module field.
The runner writes `benchmark_outcome` into the run log, JSONL summary,
measurement record and private completion summary. It carries independent
execution success, evaluation, cleanup confirmation, evaluator evidence paths,
policy, execution-attempt identity, evaluation identity and optional superseding
evaluation reference. Execution errors do not erase confirmed evaluation.
Legacy fields remain readable; a record without the new object keeps its
historical binary semantics. Explicit malformed objects fail closed.
Grade-only recovery does not invent execution, timing or token observations.
Both existing import and retained-slot closure paths expose
`legacy_evaluation_provenance`: the original execution attempt, retained source
scope and digest of its validated evidence manifest. Old sources have no
evaluation ID or supersession ID, so those stay null. Inspection regenerates
no IDs and does not create a new structured evaluation from today's policy.
Module provenance may come from the original saved selection, never the live
registry. Missing execution-plan fingerprints remain unknown.
Completed native reward outcomes can be sealed and reused without inventing
resolution; pending/error outcomes cannot earn a completed-outcome receipt.
The private checkpoint constructor parses the structured outcome once and
checks its selected module/metric policy, completed evaluation, execution
lineage, summary process/resolution fields and cleanup/evaluator-root binding
before any group, replica or reconciliation consumer receives it. Legacy
records without that object keep their Boolean-grade requirements. Measurement
records must also agree on module, structured outcome and execution-plan
fingerprint; the checkpoint does not derive a missing planning fingerprint.

Comparison preserves the structured outcome in every available report row.
Metric groups bind agent/provider/model, workload, dataset, module descriptor,
evaluation scope and versioned policy. Missing attempts remain in observed
policy coverage. The JSON and HTML reports show completed reward coverage,
selected repetitions by task, policy-defined means and task-cluster standard error. An
incomplete estimate is explicitly not an official population score. Uncertainty
requires at least two independently treated tasks; attempt means with unequal
observed cluster sizes do not receive a guessed standard error. This is a
descriptive standard error, not a calibrated confidence interval or pass@k.
Non-binary policies do not enter legacy binary-resolution score tables.

## Remaining work beyond this implementation

- Repetition counts describe selected attempts, not a new scheduler repetition
  plan or seed policy. Step 4 owns planning.
- Step 4 must populate and validate the execution-plan fingerprint against saved
  plans during execution and resume. This change carries/groups it, but does
  not invent a policy hash or claim legacy planning evidence.
- Only attempt means and task means are implemented. Custom upstream metric
  scripts, weighted policies, pass@k, bootstrap intervals and official exports
  require separate validated policy implementations.
- New evaluation lineage is recorded and checked against the execution attempt;
  existing retained-grade paths explicitly preserve legacy provenance and
  unknown evaluation identity rather than minting replacement IDs.
  There is no new regrading/async worker command or lineage-graph reconciliation
  API. Existing official-grade recovery retains its legacy path.
- Current family adapters continue their binary evaluator protocols. Their
  harness transport must explicitly supply non-binary rewards before any
  upstream suite can claim runtime support for them.
- Historical records without recorded module provenance or a saved selection
  retain unknown module identity. They cannot retroactively acquire a profile.
- The legacy measurements `success` flag remains a binary-resolution field;
  consumers of native rewards must read `benchmark_outcome` and metric policy.
