# Recording continuation implementation contract

This is the remaining recording integration design, not a delivery claim.
Replay/live continuation and shared batch dispatch do not authorize reopening
an original recording journal. The complete requirements remain in MULTITRACK.md.

## Identity and ownership

Keep `TrackEvidence.recording_id` as the physical immutable journal/archive
identity. Preserve the grouped-proof invariant that one physical ID maps to one
destination. Each continuation allocates a new physical ID and create-new
destination; the logical campaign and original batch/replica/slot IDs do not
change. Old archives, associations, outcomes and completion receipts stay intact.

Add an optional versioned runner-owned recording-generation binding to recording
evidence. Absent bindings retain the exact legacy campaign-root/name/recording-ID
checks. Present bindings must resolve through the exact admitted configuration
manifest and global admission, proving original campaign/root, slot, physical
ID/destination, producer and metadata byte bindings. A bare self-consistent
string or arbitrary destination is not an exception to the legacy policy.
The immutable producer association hashes this complete evidence including its
generation binding. Archive code must not depend on runner JSON field names.

The all-configuration admission descriptor freezes the physical destination,
logical binding, verified original receipt references and complete union/import
plan. Reserve archive, metadata, provenance, association and imported-byte costs
globally before starting any client. Retain the parent input/campaign guard
through worker settlement, union publication, new slot sealing and comparison.

## Self-contained immutable union

One owned union recorder owns a new journal and imports positively verified old
tracks verbatim: stable ID, batch, replica, bytes/SHA, completeness, outcomes and
nullable statistics. Use grouped original proofs and one bounded source index
per archive/import phase, not a per-track archive reopen or quadratic scan.

The backend import operation may share bounded payload staging/hashing with
ordinary journal commits, but must not call the runner's actual-attempt commit
API or invent a new RunResult, capture receipt, attempt or grade. Journal slot
intent is not evidence that another client was launched. Persist a bounded
generic versioned provenance section/entry inside the new archive, distinguishing
historical import, actual new capture and never-started placeholder origins.
Recovery validates original import origins, while standalone replay needs no
ancestor archive or external provenance ledger to obtain its payloads.

Only positively untouched pending slots may replace old never-started
placeholders. An observed, unsealed or uncertain prior attempt is recovery-only,
not permission to overwrite it. New pending members use the shared actual batch
executor and normal capture finalization, with a validated logical/physical
binding. The union contains every original slot's payload; it must not reference
external track files or append to an old frozen ZIP.

Ordinary completed client/grade failures still finalize the union. Uncertain
producer cleanup retains raw evidence and cannot manufacture a settled receipt.
Old slot seals continue proving the old physical archive; new slot seals prove
the new union. The union's physical identity avoids weakening the existing
same-ID/destination checks.

## Publication and no-client recovery

Reuse existing journal freeze, prepared-archive, no-clobber link, fsync and
publication confirmation contracts. After confirmed ZIP publication, publish an
immutable generation publication receipt containing the physical ID/destination,
exact archive/manifest identity and import map, then publish the explicit current
union ledger last. Default replay/report discovery follows that ledger, never a
newest-filename scan or retargeted link. No old destination is overwritten.

Recovery is a separate consuming metadata/publication operation. It validates
the original global/configuration admission and frozen union/import plan,
acquires the exact new journal owner and either publishes the exact prepared
archive or confirms its already-linked target. It finishes missing publication
and ledger records idempotently. It never calls agent/provider/model preparation,
workload allocation or evaluation and never reruns a slot. A disappeared
published target is not silently recreated. Foreign targets, conflicting
ledgers, changed origins and uncertain custody retain evidence and report a
specific recovery error. No signing service, authentication chain or production
key-management installation is required.

## Required qualification

- Public record C4/N2 interrupted after the genuinely settled first batch:
  four original receipts and four untouched slots, including a retained false
  grade. Resume starts only four new clients and preserves the final failure.
- The new ZIP contains all eight independently replayable tracks with exact old
  bytes/IDs/statistics; original ZIP/associations/receipts stay byte-identical.
  Shut down the provider and replay imported and new tracks from the union.
- Preserve incomplete/unknown observations and original noncontiguous IDs in
  dedicated subset fixtures without manufacturing prior public receipts.
- Reject late corrupt origins/configurations and aggregate capacity excess
  before any paid work or generation activation.
- Inject import, commit, freeze, archive write/sync/link/directory-sync,
  publication-receipt and final-ledger failures. Recovery starts zero clients,
  confirms only the exact expected target, and refuses foreign/missing published
  targets. Retain original failures and positively owned partial evidence.

Complete these gates before declaring sweep/report/resume delivery complete or
publishing its feature release. The offline track viewer remains a separate
feature and patch-release boundary.

## Backend checkpoint: immutable origin plans and artifact imports

The cassette backend now carries an optional version-one `OriginPlan` in the
manifest. The outer ZIP schema stays at version one; manifests without origins
retain their previous serialized field order and omit the new field entirely.
This is backward reading support, not a claim that older readers accept new
origin-bearing manifests. A plan freezes the original population, canonical
physical source commitments and exact imported full-track metadata/byte lengths.
The complement is planned local work, not evidence that any client launched.
Standalone archive replay needs only the union's own payload entries.

`RecordingJournal::import_track` records typed artifact-copy intent and uses the
same bounded staging, checksum and immutable commit publisher as ordinary local
captures. It does not run an agent, emit a capture receipt or change old grades.
Ordinary begin/commit cannot consume imported slots. Explicit `resume_import`
can finish an intent-only copy or publish a missing commit for an already exact
payload. The latter hashes and syncs the same descriptor before commit
publication. Partial, different or duplicate committed payloads are never
overwritten. Finalization requires every reserved import, and recovery/read-only
proofs check typed intent and the population before selected payload reads.

Qualification so far: the initial existing cassette suite passed 69 tests; the
expanded suite passed 80 tests in `target/recording-origin-cassette-final.log`.
Eleven new origin tests cover self-contained imports from a real backend source
archive, failed/unknown observations, no-read metadata rejection, intent and
commit-publication faults, explicit copy recovery, retained partial bytes,
recovery-sync failure, provenance drift, capacity/identity failures, custom-limit
hashing above 16 MiB, and omitted legacy wire fields. Independent review found
and verified repairs for read-only population validation, default-limit hashing
and the recovered-payload sync ordering. These are provider-free backend tests,
not public recording-continuation qualification. Runtime logical/physical
binding, all-configuration union coordination, publication ledger and explicit
public recovery remain required; no feature merge/release is claimed here.

The subsequent `recording-origin-full.log` run passed all source tests: 1,328
library tests with 13 explicit opt-ins ignored, all integration suites including
50 repeated-run/sweep tests, and doc tests. Strict all-target/all-feature Clippy,
build, formatting, diff checks, Rust documentation, UML, flow and interactive
diagram generation also passed. Qualification used nonincremental compilation
with local development/test debuginfo disabled to conserve disk; no tracked
profiles or CI gates were relaxed. The tested ASB binary SHA-256 was
`4354d7cef819be5524a0c78722275315342f6a8c6bed976acb9b0974187a7d1c`.
The older frozen qualification executable remained unchanged. Diagram generation
retained all 5,090 functions and 12,970 edges, with Graphviz's nonfatal straight-
edge fallback. Packaging/extracted-install and exact-head hosted CI are still
feature-delivery gates, not established by this backend checkpoint.

## Runtime integration checkpoint (not feature delivery)

Recording continuation now freezes a per-configuration union descriptor inside
the all-configuration generation manifest. Its configuration-qualified physical
ID and destination are distinct from the original logical campaign. The ZIP and
journal live beside the original stable recording destination, outside rotating
sweep results. Historical sources are grouped/indexed once per import phase;
their exact full metadata and payloads are imported without new producer
associations, capture receipts or grades. All configurations' imports finish
under the retained campaign owners before the global admission is published.
No verified old material means no initial journal: genuine material preparation
still initializes it, with the same selection/capacity checks as fresh recording.
Actual dispatch membership alone creates new local intents. Imported IDs bypass
local raw fallback and raw cleanup.

New recording evidence carries an optional generation binding and optional
backend `origin_sha256` commitment. Legacy absent fields stay omitted on wire.
The grouped backend proof checks the actual journal origin against that expected
commitment; the owning catalog independently binds it to the frozen descriptor,
actual original start/cohort, physical destination and logical slot. Strict
settlement checks this before payload proofs. Reporting checks the actual chosen
summary on every source route, including measurement-only fallback. Standalone
campaign reports locate the owning retained sweep by its fixed layout/index,
not a locator supplied by the recording, including under preserved history.

Confirmed backend publication provides its exact prepared compressed length and
SHA. An immutable generation publication receipt is written before an explicit
`recording-current.json` reference. Default replay follows that reference through
the validated catalog, not filename recency. Its archive reader hashes compressed
bytes and inspects the manifest on one pinned descriptor; it does not inflate
unselected tracks. Matching retained publication metadata is re-synced, not merely
accepted by byte equality. Recovery preflights all exact publication/ledger plans
before advancing any pointer. Backend archive confirmations remain independently
monotonic; this is not an atomic cross-configuration rollback transaction.

An admitted recording union without a current reference requires explicit
publication recovery before default replay. It must never silently fall back to
the older archive and omit new tracks. This check precedes clients, result
rotation and replacement reports. Explicit user-selected cassette sources remain
an intentional separate choice.

Validated immutable journal and catalog owners derive their private origin
commitments once; no cached commitment is serialized or accepted as authority.
Read-only grouped proofs still derive the commitment from actual retained intent.
Capacity is recomputed once at successful material admission, while every batch
still checks logical names, full compatibility and actual dispatch membership.
Thus checking each slot does not repeatedly serialize the complete import plan.

`asb sweep SWEEP --resume --record --recover-recording` only repairs existing
frozen publications and their receipt/ledger. It runs before execution planning
or provider preparation, returns reports rather than a worker capability, and
starts no agents or evaluators. It binds the original input identity, catalog and
receipts, but does not require the departed controller's procfs/runtime or current
provider credentials to match. File-backed metadata is checked before/after;
ordinary execution resume retains its original stricter environment check.
This operation needs a usable Python for metadata hashing. Original benchmark
retained-outcome validation still uses its installed SDK's read-only checker;
there is no claim that those SDK files may be removed. Missing/foreign published
archives and conflicting metadata are retained errors, never permission to
recreate a published target or retry an uncertain attempt.

Focused qualification: 52 repeated-run/sweep tests and 80 cassette tests passed.
The public C4/N2 fixtures use genuine first-batch receipts, retain a false grade,
launch only four pending clients, and start none on a second resume. They compare
all old track metadata/payloads and old receipts/associations byte-for-byte, replay
imported/new tracks with the provider stopped and ancestor ZIP unavailable,
follow the union by default and compare preserved history after result rotation.
A real client-created ledger blocker exercises credential-free publication-only
recovery twice with zero new clients/evaluations. A measurement-only report must
reject an altered actual generation start while retaining the previous report.
Primitive faults cover exact archive corruption, after-link publication/ledger
failures, mandatory retained-file/directory re-sync and late metadata conflicts.
These remain synthetic-provider/local-workload qualifications, not live LLM or
original-SDK continuation claims. Full source/packaging/hosted gates, additional
fault/SDK cases and repeated continuation-generation lineage remain required.
The offline viewer has not been implemented by this checkpoint.

The pre-cache runtime full-source checkpoint (`recording-union-full.log`) passed
1,331 library tests plus all integration tests and doctests, with 13 library
tests explicitly ignored. Strict all-target/all-feature Clippy and build passed.
Subsequent focused tests prove one origin hash per journal create/recovery,
fresh on-disk grouped-proof validation, one capacity check across successful
batches with later identity/material/member rejection, and missing-ledger replay
refusal without clients or changes to the saved index/report/history. These
checks do not establish hosted CI, packaging, repeated-generation lineage or
official-SDK continuation readiness.

The subsequent cache/runtime checkpoint (`recording-union-cache-full.log`)
passed all 1,333 library tests, all integration suites (including 52 repeated
run/sweep tests) and doctests. Thirteen library tests remain explicitly ignored.
Strict Clippy, offline build, formatting, seven checkout-update tests, Rust
documentation, UML and interactive dataflow/control-flow generation also passed.
The independent read-only reviewer found no defect in the private-cache and
missing-ledger changes. The append-only successive-generation design is recorded
separately in [continuation lineage](MULTITRACK-CONTINUATION-LINEAGE.md); it is
not implemented by this checkpoint and feature 5 is not merge/release ready.
