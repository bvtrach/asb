# Successive continuation generations

Status: independently reviewed design for the remaining feature-5 work, not an
implemented or qualified capability. The current single-generation path must
remain safe until this entire change and its public fault matrix are complete.

## Explicit append-only authority

Keep the existing `.checkpoint/campaign-continuation` admission as the first
node. Each admitted node can acquire one create-new `next.json` reference to a
fixed derived successor directory. The link binds the predecessor admission,
successor ordinal, generation ID, relative location and exact admission blob.
Successor admission includes a versioned predecessor identity and ordinal.
The final link is outside the child's admission, avoiding a digest cycle.

Publish the successor directory, all configuration manifests and imports, then
the child admission. Sync them before publishing/syncing the predecessor link
LAST; return worker capabilities only afterward. An existing successor stage
without its final link is retained metadata requiring explicit recovery, never
permission to mint another random generation. Exact completed publication can
be linked by a narrowly qualified zero-client metadata-recovery operation.

Follow only explicit validated links. No directory-recency selection, implicit
replacement, scanning for the newest ID or modification of existing admissions.
Bound aggregate chain metadata, configuration/slot populations and traversal
before payload reads or effects; reject cycles, aliases and foreign stages.

## Shared catalog and historical evidence

The passive catalog owns all validated nodes and an ID index. Every historical
`StartReference` resolves against its ORIGINAL admission, manifest, prepared
material, dispatch cohort and recording descriptor. Never reinterpret it against
the head or rewrite old receipt roots. A missing generation reference is allowed
only for established original-fresh membership, not merely because the head now
lists that slot completed.

For each successor, completed membership must grow monotonically and pending
membership shrink strictly. Inherited receipt blobs remain exact; newly completed
slots must have belonged to predecessor pending membership. Classification still
proves actual completion receipts and positively untouched residual slots under
all original owners. Started-but-unsealed attempts remain recovery-only. An empty
duplicate generation is not progress and must not mint execution authority.

## Recording publication

Keep historical root `recording-current.json` immutable. Successor nodes use
their generation-specific immutable publication receipt/reference; the validated
admission chain identifies the current node. Default replay requires that node's
confirmed union when its configuration declares one. Missing publication must
require recovery, never silently select an older union.

Build a new self-contained physical ZIP from exact completed-track evidence
across all ancestors plus genuinely new local captures. Preserve original slot
IDs, full Track metadata, nullable facts and all ancestor archives/associations.
Do not create new producer or grade claims for imports. Historical reports and
strict settlement use each track's own admitted node and original proof.

## Actual public qualification

Use C4/N3 fixtures for recording and saved seeded replay. Interrupt after the
first batch, resume and interrupt after the second, then resume the third batch.
Drain reused-first-batch output while waiting for all four actual second-batch
clients at a fixture barrier; closing a full undrained pipe too early would not
prove a second execution interruption. Expected actual attempt counts are
4 -> 8 -> 12 -> 12, including a final completed-only resume.

Preserve original IDs/seed/private replay choices, inherited false grades,
measurements, receipts, original index and input seal bytes. Validate each
generation's actual start/cohort/prepared binding and the final 12-track union;
all historical unions remain readable. Negative cases cover changed ancestor
admissions/links/receipts, orphan successors, missing head publication, partial
captures and late-configuration failures with zero extra clients and no rewritten
evidence. Extend the actual fixtures separately from passive metadata tests;
synthetic receipt construction is not execution qualification.

Feature delivery additionally requires full source, documentation, packaging,
extracted-install, hosted exact-head and relevant original-SDK gates. Coverage
remains an optional diagnostic under DEVELOPMENT.md, not a merge threshold.
