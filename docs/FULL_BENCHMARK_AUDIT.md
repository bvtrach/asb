# Original five-benchmark delivery audit

This is a branch implementation audit, not release approval. The target remains
all five original benchmarks through setup/run/sweep, PR merge, verified main CI,
and minor release 0.2.0. The complete checklist in
[FULL_BENCHMARK_INTEGRATION.md](FULL_BENCHMARK_INTEGRATION.md) remains authoritative.
Existing runtime qualifications use known submissions/synthetic providers, not
claims that an AI solved a complete population.

| Requirement | Current evidence | Remaining delivery verification |
| --- | --- | --- |
| Workload-owned catalogs/lifecycle | `WorkloadAdapter` installed/planning/preparation hooks; `WorkloadSession`; owning SWE/Terminal adapters | Final full-diff boundary review |
| Exact populations | Pinned catalog installers; installed Full/Lite/Verified 2294/300/500, original Terminal 1.0/2.1 80/89 | Final population-scale planning/reporting qualification |
| Functioning local SDK/tool installs | Separate `tools/swebench.rs`, `terminal_bench.rs`, `harbor.rs`; fresh-project public install qualification | Verify final release packaging/install |
| Task/submission/evaluator identity | Dataset/task/attempt records, baseline provenance, captured patch/environment and SDK/image identity | Final cross-path contract review |
| Original SWE execution/evaluation | Shared family engine, private evaluator inputs, clean isolated task, official harness; real correct/incorrect SDK qualifications | Final release regression set |
| Original Terminal 1.0 execution/evaluation | Original task definitions, SDK Compose/tmux/parser; known grades and service-bearing Jupyter qualification | Final selected-agent integration regression |
| Original Terminal 2.1 execution/evaluation | Original Harbor tasks, environment/verifier/reward parser; correct/incorrect SDK qualifications | Final selected-agent integration regression |
| Selected ASB agent/provider | Generic guest driver and existing agent adapters; shared bootstrap/endpoint/relay; real Pi with synthetic provider | Final supported bootstrap/dependency review |
| Measurement/tape targeting | Generic execution target and lifecycle boundaries; isolated collectors, scoped relay/tape; mixed original-task SVG/HTML qualification | Final measured/disabled-collection regressions |
| Population selection and scheduling | Installed IDs, planning catalogs, bounded full manifests; jobs distinct from replicas; weighted admission | Final full-population format/report qualification |
| Accurate resolution semantics | Official task grade separate from process exit; explicit selected denominators, missing/infra outcomes and unscored legacy smoke | Final population reporting review |
| Freshness and completed continuation | Input snapshots, per-slot intent/allocation, completion evidence, explicit reconciliation, grade imports | Started unsealed slots still need a safe continuation disposition |
| Failure/cancellation resource safety | Owned journals, scoped stop/preserve, inactive-owner recovery, no host-wide prune; actual interrupted fixtures | Final failure/cleanup regression audit |
| Existing result recovery | SWE official-report reconstruction and Terminal SDK-return observations; imports/compare bind source bytes | Unknown unobserved grades must remain explicit, never guessed |
| Provider-free tests | Unit/offline CLI, protocol, guest lifecycle, parallel/partial/replay/unmeasured suites | Exact final source/package/hosted runs |
| Delivery | Draft PR #227; signed incremental commits and hosted CI | Full review, merge, exact-main CI, minor release |

The next implementation target is started-slot continuation. Existing grade
imports are reporting-only and must not be treated as completion receipts or as
permission to repeat a client. Lost execution metrics cannot be invented to make
a recovered grade look like an ordinary completed run. Full-population
qualification must exercise actual catalogs/selection/report semantics, not
rename a small smoke exercise as the original benchmark.

No amount of metadata recovery can infer an unobserved verifier result. Safe
handling means preserving the attempt, reporting unknown/missing evidence and
requiring an explicit new-attempt disposition where necessary, not silently
rerunning an unresolved task until it passes.
