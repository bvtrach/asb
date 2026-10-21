# Repeated execution preparation

This is an integration design and implementation ledger. The branch exposes a
prototype `run --repeat N` for native live/legacy JSONL replay, not the complete
bundle workflow or release readiness. Repeated recording explicitly reports its
pending integration. The complete contract remains in MULTITRACK.md.

## Implemented internal preparation foundation

The runner can request a cancellable material-ready gate. Every worker registers
before fallible setup, retains its workload session on its own thread, and
submits only an immutable compatibility identity after workload preparation and
before tape startup or agent execution. All slots must match before any release.
An earlier batch's expected identity can be supplied to the same gate contract.
Single-slot batches use the same validation. Expired gates cannot admit even
otherwise matching late arrivals. Early errors and unwinding notify waiting peers.
Identity failures abort peers before potentially lengthy adapter cancellation;
preservation failures remain in the returned error rather than being lost in Drop.

File sessions capture a bounded canonical tree only when requested. The digest
binds sorted relative UTF-8 names, entry kinds, empty directories, executable
bits and exact file bytes, not absolute paths or timestamps. Unsupported links
and special files are errors for verified execution, not files to follow. The
current limits are 100,000 entries, depth 128, 16 MiB aggregate relative-name
bytes and 2 GiB contents. Pending names are charged before retention; payloads
are streamed. The digest is captured before baseline evaluation, then checked
again afterward. An evaluator that modifies modeled material makes its identity
unavailable rather than silently redefining the starting state. Later agent
edits never change the session's saved initial identity. Legacy runs do not scan
their existing workspace and retain their existing evaluation semantics.

These are local consistency checks, not authentication or historical attestation.

## Official prepared-material protocol foundation

Verified preparation requests `initial-material` from the workload controller
after readiness and before agent/proxy startup. Legacy execution never sends
that operation. The response carries only a versioned scope and SHA-256 digest;
Rust rejects unknown fields, unsupported scopes and malformed digests. A failed
capture takes the existing stopped-preservation cancellation path.

The three original SDK adapter implementations share an owned-container capture
helper. It verifies the active resource journal, selected daemon and exact
declared container before reading material. The current implementation covers a
single non-root work directory with no mounts and no changes outside that root,
apart from explicitly enumerated Docker runtime files/pseudo-filesystems. Root
workspaces, changed ancestors, mounted inputs and additional services are refused;
this foundation is not full original-environment support.

Within that coverage, identity binds image filesystem layers and declared runtime
configuration, adapter task/dataset/SDK definitions and actual prepared file
content. Container IDs, ownership labels, image aliases and timestamps are not
material identity. A bounded raw TAR walker validates extension lengths before
reading payloads, binds modes/ownership, directories, symlinks and hardlink
topology, and never extracts files. Cancellation, transport closure and aggregate
metadata/content limits apply even to malformed archives.

SWE-bench additionally verifies a clean selected base and binds semantic Git
index entries/flags and reflog ref/commit/action/actor state before and after
capture. Only raw index timestamp caches and raw reflog timestamps are normalized;
configuration, hooks, attributes and other Git files remain content-bound. The
ASB-owned reset uses a stable per-command committer identity, not a global Git
configuration change.

## Implemented native repeated execution

Explicit repetition, including `--repeat 1`, allocates a new project-local
campaign; omission preserves legacy paths. The campaign publishes a bounded
intended-slot plan before preparation. Slots identify campaign, batch, replica
and global slot independently of dispatch cohorts and actual workload attempts.
Each batch rebuilds its adapter material into new workspaces and owns separate
logs, runtime paths and measurements. Batches run sequentially with the configured
concurrency inside each batch. The confirmed first material identity becomes the
reference for later batches. Immutable prepared records publish before admission.
Start intents and official allocation references remain distinct from outcomes.

Legacy replay resolves every replica's selected source before preparation,
deduplicates and hashes bounded sources, and verifies private copies against the
plan before any agent starts. Later source edits cannot change a campaign's
replay inputs. Sparse concurrent recordings through slot 64 retain compatibility.
Measurement overrides select a separate measurement root, never move workspaces
or logs out of the project, and remain explicit in the plan. Planned measurement
paths are relative to that root; workspace/log paths are relative to the campaign.

Every completed attempt links its private outcome by exact bytes and digest.
Ordinary completed failures retain their grades and allow subsequent batches,
with overall failure returned at the end. Missing outcomes, preparation failure,
unconfirmed workload/proxy cleanup or output/publication failure stop admission.
Current workers are joined before returning. Bounded original error diagnostics
are retained in per-batch finished records and returned to the user. These are
not sweep resume receipts or permission to retry started attempts.

## Remaining integration

- Official workload identity must bind adapter-owned environment material. Image
  ownership labels and attempt tags differ per execution, so raw source-build
  image IDs cannot by themselves represent stable material equality. Stable
  definition/content identity must remain distinct from actual-image provenance.
  Extend the protocol foundation to root workspaces, declared mounts, multiple
  services and changes outside the work directory, then qualify real installed
  SDK environments. Do not replace missing coverage with definition-only hashes.
- Integrate durable recording journals and saved bundle replay selections with
  the execution campaign, preserving diagnostics before a compatibility anchor
  exists. Do not fabricate fingerprints or completed attempts after failed setup.
- Update sweep reservation/receipt/resume and comparison to enumerate every slot;
  repeated attempts must not silently become official single-attempt scores.
- Complete repeat/bundle CLI help, documentation and integration qualification
  before marking this feature/release ready.

## Current test coverage

Gate tests cover delayed siblings, early return, panic, identity mismatch,
duplicate admission, bounded timeout, expired single/multi-slot arrivals and
earlier-batch reference mismatch. Runner fixtures prove unsupported single-slot
material and differing multi-slot material fail at the preparation gate, before
proxy/agent startup. Another fixture proves peer notification precedes slow
cancellation and preserves its cleanup error. Material tests cover root/order
independence, names/content/empty directories/executable bits, resource bounds,
symlink refusal, evaluator side effects and immutable pre-agent identity.

All fixtures are provider-free. These tests do not yet qualify fresh repeated
official environments, bundled CLI replay or sweep resume. `repeated_run` uses
the actual CLI, fake clients and loopback replay to qualify native exact counts,
within-batch overlap, sequential batches, fresh fixtures/runtime paths, preserved
user material, failure continuation, initial preparation diagnostics, cross-batch
prompt mismatch, frozen replay inputs, sparse legacy mapping, measurement-only
relocation and closed-output stop/join. Internal tests retain outcomes on cleanup
failure while stopping admission, and cover publication failure/timeout races.
`benchmark_material.py` supplies Docker/provider-free archive and SDK seams,
including oversized extension rejection before payload allocation, hardlink
metadata consistency, semantic Git normalization and explicit coverage refusal.
Task-server and Rust controller tests check on-demand dispatch, strict response
validation, bounded known diagnostics and redaction of arbitrary SDK errors.
