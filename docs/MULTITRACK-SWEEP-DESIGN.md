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

## Ownership and shared contracts

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
and reuse one bounded metadata/index pass. Final bundle publication is separate
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
