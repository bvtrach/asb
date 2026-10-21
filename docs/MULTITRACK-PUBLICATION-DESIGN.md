# Durable recording publication implementation design

Status: the recording journal is implemented and independently reviewed, but
this feature is not merged or released. Final ZIP publication/cleanup remains
pending. The reusable reader/selector foundation shipped through PR #247 in
v0.2.1 after exact-head PR and both post-merge CI jobs passed.

This branch extracts persistent-inode output locking so cassette and sweep
writers share the same no-follow, regular-file, explicit-unlock contract. Its
four existing ownership/process-death tests passed. Seven journal tests passed,
including an 18-case atomic-publication failure matrix. All 23 cassette tests
and strict all-target/all-feature Clippy passed. The independent reviewer
approved only the implemented journal scope, not the unfinished ZIP finalizer.

## Implemented journal decisions

`RecordingJournal` publishes bounded immutable intent, start and commit records;
out-of-order completions reconstruct in stable ID order. Interrupted copies stay
incomplete with unknown outcomes, and never-started slots are not fabricated.
Recovery checks aggregate metadata and declared payload sizes before payload
reads; actual hashing consumes a remaining aggregate budget. Staging uses
conservative incremental byte accounting instead of rehashing all older tracks
on every commit. Explicit `snapshot()` verifies retained payloads; callers should
not invoke it after every event merely for progress counters.

An ambiguous metadata publication poisons the owner before the operation begins.
After any publication error it refuses snapshots and further starts until dropped
and explicitly recovered. This prevents a successful link followed by sync or
cleanup failure from omitting a real start or downgrading a committed track.
Test-only fault points have no environment/configuration surface in normal builds.

The next implementation work is the final ZIP builder, no-clobber publication,
durability-uncertainty reporting, explicit recovery after final publication and
conservative cleanup. These need their own fault matrix and receipt-evidence
contract before the publication feature PR. Runtime integration, repeated batches,
sweep/checkpoint/report changes and the viewer remain separate later features.

## Durable state

Use a private, destination-owned partial directory and a persistent lock file
outside it. Never unlink the lock inode. Both recording and explicit recovery
acquire the same lock; owner death releases it through the OS. Domain callers
choose paths and diagnostics, the shared lock guard owns safe open/lock/drop.

Publish recording intent before accepting execution starts: schema, recording ID,
compatibility, destination identity, concurrency and bounded expected slots.
Persist an immutable started-slot record before launching that slot. Never-started
slots stay distinguishable from real interrupted attempts. Track IDs derive from
planned batch/replica slots and publication accepts out-of-order completions.

For each started track, stage bounded private JSONL, sync it, validate exact
length/digest, then atomically publish an immutable committed-track record. Write
metadata privately and sync before create-new hard-link publication and directory
sync; a torn final JSON file must not count as a commit. Do not overwrite duplicate
commits or silently retry an already-started attempt. Unknown/uncommitted files
remain diagnostics, never automatic completion evidence.

Explicit recovery validates bounded intent/start/commit metadata and referenced
payload bytes under the lock. It reconstructs a sorted manifest, including
interrupted started attempts, without launching clients, providers or evaluators.
No production signing/authentication/key-management prerequisite is introduced.

## Final archive

Build the complete ZIP privately in the same filesystem. Finalize and sync it,
open it through the cassette reader and verify every published payload, not just
the manifest. Publish with no-clobber hard-link semantics, then sync the parent.
Refuse preexisting and racing destinations without replacing them.

If final linking succeeded but sync/status publication fails, report that the
destination exists with durability unconfirmed; retain both names and evidence.
Explicit recovery may verify an exact matching published archive and complete
publication/cleanup. It must not treat an unrelated existing file as success.

Delete only positively-owned temporary uncompressed payloads after publication
confirmation. Retain intent and immutable track metadata for diagnosis. A later
cleanup failure must disclose partial cleanup; it must not remove published data.
Unknown files and replaced symlink/FIFO paths must never be followed or removed.

## Checkpoint integration constraint

Per-execution receipts cannot hash a growing shared ZIP, nor seal payload paths
that finalization deletes. Introduce immutable track evidence verification bound
to commit metadata plus exact archive-entry digest; verify staged bytes before
publication and the final entry afterward. Final group evidence additionally
binds the actual finalized archive. Replay input seals bind the source archive,
manifest and persisted selection—not transient extraction paths.

## Required qualification

- Competing and unrelated writers, abrupt owner death, inherited descriptors.
- Out-of-order completion, duplicate commit refusal, planned/started accounting.
- Fault injection around every file sync, metadata link, archive link, directory
  sync and cleanup; disk-full/short-write/sync errors retain partial evidence.
- Missing/tampered payloads and torn private metadata; unknown files, symlink and
  FIFO substitutions; bounded directory enumeration and byte/track limits.
- Failed-but-complete and interrupted-incomplete attempts in a readable archive.
- Existing/racing destinations and recovery between publication and cleanup.
- Repeated recovery is unchanged and launches no agent or evaluator.
- Receipt verification survives legitimate payload cleanup and rejects tampering.
- Existing sweep output locking and its process-death tests remain unchanged in
  semantics; full source/package suites and strict repository gates precede PR.
