# Multi-track sweep, continuation and reporting integration

Status: design checkpoint for delivery feature 5, not an availability claim.
The preceding run integration is PR #294; this branch must retain its validated
record/replay and legacy behavior. Feature 6 remains the offline track viewer.

Foundation implemented on this branch: standalone repeated execution now writes
the owned `runner::campaign` plan/slot/input contract, preserving its schema and
wire shapes. Sweep and campaign modes share `planning::ExecutionMode`; legacy
sweep missing modes remain unknown. Admission deserialization accepts only the
current operational contract. These DTOs are not activation capabilities: the
bounded reader, semantic validation, sweep lifecycle, continuation and reporting
integration below remain required before this feature is available.

Fresh plans now use one shared, allocation-bounded slot constructor and pass
pure structural validation before publication. Validation binds schema, mode,
configuration, exact batch/replica paths, replay table references and saved track
choices; it reads no source/payload/outcome and cannot authorize continuation.
Selected numeric legacy retains the backend's selected-track size limit, while
unselected legacy retains its historical smaller transfer limit. Full source
selection validation, workload compatibility and settled attempt evidence remain
separate obligations. Tests cover path parity for every supported concurrency,
structural mutation matrices, all execution modes and input-size boundaries.

The standalone coordinator now uses a read-only `Prepared` phase followed by
consuming activation. It owns its plan/configuration once, fixes encoded plan
bytes and selected-source handles, and accepts an explicit campaign parent
without changing original project authority. The absolute original configuration
invocation is retained separately from canonical source identity, including
cross-project symlinks. Recording inspection creates no missing directories;
activation creates/rechecks them before campaign publication and any client.
Inspection is not a reservation, and the journal's no-replace boundary remains
authoritative. Unselected legacy replay now retains one opened regular input per
canonical-path-deduplicated source until private staging consumes its owner.
Discovery still uses historical base/replica/sparse precedence. All observed
metadata sizes and the aggregate bound pass before any initial hash; both hash
and staging enforce the saved length, reject changed lengths before reading and
use at most one sentinel byte to detect growth. Staging verifies exact saved
checksums without reopening display paths. Result-tree moves, path replacement
and unlinking cannot substitute another source; in-place changes fail before any
batch/client. Selected-track and legacy staging have mutually exclusive owners.
Prepared campaigns own saved policy and presentation/measurement-override
settings; they borrow neither caller option strings nor their own configuration.
Activation derives routing, mode, timing and counts from the frozen plan/source
and constructs per-batch options explicitly. The earlier caller-provided cohort
was overwritten by the runner; partial-resume cohort handling still requires the
deliberate admitted-slot/cohort seam below, not reuse of that inert input.

Publication is now its own consuming phase between preparation and execution.
It exclusively creates/locks the campaign root, publishes its immutable plan,
stages private inputs and measurement directories without a provider, tool,
agent, workload gate or recording journal. Its owned result retains the lock
through execution; dropping it releases ownership but keeps published evidence.
An explicit isolated campaign parent defaults recording into that unique root;
standalone defaults and explicit cassette overrides stay unchanged. Read-only
inspection precedes root creation, while parent creation and authoritative
destination rechecks follow exclusive ownership, so a root-contained cassette
cannot accidentally pre-create or adopt the campaign root. Sweep must collect
all successful publications and durable index references before executing any.

The in-progress report reader now accepts an explicit campaign `plan.json`,
validates its structural layout and enumerates every saved batch/replica slot.
It prefers the slot's checksummed immutable outcome reference, with exact-slot
log fallback only when no reference exists. Present invalid references never
fall back. Measurement-only rows must match the full saved execution identity,
configured task and actual replica/cohort; their JSON snapshot is retained once.
Cross-source execution, task, outcome and recording claims must agree before
publication. Missing slots remain rows without invented observed identities.
Machine-readable rows retain batch/replica and observed execution metadata;
campaign metadata and HTML disclose mode and repeated-attempt limitations.

Primary reference/log/outcome/measurement metadata receives a 256 MiB aggregate
preflight before outcome hashes or recording payload verification; actual reads
share a bounded budget, including retained measurement snapshots. Unix reads
traverse observation components relative to an opened root descriptor and refuse
intermediate/final symlink aliases. These are local integrity/path checks, not
authentication or historical attestation. Report-only recording summaries verify
the actual producer association and selected payload without manufacturing missing
capture receipts or granting continuation authority. Full outcome records still
validate capture agreement separately. Measurements now carry the same optional
execution/recording identity as logs, and new private summary output retains its
measurement directory; older private summaries can omit that directory.

Real CLI regressions cover six rows at N3/C2 including a completed process failure,
invalid reference identity/path/hash without report replacement, missing/log-only
slots, measurement-only execution mutations, aggregate sparse-log exhaustion,
intermediate aliases, measurement/recording disagreement and output-overflow log
recording proof. Grouped per-recording proof verification, retained population
selection semantics, sweep scheduling/continuation wiring and complete feature
qualification remain unfinished; these checkpoints are not feature readiness.

Qualification checkpoint: the exact campaign-reader revision `b8dbab6b` passed
the complete offline suite (1267 passed, 21 existing ignored, zero failures),
strict Clippy, build, formatting, Rust docs and UML generation. Subsequent main
integration retains the shared synthetic recording-provider fixture and the
controller/evidence-publication repairs. Its repeated CLI suite (35 tests),
actual bundled publication-fault qualification, nine measurement-publication
tests and strict Clippy pass after adapting the new fixture constructor to the
optional metadata fields. This does not substitute for full qualification of
the integrated head or the remaining feature work.

## Fresh sweep integration checkpoint

The source branch now wires repeat, numeric/statistic selection and seed options
into the existing consuming sweep phases and weighted scheduler. Schema-two
indexes retain execution intent plus one bounded checksummed campaign reference
per configuration; schema-one indexes remain the legacy reader. The whole
reference population is validated before observation lookup. Comparison binds
each referenced configuration/mode/timing/repeat/selection, keeps the saved task
selection and enumerates original configuration/batch/replica identities.
User-supplied seeds are distinguished from generated shared seeds, which apply
only to bundled inputs; legacy seeds/statistics are never fabricated.

Fresh preparation pins every configuration before preserving old output. The
index is published once, then every campaign plan/private input is published
before the scheduler receives execution authority. A separate publication marker
binds the index bytes, not completion. Partial publication retains the index and
published roots without starting clients. Each campaign lock is consumed outside
its short ownership mutex. Ordinary settled failures can continue; uncertain
settlement or typed input/IO failures stop new configuration admission while
admitted siblings settle. Scheduling counts original C, not N times C; only the
bounded file-backed presentation spool scales to the execution population.

Default recordings live under project `cassettes/sweep-RUNID/CONFIG/` so moving
result trees never moves authoritative recording journals. Explicit cassette
roots retain precedence. Discovery shares invalid-present semantics with run:
a dangling link/directory cannot silently select a different legacy input.
Campaign plans retain the nominal replay source separately from actual opened
input files, preserving sparse legacy routing on subsequent default replay.
Both display locations are rebased before immutable index hashes are computed;
publication still stages from the previously opened source handles.

New optional measurement-storage metadata distinguishes campaign-owned files
from explicit overrides. Historical comparison resolves owned measurements from
the actual retained campaign root, while validating saved logical declarations;
external overrides stay fixed. Older plans without this distinction retain
unknown historical behavior rather than guessed relocation authority. JSON and
HTML disclose mode, repetition, individual execution/replay identity and the
limits of interpreting replay as provider performance.

Real CLI tests cover two configurations at N2/C2/jobs2 recording eight tracks,
seeded N3 replay with jobs1/2 producing the same choices and twelve report rows,
no replay inference calls, second-input failure before output rotation, default
N1 recording with changed-concurrency replay/replay-again, measured history
comparison, invalid-present bundle refusal, numeric legacy intent mutation and
distinct sparse legacy replica responses across repeated result rotations.
Pure tests bound the whole index before configuration callbacks and keep legacy
spool concurrency bounds separate from campaign presentation counts.

This is not full feature-five readiness: modern continuation/reconciliation
and grade import explicitly refuse before historical writes instead of falling
through to legacy receipts. Campaign completion/continuation seals
and complete source/package/hosted qualification remain required. The preceding
integrated-head regression gate passed 1312 tests with 21 existing ignored and
zero failures before this fresh-sweep wiring; that evidence does not qualify the
new implementation by itself.

## Grouped recording-proof checkpoint

Campaign reports now validate every summary's execution/producer association and
every actual outcome's capture agreement before one shared payload-proof call.
Reporting-only summaries still do not manufacture capture receipts or authorize
continuation. The backend validates all reference metadata, then every selected
recording's immutable journal/catalogue metadata, before reading JSONL. Identical
references share one payload read; conflicting IDs/recording identities fail.

Published recordings use two bounded catalogue passes rather than a parse per
track. Only one archive descriptor is retained at a time, so recording count is
not capped at 256. The second pass must match the original publication receipt
and exact directory/manifest sizes. Receipt length checks and archive decoding
consume the same opened regular file, never different pathname refreshes. A
staged-to-published transition uses remaining global budgets; that race can read
a selected track from both sources. Missing published targets never fall back
to leftover staging, and no writer lock or recovery operation is acquired.

`ProofLimits` distinguishes whole-operation budgets from per-archive `Limits`.
Defaults permit 65,533 reference occurrences, 256 MiB each of encoded references
and journal/manifest metadata, 64 MiB of directory/end-record snapshots, and
2 GiB each of compressed recordings and distinct selected uncompressed bytes.
Reference encoding is counted without allocating encoded copies. Single proofs
preserve their previously separate journal, publication-marker and manifest
allowances. Directory snapshot bounds include the end record/comment and are
checked before allocating the snapshot. These are bounded local integrity
checks, not authentication or whole-archive checksum assertions.

Backend regressions cover 257 simultaneous recording references, exact and
one-byte-short aggregate budgets, duplicate/conflicting identities, late metadata
failure before earlier payloads, selected/unselected corruption, publication
cleanup races, disappeared targets, between-pass mutation, descriptor pathname
replacement, large reference paths and the original single-proof boundary.
Fresh-sweep revision `4499e6ac` completed its full offline suite with 1322 passed,
21 existing ignored and zero failures. Grouped-proof revision `3b10be01` passed
its complete offline suite with 1335 passed, 21 existing ignored and zero
failures. Newer main's artifact/raw-custody integration and reservation edits
require their own complete gates. Modern continuation/reconciliation and final
delivery qualification remain unfinished.

## Recording metadata reservation checkpoint

Fresh recording plans retain the real read-only task selection and a categorized
metadata reservation. Standalone runs use the existing planning catalog/selection
API; sweeps pass cached selections instead of reloading a catalog per task.
Reservations are recomputed from names, destination, recording identity and N/C.
A saved number that merely fits a quota is not trusted: layout validation
requires exact recomputation, and comparison binds the global sum to all retained
campaign plans and original selections before reading outcomes.

Actual compatibility, manifest and track-reference serialization share private
wire projections with a capacity-only shape. The shape has no observed digest
contents and cannot initialize a journal or pass replay compatibility. The pure
estimator counts bytes without allocating encoded copies, using maximum-width
statistics/false outcomes and present optional digest fields. Journal creation
reuses the same serializer-based reservation. ZIP32 directory/end-record widths
are shared with the bounded parser and tested against the pinned writer across
decimal-ID transitions. Payload lengths are not guessed from their maxima.

Each recording and the whole sweep must fit deterministic metadata budgets
before publishing campaigns, warming dependencies or launching clients. At the
genuine material gate, the recorder independently checks selected names/task and
recomputed capacity before journal creation/start. Only actual verified
compatibility is persisted; capacity is neither dataset/material provenance nor
completion/continuation authority. Historical absent fields remain unknown and
do not grant modern resume permission.

Producer association receipts have a separate runner-owned 128-byte allowance
per slot, not part of backend track-reference encoding. The complete sweep's
65,533-slot bound limits their aggregate to less than 8 MiB, within the existing
16 MiB allowance. Response-dependent payload/compressed lengths still use
runtime archive/proof bounds; metadata preflight cannot predict them.

Tests preserve byte-exact historical wire order/optional fields and escaped
logical strings; actual maximum-statistics journals/manifests/ZIPs/proofs fit the
reservation. Exact/one-byte-short quotas, overflow, invalid shapes and the full
pure ZIP32 population are covered without introducing a smaller cardinality cap.
Layout/index mutation matrices reject forged capacity. Genuine-gate tests reject
same-length wrong identities before journal creation and retain observed hashes.
Real CLI tests reject 60 configurations at N1000 with long model names before
any client/provider call or previous-result rotation; ordinary recording,
all-execution comparison and offline replay still pass.

## Ownership and shared contracts

The input-inspection checkpoint separates pinned source metadata from observed
payload identities. `Source::open` still performs both steps for ordinary
callers; the internal inspection owns the exact file used by the ZIP decoder,
not a pathname reopen after format probing. Legacy routing likewise retains
typed metadata without constructing empty-digest `ReplayInput` values. Each
observer checks all pinned lengths before its first payload hash. Regressions
cover late length changes, retained descriptors after pathname replacement and
the ZIP probe-to-decode replacement window. These are prerequisites, not a
claim that modern continuation is finished.

Sweep-wide admission now retains a typed runner `Inspection` for every selected
configuration before consuming any into publishable `Prepared` state. Each
configuration freezes selection from metadata; legacy hashes are observed only
after all inspections and aggregate costs pass, followed by a whole-population
pinned-length recheck. An inspection's incomplete internal draft is never
published or serialized as evidence, and its observing transition supplies real
digests before normal layout validation and encoding. Standalone runs consume
the same inspection path without a second implementation of selection.

Global costs independently bound selected inputs (65,533), private selected
bytes (2 GiB), actual retained descriptors (256), legacy bytes to hash (2 GiB),
directory snapshots (64 MiB) and manifest metadata (256 MiB). Numeric selection
charges all retained legacy files and their hash workload, not just the chosen
track. Conversely, one ZIP with more than 256 selected tracks consumes one
descriptor, not an artificial track-count quota. These are retained admission
budgets; constructing the next individually bounded inspection has transient
overhead before its cost is added. Metadata probing may read a format prefix,
ZIP indexes and manifests; it does not hash or decompress legacy JSONL payloads.

Actual sweep admission regressions use sparse files to exceed the global legacy
byte budget across individually valid configurations, 320 files to exceed the
descriptor budget, and an invalid later source. Both selection routes require
zero payload observations and unchanged previous index/report/history. A real
257-track bundle inspection and pure exact/overflow matrices separately qualify
track-versus-descriptor counting and every cost category.

This checkpoint passed the complete stable-source offline test suite (including
integration targets and doctests), strict all-target/all-feature Clippy, binary
build, formatting, Rust documentation and UML generation. It is not final
feature-five package/install/hosted qualification and does not enable resume.

Extract a reusable runner-owned campaign plan and bounded reader from the
standalone repeated coordinator. Sweep and compare consume that same contract;
they must not implement parallel copies of selection, isolation or validation.
An execution slot binds campaign/configuration/task identity, batch and original
replica index/concurrency. The actual attempt identity is allocated separately;
a recording track ID is an association, never a replacement execution identity.

The sweep owns its output lock, configuration scheduling, resource admission,
input seals and final comparison. A configuration campaign owns its fresh roots,
material/launch gates, recording journal, selection plan and attempt lifecycle.
Preserve the current consuming sweep phases and sticky presentation writer.
Keep archive operations above provider/proxy interfaces. Use the existing
validated outcome reader for grading, disposition, lineage and evidence checks.

Do not simply remove the repeated runner's legacy-observer/reuse rejections.
Introduce a campaign lifecycle sink with full slot identities and adapt existing
single-batch observers at the boundary. Original concurrency is never replaced
by repeat times concurrency to make old replica-shaped structures appear valid.

## Planning and activation

Persist all configuration execution slots before launching any sweep client.
The plan records repeat/concurrency, effective policy/admission/task selection,
source manifest or observed legacy payload identities, actual seeds, versioned
selection algorithms and exact per-slot track choices. Generate an omitted seed
once at sweep planning; all configuration choices are schedule-independent in
configuration/batch/replica order. Validate the whole bounded plan before moving
old results or admitting work. Decode only selected payloads into private inputs.
Bound total sweep slots, plan bytes, private selected bytes and outcome/proof
enumeration across all configurations, not merely each N times C population.
Reject predictable aggregate exhaustion before activation or paid work.

Plain live/legacy N1 sweeps retain their existing single-batch layout and behavior.
Explicit repeats and new ZIP record/replay use campaign layouts. Keep migration
tests for old saved plans, legacy recordings, imported grades and continuation.

Never fabricate a prepared material hash to initialize a recorder. A replay
source identity is an expectation; current material comes from the workload
adapter and is checked at admission. Save real prepared identities for later
batches/continuation. Recordings use one bundle per benchmark configuration.

Campaign roots must have an explicit owner/path beneath the configuration's
isolated result directory, rather than discovering an arbitrary newest global
standalone campaign. Standalone runs retain their existing project result root.
Scheduler count/resource weight is concurrent admitted work, not repeat times
concurrency. Batches remain sequential inside a configuration; do not introduce
nested pools or gates waiting on workers that cannot obtain held permits.

## Continuation and publication

Campaign input sealing now follows complete plan/private-input publication and
precedes client admission. Modern and legacy sweeps share declared project,
tool, interpreter, environment and executable/runtime discovery, but modern
roots come from the same exact index-to-campaign validator used by comparison.
They bind each campaign plan, canonical original configuration source and
private selected JSONL, with whole-population declared count/byte bounds. They
never include original replay archives, growing ZIPs/journals, raw captures or
attempt directories. Final sealing happens after joined settlement, including
ordinary benchmark failures. The campaign mode cannot feed its input checkpoint
to legacy replica/group observers.

Missing/invalid local key or unavailable/failed input seal remains an optional
continuation warning, not a fresh-grade gate. Tests preserve those faults and
all four grades/report rows without client retries; real CLI fixture recording
and provider-closed replay retain stable seals across publication and cleanup.
Input-root tests independently distinguish output/source-display changes from
private-input/configuration changes. This checkpoint includes current-main
runtime-discovery and collector custody changes via merge `15126726`; their
historical qualification is not substituted for new integrated gates.

These are input-stability observations, not completed-slot receipts or resume
authority. Resume must still verify private payloads against saved SHA/length,
positive reservations/admission, original starts/allocations, strict outcomes,
cohorts and immutable recording associations. No production authentication or
signing prerequisite is introduced.

Bundled replay plans also retain the original source `Compatibility`, copied
from the same inspected manifest as their frozen selection. This is expected
identity, not evidence that any workload was prepared. Batch material gates use
that saved identity. A publication regression drops the source owner and moves
the original archive before the first batch, then reloads the expected identity
and private inputs from disk. Historical plans without this optional field
remain inspectable; absence must never authorize pending bundled continuation.
Continuation still needs the input and slot checks above before using it.

The modern input-resume primitive now validates all campaign references and
aggregate selected-input costs, checks every private file's metadata, then
streams each payload against its original saved length and SHA-256. On Unix,
descriptor-relative opens reject intermediate and final symlinks. Verification
uses one payload descriptor and a fixed 64 KiB buffer; it neither decodes the
original archive nor creates receipts. Tests cover same-length corruption,
later metadata rejection, removed original sources and both alias levels.
This guards the gap between staging and initial sealing, but is not an atomic
filesystem snapshot or permission to resume: full slot admission remains pending.

Strict outcome validation now takes a borrowed configuration/selection context;
legacy entries adapt into that same context without cloning or changing their
wire records. The protocol, capture, cohort, identity, grade, lineage, cleanup
and evidence pipeline remains single and unchanged. A direct-context/legacy
matrix checks accepted true/false and native nullable grades, rejection of
incomplete or foreign evidence and identical diagnostics. Campaign-specific
execution/cohort/start/path authority must be composed with this validator,
not substituted for it or synthesized through a fake legacy entry.

Checkpoint qualification after merging main `fb4cce4d` into `70fe331a`:
formatting, strict all-target/all-feature Clippy, build, the complete offline
`cargo test --no-fail-fast` suite (including doctests), private Rust docs and
UML generation all passed. The full suite includes 1,221 passing library tests
with 13 explicit opt-in ignores and all 43 repeated-run/sweep regressions.
The actual executable SHA-256 remained unchanged across that full test run.
Independent read-only review approved each checkpoint and the mainline merge.
These are integrated foundation gates, not modern-resume, viewer, package,
hosted-PR or release qualification; those delivery obligations remain open.

The first current-main integrated suite retained one library failure: Linux
returned `ETXTBSY` when the native-gate argument/environment fixture executed its
new mock script; all other library and integration targets completed normally.
Another parallel fork/pre-exec fixture retaining the writable descriptor is a
credible explanation, not an experimentally traced historical writer. The
fixture now creates that executable in a dedicated exact-test subprocess and
waits for the creator to exit before the unchanged actual execution assertions.
Its parent never holds the writable descriptor. No retries, sleeps, production
launch policy changes or weakened routing assertions are used. The original
failed log remains separate from qualification of the repaired source.

The repaired integrated source passed its full offline suite, integration
targets and doctests, strict all-target/all-feature Clippy, formatting, binary
build, Rust documentation and UML generation. The invoking executable hash
remained unchanged across qualification; the fixture repair changes tests only.
This does not substitute for the remaining modern resume implementation or
final feature-five package/install/hosted delivery gates.

Independent continuation review identified a required distinction before modern
resume can be enabled. A published campaign plan is intended work, not positive
proof that a missing slot was never attempted. Initial publication must retain
an explicit untouched reservation, consumed by the slot's create-new start
record before workload allocation. Saved starts, allocations, outcomes and
runtime/workspace observations are checked together; absence of a completion
record must never become automatic retry permission. Complete negative grades
remain completed observations. Missing or uncertain settlement requires explicit
owned-resource recovery, not a fresh verifier/client invocation.

Recording recovery also has a terminal boundary. An interrupted, unfrozen
journal and a frozen/published archive are different states. The latter cannot
accept remaining planned slots: final no-overwrite publication and stable track
IDs prohibit reopening it as a new recorder. Publication-only recovery must
launch zero clients. Continuing pending slots after terminal publication needs
an explicit immutable continuation-artifact design; until that is implemented,
the operation must refuse before historical writes and explain the retained
terminal state. Such refusal is not full modern-continuation acceptance.

Resume loads the original saved plan and verifies choices against the frozen
source identity. It never generates another seed or silently resamples a changed
catalogue. Validate all reuse candidates before publishing any replacement state.
Resolve omitted seed/selector by loading the saved plan first; explicitly supplied
values that disagree must fail rather than silently override historical choices.
False grades are completed observations, not permission to rerun an agent.
Started/unsettled attempts and unknown cleanup retain explicit recovery policy;
they must not be mistaken for untouched slots.

For partially completed batches, only pending participants join material/launch
gates, while each keeps its original slot/replica identity. Compare their genuine
material with the saved prepared identity; do not fake arrivals for reused peers.
Any changed resumed execution cohort/resource population must be disclosed rather
than claimed to be an uninterrupted concurrency measurement.
Persist actual admitted participant count, cohort and continuation identity as
well as configured C, so a resumed partial batch cannot masquerade as full C.

Bind outcomes to immutable track/association proofs, not a growing ZIP or raw
JSONL that confirmed publication can remove. Group proof validation by recording
and reuse a bounded constant number of metadata/index passes. Final bundle publication is separate
from per-slot completion: publication-only recovery must never relaunch clients.
Preserve interrupted/failed attempts and no-overwrite ownership. Any explicit new
attempt needs a distinct attempt association; never rewrite a committed track.

Keep three independent state planes: planned slot/recording intent, actual
attempt allocation/start/settlement, and final bundle publication. A journal
Started record reserves a slot after material verification; it does not prove
that an agent launched. Actual started-but-unsettled attempts require explicit
recovery, not an implicit new client. No automatic retry feature is introduced.
A deliberate new run uses a new campaign/recording while preserving old attempts;
it must not append replacement tracks that violate batch/replica ID mapping.
Input seals bind immutable declared replay identity and selected payloads, never
mutable recording outputs or raw captures removed after publication.

## Comparison

Read bounded campaign slots and exact immutable outcome references. Project each
attempt's task/configuration/batch/replica identity, measurements, grades,
operational failures, selected track/seed/algorithm and recording proof through
the shared validation boundary. Deduplicate a measurement and outcome for the
same actual attempt; neither is a second benchmark result.

Do not recursively discover arbitrary run.json files, select only the latest log
row, or collapse batches into one replica. Missing/planned/failed/unknown-grade
slots remain explicit. Incoming main's unsealed-log fallback stays reporting-only
and never authorizes continuation. Legacy layouts remain reader adapters.
Disclose live/replay mode, repeats, concurrency and cohorts. Repeated attempts do
not become an official independent-task population or conceal failed attempts.

## Acceptance matrix

- N1/C1 and N3/C2; multiple configurations/tasks with jobs 1/2 and count/resource
  budgets; fresh workspaces, preserved user data, exact all-slot result counts.
- Live, ZIP record/replay, legacy JSONL and sparse concurrency layouts. Shared
  seed/selector options, fixed choices across changed scheduling and resume.
- Interrupt after plan, allocation/start, commit, batch completion, ZIP linking
  and cleanup. Recover publication without clients; retain uncertain resources.
- Reuse validated completions without new provider calls or agent/evaluator
  launches; continue untouched slots with saved choices; reject source, outcome,
  receipt, policy, capture, disposition and sealed-output changes before writes.
- Partial reuse with noncontiguous original replica IDs, no fabricated gate
  arrivals, no deadlock, no completed-failure retry or duplicate track commit.
- Mixed measured/unmeasured reports enumerate every execution exactly once,
  preserve unknown fields/failures and avoid biased task/attempt denominators.
- Existing legacy resume, reconciliation, grade import, output-failure and
  resource-admission suites continue passing. No auth/signature/key prerequisite.
- Independent review plus fmt, strict Clippy, full offline source/extracted
  tests, package/install, build, documentation/diagrams and exact-head hosted CI.
  Deliver in a separate PR and publish a patch release after verified merge.
