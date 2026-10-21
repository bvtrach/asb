# Original five-benchmark delivery audit

This is a branch implementation audit, not release approval. The target remains
all five original benchmarks through setup/run/sweep, PR merge, verified main CI,
and minor release 0.2.0. The complete checklist in
[FULL_BENCHMARK_INTEGRATION.md](FULL_BENCHMARK_INTEGRATION.md) remains authoritative.
Existing runtime qualifications use known submissions/synthetic providers, not
claims that an AI solved a complete population.

| Requirement | Current evidence | Remaining delivery verification |
| --- | --- | --- |
| Workload-owned catalogs/lifecycle | Reviewed adapter hooks and `WorkloadSession`; shared runner/sweep use generic invocation, execution, evaluation and cleanup contracts | Verified implementation; release-head checks below |
| Exact populations | Pinned installed catalogs: 2294/300/500/80/89; complete installed-catalog 3263-entry planning/reporting qualification passed | Verified; no complete-population AI score claimed |
| Functioning local SDK/tool installs | Separate SWE/Terminal/Harbor recipes; fresh-project public install passed in 151.349s (`benchmark-cold-install-final.log`) | Release-head package/install verification |
| Task/submission/evaluator identity | Reviewed task/dataset/attempt, baseline, submission and SDK/image records; stale/conflicting/duplicate evidence regressions pass | Verified contracts |
| Original SWE execution/evaluation | Owning family engine; private evaluator inputs; real official gold/incorrect checks for all three passed in 211.894s (`benchmark-resources-official-swe.log`) | Final 0.2.0 scripted-setup/runtime refresh |
| Original Terminal 1.0 execution/evaluation | Original SDK task/Compose/tmux/parser; correct/incorrect grading; service-bearing Jupyter qualification and SDK-owned post-grade teardown | Final 0.2.0 scripted-setup/runtime refresh |
| Original Terminal 2.1 execution/evaluation | Original Harbor environment/verifier/reward parser; real correct/incorrect grading and retained-grading qualifications | Final 0.2.0 scripted-setup/runtime refresh |
| Selected ASB agent/provider | Reviewed generic guest driver and recipe-derived bootstrap for every registered agent; actual npm/Python/standalone install checks; real Pi with synthetic provider | Verified supported recipe classes; not every agent/model pairing is equivalent |
| Measurement/tape targeting | Scoped execution targets/lifecycle; actual measured and disabled-collection recovery sweeps passed in 108.767s/99.966s; record/replay regressions | Verified; release-head regression checks |
| Population selection and scheduling | Complete catalog plan/report passed; installed IDs; independent jobs/replica budget; typed `setup --trial`/`--trial-set` validates selections before writing | Source/package tests and final scripted-setup refresh |
| Accurate resolution semantics | Reviewed evaluator-based resolution and coverage; missing/infra attempts stay in selected denominator; smoke unscored; repetitions not silently pass@1 | Verified unit/reporting and real grader evidence |
| Freshness and completed continuation | Original input/intent/allocation and artifact seals; ordinary reconciliation; report-only imports; explicit grade-only closure and no-repeat runtime checks | Verified safe disposition: unknown/incomplete evidence refuses reuse; a fresh sweep preserves old history |
| Failure/cancellation resource safety | Owned journals and scoped stop/preserve; real controller SIGKILL/EOF/TERM fixtures; unrelated sibling/image preservation; no global prune | Verified; unresolved attempts are not silently retried |
| Existing result recovery | SWE raw-report and Terminal SDK-return reconstruction; source-byte bindings; true/false preservation; unknown outcomes remain unknown | Verified, not historical attestation or invented execution metrics |
| Provider-free tests | 454 unit, 87 offline CLI, 21 benchmark protocol suites, guest/parallel/partial/replay/unmeasured checks; added all-five setup-selection CLI tests | Release-head source/package/hosted rerun |
| Delivery | Signed PR #227 head 923ffa28 passed hosted run 39546156258; 0.2.0 preparation follows on the same PR | Merge, exact-main CI, minor release still required |

Started-slot continuation now has an explicit grade-only closure path. Existing
grade imports remain reporting-only and are never completion receipts or
permission to repeat a client. Lost execution metrics remain unknown, not an
ordinary completed run. Complete-population planning/reporting qualification
exercised the actual installed catalogs and all 3263 selection entries, reporting
every attempt unevaluated and no full-population score. It did not execute all
instances or establish AI benchmark performance.

No amount of metadata recovery can infer an unobserved verifier result. Safe
handling means preserving the attempt, reporting unknown/missing evidence and
requiring an explicit new-attempt disposition where necessary, not silently
rerunning an unresolved task until it passes.

The final source review found and repaired two integration-facing gaps rather
than treating green checks as completion: README still described the real
adapters as smoke exercises, and non-interactive setup lacked an instance
selection argument. Setup now uses generic catalog/selection validation, with
provider-free all-five-catalog tests for valid, missing, unknown and stale
selections. A final real selected-client sweep for every workload additionally
uses the public setup command before execution. The Terminal runtime fixture's
old pre-evaluation quiescence assertion was corrected to require the adapter's
service handoff followed by actual SDK-owned container removal, not to accept
an unverified cleanup claim.

### Retained-grading integrity review follow-up

The independent review of release candidate `2d0220bc` correctly identified
two cases absent from the earlier verification scope. They are product bugs,
not authorization or signing requirements: copied reconstructed reports were
not individually bound to their original manifest entries, and an original
grader could publish between an absence scan and quiescence.

Both have targeted repairs. Every copied original input/report/journal/cleanup
now must match its source entry; only the intentionally derived normalized
wrapper is exempt from copy equality. The new-evaluation decision now follows
a fresh scan while all prior evaluator journals remain exclusively locked.
The preliminary scanner is separate from final evidence collection, so a
legitimate late publication is preserved rather than mistaken for corruption.
Reconstruction borrows and checks the same held source lock.

Sixteen provider-free retained-grading tests pass, including a consistent
three-file false-to-true edit with unchanged original bytes/manifests, individual
copied-artifact edits, both true/false normalized and raw late publications in
all three populations, and an owner reacquiring its lock during exclusion.
Twelve import/compare tests pass, including late-candidate rejection before any
publication and unchanged imports/source bytes after rejection. Complete source
tests, strict Clippy and build pass. Real SDK retained-grading, exact package
and hosted verification follow this repair before merge or release. Earlier
green candidate checks are not treated as proof that these cases were covered.
