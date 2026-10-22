# Known producer evaluation kinds: class47 design and baseline

Current qualified source integration: signed merges8b660f7e and17bf4e18 retain the
reviewed material/fair/runner/fixture stack and qualified procfs hotfix alongside
the independently reviewed typed extraction. Root coordinated provisional
Cargo0.2.48; earlier45 was inherited metadata, not this patch's reserved slot.
Fresh combined local qualification at bd780473 is recorded below; actual-main
reconciliation, independent final review and public delivery remain pending.

## Historical design/baseline checkpoint

Base: qualified runner43 `6e21f19a3362b7253bd43c40ad9c6524a3646694`;
version remains0.2.43. This is DESIGN + BASELINE-TEST ONLY. No production
repair, false-grade reproducer, full qualification or release is claimed.
The original report's high4 coupling is real maintainability debt: producer
traits return strings and consumers repeat spelling-sensitive domain matches.
Current scoring intentionally excludes unknown categories; this audit does not
claim an observed typo or incorrectly granted grade.

AGENTS and full original report were read. DEVELOPMENT/ARCHITECTURE are
byte-identical to fully read `f3883ca8`; identity was verified before editing.
Source/fixtures/cache remain in this owned HOME project. Class44 installer API
and parked class45 publication branches are out of scope.

## Owners and seams inspected

- `workloads/mod.rs:527-546`: known producer dispatch, resolution eligibility,
  derived invalid-baseline label; ResolutionSummary consumes saved categories.
- `workloads/adapters/mod.rs:119,220`: default reference smoke and prompt
  producers; PromptAdapter chooses response review versus bundled fixture.
  `prompt.rs:7` dead-code assertion is redundant only if actual trait-object
  registry coverage remains, not merely because the function looks unused.
- `workloads/session.rs:318,364,430`: BaselineEvidence eligibility support,
  WorkloadSession trait and FileSession storage/constructor.
- `workloads/remote.rs:1059`: official session produces upstream-harness.
- `workloads/selection.rs`: PlanningCatalog is a known producer; TaskSelection
  is a persisted extensible string with descriptor agreement and size checks.
  `contracts.rs:71` maps local kinds to exact protocol/support/score policy.
  SWE/Terminal catalogs explicitly produce upstream-harness.
- `runner/attempt.rs:523,952` and `runner/disposition.rs:83,158`: producer
  scoring/disposition boundary then unchanged public RunResult string field.
  `runner/batch.rs` contains a test WorkloadSession implementation to migrate.
- `measurements.rs:1358`, `compare/source.rs`, `compare/outcomes.rs`,
  `compare/coverage.rs`, `compare.rs`: historical string readers must stay
  tolerant; do not close categories or drop extension bytes.
- Sweep selection/plan/checkpoint reconciliation embeds these identities and
  compares saved strings and baseline evidence. Preserve descriptor JSON,
  protocol IDs, context fingerprints and current grade/reuse constraints.

## Proposed contract, awaiting approval before production

Introduce internal `KnownEvaluationKind` with explicit `as_str()` for exact
existing spellings: BundledFixture, ResponseReview, AdapterSmoke,
UpstreamHarness, Ungraded, and derived InvalidBaseline. Adapter/session/catalog
traits and storage return the typed known domain. Producer RunFacts and
baseline gating consume that domain, not reconstructed strings. Convert only
at unchanged result/selection/descriptor/wire boundaries. No implicit Display
as policy and no string-based recovery/fallback.

Separately classify borrowed saved strings as Known(kind) or Extension(&str).
Extension preserves exact original bytes; absent historical values retain the
existing caller default unknown. Do not implement a closed serde Deserialize
enum for persisted evaluation categories. Existing bounded TaskSelection,
descriptor-agreement and baseline/hash validations remain unchanged. Arbitrary
protocol IDs and historical evaluation extensions are not registration errors.

Known kinds centralize the existing eligibility set and baseline downgrade.
Keep measured-false baseline `supports` true even for an extension: support is
not itself eligibility, and silently tightening this low-level predicate would
be a behavior change. Official baseline hashes support only upstream-harness.
Keep native reward resolution null, unknown/extension unscored, ungraded and
smoke labels, operational failures, receipt and continuation policy unchanged.
No provider, evaluator, authorization or native-grade policy is added.

Potential files: workloads evaluation module/mod/session/adapters/remote/
selection/contracts; runner attempt/disposition and mock session; explicit wire
projections plus reader classifier delegation. Do not replace runner bodies or
change controller/installation callbacks. Keep new generic source modules in
exact adapter-layout allowlists when required; no blanket exclusions.

## Baseline before implementation

New test-only include and five tests lock literal producer kinds, actual prompt
registry trait-object proof, all known eligible categories and unsupported
baseline downgrade, extension/case/empty strings, official hashes, tolerant
legacy selection roundtrip, exact descriptor/selection canonical JSON and its
unchanged fingerprint input bytes. The digest check is selection-context input
coverage, NOT an executed whole checkpoint or protocol fingerprint fixture.
Existing owning session, disposition, selection, planning/checkpoint and compare
families must run alongside it. Later production qualification also requires
native null resolution, true/false/Pending and extension report cases, exact
wire projection/fingerprint agreement and no policy changes. No RED is expected:
these tests establish existing behavior before a typed-contract refactor.

No production implementation is authorized by this document. Source checkpoint
and baseline execution evidence will be submitted for independent root review
before any such change. No full suite/PR/push at this phase.

## Executed baseline evidence, source `5ebc6cb1089cea7f599e4691861ba33126932eb0`

Only the four-line cfg(test) include and the new five-test file differ in Rust
from the qualified base; no producer/consumer implementation changed. The actual
ASB built FIRST (`target/evaluation47-baseline-build.log`,exit0,14.18s). The owned
HOME cache began as a copy of the verified idle class45 debug cache; its copied
artifacts do not qualify this source. Class45 source/cache were not modified.
Every command used this worktree's own target, offline/locked, jobs1/incremental0;
approximately194GiB was free before the build and no other cache was used.

Actual and frozen `target/qualification/evaluation47-baseline-asb` SHA256:
`d1da6f73b9447e649aca64bd66d9b079bef7748cc7aa6b8df45739032af2deeb`.
Before, after and final hash logs agree. Post-test libtest SHA256 is
`3200aaa3b506e7216cdd34eae2dacdaf8b0c3a386eeda735994774e1b6c7af97`;
there is no pre-run libtest stability claim. All handles are terminal.

| Retained target log | Result |
| --- | --- |
| evaluation47-contract-focus.log | New5/0/0ignored;25.39s compile,0.00s runtime |
| evaluation47-existing-contract-baselines.log | Unchanged2/0/0; exact five official descriptor and protocol-fingerprint goldens |
| evaluation47-session.log | 9/0/0 |
| evaluation47-selection.log | 3/0/0 |
| evaluation47-disposition.log | 4/0/0 |
| evaluation47-planning.log | 6/0/0 |
| evaluation47-unknown-report.log | Existing legacy smoke/unknown scoring1/0/0 |
| evaluation47-native-report.log | Existing native partial/null reward completion+HTML1/0/0 |
| evaluation47-baseline-clippy.log | Strict workspace/all-target/all-feature -Dwarnings exit0,15.25s |
| evaluation47-baseline-fmt.log / evaluation47-baseline-diffcheck.log | exit0 / exit0 |

This is31 passing focused outer tests, not a full suite. Native Linux x86_64
actually executed the existing literal protocol fingerprint expectations; ARM
expectations remain calculated/source-only, not runtime-qualified here. No
literal baseline was regenerated. No failure occurred in these baseline runs.
No SDK, provider, Docker daemon or privileged operation was used. Production
remains held for root's independent design/baseline approval; after extraction,
the exact unchanged official goldens must run again with all other owning tests.

## Independent root review and sequencing

Root independently read the complete design and five-test baseline, inspected
the actual producer dispatch, eligibility, baseline support and descriptor
construction, and checked all focused result logs against the reported totals.
The actual binary and frozen baseline both still hash to `d1da6f73` above.
The 31 focused tests establish existing behavior; they do not demonstrate an
incorrect grade or qualify a future implementation.

The typed known-producer / tolerant borrowed saved-reader design is approved.
Preserve all existing wire spellings, unknown extension bytes, baseline support,
eligibility and literal official goldens. Do not introduce new authorization,
verification or grading policy. Production work remains deliberately parked
while the same worker implements the higher-priority compatibility-listener
repair; this approval is not permission to run both edits concurrently. After
implementation, independent source review and fresh qualification are still
required before PR delivery. The removed 90% coverage gate remains removed.

## Resumed producer extraction (source checkpoint before runtime)

The actual/frozen integrated pre-extraction baseline d57bb98e and original
d1da6f73 remain unchanged. The integrated31 focused tests, including the
unchanged two official descriptor/fingerprint tests, passed before production
extraction. Its logs use `target/evaluation47-integrated-baseline-*` names.
The interrupted implementation had only a successful13.16s actual build
(`evaluation47-implementation-build.log`), SHA256
`dd4655759b4082ad0fc3feef3323496890d23ae207a08ffa1b8ef1440c083c9b`;
that binary has no implementation runtime qualification.

`workloads/evaluation.rs` owns six typed known categories and exact string
projections, plus tolerant borrowed saved classification. Adapter/session/
FileSession/planning-catalog producers and runner RunFacts consume the typed
domain; existing public result/selection/descriptor strings remain unchanged.
Historical readers still accept extension bytes, including case differences,
empty strings and Unicode. No closed Deserialize, new evaluator policy,
provider branch, grade authorization or continuation requirement is introduced.
Measured-false baseline support remains true for extensions, independently of
their existing ineligibility. The native metric fixture preserves its arbitrary
protocol ID through descriptor construction before retained plan publication.
The redundant prompt dead-code assertion is replaced only by the actual
registry trait-object proof. The generic layout allowlist adds evaluation.rs
explicitly, preserving every registered-adapter exclusion.

Next gates are actual-FIRST/frozen implementation, the six-category/extension
test and all existing31 baseline cases (unaltered official five goldens), plus
owning report/disposition/selection/planning/checkpoint regression families and
strict lint/fmt. No source edits/relink occur during runtime. Current-main and
reviewed fixture-repair ancestry will integrate only after this signed source
checkpoint and focused review; no old-base full run is planned. Class44
installation callbacks, parked45 publication and other source branches remain
out of scope.

## Executed extraction focus, source `9a80e867`

Root independently reviewed the complete `972e7f37` producer/reader extraction
and approved its exact wire/extension preservation and unchanged policy. The
first owning test compilation exposed three test-only projection misses; no
test ran in that failed command. Its complete log remains
`target/evaluation47-972e-evaluation_kind_contract_tests.log`. Signed followup
`9a80e867` corrects two nested enum paths and projects the remote-session kind
with `.as_str()` for its existing literal assertion; no product behavior changed.

The actual ASB was built FIRST again at that followup (exit0,14.57s,
`target/evaluation47-followup-build.log`) before runtime. Actual ASB and frozen
`target/qualification/evaluation47-9a80e867-asb` share SHA256
`dd4655759b4082ad0fc3feef3323496890d23ae207a08ffa1b8ef1440c083c9b`.
The before/after/final `evaluation47-followup-*.sha256` records agree. Original
d1da6f73 and integrated d57bb98e baseline copies remain unchanged, rechecked in
`target/evaluation47-followup-baselines-final.sha256`. No source edit or actual
executable relink occurred during runtime; all handles are terminal.

All following logs are under `target/evaluation47-followup-`:

| Log suffix (without `.log`) | Passed outer tests |
| --- | ---: |
| evaluation_kind_contract_tests | 6 |
| workloads__contracts__baselines | 2 |
| workloads__session__tests | 9 |
| workloads__selection__tests | 3 |
| runner__disposition__tests | 4 |
| planning__tests | 6 |
| legacy_smoke_and_unknown_scores_cannot_become_task_resolution | 1 |
| native_partial_rewards_survive_completion_collection_and_html_report | 1 |
| unknown_historical_run_categories_remain_display_only_and_unscored | 1 |
| adapter_layout_tests | 1 |
| workloads__contracts__tests | 3 |
| sweep__checkpoint__completed__tests | 16 |
| workloads__task_protocol__ | 7 |
| full_runner_entry_disposition_keeps_pending_and_operational_failures_consistent | 1 |
| actual_unmeasured_sweep_capture_failure_keeps_grades_reporting_only | 1 |
| workloads__remote__coverage_tests | 5 |
| workloads__remote__execution_tests | 14 |
| workloads__adapters__swe_bench__catalog__tests | 2 |
| workloads__adapters__terminal_bench__tests | 1 |

This is84 passed/0failed/0ignored in19 nonzero focused outer selections, not a
full suite. Internal runner/sweep scenarios are not added again to that count.
The remote execution family includes actual local Python task-loop/Rust-session
transport, not external SDK qualification. The unchanged official five
descriptor/protocol-fingerprint literal goldens ran again; their file is
byte-identical to integrated baseline a881645a (golden-parity log exit0), with
no regenerated expectations. Native Linux x86_64 executed; ARM remains
calculated/source-only. No provider, SDK, Docker daemon or privilege was used.

Strict workspace/all-target/all-feature Clippy with `-D warnings` passed16.74s
(`evaluation47-followup-clippy.log`) and final cached verification passed0.06s
(`evaluation47-followup-final-clippy.log`). Final fmt/diffcheck logs both exit0.
Only this owned HOME cache was used. Cargo45 remains inherited provisional
metadata, not an allocated release. Current-main/fixture-stack integration,
fresh combined qualification and independent delivery review remain pending;
no full suite, PR, push or release is claimed by this checkpoint.

## Reviewed-stack integration, source only staged48

Normal signed merge8b660f7e combines focused typed72823bce with root-approved
material eafbb48d1dd49111280f78d35d85df0a8591e577. There were NO conflicts;
remote.rs, lib.rs and runner_phases.rs merged automatically. The only manual
integration edit changed the workspace/package Cargo version pair47→48, as
coordinated. Normal signed merge17bf4e18 then adds qualified hotfix
00eebceae959f417f19f5b643226d01c2bfd413a without conflicts: exactly three procfs
test-path literals and its owning ledger. The exact reviewed ab877 consent-input
fixture was already present through material/fair, so no duplicate delta arose.

The shared remote seam preserves all four incoming ControllerCommand producers,
their preconfiguration materialization and gate custody, alongside typed
UpstreamHarness/session/baseline projections. lib.rs keeps both incoming private
fixture/guardian wiring and the exact evaluation.rs layout exception. The two
runner-phase mock session producers remain typed; incoming isolated-writer setup
and every phase test remain. No runner body, controller, resource/grade policy,
saved wire value, eligibility rule or literal official golden was reconstructed.

Direct diff parity proves the incoming process_cleanup fair worker, entire gate
and controller trees, material Python transports and guardian fixtures/tests
byte-identical to eafbb48d. All three hotfix/consent owning files are byte-identical
to00eebcea. The typed evaluation core, contract tests and unchanged official
goldens remain byte-identical to72823bce. Historical84 typed passes and incoming
1489 material/1466 hotfix passes remain separate parent evidence, NOT combined
qualification. All historical caches/binaries/logs are unchanged; no build,
runtime, cache copy, PR, push, release or claim preceding patches are delivered
has occurred in this integration step. Root independently reviews this signed
source checkpoint before authorizing combined actual-FIRST/frozen qualification.

## Fresh combined local qualification, source `bd780473`

After root independently reviewed all integration seams and parity proofs, actual
ASB built FIRST13.02s, locked/offline/jobs1/incremental0 in this owned HOME cache.
Frozen `target/qualification/evaluation48-bd780473-asb` SHA256:
`4d72136ae03b9fce57e83ffbdd8989e08cc54b9de7a7b7a26fa179f01922e4eb`.
Before/after focus, before/after full and after normal gates hashes agree. No
source edit or actual-binary relink occurred during runtime. Original baseline,
integrated-baseline and84-test focused copies/logs remain separate and intact.

Fresh focus passed211/0/0: the original84 typed/golden/extension owners,
complete controller34 (including five actual material custody paths), guardian3,
gate30, fair reaper7, Python Docker9/material8/task-server13/resources23. An
earlier narrow controller::tests16 selection also passed but is excluded from
211 because the later complete controller family includes it. Every selection
was nonzero. Actual retained-owner/release/reap/sentinel and independent grade
semantics ran on this combined source, without Docker/provider/SDK/privileges.

Fresh locked/offline workspace/all-target/all-feature full suite exited0:
**1495passed/0failed/21existing opt-in ignored across24outer targets**. Library
1240/0/13, offline_cli98/0/5 and repeated_run33/0 all passed. The compact
`target/evaluation48-full-outer-summaries.log` excludes nested reexec summaries.
Complete evidence uses `target/evaluation48-{build,focus-*,python-*,full,doctests,
clippy,fmt,diffcheck,make,update-tests,completion-syntax,package,package-help,
package-assets}.log` and corresponding digest files. Doctests0/0, strict workspace
all-target/all-feature Clippy `-D warnings`15.96s, fmt/diffcheck, sequential
make/private Rustdocs/pinned AutoUML0.7.2/type/flow/interactive/completion gates,
both shell syntax checks and all7 checkout-update tests passed. This is the
update-test gate, not make update or real checkout mutation. No failure occurred
in this combined qualification; all handles are terminal.

After those handles terminated, root approved preserving only the generated
regular/nonsymlink/zero-byte install.log with source/dev/inode/mode metadata under
target/fixture-preservation; only its empty .asb parent was removed with rmdir.
Crate target diagnostics remain. Locked/offline cargo package passed518files,
6.2MiB/1.4MiB compressed, verification13.37s. Archive checks confirm typed
core/contract/goldens, hotfix/consent fixtures, material controller/Python/guardian
assets and both shared fixture helpers match checkout bytes; the immutable mock
gate is0755 and no generated crate target tree is packaged. Package help passed.
Archive SHA256 is
`4f72c93df9eed0953d09c58a18deadf09537cf303b2f0cffd11809fb6c856dcf`.
Package verification legitimately relinked ordinary debug ASB afterward; its
separately frozen help-only evaluation48-bd780473-packaged-asb SHA256 is
`065d9847fdc9dca2c136da32815dddec316b4f1b10477d57b91384f4203b56a1`,
NOT the full-suite4d721 executable, which remains frozen alongside its libtest.

Protected parent/core/golden parity checks still pass. Scope is local Linux
x86_64 offline qualification; no ARM/Windows runtime, original SDK, Docker/live
provider or extracted-package full-suite claim is made. Cargo48 remains staged
provisional, not a claim preceding delivery patches have released. Independent
final proof review and actual-main ancestry reconciliation precede any PR/push;
hosted CI, both postmerge jobs and signed release remain pending. The removed
coverage gate stays removed; this worker has performed no public promotion.
