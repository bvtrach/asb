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

## Ownership and extension points

| Concern | Existing implementation | Module boundary and required extension |
| --- | --- | --- |
| Discovery and setup | Workload registry, generic CLI/TUI configuration | Owning adapters publish versioned descriptors; setup consumes capabilities rather than a family-name switch. |
| Dataset and tasks | Pinned catalogs and saved task selections | The module owns provenance, population, splits, task interpretation and structured inputs. Saved selections bind the descriptor. |
| Environment and harness | Native sessions, local Docker controllers, SWE SDK, original Terminal SDK and Harbor | The module chooses a qualified protocol; neutral harness helpers implement reusable machinery. Harbor is not a family identity. |
| Execution planning | Generic run/sweep and replica scheduling | ASB negotiates backend, input support, policies, phase caps, repetitions, resources and collector scopes before admission. |
| Evaluation | Binary family evaluators and retained grading | Modules return structured completion, native rewards, optional resolution and evidence; execution and cleanup remain separate facts. |
| Measurement | Collector registry, lifecycle phases and verified target attachment | ASB selects and operates collectors against supported targets. A capability declaration is not a successful observation. |
| Results and presentation | Run artifacts, completion receipts, compare JSON/HTML | ASB preserves recorded identities and metric policies, reports coverage, and separates incompatible experiments without consulting today's registry. |

A new benchmark family should therefore register an owning adapter and its
qualified dataset/protocol, not add special cases to the generic runner,
installer, measurement UI or report renderer. Reuse a harness only when its
environment, submission and evaluator contracts actually match. New backend,
modality, metric algorithm or artifact transport capabilities need their own
generic implementation and qualification before a module can require them.

## Full-family support versus architectural readiness

These contracts do not establish support for every suite sharing a family name.
The qualified official profiles remain SWE-bench Full, Lite and Verified,
original Terminal-Bench and Terminal-Bench 2.1. Other registered reference
profiles, including SWE-bench Pro, remain smoke/reference profiles unless their
own descriptor and runtime qualification say otherwise.

Additional SWE/Terminal variants need pinned sources and licenses, population
and split validation, environment/platform requirements, agent input and
submission support, official evaluator parity, native metric policies, artifact
integrity, interruption/recovery tests, and known-correct/incorrect submission
qualification. Multimodal payload transport, additional execution backends,
custom official metric algorithms, automatic repetition/seed planning and a new
asynchronous regrading API are extensions, not consequences of adding a suite
name. ASB remains the orchestrator for all of them.

Implementation details and current limits are documented in
[harness extraction](HARNESS_EXTRACTION_STEP2.md),
[outcomes and metrics](BENCHMARK_OUTCOMES.md), and
[planning and measurements](BENCHMARK_PLANNING.md).

Use provider-free Rust and Python contract tests, public setup/run/sweep/compare
fixtures, packaging tests and hosted CI. Real SDK/container qualification uses
the documented opt-in fixtures and known submissions, with no live model calls.
Preserve owned-resource recovery, content bindings and false grades throughout.
