# Official workload engine binding: source and focused qualification

This is a narrow ownership/duplication repair, not a new benchmark, evaluator,
protocol, grade or runtime qualification. Full current-stack qualification,
independent final review, hosted CI, publication and release remain outstanding.
Version 0.2.41 is reserved for later coordinated integration, not released here.

## Provenance and baseline first

The original 52-line code-smell report identifies near-identical retained
grading/inspection/recovery/preflight envelopes in the SWE and Terminal adapters.
Read-only review did not establish a new functional defect from that duplication.
The historical `CODE_SMELL_REMAINING_AUDIT.md` classification is not an assertion
that every item in that report has been repaired.

This work starts at reviewed setup source `24d7a880`. The entire workload subtree
was verified byte-identical to fresh actual main `8a115715e312b25b17385a0505c78d49994495b0`
(0.2.35) before changes. Project AGENTS, development and architecture instructions,
the original report, benchmark module/outcome/evaluation contracts, and relevant
retained grading/reconstruction delivery records were read before implementation.

Signed **test-only** checkpoint `ef0f24516a2dd8ef3195bb395ab81a4d3f4d97ab`
precedes every production change. Against unchanged owning production:

- Actual ASB 0.2.40 built first, offline, then its five public `workload describe`
  outputs were inspected. SHA256:
  `6a27715d9e1fb802564ce04ae0cfd646cbd67ec465484afeed0714d5fe4210d5`.
- Exact serialized descriptor tests and literal protocol-fingerprint tests passed:
  **2 passed, 0 failed, 0 ignored**. All five **native Linux x86-64** fingerprint
  comparisons were actually executed before extraction.
- The ARM literals were calculated before production edits by a read-only Python
  SHA256 calculation over those unchanged descriptor JSON objects and explicit
  frozen policy fields, with `host_architecture` set to `aarch64`. These are
  computed expectations, **not executed or native-ARM qualification**. The same
  calculation's x86-64 literals passed the actual Rust implementation.
- No baseline expectation has been regenerated or changed after extraction.
  Its file was later moved unchanged to `tests/unit/workload_contract_baselines.rs`
  using a cfg(test) path from `contracts.rs`, respecting the existing layout
  allowlist without broadening it. The baselines passed again after extraction.

The fingerprint fixture uses no optional collectors, the local Docker metadata
policy, two replicas, a 600-second task execution default, and unchanged 900/1800
preparation/evaluation defaults. An isolated libtest child removes inherited user
Docker configuration; no Docker process, daemon, SDK or task is invoked.

## Boundary and unchanged contracts

Private immutable `EngineBinding` contains only the SDK tool ID, Python module
stem and owning helper publisher. The script filename derives from that one stem.
SWE, original Terminal and Harbor bind their own constants; there is no adapter-ID
switch, new public trait, serialized identity, fingerprint component or registry.

Shared methods implement retained inspection/verification/recovery, dependency
preflight, runtime input declaration and the retained grading tail. They delegate
to the existing bounded remote/controller protocol unchanged. Ordering remains:

- Grade: **owning catalog and dataset validation first**, private inspection
  runtime/helper publication next, then the unchanged remote request validation.
- Inspect: location and SDK validation before private runtime/helper publication.
- Verify/recover: existing installed helper routes, without newly publishing
  helpers. Preflight: SDK validation, owning helper publication, then prefetch.

Catalog parsing, dataset/SDK identity checks, installation, task selection and
preparation, Terminal runtime probing, SWE `session` subcommand, reservation
requirements, evaluator rules and process lifetime remain in their owners.
SDK/tool accessors and derived entry filenames preserve their exact prior bytes.
The five population constants remain 2294/300/500/80/89 with unchanged pins.
SWE still uses Patch/FreshEnvironment/Quiesce; both Terminal profiles use
EnvironmentState/InPlace/ThroughEvaluation. Reference aliases stay unscored smoke.
No successful process is converted into a grade, unknown execution observations
stay unknown, and generic native reward/pending/error outcome contracts are not
narrowed to the legacy Boolean retained-grade protocol.

## Focused evidence

Exclusive cache: `/srv/data/projects/asb-bpftrace-scope/target`, verified idle before
use. Builds used locked/offline Cargo, one job and `CARGO_INCREMENTAL=0`; no peer
cache was cleaned, relinked or used. Actual ASB was built before CLI fixtures.

Actual 0.2.41 binary SHA256 before/after runtime and lint gates:
`a11c58a9665470810b7d8b3a7b6d8390c39827774812b8c2473d92effc5a2ff2`.

| Gate | Observed result |
| --- | --- |
| Pre-extraction baseline | 2 passed / 0 failed / 0 ignored |
| Owning workloads libtest focus | 179 passed / 0 failed / 0 ignored, 5.42s |
| Exact adapter layout | 1 passed / 0 failed / 0 ignored |
| Baselines after unchanged file relocation | 2 passed / 0 failed / 0 ignored, repeated subset |
| Actual offline `benchmark_protocol` integration | 35 passed / 0 failed / 0 ignored, 73.86s |
| Workspace all-target/all-feature Clippy, `-D warnings` | passed |
| `cargo fmt --all --check` and `git diff --check` | passed |

These are **215 distinct outer Rust tests**, not an invented Python subcase
total. The repeated baseline run is not counted again. The 179 include eight
new binding tests: exact routing/read-only dependencies, location/SDK-before-
publication, helper failure and private cleanup with unrelated sentinel,
preflight failure-before-bootstrap, verify/recover no-publication, actual private
synthetic Python protocol with false grade/null execution observations and exact
catalog argument, invalid report refusal, and all three SWE owner dataset checks.
Existing Terminal owner tests retain both populations' missing-SDK, mismatched
catalog, runtime/selection/allocation-before-launch checks.

The 35 integration wrappers exercise existing provider-free retained SWE and
Terminal/Harbor grading, both grades, resource preservation/cancellation,
already-started verifier refusal, SDK-return/raw-report reconstruction, import
and compare, source mutation refusal and no-repeat rules. They use synthetic
SDK seams and actual local CLI processes, **not live SDKs, providers or Docker**.

Transcripts have prefix `/srv/data/projects/asb-official-engine-binding-`:
`baseline-build.log`, `baseline-tests.log`, `build.log`, `final-build.log`,
`workloads-fixed.log`, `layout.log`, `baselines-after.log`,
`benchmark-protocol.log`, `clippy.log`, `fmt.log`. The original failed focused
compile is retained as `workloads.log`: ten stale nested Terminal test-fixture
field references were converted to the new binding. It was a fixture migration
failure, not a claimed new production defect. No blind retry or baseline rewrite.

No full workspace test run, live environment qualification, PR, push, main merge
or release is claimed by this checkpoint. Independent source review must precede
the next qualification stage.
