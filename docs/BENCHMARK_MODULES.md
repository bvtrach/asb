# Benchmark module architecture

ASB owns setup, planning, admission, agent/provider selection, measurement,
artifact collection and presentation. A benchmark module owns its dataset,
task interpretation, submission contract, official evaluation and score policy.
Harnesses implement reusable environment/verifier machinery; a harness is not a
benchmark identity. In particular, Harbor may serve more than one family.

## Delivery and acceptance

1. Contracts and descriptors: owning adapters describe family, pinned dataset,
   harness, execution/input/submission/evaluation contracts and support level.
   Generic discovery exposes them and saved selections bind them. Historical
   records without descriptors remain readable and cannot silently acquire new
   protocol assurances. Smoke references never advertise official evaluation.
2. Harness extraction: shared transport, Docker/network and artifact operations
   have neutral ownership. Harbor does not import infrastructure from the
   original Terminal-Bench harness. Original SDK pins and historical artifacts
   retain their meanings. Reusable harness machinery does not imply that an
   additional dataset has been qualified.
3. Outcomes and presentation: preserve execution, evaluator completion, native
   rewards, resolution, cleanup and evidence separately. Metric policies define
   aggregation and repetitions; unknown grades remain unknown. Compare reports
   coverage, identities, metric definitions and original artifacts independently
   of optional measurements. Regrading creates evaluation lineage rather than
   overwriting execution observations.
4. Planning and measurements: generic plans negotiate task inputs, backend and
   policy requirements, phase budgets, resources and collector target scopes.
   Reject unsupported requirements before admission. Keep selected tasks,
   repetitions and concurrency distinct. Report unavailable collection honestly;
   never attach local collectors to an unverified remote PID. Persist resolved
   choices so continuation and comparison cannot silently change experiments.

Each step is reviewed and tested in a separate PR, merged only with successful
checks. A patch release follows all four steps. New benchmark populations are
separate work; these changes must preserve the existing five official adapters.

## Compatibility and validation

`asb workload describe WORKLOAD` prints a registered module's JSON descriptor
without an initialized project, SDK installation, environment or provider call.
The same descriptor enters new task selections in `plan.json`. New continuation
requires the saved descriptor to match; historical selections without one remain
readable but cannot be resumed under a newly declared protocol automatically.

Contract versions describe ASB's wire/storage schema. Dataset revisions,
harness identities and protocol revisions independently describe the experiment.
Changing verifier semantics requires a new evaluation identity. Changing the
agent environment or inputs requires a new execution. Neither a successful
process nor successful installation establishes a task grade.

Use provider-free Rust and Python contract tests, public setup/run/sweep/compare
fixtures, packaging tests and hosted CI. Real SDK/container qualification uses
the documented opt-in fixtures and known submissions, with no live model calls.
Preserve owned-resource recovery, content bindings and false grades throughout.
