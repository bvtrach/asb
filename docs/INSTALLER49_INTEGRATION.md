# Installer process-boundary stack integration: source checkpoint

This is a SOURCE-ONLY candidate, not combined runtime qualification or a release.
Workspace/asb-cli version0.2.49 is provisional under root's delivery plan after
typed48. No build, test, cache allocation, push, PR, merge-to-main or release was
performed for this integration. The new checkout has no target directory.

## Immutable inputs and conflict decisions

The isolated checkout is /home/martin/projects/asb-installer49-integration,
branch codex/installer49-integration. It began at installer focused checkpoint
aa122d4aa64701f8b8c29a53f40b681129fda742. Signed merge
e7d3eb5dea5c8f38f36437d91b922aaef6a9bc94 adds qualified typed checkpoint
c1d91e61e3cad7ca53c7f5d233544bed056a3c64. Only Cargo.toml and Cargo.lock
conflicted: inherited44 versus typed48 was explicitly resolved to planned49.
No dependency or other lockfile record changed. All source merges were automatic.

Signed merge18d67ee5f7e9257f84aa239b3ec8ef3de59d3f71 then adds qualified parser
checkpoint975ad96827cb844f932ecd973f979f9b18255ab5 without conflicts. Its delta
is exactly the six parser/launch/CI/evidence paths recorded in the parser ledger;
temporary diagnostic workflow, test and script remain absent. Both merges use
explicit Martin Beck <martin.beck2@gmx.de> identity, valid signatures and DCO.
All original worktrees, caches, logs, namespaces and provenance remain untouched.

## Protected source parity and shared seams

Direct git byte comparisons against the immutable inputs establish:

- Against aa122: install.rs, the entire install/ and tools/ trees, project.rs,
  project/installation.rs, python_runtime.rs and installer_process_boundary.rs
  are unchanged. Actual OperationContext, typed Ordinary/Abort propagation,
  original prepared Command, bounded capture/logging, owner-generation proof,
  consumed Stage guard and Source lifecycle/error diagnostics are preserved.
- Against c1d91: the entire runner/, process_gate/, compare/ and
  workloads/controller/ trees, controller.rs, process_cleanup.rs, Docker/material
  Python assets, evaluation.rs, contracts.rs, session.rs and the evaluation-kind
  contract tests are unchanged. Typed known producers and tolerant saved readers,
  wire strings, baseline-versus-eligibility behavior, literal protocol/descriptor
  goldens, runner publication/receipt policy and material resource custody are
  not rewritten by the installer integration.
- Against975: ci.yml, asb_runtime.py, runtime_inputs.rs, launch/tests.rs,
  benchmark_runtime.py and loader-ubuntu26-asb.txt are unchanged. The strict
  leading anonymous loader-header exception, negative controls, non-UTF8 parent
  directory with unchanged echo basename, and CARGO_INCREMENTAL=0 remain intact.
- Against aa122: install/operation.rs, agents/, evidence/ and measurements.rs
  are unchanged; no new TUI/progress, artifact, publication or measurement
  mechanism is introduced here.

The shared workload files intentionally combine BOTH contracts. Relative to
typed48, WorkloadAdapter::install and official SWE/Terminal implementations use
InstallResult, InstallationContext retains its OperationContext, and official
catalog/source/download owners use the installer's ProcessScope and settlement.
Their KnownEvaluationKind return values and explicit wire projections survive.
The remote Session retains typed evaluation plus incoming material ControllerCommand;
its ProjectConfig test literal additionally supplies InstallationConfig::default.
These were inspected as automatic merges, not resolved by restoring old bodies.
The inherited dev/architecture typed-producer instruction additions are retained.

## Evidence boundary and next qualification

The installer86 focused tests, typed1495 full tests and parser1466 full tests
belong to their respective original immutable candidates; none qualifies this
combined tree. No old frozen binary is relabeled as a new integration binary.
git diff --check passed; no Cargo command or runtime was invoked in this checkout.

Final actual-main delivery ancestry and the separately owned Python bootstrap
prerequisite repair are still pending. This checkpoint does NOT claim the four
later hosted offline_cli/bootstrap failures are repaired. The prerequisite
worker's source is not imported or edited speculatively.

After root source review and explicit cache coordination, recommended fresh
qualification is actual-ASB-first/frozen hash, the install Source/process/native/
coverage/public-CLI families, typed contract and unchanged literal goldens,
material/controller/remote/resource and baseline seams, parser/launch controls,
and the owning offline_cli bootstrap/proxy/reset cases once their approved repair
is integrated. Strict lint/fmt and a fresh full/package sequence require later
authorization; current source parity is not a substitute for those executions.
