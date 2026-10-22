# Installer 49 fixture-root correction

This is a source-only correction to the PR318 package failure at reviewed head
`db034bdd29a30615ce2f3913e408a28e57d1b3a2`. No production behavior, workflow,
package inclusion rule, installation policy, or version is changed. Runtime
qualification of this correction is pending independent source review.

## Observed failure and source attribution

Hosted run `39734780532` passed tests and lint, then plain `cargo package`
correctly refused thirteen untracked
`crates/asb-cli/.asb-install-step-*/output.log` files. The retained log is
`target/pr318-hosted-failure.log` in the installer49 delivery checkout.
Eleven diagnostics were empty; two contain the echo fixture's Python version
arguments. All thirteen namespace names identify the parent library-test PID.

The source accounts for the thirteen parent-cwd probes as follows. This is
source/count/timing attribution, not a new instrumented execution claim.

| Owning library test | Probes outside its intended fixture before this correction |
| --- | --- |
| `python_runtime::boundary_tests::noninterpreters_and_missing_programs_are_not_accepted_as_python` | Six: `version` and `matches_project` each probe `/bin/true`, `/bin/false`, and `/bin/echo`. The absent executable never spawns. The two echo probes explain the two nonempty logs. |
| `workloads::adapters::terminal_bench::adapter_tests::preparation_checks_selection_runtime_and_allocation_before_any_task_launch` | Four: `/bin/false` and `/bin/true` evaluator probes for each of the two original adapters. Missing-runtime and invalid-selection checks do not spawn. |
| `workloads::adapters::terminal_bench::catalog::tests::local_install_verifies_runtime_and_catalog_without_fetching_or_replacing_subset` | Three: direct legacy `runtime_available` calls against successful, exit-7, and exit-127 executables. The final absent-runtime call does not spawn. |

The six Python diagnostics precede the test completion around 06:35:51 UTC;
the four preparation diagnostics occur around 06:36:30 and the three catalog
diagnostics around 06:36:32. The catalog install's other probes already use
its explicit fixture-owned `InstallationContext`; those are not attributed
to these thirteen misplaced diagnostics.

`OperationContext::load` intentionally uses `project::load`. When no ancestor
contains `.asb/project.toml`, that production lookup returns the default root
`.`. The Python test-only wrappers explicitly loaded `.`, whereas the two
Terminal fixture constructors did not initialize projects. Thus even the
real `runtime_available(tool_root)` lookup fell back to the parent test cwd.
The existing explicit context for catalog installation did not change that
separate legacy runtime-lookup route.

## Narrow correction and regression contract

- The two private `cfg(test)` Python wrappers now require the caller's owned
  `OperationContext`. The same actual `version_in`/`matches_project_in` probes
  and invalid-interpreter assertions remain. Their test uses a unique private
  root and checks all six actual diagnostic files there, including the two
  nonempty echo diagnostics.
- Both Terminal fixture constructors initialize their own project config.
  Tests still exercise the actual legacy runtime lookup, not a replacement
  mock or an explicit-context bypass. The loaded context must resolve to the
  exact private project before probing.
- Linux preparation checks require both real probe diagnostics in each
  adapter fixture; the catalog test checks the exact three-log delta around
  its legacy calls, excluding legitimate earlier installation diagnostics.
  Existing exit-127 marker, missing-runtime diagnostic, no-task-admission,
  catalog/selection, and retained-cache assertions remain unchanged.

There is no global cwd/environment mutation, diagnostic suppression, workflow
cleanup, glob ignore, package `--allow-dirty`, or production lookup change.
The fixture roots retain normal production diagnostic placement; their
existing owned cleanup (or explicit cleanup after successful Python checks)
settles only their own test contents. A failed assertion retains the Python
fixture for investigation. The new namespace counts are Linux-specific,
matching the native managed-install log contract, rather than a new portability
claim about other execution backends.

## Qualification plan (not yet executed)

After independent source approval: build actual ASB first, freeze and hash it,
select the exact library test image from Cargo compiler-artifact JSON, run
these three origin tests first, then the owning Python/Terminal families.
Check source/package cleanliness without manually moving the thirteen
misplaced diagnostics. Broader fresh qualification and plain packaging remain
subject to the coordinator's authorization. Historical db034 local proof
1556/0/21 and hosted package failure are preserved, not relabeled as proof of
this correction.
