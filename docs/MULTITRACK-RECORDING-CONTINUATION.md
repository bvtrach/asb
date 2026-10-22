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
