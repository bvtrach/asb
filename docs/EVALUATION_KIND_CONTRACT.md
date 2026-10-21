# Known producer evaluation kinds: class47 design and baseline

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
