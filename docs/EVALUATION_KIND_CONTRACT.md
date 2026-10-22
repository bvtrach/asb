# Known producer evaluation kinds: class47 design and baseline

Current implementation checkpoint: normal signed integration a881645a retained
the reviewed runner/transport stack before the urgent fixture-repair diversion.
Inherited Cargo0.2.45 is provisional metadata, not a delivery-slot allocation.
The source extraction is now resumed under root approval; no full qualification,
new current-main integration, PR, push or release is authorized at this stage.

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
