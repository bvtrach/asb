# Repeated execution preparation

This is an integration design and implementation ledger. The branch exposes a
prototype `run --repeat N` for native live/legacy JSONL replay, not the complete
bundle workflow or release readiness. Repeated recording explicitly reports its
pending integration. The complete contract remains in MULTITRACK.md.

## Implemented internal preparation foundation

### Repeated native client state

Explicit repeated native execution allocates create-new client state under its
unique run directory, outside workload files. HOME, XDG configuration/data/cache/
state/runtime directories and temporary files use distinct initially empty roots;
Unix directories are private mode 0700. No parent HOME contents or login/session
files are copied. Existing state is never reused or deleted on allocation failure.
The generic driver removes mutable-state aliases declared by adapter owners
before selected-adapter preparation installs its per-execution configuration.
Provider credentials, proxies, PATH/tool resolution and runtime instrumentation
remain available. This is modeled client-state isolation, not a sandbox against
arbitrary custom code that deliberately accesses unrelated external files.

Legacy native runs without explicit repeat retain their configured HOME/XDG
behaviour. Original SDK execution keeps the task image's HOME files as workload
material rather than erasing them through this native-only policy. The real CLI
regression runs two batches of two concurrent custom clients: 28 state roots
must be distinct and empty before owned writes, enclosing aliases are absent,
parent sentinels stay unchanged, and model selection and strict replay still
work. Additional tests cover legacy behaviour, create-new failure preservation,
provider/proxy/profile retention and selected-adapter configuration after cleanup.

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
declared container before reading material. The CLI supplies its running
executable to stage and invoke the guest census for a single owned container's
root filesystem plus full-content shared memory. Root workspaces and changes
outside the work directory are covered. Actual mounts, additional services and
complete runtime/network topology still need composition; this is not full
original-environment support. The old non-root TAR path remains for direct
protocol fixtures without a binary, never as a verified CLI fallback.

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

### Controller-to-root observation integration

`asb_material_root` binds two complete root observations, each bracketed by
actual image/runtime and canonical Docker change inventories, including deletion
facts. Exact Git-policy exclusions are rebased beneath the task workspace.
Kernel pseudo-filesystems and enumerated Docker identity files retain explicit
external-runtime exclusions; `/dev/shm` is read separately.

Helper staging requires an absent destination and a real `/opt` ancestor. The
existing bounded TAR reader verifies the exact file set, bytes, lengths and
modes before execution and after each observation. Links, extras and substitutions
are rejected. Reuse also requires the original verified metadata digest, not
just a cached pathname; changed staging is never silently reuploaded.

The private transport bounds input at 1 MiB and stdout/stderr at 4 KiB each,
drains concurrently, supervises blocked stdin, inherited output, deadline and
cancellation, and exposes no raw SDK/guest diagnostics. Local child reaping has
a one-second bound; unresolved cleanup explicitly requires workload preservation.
Stopping that CLI does not establish guest termination. SDK calls check the
shared 420-second deadline around operations, but a blocked read can overshoot
it by its 120-second socket timeout. The Rust controller's 600-second observation
deadline is the outer watchdog; cancellation takes stopped preservation.

Pure tests cover nonempty/deleted inventories, exact staged-tree reuse/mutation,
two-round disagreement, no fallback and cancelled/blocked/oversized/unreapable
transports. Supplying `ASB_TEST_BENCHMARK_ASB` to the opt-in original SWE material
fixture qualifies this root path across two fresh preparations. A narrow or
skipped fixture does not establish root integration.

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

## Single-container mounted material

The root composer now declares full-content roots for owned local volumes,
private tmpfs (both Docker option representations), and attempt-local binds.
Nested roots exclude descendants from parent walks. A single guest census
observes all roots together, retaining cross-root inode equivalence. Canonical
storage groups distinguish even disjoint subpaths of shared versus independent
volumes; overlap edges preserve parent/subdirectory aliasing. Mount access,
propagation, subpath and tmpfs options enter the identity, while generated source
paths, volume names and numeric inode values do not.

Bind declarations pin nofollow ancestor/source inode identities before container
creation. Capture checks the durable declaration and a local-kernel/container
mapping, then requires the actual guest census root to match the source's
device/inode/type. Same-kernel locality alone does not establish source namespace
authority. Private expected identities authorize traversal but neither change
the public content digest nor appear in its output. Volume inspection requires
the exact declared resource name and ownership labels on every snapshot.

Two complete environment rounds compare topology, image/change inventory and
all observed roots. This is bounded observation, not an atomic snapshot or proof
against concurrent malicious writers. The opt-in
`benchmark_live_material_mounts.py` exercises two fresh owned mounted fixtures,
repeat observations and a volume mutation. Local qualification passed against a
cached original SWE-bench image: both independently created fixtures produced
the same identity, repeated observations were stable, and changing a writable
volume changed that identity. Both successful fixtures removed only their owned
containers/volumes. No agent, provider or evaluator ran. This representative
qualification does not cover multi-service/network composition or complete
runtime equivalence, which remain separate unfinished work.

The root implementation separates `prepare` (owned instrumentation staging)
from the returned single-observation sampler. The single-container entry point
still compares two samples with the same hash domain. A deterministic seam
stages two distinct service contexts before taking A(main, database) and
B(main, database), demonstrating the interface needed by a whole-environment
composer without claiming that production multi-service dispatch exists yet.
That composer must own durable logical roles, exact resource witnesses,
aggregate budgets, global runtime/network snapshots and shared-storage groups;
it must not run isolated A/B pairs per service or silently exclude generated
Docker configuration files without corresponding semantic observations.

### Shared observation allowance

The private guest request optionally carries entry, filename-allocation,
payload-read and visible-attribute-read allowances. Each is checked against the
original per-invocation ceiling before traversal: 100,000 entries, 16 MiB names,
2 GiB payload and 16 MiB attributes. The response reports actual charged usage
only when that extension was requested. Usage and limits are neither content
fingerprints nor public coverage; the controller validates the counters, debits
the shared ledger, and removes the private usage object before composition.
Requests without the extension retain the old response shape.

Single-container capture owns one two-observation ledger (200,000 entries,
32 MiB names, 4 GiB payload and 32 MiB attributes total). A future multi-service
composer can share that same ledger across its service/round calls, declaring
at most 32 root observations (16 services, two rounds), never silently resetting
allowances. Each call remains subject to the original smaller ceilings. The
constructor bounds aggregate allowances by those 32-root-observation maxima. Actual usage,
not the maximum reservation, is subtracted, so unused allowance remains usable.
A failed transport or invalid consumption leaves the ledger unresolved and
unusable; any later ownership/snapshot failure must likewise abort the capture,
not authorize another attempt. Helper staging/receipt validation has separate
bounded transport limits and is not counted as workload payload consumption.

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

### Pre-creation service authority

Compose preparation writes private, bounded `material-services.json` beside
the recovery journal. It maps stable original service/network/volume roles to
exact attempt-owned resource names and retains hostname/network intent. Nested
intent data is copied, not aliased to mutable SDK configuration. Both service
and bind receipts are frozen on the first Compose hook: Harbor invokes that
hook again for later lifecycle commands, which must not authorize replacement
bind inodes or rewrite historical intent. Unsupported declarations remain
unavailable rather than blocking legacy Compose preparation.

Verified material capture checks the private durable receipt against its cached
copy and active owner, then resolves exactly the complete declared container
set without daemon inventory listing. Each role needs a distinct live owned
container; paused, restarting, missing or replaced resources fail. Ownership
and exact name are rechecked after reload, and the primary role is identified
by the task container ID rather than assuming a service named `main` or
`client`. Resolver results are not evidence that content/topology was observed.
The network-none composer below consumes these roles; other network topologies
still require additional explicit coverage.

The shared receipt reader is also used for bind declarations, sanitizes corrupt
or unavailable receipt errors, bounds input to 1 MiB and returns independent
copies. These local consistency checks introduce no signatures, credentials or
production authorization prerequisite.

### Routed-network creation intent prerequisite

The immutable pre-creation service receipt now also retains each network's
deep-copied Compose definition, excluding its physical name and annotation
labels. This includes unknown options: a later topology observer must model or
refuse them, never silently discard them. Explicit IPAM pools/gateways therefore
remain distinguishable from omitted, daemon-allocated pools. Later Compose calls
cannot rewrite the original receipt. This prerequisite does not enable routed
network observation; the existing network-none refusal remains in force.

The next composer must inspect only exact declared owned local bridge networks,
verify bidirectional network/service membership, and retain raw network/endpoint
identities privately across whole observation rounds. Explicit addresses, aliases,
subnets and driver options remain literal and checked. Only positively proven
automatic allocation may become tagged logical service/network identities.
Unlisted members and unsupported dependencies are errors, not omitted state.

`asb_material_networks.observe` now provides the private observation component:
it freshly resolves the complete service set, looks up only named declared owned
local bridge networks, rechecks ownership after reload, and requires exact
bidirectional membership. Container/network endpoint IDs, network IDs, owned
names, IPv4/IPv6 addresses and prefix lengths, and MAC addresses must agree.
It retains bounded deep-copied raw inspect data, creation intent, attachment
intent and service network modes. It neither modifies Docker state nor claims
canonical environment equivalence. In particular, raw consistency does not
validate requested static addresses/IPAM against actual allocation. Runtime
configuration and generated-file integration still reject routed environments
until that canonicalization and qualification are complete.

The opt-in `benchmark_live_material_networks.py` uses an explicitly cached image
and two owned idle containers on an owned internal bridge. It checks stable raw
observations, rejection after disconnecting a declared endpoint, and a changed
private witness after reconnecting it. Successful cleanup confirms absence by
exact owned names; failure stops and preserves only its own resources. It starts
no agent, provider or evaluator. This qualifies membership observation only,
not cross-attempt routed equivalence or deterministic network behaviour.

The next component, `asb_material_network_identity.canonicalize`, validates
frozen network/endpoint intent against this private observation. It covers owned
local bridges with default IPAM; custom plugins remain external dependencies.
Explicit pools/gateways/auxiliary addresses, static IPs, MACs, aliases, link-local
addresses, gateway priority and driver options are checked and retained.
Only absent creation intent permits typed automatic pool/address/MAC identities.
Generated aliases use exact owned names, never substring replacement, and
explicit hostnames/aliases take precedence. Driver options and unmodeled observed
fields remain bound; unknown declared dependencies are refused.

Automatic pools preserve family/prefix and gateway offset. Internal bridges keep
their absent endpoint default route even when IPAM has allocated a pool gateway.
IPv4/IPv6 pool order is canonical by family; duplicate addresses compare parsed
IPs and duplicate MACs compare case-insensitively. Generated address maps are
scoped private evidence for Docker runtime-file locations, not authority to
rewrite workload data or provider requests. Two freshly created internal-bridge
fixtures qualify equal canonical topology despite unequal raw identities.
The service receipt now explicitly records service-level MAC intent; old receipts
without that evidence cannot authorize automatic MAC normalization. This pure
component is embedded and tested. The runtime integration below consumes it;
whole-environment qualification remains separate from pure canonicalizer tests.

Runtime integration must use the same whole-environment composer for verified
single-service routed tasks, not only multi-service tasks. It must bind the
canonical endpoint topology and network-mode role while retaining private raw
network snapshots across every round. Generated runtime-file address tags are
permitted only on positively identified Docker-generated host lines; explicit
ExtraHosts, static addresses, literal aliases, comments and resolver contents
remain distinct. Legacy aggregate NetworkSettings fields and sandbox identity
need explicit coverage or refusal, not silent omission. Config/HostConfig fields
outside already modeled mounts and network identity remain content-bound.

The implemented composer now performs exact network observation and intent
validation before staging helpers and during every whole-environment snapshot.
Routed single-service receipts take this composer too, without a root-only
fallback. Private raw network state remains part of before/after round witnesses;
canonical topology and endpoint/runtime settings enter the material identity.
Sandbox IDs and their expected Docker namespace paths become per-service tags
only after identity/path checks; distinct private services cannot share either.
Deprecated aggregate network fields must jointly identify one verified endpoint,
not independently match different networks. Unsupported secondary/legacy
dependencies are refused. Other inspected runtime fields remain bound.

Generated addresses are tagged only on the exact own generated hostname/FQDN
line in `/etc/hosts`. Explicit ExtraHosts names and addresses, static endpoint
addresses on any network, explicit aliases/hostnames, comments, unknown lines
and resolver contents stay literal. ExtraHosts protection is applied after
generated alias construction so a generated-looking explicit name cannot be
accidentally reauthorized. The owned cached-image qualification has separate
internal, non-internal/default-route and single-service bridge cases. Successful
fixtures verify ownership before cleanup and absence afterward; failure stops
and preserves the owned resources. These material checks start no agent,
provider or evaluator and do not establish original-catalog-wide coverage.

All three exact-code owned bridge cases passed together in 263.666 seconds with
the selected ASB helper binary unchanged before and after qualification. They
cover two-service internal and non-internal/default-route bridges, single-service
dispatch, fresh-attempt equality, shared-versus-independent storage, mutation
visibility, repeat observation and verified owned-resource cleanup. The source
suite passed 725 tests (zero failures, 20 explicit opt-in tests ignored), with
formatting, strict Clippy, build, private Rustdocs, diagrams and checkout tests.
This evidence is scoped to the reviewed integration increment; current-main
refactor/coverage integration and remaining delivery tracks still require gates.

### Owned network-none environment composer

Verified capture now routes multiple declared services to an environment
composer, without falling back to a weaker main-only capture. The first
supported runtime profile requires networking disabled, private/default
namespaces, no external devices, inherited volumes, links or published ports.
Unsupported profiles fail before instrumentation. This is not complete routed
network or original-catalog support.

Each role owns its own context, runtime cache and immutable container-ID anchor.
All helper staging finishes before any observation. Complete rounds visit all
services in stable role order; global snapshots bracket each round, retaining
both normalized material and private raw resource/configuration/file witnesses.
Replacement, ownership changes, cancellation, topology mutation and different
A/B rounds abort preparation. Canonical storage groups and parent/subpath edges
use service-qualified mount roles, distinguishing shared versus independent
volumes even when bytes match. Disjoint cross-service bind roots on one device
are refused until global file-level alias coverage exists; declarations alone
cannot establish their hardlink relationships.

The guest's private `--runtime-files` material-helper mode observes only
`/etc/hosts`, `/etc/hostname` and `/etc/resolv.conf`: nofollow pinned paths,
regular files, 4 KiB per file, printable ASCII plus tab/newline/CR, visible
attributes, inode groups and stable per-inode versions/content. Non-text,
oversized, linked-path or contradictory alias observations fail. The explicit
private transport cap is 64 KiB; no file text enters the public result.
Normalization uses tagged exact generated-hostname/container-name tokens, not
substrings. Explicit hostnames/host mappings, whitespace, comments and resolver
text remain literal. Raw snapshots are compared as well, so normalization cannot
hide preparation-time mutation.

Root calls and generated-file observations debit the same shared ledger. There
are two root calls and four generated-file calls per service (before/after each
whole round), not merely two total helper calls. The aggregate allowance remains
`2 * service_count` times the original per-root maxima, shared conservatively
across all six calls; it is not a resetting per-call or helper-count quota.
Generated-file reads additionally cap attributes at 64 KiB per invocation and
report actual consumption; exhausting any aggregate dimension aborts capture.

- Official workload identity must bind adapter-owned environment material. Image
  ownership labels and attempt tags differ per execution, so raw source-build
  image IDs cannot by themselves represent stable material equality. Stable
  definition/content identity must remain distinct from actual-image provenance.
  Qualify declared mount integration, then extend to multiple services and complete
  runtime/network topology, then qualify real installed
  SDK environments. Do not replace missing coverage with definition-only hashes.
- Integrate durable recording journals and saved bundle replay selections with
  the execution campaign, preserving diagnostics before a compatibility anchor
  exists. Do not fabricate fingerprints or completed attempts after failed setup.
- Update sweep reservation/receipt/resume and comparison to enumerate every slot;
  repeated attempts must not silently become official single-attempt scores.
- Complete repeat/bundle CLI help, documentation and integration qualification
  before marking this feature/release ready.

## Current direct-container and single-service coverage

The modern verified path now uses the complete environment composer for every
service declaration, including a single network-none service. Earlier sections
describing a single-service root-only fallback are historical implementation
steps, not the current dispatch contract. Missing pre-creation declarations stop
verified preparation rather than silently reducing material coverage.

Direct SDK creation freezes a private receipt before creating the container,
without changing its Docker options. Original SWE-bench keeps Docker's shared
default bridge. The observer looks up that named bridge and validates only the
owned container's endpoint; unrelated peer membership is neither enumerated nor
included in baseline identity. Shared bridge configuration, address pools,
gateways, flags and driver options remain literal external observations. Only
positively generated own endpoint identities are normalized. The bridge is never
journal-owned or removed. This is relevant workload-environment comparison, not
an external-network snapshot or a guarantee about peer activity.

Pure regressions cover frozen direct intent, foreign membership churn, own
endpoint disagreement, external IPv6/IPAM options, fresh generated identities,
and single-service runtime-file/environment/resource-limit changes. Actual
original SWE-bench fresh preparations, the single-network-none composer and
public original SWE-bench repeat-two/concurrency-two qualification passed with
one unchanged binary. The public test observed four distinct attempts, eight
loopback inference calls, four official resolved grades and confirmed cleanup;
it used a known repair, not an AI-generated solution.

The opt-in repeated fixture also accepts `ASB_TEST_BENCHMARK_SUITE` values
`terminal-bench-1.0` and `terminal-bench-2.1`. Initial qualification exposed
failures before agent launch: 1.0's prepared census encountered SDK tmux/session
instrumentation, while 2.1 reported mutation within a complete service round.
Subsequent cross-slot comparison also exposed physical subnet keys in allocator
status. These failures were repaired below, without skipping material checks.
Cross-service disjoint bind alias coverage remains open. Representative SDK
successes do not establish full-catalog or same-shell task support.

The subsequent repairs address demonstrated causes, not broad exclusions.
Terminal 1.0 tasks declaring `run_tests_in_same_shell: false` do not create an
unused agent tmux/asciinema session: their verifier already creates its own test
session. True declarations retain the original configured-user session. This
does not establish full client/verifier same-shell integration for those tasks.
Fresh lifecycle tests check both declarations and the exact session passed to
the official grading method.

Docker inspect mount enumeration is unordered. The private runtime witness now
uses the same validated, destination-sorted mount list as the topology composer;
source, access and all admitted mount fields remain bound. Regressions reorder
mounts during a complete round and separately reject source/access mutations.

The [Moby network status API](https://pkg.go.dev/github.com/moby/moby/api@v1.56.1/types/network)
also keys allocator counters by actual subnet. Owned-network status validates
exact pool correspondence and bounded unsigned counters, then tags only
positively automatic subnet keys with their existing role/family/prefix identity.
Explicit pools and counters remain observable; older missing/null status does
not become fabricated zeroes. The private owned-network observation still
detects status mutation within a round. On Docker's externally shared bridge,
aggregate status is excluded before copying or hashing, like foreign membership:
it describes unrelated peer allocation, not our owned endpoint or configuration.
With one unchanged binary, both original Terminal SDK public repeat-two/
concurrency-two qualifications now pass: four distinct attempts, eight synthetic
loopback inference calls, four official resolved grades and confirmed cleanup
per SDK. The representative tasks are 1.0 `hello-world` (separate verifier shell)
and 2.1 `cancel-async-tasks`. Known repair commands qualify execution/grading,
not AI solving. Other explicit feature requirements remain open.

A subsequent separate-frozen-binary qualification passed SWE-bench's four
attempts, but exposed external verifier dependency failures in both Terminal
SDKs: `curl` could not connect to `astral.sh`, preventing installation of `uv`.
The original retained evidence must stay authoritative: 1.0 returned unknown
grading (`parse_error`), and 2.1 returned an official zero reward for one attempt
while continuing all four slots. Neither outcome may be changed into success or
silently regraded. These failures do not contradict the repaired initial-material
checks, but the latest all-success live qualification is not green.

The 1.0 failure additionally exposed an execution-continuation defect:
`Session.finish()` successfully confirms cancellation/stopped preservation, then
returns an error solely because grading was incomplete. The runner conflates
that grading state with unconfirmed resource cleanup and stops later batches.
Separate resource disposition (removed, not created, positively stopped/preserved,
or unconfirmed) from the evaluator outcome. Positively settled preservation may
permit new distinct attempts while retaining an overall grader failure and
artifact locator; unknown ownership, timeout, unreaped controllers or uncertain
proxy cleanup must still stop admission. Never retry the same attempt's verifier
automatically. Add a real-CLI failure matrix proving all four slots execute after
a confirmed preserved grading error and no later batch launches on uncertainty.
The disposition repair is implemented with typed settlement independent of
grading, cached positive acknowledgements and controller reaping. A frozen-binary
Terminal 1.0 fault qualification (`repeated-original-v6ew5dxt`) completed four
distinct attempts across both batches with eight loopback inference requests:
the deliberately blocked verifier retained an unknown grade and stopped-preserved
artifacts; the other three received official resolved grades. Overall command
failure remained visible. Missing versus explicit-null disposition validation
and measurement agreement now have dedicated regression coverage; the latest
validation changes and strengthened fixture subsequently passed qualification
with frozen binary SHA-256
`91bea889cc8cb4bc2d04620d54a927c8a00ac4adc1725414a68c7e2617cacedd`
(`repeated-original-9cz_cr5q`, 86.995 seconds). Its binary hash was unchanged
after execution. The separate protocol test counts exactly one evaluate request
and one cancellation across repeated finish calls and drop. The live fixture
requires exactly one injected fault and attempt-bound exclusive grading-start
evidence, with no recovered or selected replacement evaluation.
The separate strict all-success Terminal 1.0 qualification also passed with the
same binary (`repeated-original-08ju28jm`, 81.977 seconds): four distinct original
attempts, four official resolved grades, eight inference requests and confirmed
cleanup. This does not establish the remaining same-shell path or full catalog.
The repaired source passes the full offline suite, strict Clippy and coverage:
90.8313% production lines and 90.6212% aggregate lines.

### Cross-service bind alias component

Reuse the existing bounded multi-root Rust census rather than inventing an inode
transport. A joint observation of verified local, owned bind sources can retain
file-level hardlink relationships across service roots alongside the existing
guest-visible content scans. Order logical roots by qualified service/destination,
not random source paths; keep a binding-to-root/view map and private expected
inode witnesses. Reverify ownership, nofollow source chains and daemon locality
around observations, bracketing the complete guest rounds with one shared budget.

Exact-source deduplication must preserve guest visibility. Bindings can hide
different subtrees behind nested mounts: prove identical views before merging or
construct a union-visible view that excludes only subtrees hidden in every view.
Do not hash arbitrary hidden host files. Retain all per-binding visibility and
mount relationships. Test cross-service hardlinks versus independent equal bytes,
alias mutation, nested roots/views, replaced/symlink sources, remote daemons,
aggregate exhaustion/cancellation and actual owned fixture cleanup. This component
is now implemented; the final lifecycle delta and hidden-view behavior remain
under live qualification as described below.

The joint bind planner must group identical source paths using a logical
service/mount representative, while retaining every binding's own exclusions.
For each group, hide only the intersection of hidden prefix subtrees across
views: exclusions `{a}` and `{a/b}` yield `{a/b}`, and any unrestricted view
yields no hidden subtree. Rebase semantic exclusions exactly as the guest root
sampler does; a host census must not reintroduce intentionally omitted SDK/Git
material. Distinct nested source roots stay distinct to preserve inode aliases.
The helper now groups regular-file and symlink inodes, with kind, link-target,
attribute and version agreement; symlinks never enter deferred file-payload
reading. The actual CLI regression first exposed equal digests for shared versus
independent symlinks, then passed after repair. Per-root observations remain
equal in both cases while the joint graph correctly differs.

`ObservationBudget.observations` bounds the initial aggregate allowance, not the
number of helper invocations. Allocate its bounded multiplier once (never above
32), share remaining allowance across host and guest calls, and refuse honest
exhaustion without resetting or silently enlarging per-call maxima. A host bind
observer must reuse the exact host executable and prove local daemon/kernel and
source-root identities before reading; a guest runtime bundle can have a
different architecture and is not the host helper.

`PreparedSampler` requires a successful guest census matching every bind inode
before exposing host views. A new census or failed callback invalidates the
cached authorization. The composer brackets a complete initial guest round,
requires later public/private state and samples to match it, and brackets each
later complete guest round with joint host observations. Each host observation
also checks the private source plan before/after reading. Initial guest scans,
runtime-file reads and all host/guest rounds share one allowance allocated with
multiplier `min(32, 3 * services + 4)`. This is not an atomic filesystem snapshot;
mutation/refusal and exhausted allowance fail rather than silently narrowing
coverage or retrying.

The first frozen-binary owned Docker qualification passed in 170.273 seconds
(`environment-material-hdfqe2f_`, binary `978a675d...`): fresh disjoint bind
fixtures normalized equally, source mutation changed identity, and independent
equal copies differed from shared regular/symlink inodes. A subsequent lifecycle
repair invalidates stale coverage after failed sampling; its new frozen binary
`788b25adb9a1b692ba2bf97a4175f7dddb6e1874539ab89dff7e75b3dfe4bb3d`
passed both final alias and nested-view qualifications in 331.252 seconds
(`environment-material-84w3d476` and `environment-material-0v31d4z0`), with an
unchanged executable hash. Hidden-from-every-view source mutation preserved
identity, while material visible in either view changed it. The retained
shared-view fixture's older generic bind flag is not evidence of disjoint
sources; its named test and nested-view flag identify that scope, and subsequent
fixture output labels disjoint sources only for the disjoint case. These
checks start no agent, provider or grader; successful declared resources are
removed and failed fixtures are stopped/preserved.

### Remaining Terminal 1.0 installed-client same-shell execution

The pinned SDK's installed-agent contract launches its coding CLI commands in
the configured-user persistent `TmuxSession` and later runs the verifier in that
same parent shell. It does not propagate arbitrary nested tool subprocess
exports back into the parent shell. ASB must match this actual contract rather
than claim either stronger tool-shell persistence or that merely sharing the
container is sufficient. Current direct Docker exec does not meet the contract.

Use a workload-owned execution-launch strategy beneath the generic guest
execution boundary. The default Docker-exec transport remains unchanged;
Terminal's same-shell strategy launches the existing `__asb-agent` foreground
child through the exact SDK session without replacing its parent shell. Shared
gate, deadline, bounded output, collector attachment and cancellation logic
must consume a transport contract instead of duplicating the runner. The
strategy returns child target identity, demultiplexed output, final status and
confirmed settlement, not merely a successful `send_command` call.

The launch-transport foundation is now implemented. `ManagedDockerWorkspace`
keeps the existing lock, gate, deadline, resume, bounded-output and workload
lifetime state machine. Its iterable transport owns launch IO, actual target
identity, child exit status and idempotent closure. The default Docker transport
uses explicit boolean daemon lifecycle evidence; alternate transports never
fall back to inspecting their bridge PID. Namespace-only targets explicitly
omit a daemon-host PID. Invalid identity or host-collector refusal occurs before
resume. Retained workloads hold the whole transport through namespace teardown,
including launcher cleanup, and cancellation-class closure failures remain
uncertain rather than disappearing in a background drain. This foundation is
not yet an SDK-session launcher or proof of same-shell client execution.
The real owned Docker transport qualification passed in 3.455 seconds
(`execution-transport-azu7qwhn`), binding source SHA-256
`cf7056826669b036e0e82651c89fb21d232d7012556efa49e05ff7bb16cec3c4`.
It checks the actual stopped target's host/namespace PID, separate output,
nonzero child exit, short execution deadline, cancellation and malformed-gate
recovery; its declared container was removed. The initial timing-fixture failure
used an execution-sized preparation budget; the corrected fixture gives
preparation its own budget and starts cancellation only after attachment. That
older failed owned fixture remains stopped/preserved, not promoted to success.
Pure execution tests also cover custom target/bridge separation, startup and
stream cancellation, strict daemon lifecycle/status fields, idempotent sticky
closure and retained transport cancellation failures.

Private bounded streams and a nonce-bound exit receipt can bridge the SDK
session to the controller. Launch credentials belong in a private environment
channel, not recorded tmux keystrokes. A blocking SDK call must not occupy the
controller's cancellation reader. Own and join every launcher worker and bridge;
uncertainty requires stopped preservation. The SDK parent shell persists through
evaluation independently of the measured client process tree.

The bridge's Docker exec PID is not the client PID. Keep transport exec identity
separate from the real stopped application's namespace PID and start-time
identity. Host collector attachment requires an authoritative mapping to the
actual daemon-host process, never a bridge-PID substitution. Where only a
namespace target is available, disclose it and reject incompatible collectors
before execution; do not fabricate host identity.

Initialize the SDK session after initial workload material verification, as an
explicit preparation stage, and bind its pinned recipe, configured user and
recording policy into preparation identity. Only positively owned declared
instrumentation paths may change; no broad HOME or `/tmp` exclusion is allowed.
Validate shell identity, user, cwd, environment and readiness before release.
Batch readiness must cover this stage and the paused real client gate: a failed
sibling initialization aborts all waiting clients before any is resumed.

Tests must distinguish bridge and client PIDs, prove the same shell survives
into verification, preserve launcher-inherited user/cwd/environment, retain
bounded streamed stdout/stderr and exact child exit, and exercise cancellation
at launch/gate/execution/return. Test shell replacement/death, malformed or
duplicate receipts, blocked channels, sibling preparation failure and teardown
uncertainty. Qualify an actual installed client on a true same-shell original
task with repeat-two/concurrency-two. This strategy remains unimplemented; the
successful separate-shell fixtures do not close it.

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
