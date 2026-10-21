# Code-smell verification and repair ledger

Scope: the supplied ASB CLI code-smell report, independently checked against
main `19033110543252063d27c495584caa6f982cef7c` (21 October 2026). This ledger
tracks work, not a claim that the whole audit is complete. Each repair class
needs an independently reviewed PR, ordinary and packaged offline CI tests,
post-merge verification, and a patch release. PR #264 originally introduced
a required 90% production/aggregate coverage gate at
`cbfaa4c1ebc2ea3880916f7767ed08bd64b6c2f7`. Main commit `398c36cc` later removed
that hosted job and branch-protection requirement. The user explicitly
confirmed keeping the newer removal on 21 October: do not restore the gate or
treat a local percentage as a delivery threshold. Local coverage remains
informational, not a substitute for meaningful offline regressions. Required
functional CI, independent review, exact-head validation and patch releases
still apply to each repair class.

Integrated candidate, staged patch **0.2.21**: the independently reviewed
launch-descriptor/one-shot admission repair (`b1a54fde`) is combined with the
reviewed workload-wizard stack (`6af7939b`), retaining shared benchmark paths,
bounded flamegraph cleanup, host prerequisite consent and adapter-owned
executable readiness. Adapter package preferences and source readiness coexist
with the raw-OS launch descriptor; native command configuration and exclusive
gate admission remain intact. The new planning execution budget and recorded
report-metadata boundaries remain intact as well. The typed comparison pipeline
retains bounded source snapshots, nullable series, signed bars and complete
generation publication. Planning, controller caps and module revisions stay
metadata rather than measurements. Independent integration source review
approved parent `3259da7e`; its full offline suite passed 1,008 tests followed by
strict Clippy, formatting and build/docs/diagrams checks. The independently
approved report integration `1fb9bdf1` passed 1,036 full offline tests with
21 optional skips, strict Clippy, formatting and build/docs/diagrams checks.
Actual main ancestry through launch repair PR #286 changes only this ledger,
not that qualified production/test tree. Exact-head hosted/package CI, PR merge,
post-merge verification and patch release remain pending. This staging entry
does not claim delivery or close the other audit repair classes.

## 1. Endpoint preparation invariants

Eight production adapter branches used `unreachable!` for unsupported wire
interfaces. The ninth cited site, in Claude Code, is inside a test. Both native
negotiation and the guest driver already check interface/authentication
compatibility: a newly offered provider interface alone does not demonstrate
a user-reachable panic. Defensive validation at the actual adapter boundary is
still needed to make the contract robust to future callers and declarations.

Implemented candidate: reject unsupported interface/authentication pairs in
`StaticAdapter::prepare` before child-environment or configuration changes;
replace production panic fallbacks with errors containing protocol metadata
only, never URLs or credentials. Registry-wide tests exercise every rejected
interface/authentication pair and all advertised pairs without executing a
client or contacting a provider. Independent review found no blocking issue.
Delivery: independently reviewed PR #256 merged as
`bc0b1ae662b4e854082e3e25e5fef86aed8e0e59`, patch version **0.2.4**.
Exact-head PR CI passed both ordinary checks and the mandatory coverage gate.
Post-merge CI passed and **v0.2.4** was released from that exact main commit.
This entry does not mark the broader audit complete.

## 2. Report identities and shared path handling

Independent review found a correctness issue beyond the original duplication
claim: distinct metric keys can produce identical lossy SVG filenames, causing
one chart to overwrite another. Flattened JSON paths can also collide when
keys contain dots or brackets. Required repair: preserve distinct metric
identities and allocate collision-free artifacts, test punctuation/Unicode and
nested-versus-literal keys, then consolidate genuinely equivalent sanitizers
without changing established result/workspace identities unintentionally.
Also review duplicated manifest reference validation and executable resolution;
same names alone do not prove equivalent semantics. Pending.

Executable-policy repair, released patch **0.2.18**: project-local source
installation, readiness and launch share one registry-neutral executable lookup.
Explicit source overrides precede adapter-owned package preferences; unusable
present candidates cannot silently fall through to stale fallback code. Codex
owns its native-package preference, and host discovery cannot claim a project
installation. Root independently reviewed the original production repair and
reran all seven isolated policy fixtures after current-stack integration. These
exercise actual source installation and execution for every registered agent
and tool, custom roots, override precedence, broken/non-executable/symlink
entries and host-only discovery. Existing installer regressions now call the
same shared executable predicate rather than a removed duplicate. Current-main
integration `b395617e` is independently source-approved; its full offline suite
passed 996 tests, followed by strict Clippy, formatting and build/docs checks.
PR #263 merged as `c9beafb6dfb52562dac59dc6de7bcbc7f7421204`.
Exact-head hosted/package CI and both post-merge jobs passed in run
`39635610656`; the released signed tag is GitHub-verified at that exact commit.

Shared benchmark-path repair, released patch **0.2.13**: all five equivalent legacy normalizers now
use one component/stem policy, and both sweep execution and comparison use the
same checked TOML-reference contract. Filename prefixes, Unicode-per-character
replacement, non-UTF-8 fallback and existing symlink behavior are unchanged.
The graph allocator still owns collision-free identities; the shared lossy
normalizer is not a uniqueness or confinement mechanism. Tests preserve those
boundaries and exercise actual CLI rejection in both commands before artifact
creation. Independently reviewed PR #271 merged as
`c5674eabc051aa071539e6cb324d73924e058cbf`; exact-head ordinary/package CI
and both post-merge jobs passed (run `39625633010`). The signed **v0.2.13**
release points to that commit. This does not close
the separate executable-resolution or orchestration classes.

Candidate report repair: flatten numeric paths with escaped object separators
and empty-key identities; reserve bounded portable SVG names, using digest
suffixes for unsafe/long/colliding metric or series names. Shared series names
and cross-source metric conflicts fail explicitly instead of last-writer-wins.
Tests verify real summary values, distinct SVG titles/data and HTML references,
repeat generation and repeated configuration stems. Existing safe unique
artifact names stay unchanged; unusual keys use escaped path notation and
consumers should obtain graph filenames from `comparison.json`.
Delivery: independently reviewed PR #257 merged as
`552cd640124ca1ad02d8c4fcfd8f85c8400fa62f`, patch version **0.2.5**.
Exact-head tests, mandatory coverage and hosted checks passed.
The first main run exposed a test-only Perfetto descriptor observation race.
Independently reviewed PR #267 merged as
`9e7a1ba49e60f394cf1339315d0df9c8c939eafd`, preserving lease rejection
assertions behind a child-authored post-exec observation. Every post-merge job
passed at that repaired main in run `39599622348`, and **v0.2.5** was released
from that exact commit with a GitHub-verified signed tag.

## 3. Completion persistence, subprocess and cleanup failures

Completion-observer and checkpoint errors must not erase an already determined
task grade or implicitly repeat a client. Their current fail-open treatment
is deliberate, but silent loss of continuation evidence needs explicit,
deduplicated diagnostics and fault-injection tests. Audit curl kill/wait,
installer temporary publication/cleanup and consumed agent artifacts for
bounded teardown and actionable errors. Best-effort cleanup in destructors
is not automatically equivalent to an incorrectly ignored required write.
Completion-diagnostic candidate: fresh outcome/observer/group-checkpoint and
initial/final input-seal failures now produce deduplicated human warnings or
typed JSONL events. Original grades, process results, sealed logs and primary
errors remain intact; missing evidence never authorizes an automatic retry.
Actual filesystem-publication and broken-writer fixtures cover true/false
grades, concurrent replicas and conservative resume. Review regressions also
distinguish receipt destinations from source outcomes and preserve every
admitted configuration's failure after parallel-sweep output breaks, without
further writes or retries. Independent source and fixture review approved
candidate `16528073`; exact-head ordinary/package checks and both mandatory
coverage gates passed before integration with newer main. The integrated
candidate shipped as **v0.2.9** after independently reviewed PR #269 merged as
`3211d1bf330cbc81731e10cc83e7843f58152707`. Exact-head hosted/package CI and
both post-merge jobs passed in run `39614537662`; GitHub verified its signed
release tag. Other subprocess/publication cleanup
items in this class are not claimed complete.

Model-catalog transport repair, released patch **0.2.11**: replace blocking curl stdin/stdout operations,
stderr-reader joins and caller-side waits with fair nonblocking Unix polling
under one operation/cleanup deadline. Preserve proxy settings, response limits
and sanitized protocol diagnostics. Direct-child ownership uses the shared
pre-established reaper on uncertain cleanup; no successful catalog can bypass
exit/EOF checks. Actual offline process fixtures cover pressure, inherited pipes,
timeouts, custody and reaping. The body limit and I/O timeout are shared with
curl's actual arguments; exact-limit success and one-byte-over rejection have
real subprocess regressions. Independent source review and a separate execution
of all 36 focused model tests passed. Non-Unix discovery fails before spawn;
cancellable non-Unix transport remains a separate portability contract.
Independently reviewed PR #274 merged as
`8b6e2f4ad7e093fa6574cc00a5e764e8c671f64d`; exact-head ordinary/package CI
and both post-merge jobs passed (run `39621160735`), followed by the signed
**v0.2.11** release from that commit. Main `398c36cc` removed
the hosted coverage gate and the user explicitly confirmed keeping that removal;
local measurements are optional diagnostics, not a required hosted check.

Flamegraph cleanup repair, released patch **0.2.15**: the perf-script/stack-collapse/render pipeline
owns every directly spawned stage. A failure, timeout or unwind signals all
remaining stages before polling against one shared cleanup deadline, preserving
the original error and retaining unconfirmed children in the shared private
`OwnedReaper` established before any spawn. It no longer selects processes by
parent PID or returns from an upstream failure while forgetting a downstream
child. Offline real-child tests cover stage/file/spawn failures, timeout,
unwind, fallback-custody handoff, actual reaping and unrelated sentinel survival.
Independently reviewed PR #276 merged as
`57eee9b12a40327407b3cf911371b74ec932902a`, preserving main's benchmark
planning changes. Exact-head ordinary/package CI and both post-merge jobs
passed (run `39630284995`); **v0.2.15** is signed and released from that
commit. Coverage is optional, not a delivery gate. This does
not claim arbitrary descendant or crash-persistent cleanup.

## 4. Typed domain and wire contracts

Review run status/evaluation/failure categories, execution mode, result source,
controller messages and internal dispatch constants. Use typed enums or
central constants where they enforce a real contract, preserving existing
serialized representations and backwards reading. Add malformed/unknown
protocol fixtures, round-trip and compatibility tests. Trial-set display
suffixes should not serve as hidden identifiers. Pending.

## 5. Orchestration and structural decomposition

Split the responsibilities of runner execution, comparison loading/rendering,
sweep orchestration, Perfetto configuration, BPF scope validation, remote
receive/deadline handling, provider startup, agent driver, model HTTP fetch,
matrix publication and installation. Extract cohesive phases with explicit
state ownership; moving a giant body to a renamed function is not sufficient.
Protect ordering, independent grading, output/error handling, cancellation,
partial failures and cleanup with provider-free lifecycle integration tests.
Simplify checkpoint validation into named invariants without losing checks.
Review broad parameter lists, result structs, visibility, dead API surface and
module names in those owning boundaries. Pending.

Runner repair, released patch **0.2.10**: private consuming admission,
preparation, execution-evidence, proxy-settlement, evaluation and finalization
states now separate owned resources and independent outcomes. It repairs the
verified path where collector lifecycle errors discarded client output or an
official grade and skipped explicit workload cleanup. Native supervised-exit
time excludes collector/proxy teardown, and terminal-answer validation belongs
to the owning agent adapter. Broken human presentation retains admitted work
and its grade; broken owner transport remains cancellation. Log-publication
failure still attempts measurements and presentation. Both log and measurement
publication must be confirmed before a completion receipt can be emitted.
Independent review and seven isolated full-entry fault cases verify that
original grades, primary errors, secondary publication/output diagnostics,
observer redaction and exact-once execution/evaluation/cleanup survive the
combined failure paths. The larger phase matrix uses synthetic execution
evidence, not full benchmark runs. This does not close the remaining comparison/sweep/driver structural
work, stream-memory limits, blocking child-wait class or exact inner-principal
native timing. Independently reviewed PR #273 merged as
`f2c8d7819d8f79e11c398f2a87ab6b938fa6c9ab` after exact-head hosted/package checks
passed. Both post-merge jobs passed in run `39619889953`; **v0.2.10** was
released from that exact commit with a GitHub-verified signed tag.

Audit correction: current `perfetto_instance_config` is seven lines, not the
reported 630. Comparison's filter JavaScript is already a separate asset.
Do not perform redundant extraction based on stale line counts.
Comparison candidate, staged patch **0.2.21**: explicit loaded/validated/prepared/rendered/published
states replace intertwined loading, grading coverage, graph planning and output
publication. Stable identities, typed result provenance and one-time population
aggregation preserve existing evidence semantics. Confirmed fixes retain null
positions with original timestamps (no shifted samples), disclose bounded
original-index extrema-preserving line rendering instead of silent truncation,
and provide nearest observed-sample hover text. Opened-once bounded source reads
reject nonregular files and growth; retained physical SVG paths prevent
symlink-parent components from being misread as logical output links.
Generation-specific private staging and a separate persistent report lock
publish whole generations with Linux/macOS native atomic exchange and preserve the old tree
as a recoverable backup. Pre-commit failures leave old bytes unchanged;
post-commit sync/output errors truthfully report published-but-unconfirmed.
Offline source/DOM/publication regressions require no provider or privileged
collector. Real browser qualification remains opt-in and host-dependent. This
candidate passed independent source review and a separate execution of all 25
pipeline regressions (one explicit real-browser opt-in remains ignored). Signed
scalar bars preserve negative observations, finite zero-based geometry even at
opposite-sign floating-point extremes, and exact original values in embedded
HTML hover. Worker full qualification passed 990 tests with 21 explicit skips
at `4cb19e48`. Integrated version/package and ordinary hosted verification remain
pending; this is not a release or a universal non-Linux atomicity claim.
When the filesystem lacks exchange, a private no-replace probe precedes the
recoverable backup/commit/rollback fallback. This path honestly discloses a
temporary absent-destination/crash window, retains the old backup on failed
restore and never overwrites a racing foreign destination. Missing safe native
no-replace support fails closed without first moving user data. MacOS native
calls are selected explicitly; Linux fixtures do not qualify a macOS runtime.
Separate confirmed follow-up: bar charts still clamp signed values to zero.
The signed-domain repair above covers line-series axes only; bar baselines,
rectangles, signed ticks and labels need their own offline coordinate tests
and repair before claiming all chart families faithfully display negatives.
The custom BPF scope validator is also a small function (about sixty lines),
not the reported 533; its confirmed correctness defect is tracked below.

## 6. Shared implementation and lifecycle policies

Review repeated sudo consent/probes, three wizard progress-worker patterns,
usage aggregation and original workload grade/inspect/recovery implementations.
Share mechanics while leaving actual agent/provider/workload semantics in
their adapters. Audit mutable globals for necessary scope: atomics for unique
names and synchronized process ownership are not intrinsically defects.
Avoid removing synchronization without an ownership replacement. Pending.

Host prerequisite consent/probes shipped as **v0.2.16**: private
typed requests, consumed approvals, one capability probe per sysctl/cgroup setup,
and explicit independent-versus-dependent failure policies. Sysctls retain
approved interactive fallback; cgroup delegation remains passwordless-only and
requires separate consent. Offline fake-executor tests and the existing isolated
fake-sudo fixtures cover both routes. This repair does not change wizard
worker ownership or change tracefs lease/installation semantics. Independently
reviewed PR #279 merged as `76a604bd5ef850e4aa2b29c6cd0dc031a965403b`;
exact-head ordinary/package CI and both post-merge jobs passed (run
`39631941396`). The signed release points to that exact commit.

Process-gate inspection found an additional launch-contract problem: its
`pre_exec` closure allocates buffers and enumerates the environment after fork
in a multithreaded process. That needs async-signal-safety review and a launch
descriptor/command construction design that performs fallible allocation in
the parent. Rebuilding inherited environment also loses `Command::env_clear`
intent, although no current production caller uses that method; do not claim
an observed credential leak. Tests must preserve explicit environment removal,
clearing, non-UTF8 arguments/environment, I/O, owner death and descendant cleanup.

Separate launch-descriptor repair: PR #286 merged as
`f2e805824c0815c31269b586ebca677c9a71218f`; post-merge verification and the
**0.2.20** release remain pending. The adapter
returns an OS-string client executable/prefix/explicit argv[0] descriptor;
native preparation constructs the gate command once before all configuration.
The same actual `Command` retains standard-library environment clearing,
removals/overrides, cwd, I/O, credentials and process settings. No getter-based
rebuild or private `Command` layout is used. The post-fork ASB callback only
adjusts the private receipt FD with async-signal-safe `fcntl` and captures an
OS error; allocation, environment enumeration and manual exec argument arrays
are gone. A typed target view keeps collectors separate from gate framing;
route validation checks the original carrier and raw framing before any reaper
or child is created. Only its exclusive validated token can establish admission;
replacement/shortened/retargeted commands and direct routes are refused.
bare unprovable PATH identities remain unavailable. The private ASBGATE2 ABI
refuses the prior valid ASBGATE1 Hello before resume. Offline actual-binary
fixtures cover clear/remove/override, child-only PATH, cwd/I/O, raw executable,
argument and environment bytes, explicit client argv[0], current UID/GID,
an owned process group, an existing async-safe callback and launch failures.
This does not expand the separate runtime-fingerprint JSON wire to arbitrary
filesystem byte encodings or fix output-reader memory/wait limits. Full
independent integration review and fresh ordinary/package CI remain required.

Independent review additionally found that wizard installation workers are
detached: a draw error can return while an installer still mutates project
files. Shared progress handling must own worker completion/cancellation, with
a fake-install regression for terminal failure during active work.

Prerequisite native gate custody repair shipped as **v0.2.12**, separately
from the still-pending TUI worker repair.
Its private `ManagedGate` is used by the actual runner ownership boundary:
versioned PID-bound Hello precedes admission, completion requires Quiescent
plus direct exit, startup failure never resumes an unsupported launcher, and
admitted cancellation never kills the supervising gate. A precreated wait-only
fallback retains uncertain custody; the gate retries identity-scoped teardown
and its existing independent watcher handles stopped parent death. This does
not yet repair the TUI workers, the pre-exec allocation contract or stream
memory/reader lifetime. Independent review approved source candidate
`ca4ba33b`; its ordinary offline suite passed 897 tests with 20 explicit
optional skips. Tests include the actual gate's double-forked TERM-resistant
helpers, proof-writer noninheritance and owner loss after Hello, while stopped,
after resume and after the spawning thread exits. Protocol stand-ins separately
exercise malformed/missing receipts, bounded uncertainty and wait-only custody;
they do not qualify kernel teardown. Starting polling errors retain custody
before returning their original diagnostic. Independently reviewed PR #275
merged as `ac80fcb4201cc679afd280b79775b46cfe20ef5b`. Exact-head
ordinary/package CI and both post-merge jobs passed (run `39622744776`);
the signed release points to that exact commit.

Workload-wizard repair, released patch **0.2.19**: explicit browsing, editing
and saved states repair the advertised Enter-to-finish action. Saved subsets
are restored when editing, replaced rather than appended, retained across
deselection, and published only for selected workloads. Every selected subset
must be nonempty and present in the reloaded catalog; all configurations are
validated and serialized before the first write. This is semantic validation,
not a claim of transactional multi-file I/O. Independent source review approved
`6758b426`; a separate execution passed all 39 TUI unit tests and both terminal
integration tests, including ten actual PTY workflow scenarios and terminal
restoration. Worker full offline qualification passed 956 tests with 20 explicit
optional skips. Current-main integration `6af7939b` is independently
source-approved; a fresh full offline suite passed 998 tests, followed by
strict Clippy, formatting and build/docs checks. PR #284 merged as
`d33064f04abd84e89aad7b451efdf369c0cb7400`; exact-head hosted/package CI and
both post-merge jobs passed in run `39637435805`. The released signed tag is
GitHub-verified at that exact commit. Detached installation-worker custody and
other TUI structural issues remain separate repair classes.

## 7. Limits, diagnostics and CLI consistency

Completion candidate: removed hand-maintained command/flag lists and generate
Bash/Zsh directly from Clap's actual public command tree, with existing tool
registry suggestions retained. A private first-argument helper keeps both
`--__generate-completion bash` and `--__generate-completion=bash`; invalid
arity/shells and output failures are tested. Every public command context is
exercised under Bash, and independent review exercised real interactive Zsh
completion as well as both shell syntax checks. New schema entries need no
second completion definition. PR #260 targets patch **0.2.8**, following the
installer repair in PR #259 and BPF scope/startup repair in PR #268.
The current-main coverage regressions remain in
place with an explicit test-module import after removing the old generator.
PR #260 merged as `0897aeb4e4ee5656b80f2e998b52dbe48acee8fe` after final
source/package/install-smoke CI passed in run `39611418478`. Post-merge
verification and the signed v0.2.8 release are tracked in the PR. Coverage is
optional under the user-confirmed policy above, not a remaining release gate.

Unify intended limits (including TUI concurrency versus benchmark maximum),
replace positional pane access and scattered protocol deadlines with named
contracts, and audit shell-completion drift. Preserve signal-exit information
where the wire protocol can represent it. Check error-type consistency,
shadowing and redundant display implementations in their owning classes.
The report's `install::json_string` claim is incorrect: it serializes a string;
it does not swallow a JSON parse error. Likewise matrix `configs[0]` has not
been shown reachable with an empty validated selection. Add regressions before
claiming such cases are fixed. Pending.

## Verification rules

Ordinary CI must use offline/synthetic inputs, not live provider credentials,
privileged collectors or paid requests. New regression tests run in the
ordinary suite and extracted package. Maintain honest null/unavailable
measurements and preserve existing original-task evaluation evidence. A green
focused test does not establish completion of a whole repair class or replace
the required functional source/package and post-merge checks.

Additional safety finding from coverage tests: custom BPFTrace scope validation
can skip multiline probe headers and accepts an OR-true predicate containing
the process-tree variable. This contradicts the documented fail-closed scope
contract. It needs a separate repair class with supported probe/predicate
parsing, multiline/OR regressions, and preservation of valid process-tree
profiles; it must not be hidden as a test-only cleanup. Pending.

Custom BPF candidate: PR #268, patch version **0.2.7**, parses complete multiline blocks and positively
proves each process predicate, rejecting side effects (including map ++/--),
OR-true bypasses, reserved membership-map actions and opaque preprocessing.
Numeric `ASB_PARAM_` tuning replaces the unsupported bpftrace 0.20.2 `-D`
argument with bounded canonical integer expression tokens. Independent review
found and fixed accidental binding inside unquoted attachment paths; provider,
path and symbol fields now remain unchanged. Generic typed per-session state
retains the exact source validated at preflight, including across path
replacement or deletion. No shared orchestration branches on BPF identity.
Custom startup uses the independently reviewed shared `OwnedReaper` dependency
from PR #259 before admitting a child; readiness failure/unwind has bounded
caller-side polling and retains uncertain child custody. Offline tests cover
predicate proof/mutation, multiline/UTF-8/malformed inputs, numeric binding,
frozen bytes, real child identity/reaping and ownership transfer. The installed
tool's unprivileged `-e SOURCE -V` check validates actual option routing only,
not script compilation or privileged attachment. Custom actions remain trusted
code, and other collector lifecycle paths are not claimed repaired. Delivery:
PR #268 merged as `49a8a2b722ed66d4a084ef17f5f373e80b7859cc`. All post-merge
jobs passed in run `39602305053`, with 90.940026% production and 90.542923%
aggregate coverage. **v0.2.7** was released from that exact commit with a
GitHub-verified signed tag.

Installer inspection found that HTTP downloads write directly to the published
destination. A failed/truncated or oversized response can replace a usable
executable with partial bytes while retaining its executable mode. Required
repair: stage downloads, validate size/checksum/exit status before atomic
publication, preserve existing tools on failures, and test readiness queries
with offline fake download commands. This is a verified publication issue,
unlike blanket claims about every ignored scratch deletion.

Delivery: PR #259 merged as `55a3d0d69972707ce2db105f93ddb5806563776f`,
patch version **0.2.6**. Private sibling staging
validates HTTP status, bounded size, optional checksum, executable mode and
version readiness before atomic per-file publication. Existing executable
bytes and mode survive every pre-publication failure. Download and version
children have bounded caller-side cleanup and a fallibly pre-established,
shared private `OwnedReaper`; uncertain cleanup retains child custody,
preserves the original diagnostic and blocks publication. Independent agents
reviewed the production repair and the offline lifecycle tests separately.
Tests cover partial/empty/oversized transfers, checksum/probe failures, local
copies, symlinks, rename errors, blocked publication, worker-creation failure,
poll/deadline handoff, unwinding, real direct-child reaping and an unrelated
surviving sentinel. No live provider, privileged collector or network is used.
This does not claim whole-install transactionality, parent-directory crash
durability, arbitrary descendant supervision or crash-persistent reaping.
Delivery: PR #259 merged as
`55a3d0d69972707ce2db105f93ddb5806563776f`. Every post-merge job passed in
run `39601181355`, including the source/package/install-smoke checks and the
local-host job. Hosted coverage was 90.779707% production and 90.431599%
aggregate. **v0.2.6** was released from that exact commit with a GitHub-verified
signed tag. The broader cleanup and orchestration audit remains incomplete.
