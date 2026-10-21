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

### Broader coverage design constraints

The raw reader can capture only a directory's root metadata and close before
reading descendants, including the Docker archive root named `/`. This is a
building block, not complete root-filesystem coverage. Change inventories are
bounded, canonicalized and retain deletion tombstones independently of parent
directory changes. Duplicate, noncanonical and invalid paths are errors.

A per-path archive experiment was rejected: separate archives lose hardlink
relationships across paths, and a representative original preparation exposed
over 2,000 changed paths, making one request per path unsuitable. Do not wire a
set of independent file digests into the environment-identity protocol. Broader
coverage needs grouped content plus canonical cross-root inode relationships,
directory metadata and explicit deletion facts. Shared mounts, service roles
and logical network topology must remain bound; copied content is not equivalent
to shared writable storage. A before/after inventory check is not by itself a
snapshot of concurrently changing services or external writers.

The task-owned Docker client uses a raw-response stream that retains its read
timeout for container archive endpoints and closes the actual HTTP response on
early close, completion and read failure. Nonarchive TTY/log/attach streams
retain the SDK's long-running stream semantics. Closing only the SDK's default
raw generator is insufficient for bounded archive capture: that path
also disables socket timeouts. The override is scoped to the owned client, not
a global SDK patch. Root-header inspection must use this owned stream lifecycle.
Construction errors close the response too, and transport-child cleanup runs
even if the SDK client close fails.

### Declared-root guest census component

The private Linux `__asb-material` helper now accepts a bounded JSON declaration
over stdin and returns only an observation digest, entry/byte counts and explicit
coverage flags. It starts no provider, agent or evaluator. This is not the
`prepared-workload-material-v1` response and must not replace that protocol's
complete environment declaration. Version three additionally binds visible Linux
attributes, including visible ACL/capability bytes; it does not claim complete
privileged attribute enumeration, namespace coverage or an atomic snapshot.

Traversal pins roots and children with descriptor-relative `O_PATH|O_NOFOLLOW`.
Only already verified regular-file descriptors are reopened for reading through
the private proc descriptor reference; symlink targets use `readlinkat` on the
pinned symlink. Devices, FIFOs and other unsupported nodes fail without reading
their payloads. Files/directories are checked before and after observation;
root path replacement is additionally checked. Records bind logical names,
modes/ownership, observed file content and symlink targets, not physical root paths
or raw timestamps. Temporary numeric inode/device pairs only establish observed
equivalence groups; canonical logical representatives enter the hash. Conflicting
content, metadata or timestamp/link-count versions for the same inode are errors.
Link counts are not treated as proof of namespace-wide alias completeness.

Each root normally reads all regular-file payloads. An optional `content_paths`
list instead selects exact relative entries (not subtrees), and requires a
canonical `image_sha256` anchor. The helper binds both values but cannot validate
that the caller supplied the actual image or a complete change inventory. An
outer adapter must do so before this observation can contribute to environment
equivalence. Every non-excluded entry still contributes names, metadata, visible
attributes and inode relationships. Selected directories contribute metadata;
selected symlinks contribute their own targets, never target file bytes.
Missing or excluded selected paths are errors. Selectors are sorted/deduplicated.

Payload selection closes over observed cross-root inode groups. A full-content
root can supply a selected alias's already-read payload; otherwise a selected
group is read once after traversal and its digest propagated to all aliases.
Deferred reads re-resolve without following symlinks and reject changed inode,
version, size, ownership or mode before reading. This is not an atomic snapshot.
Selective output explicitly sets `file_content=false` and
`image_anchored_selection=true`; unread bytes must not be described as measured.
Protocol scope/domain are `declared-root-material-observation-v3` and v3.

One invocation budgets 100,000 entries, 16 MiB retained/pending names, 2 GiB read
content and depth 128 across all declared roots. The 60-second observation deadline
is cooperative between operations, not a hard syscall/stdin timeout: an owning
controller must enforce cancellation and a wall-time watchdog. The opt-in live
fixture uses a 90-second owned `docker exec` subprocess deadline. Terminating that
CLI is not proof that its guest exec stopped: a timeout must also take the
workload's exact-owner stopped-preservation/cancellation path, with cleanup
confirmation or an explicit failure. Cross-root
equivalence does not by itself model shared mount topology or external writers.
Do not stop all task processes or pause a container and assume its in-container
collector can still execute; broader preparation quiescence remains adapter-owned.

Remaining wiring must compose visible xattrs/ACLs, grouped root/delta content, mounts and
namespace-visible aliases, service roles and logical networks, validate stable
before/after observations, and exclude only positively owned staging material.
The current helper is qualified as a reusable component, not complete official
root/mount/multiservice support or an atomic machine snapshot.

The Linux attribute helper uses descriptor-based reads for files/directories
and retained-parent, no-follow reads for symlinks, checking the current directory
entry against its pinned inode around every call. Two actual name/value passes
must agree, and attribute digests participate in inode-equivalence consistency.
Names and values are byte strings; only their canonical digest enters records.
Lists/values are bounded at 64 KiB before allocation, with a shared 16 MiB budget
for both passes' allocations/read bytes and retained name copies across roots.
Permission, unsupported-filesystem and race errors are not empty attributes.
The output distinguishes `visible_extended_attributes` from unconditional
`extended_attributes`: Linux may filter privileged attributes without additional
capabilities, and ASB never escalates privileges just to claim full visibility.

Attribute probes are upper bounds, not necessarily actual output lengths.
[OverlayFS](https://github.com/torvalds/linux/blob/master/fs/overlayfs/xattrs.c)
can return the underlying list size for a null-buffer probe and filter private
names only for the buffered read. The helper charges the full allocation but
truncates to the actual successful result, then compares two complete visible
observations. Oversized results, negative errors and changed actual observations
still fail. Fixed local diagnostic classes expose no paths or attribute values.

The opt-in original SWE-bench fixture also qualifies selective `/` traversal,
excluding kernel pseudo-filesystems, Docker-managed host identity files and its
positively owned helper runtime, with a separate full-content `/dev/shm` root.
It binds the actual image/runtime digest and exact non-deleted Docker change
paths and requires two identical observations. This is representative real-root
qualification, not complete mount, network, service or full-population support.
Ordinary regressions cover mixed full/selective hardlinks in either root order,
exact directory and symlink selectors and deterministic replacement between walk
and deferred read. No timing sleeps or privileged fixtures are needed.

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
