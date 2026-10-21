# Multi-track sweep, continuation and reporting integration

Status: design checkpoint for delivery feature 5, not an availability claim.
The preceding run integration is PR #294; this branch must retain its validated
record/replay and legacy behavior. Feature 6 remains the offline track viewer.

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
False grades are completed observations, not permission to rerun an agent.
Started/unsettled attempts and unknown cleanup retain explicit recovery policy;
they must not be mistaken for untouched slots.

For partially completed batches, only pending participants join material/launch
gates, while each keeps its original slot/replica identity. Compare their genuine
material with the saved prepared identity; do not fake arrivals for reused peers.
Any changed resumed execution cohort/resource population must be disclosed rather
than claimed to be an uninterrupted concurrency measurement.

Bind outcomes to immutable track/association proofs, not a growing ZIP or raw
JSONL that confirmed publication can remove. Group proof validation by recording
and reuse one bounded metadata/index pass. Final bundle publication is separate
from per-slot completion: publication-only recovery must never relaunch clients.
Preserve interrupted/failed attempts and no-overwrite ownership. Any explicit new
attempt needs a distinct attempt association; never rewrite a committed track.

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
