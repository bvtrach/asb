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

Integrated candidate, staged patch **0.2.31**: the independently source-reviewed
Unix controller transport `eec99cf8` is combined with typed-controller integration
`cf658ba2`, repeated preparation and current capture. It retains the independent
positive resource settlement and normal-close diagnostics instead of converting
confirmed preservation into uncertain cleanup. Initial-material writing/reading
share one absolute budget; shared-launch dispatch cannot borrow its later
execution allowance while sending the preparation request. Python task-loop
deadlines retain the shared preparation budget and reject late dequeued requests.
The existing non-Unix compatibility path receives the same settlement semantics,
but its blocking I/O/Drop remains explicitly unqualified by Unix tests.
Independent source review approved integration `a5aa6e03`; its only minor finding,
a duplicate Python `time` import, was removed at `faf22231`. Initial focused
qualification found one obsolete test accessing a child after correct Unix
fallback handoff; independently reviewed test-only `8b9a009d` instead requires
explicit custody uncertainty and the original pre-established worker's exit.
Focused controller/protocol/remote and actual protocol/repeated-run targets
passed 112 tests. The first full run then rejected the new generic controller
directory through the strict layout guard; independently reviewed test-only
`9e874106` adds exactly that directory and also forbids registered adapter stems
as generic directories. A fresh full all-target/all-feature offline suite at
`9e874106` passed **1,217 tests**, zero failures and 21 explicit optional skips
across 22 targets. Both earlier failure transcripts remain retained.
Strict Clippy subsequently rejected the test's complex tuple. Independently
reviewed test-only `4a3b8647` replaces it with a named `CategoryLayout`, retaining
the same five registry populations, allowlists and assertions without lint
suppression. Its focused layout regression, strict all-target/all-feature
Clippy and formatting passed; the full 1,217-test evidence above remains tied
to `9e874106`, not falsely claimed as a fresh full run at `4a3b8647`.
Sequential offline build/private Rust docs/pinned AutoUML 0.7.2/flows/interactive
views/bash and zsh completions passed. The actual executable SHA-256 stayed
unchanged before/after runtime tests and the final build:
`26627f73727662096c474899ad97cde664f5236b1eaf1bb8c991345f1c30e1c8`.
Signed ancestry merge `e8313859` retains typed-controller delivery head
`940a9d39` and changes only the ledger, not production, tests or Cargo bytes.
Subsequent signed main merge `4074c87b` retains actual main `1bcc8253` with a
tree identical to qualified-ledger checkpoint `3b8e5d4c`.
Hosted exact-head/package CI, PR merge, post-merge verification and release
remain pending. No optional SDK/host/provider qualification is inferred.

Integrated candidate, staged patch **0.2.30**: independently source-approved
typed task-controller contracts `ac8c7753` and historical layout correction
`da9c1f51` are combined with capture checkpoint `8dabf284` and current repeated
execution contracts. The latest matching grade is still consumed before close;
settlement remains independently cached and can positively retain preserved
resources without pretending normal removal succeeded. Typed initial-material
requests and preparation-failure events preserve the new material and launch
gates; agent requests retain the shared preparation budget. Invalid later grades
revoke normal-close permission, never confirmed preservation. The generic
transport still permits untagged retained-inspection responses and drains unknown
in-flight cancellation events. Independent source review approved `c9a21b26`;
the subsequent `cf658ba2` merge adds only the qualified capture delivery ledger.
Fresh all-target/all-feature offline qualification at that source passed
**1,203 tests**, zero failures and 21 explicit optional skips across 22 targets.
Focused protocol/controller/remote, actual Python task-server and repeated-run
checks additionally passed 98 tests. Strict Clippy, formatting and sequential
offline build/private Rust docs/pinned AutoUML 0.7.2/flows/interactive views/bash
and zsh completions passed. The actual executable SHA-256 remained unchanged
before/after runtime tests and the final build:
`c489e15faa664c44df3201f8ddeff8738a6d8feecc3cd93b665efb6414115a41`.
Exact-head ordinary/package CI passed in run `39657670241`; PR #296 merged
as `1bcc8253e563847e8898dd6a4562b6d384220f92`. Both post-merge jobs in run
`39659641538` and release remain pending.
Signed merge `889702ed` retains actual main `b12d8488` with a tree identical
to qualified `64e7c45e`; no production, tests or Cargo bytes changed.
The older pre-main full suite passed 1,100
tests with 21 optional skips, but is historical, not qualification of this merge.
Blocking controller request/cleanup transport is a separate pending class.

Integrated candidate, staged patch **0.2.29**: independently source-approved
bounded capture `823f5081` is combined with newly qualified sweep checkpoint
`1657ae18`, preserving repeated native/official execution, fresh preparation,
resource/artifact contracts and typed sweep/disposition ownership. Independent
integration source review approved `8dabf284`. Focused
offline checks passed 335 tests with five explicit optional skips: 103 runner,
70 comparison, 74 sweep, one shared-isolation test and 87 tests across nine
actual CLI/protocol targets. The actual binary was built first; strict
all-target/all-feature Clippy and formatting passed. Shared launch failures
abort peers before bounded owned-child settlement. Actual repeated-run capture
overflow preserves independent true/false file grades and the already-admitted
sibling, but cannot publish the incomplete slot's outcome or admit batch two.
The unmeasured true/false/native reporting regressions retain resource/artifact
fields and recorded planning/cap metadata without authorizing continuation.
The private native fixture remains test-only, not new SDK qualification. The older
qualification below is historical, not evidence for this new combined tree.
The fresh all-target/all-feature offline suite at reviewed `8dabf284` passed
1,193 tests with zero failures and 21 explicit optional skips across 22 targets.
Strict Clippy, formatting and sequential build/private Rust docs/pinned AutoUML
0.7.2/flows/interactive views/bash and zsh completions passed. Actual executable
SHA-256 stayed unchanged before/after tests and final build:
`3acc60d2381d4f6037159d1a7c41fb5f6ae6d0ed2b9acd64183de388934d59a6`.
Exact-head hosted/package CI passed in runs `39655735876` and `39655737318`;
PR #295 merged as `b12d84882609aae00d16dabd497126348e9ef1ef` after independent
main-ancestry publication review. Both post-merge jobs passed in run
`39657490808`; **0.2.29** is released with GitHub-verified valid annotated SSH
tag `a71266ce3167f68ba49ca3ea4538813eaa396aaa` at that exact merge.
This does not qualify optional skipped host/provider tests,
runtime discovery, or the remaining overall audit.
The subsequent signed ancestry merge retains actual main
`46d20e382b7781c3fb3bf48216e6090ca4b37998` with a tree identical to qualified
`650b3944`; production, tests, Cargo files and the qualified executable are
unchanged. No redundant full test run substitutes for that exact tree check.

Prior pre-main integration qualification: independently source-approved
bounded capture/reporting repair `823f5081` is combined with qualified sweep
phase stack `48609d50`. The original capture-failed unmeasured-sweep regression
remains covered by true/false/native-reward replay fixtures; reporting-only logs
cannot authorize continuation. Saved-mode wire values, joined sweep settlement
and preserved report metadata remain intact. Matching logs can retain original
planning/cap objects omitted by an older private outcome, while conflicting
objects are rejected rather than plotted as measurements. The actual integrated
binary builds; focused runner/sweep/report tests passed 84/73/68 tests with five
explicit opt-in skips, followed by 21 actual CLI tests, strict Clippy and
formatting. Independent integration source review approved `64a44bc1`.
Its full all-target/all-feature offline suite passed **1,090 tests** with
21 explicit optional skips and no failures across twenty targets. Strict Clippy,
formatting and sequential build/private Rust docs/pinned AutoUML 0.7.2/flow
diagrams/interactive views/shell completions all passed. The actual executable
SHA-256 stayed `46792b8401bc9ca7bb0052cbcaac3e2a2c05cc0ea00bb926218d56d74aba1ad4`
before/after the full suite and final build. Hosted/package CI, PR merge,
post-merge verification and a patch release remain pending. This entry neither
closes other audit classes nor restores the removed coverage gate.

The parent consuming sweep-phase integration is staged as **0.2.28**, on qualified
typed-disposition checkpoint `6553da9c`. Independent integration source review
approved `b1459a9f`; its fresh all-target/all-feature offline qualification
passed 1,178 tests with zero failures and 21 explicit optional skips across
22 targets. Strict Clippy, formatting and sequential build/private docs/pinned
AutoUML/flows/interactive views/completions passed. Exact-head hosted/package CI
passed in runs `39653948153` and `39653949094`; PR #293 merged as
`46d20e382b7781c3fb3bf48216e6090ca4b37998`. Both post-merge jobs passed in
run `39655443641`; **0.2.28** is released with GitHub-verified valid annotated
SSH tag `69af3fc8d835876856ff3a19bcd7af30a3b7e326` targeting that exact merge.
The earlier 1,077-test sweep evidence is not substituted for these checks.

The parent typed run-disposition integration is staged as **0.2.27**, on the
independently source-reviewed selection-identity integration **0.2.26**, after
independent main feature PR #250 consumed version 0.2.25 at
`f48953476265e1f77373bb8623f65224c32cb047`. The selection-identity source merge
was conflict-free;
the TUI choice-key, named-pane and concurrency changes remain intact. This
candidate additionally retains the private typed operational disposition and
pending-evaluation failure repair while preserving main's repeated execution,
fresh adapter preparation, workload transport and resource/artifact contracts.
Earlier 1,066-test evidence predates that feature and does not qualify this new
combined tree. For selection-identity **0.2.26**, independent integration review
approved `381a653e`; that parent candidate's fresh
all-target/all-feature offline suite passed 1,167 tests with no failures and
21 explicit optional skips across 22 targets. Strict Clippy, formatting and
sequential build/private Rust docs/pinned AutoUML 0.7.2/flows/interactive views
and completions passed. Executable SHA-256 remained
`1267b651841334b513585b948e3759ca50625345dded2f10c71a90b866c3ba30`
before/after tests and final build. Independent source review also approved the
typed-disposition **0.2.27** integration `0d7cf8a3`; its own fresh full offline
qualification at ledger-aligned `e3c39f6e` passed 1,172 tests with no failures
and 21 explicit optional skips across 22 targets. Strict Clippy, formatting and
sequential build/private docs/pinned AutoUML/flows/interactive views/completions
passed. For **0.2.26**, exact-head hosted/package CI passed in run
`39650217102`; PR #291 merged as
`c3498d273c0793ab21225da8b2117b50b3348ac8`. Both post-merge jobs passed in
run `39651552412`; **0.2.26** is released from that exact commit with
GitHub-verified valid annotated tag `6643f6676d114c1b143ecb251b4eaf56909f1a67`.
For **0.2.27**, exact-head hosted/package CI passed in runs `39651772993` and
`39651776713`; PR #292 merged as `a0be2ed445fddb56231447adc03260607eb76f87`.
Both post-merge jobs passed in run `39653471931`; **0.2.27** is released at
that exact commit with GitHub-verified valid annotated tag
`74934951a88a675e13b74ddf5fa961464ac33f1b`. The parent sweep candidate's
current-main ancestry merge changes no production, test or Cargo bytes from
qualified `1657ae18`.

Integrated repair, released patch **0.2.21**: the independently reviewed
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
not that qualified production/test tree. Exact-head hosted/package CI passed;
PR #285 merged as `b236ae9fb6aae3789d5623e6d0c9c357a0654a0f`.
Both post-merge jobs passed in run `39640745540`; **v0.2.21** was released
from that exact commit with a GitHub-verified signed tag (verification `valid`).
This delivery does not close the other audit repair classes.

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

Bounded child-capture candidate, staged patch **0.2.29** (original source
`823f5081` independently approved; integration `64a44bc1` source-approved and
offline-qualified above; hosted delivery and release pending): the common runner no longer accumulates an
unbounded channel/transcript, treats read failures as EOF, eagerly waits after
pipe closure, or leaves blocking reader threads behind. Owned nonblocking pipe
pumping caps evidence during capture, preserves the confirmed child-exit clock
(including the existing native-gate quiescence boundary), and separately bounds
execution, post-exit drain and direct-child cleanup.
Pre-established fallback custody retains an unconfirmed direct child. Native
presentation failure preserves evidence; Unix quiescent-guest controller pipes
use owned deadline-aware transport and cancel on failure. Independent official
grades, adapter-file metrics and validated cassette usage survive capture
uncertainty, while transcript-derived claims and continuation receipts do not.
Unmeasured sweeps retain those independent grades through a validated, bounded
latest-isolated-log reporting fallback; source transcripts never enter reports,
conflicting latest attempts fail before publication, and no resume/outcome or
completion receipt is created. Actual sequential sweep fixtures exercise both
binary grades, nullable native rewards and refusal to resume without retry.
Borrowed writers cannot be made generically cancellable; non-Unix guest writes
retain that limitation. Windows pipe pumping is source-only here, not native
runtime qualification. Non-Unix/non-Windows direct execution fails before spawn
until a platform backend exists. Real-child, fault, custody and replay full-entry
regressions cover these boundaries. Retained ThroughEvaluation services and
ancillary adapter-file deletion diagnostics remain separate classes. This
candidate does not close the overall cleanup or driver/orchestration audit.

## 4. Typed domain and wire contracts

Review run status/evaluation/failure categories, execution mode, result source,
controller messages and internal dispatch constants. Use typed enums or
central constants where they enforce a real contract, preserving existing
serialized representations and backwards reading. Add malformed/unknown
protocol fixtures, round-trip and compatibility tests. Trial-set display
suffixes should not serve as hidden identifiers. Pending.

Run-disposition candidate, provisional patch **0.2.27**: an actual
inconsistency allowed pending evaluator output to report `evaluation-pending`
while collector/presentation errors recorded a primary failure. Private
`RunDisposition::from_facts` now derives status, typed primary failure and
operational success together, preserving existing failure precedence and wire
strings through `RunStatus`/`FailureKind`. Evaluation outcomes, numeric rewards,
nullable resolution, legacy grade fields, schema and nullability remain intact;
public JSONL summaries do not gain status. Workload evaluation-kind extension
strings and unrelated protocols are deliberately not closed by this class.
Measurement producers use the typed decision; reports disclose recorded failures
even when legacy measurement `success` preserves a true grade, and avoid inventing
a run failure for healthy pending evaluation. Historical categories remain
readable/display-only with unknown grader scopes unscored. The offline matrix
covers true/false/native/pending/error outcomes and independent operational
faults. Twelve actual replay-only full-entry cases bind logs, measurements,
private outcome records, report JSON/HTML and return errors while requiring
evaluation and cleanup exactly once even under broken presentation. Existing
completed-outcome validation remains authoritative for reusable receipts.
Focused offline qualification passed: runner 73 tests (three optional skips),
report pipeline 27 (one optional browser skip), completed-outcome invariants 28,
actual replay CLI two, continuation-diagnostics CLI six and unmeasured-sweep CLI
two. The binary stayed unchanged throughout the final focused run. Strict
all-target/all-feature Clippy and formatting pass. Independent source review
approved `5c4ed922`; its full offline all-target/all-feature suite
passed 1,068 tests with 21 optional skips, followed by strict Clippy, formatting
and build/docs/diagrams checks. Integration with the qualified `656aeed1`
identity/provider/report/checkpoint stream preserved the typed disposition
contract and recorded planning metadata. The new integration with reviewed
`381a653e` additionally retains main's fresh repeated execution, official material
preparation and transport paths, independent resource settlement and artifact
references. Its output-loss path retains the I/O class needed to stop later
batches even when another failure has diagnostic priority. Earlier full evidence
does not qualify this newly combined tree. New focused offline qualification
passed 438 tests with five explicit optional skips: 91 runner, 145 workload,
69 comparison, 68 sweep, 35 Python protocol wrappers, 14 actual guest-driver
and 16 actual repeated-run tests. The actual binary was built first; strict
all-target/all-feature Clippy and formatting passed. The twelve disposition
full-entry fixtures also require agreement on resource disposition and artifact
references across logs, measurements, summaries and private outcomes.
Independent source review approved `0d7cf8a3`. The subsequent parent-evidence
merge `e3c39f6e` changes only this ledger; all production, tests and Cargo files
remain identical to that reviewed source. Its fresh all-target/all-feature
offline suite passed 1,172 tests, zero failures and 21 explicit optional skips
across 22 targets. Strict Clippy, formatting and sequential build/private Rust
docs/pinned AutoUML 0.7.2/flows/interactive views/completions passed. Executable
SHA-256 was unchanged before/after tests and final build:
`b65b52f105980cc7d1cd74953c9e04f418c36d2e95eda35abc52c5ccc3464c5d`.
Exact-head ordinary/package CI, merge, post-merge verification and release remain
pending. No live-provider/host qualification is claimed.
This is not a controller/dispatch protocol rewrite, workload-kind registry
closure, live-provider qualification or delivery of every string-typing item.

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

Sweep-phase candidate, provisional patch **0.2.28**: audit
found that sequential JSON writer failures still admitted later configurations,
and early post-activation diagnostic failures skipped final input settlement
and comparison. Consuming prepared/fresh-or-resumed/activated/settled phases
now retain validated planning/resources, lock lifetime, joined cleanup and
partial reporting. A sticky writer distinguishes real output failure from
benchmark failure without parsing error strings. Worker-side typed spool
failure stops admission before its next claim; already-admitted siblings finish.
Cleanup is authorized only for spools created by this invocation, preserving
unadmitted interrupted-resume output and failed-preservation/creation targets.
Independent preliminary review found an Interrupted-I/O retry loop in the
initial writer wrapper; Interrupted is now transient, with deterministic retry
and subsequent-permanent-failure regressions. A fixture initially assumed JSON
newlines were separate writes; it now covers the actual concurrent buffered
write boundary. Those intermediate failure logs remain diagnostic evidence.
Nine actual replay-only scenarios bind client counts, grades, completion
metadata, plan fingerprints, final seals, partial reports and returned errors;
the output lock is checked through final comparison. Existing native outcome,
resource admission and checkpoint reconciliation invariants are preserved.
Saved execution mode is also typed at this admission boundary, keeping all four
wire values, missing historical unknown mode, rejection of null/unknown spelling
and replay-only timing. No live-provider/host qualification,
transactional preservation, arbitrary crash recovery or delivery of the other
orchestration/protocol repairs is claimed. Focused qualification passed: 73
sweep-library tests (one optional skip), 72 runner tests (three optional skips),
the shared isolation helper and 21 tests across six actual CLI targets. Strict
all-target/all-feature Clippy, formatting and unchanged executable hashes pass.
Independent final source review approved `17b97150`; no redundant full suite was
run on that pre-integration head. The candidate now includes the independently
qualified disposition/report-metadata and delivery-ledger checkpoint `a0dea95b`.
Declared planning, controller caps and module revisions remain recorded metadata
outside observed charts; typed sweep modes, output admission stops, spool
ownership, native outcomes and continuation obligations remain intact. Fresh
integration source review independently approved `e8ca48ad`. Its full offline
all-target/all-feature suite passed 1,077 tests across 20 groups, with 21 explicit
optional skips and no failures. Strict Clippy, formatting, sequential offline
build, private documentation, pinned AutoUML 0.7.2, function-flow diagrams and
shell completions passed. The actual executable SHA-256 remained
`25ad2738aa4261810672403085bef3a424a0495e517db43017e48bb9e1e41fb7`
before and after the full suite and build. Exact-head ordinary/package hosted
CI, merge, post-merge verification and release remain pending. The new merge
with qualified `6553da9c` preserves current main's repeated execution, material
preparation, resource/artifact fields, output-loss I/O stop policy and new runner
options alongside the consuming sweep phases and typed saved execution mode.
Fresh focused offline qualification passed 320 tests with five explicit optional
skips: 74 sweep, 90 runner, 69 comparison, one shared-isolation test and 86 tests
across nine actual CLI/protocol targets, including repeated-run, guest driver,
official protocol, planning admission and measured/unmeasured sweep paths.
The actual binary was built first and its hash stayed unchanged; strict
all-target/all-feature Clippy and formatting passed. Independent integration
source review approved `b1459a9f`, including the docs-only current-main ancestry
merge. Its fresh full offline all-target/all-feature suite passed 1,178 tests,
zero failures and 21 explicit optional skips across 22 targets, followed by
strict Clippy, formatting and sequential build/private Rust docs/pinned AutoUML
0.7.2/flows/interactive views/completions. Actual executable SHA-256 stayed
unchanged before/after tests and final build:
`397f8ff5e13c5792dbd01598841ddef686e662bf3d1a6f100d939676d92a2652`.
Exact-head hosted/package CI, merge, post-merge verification and release remain
pending. This does not close the remaining audit or runtime-discovery repair.

Checkpoint repair, released patch **0.2.22**: completion group/replica sealing, partial continuation
and retained-outcome reconciliation now consume a private validated outcome
instead of passing an unchecked wire record after a long Boolean expression.
Named protocol/cleanup, cohort, identity, grade/baseline and evidence-path
invariants retain schema-1 group compatibility, schema-2 slot requirements,
dataset/evaluator bindings and false grades. Independent mutation fixtures
exercise every invariant through sealing, continuation and reconciliation,
requiring no receipt publication; actual replay-only CLI rejection checks keep
client-launch counts and historical receipt bytes unchanged. Removing the
invalid native member's group/slot receipts additionally proves public CLI
unsealed-slot validation reaches the exact named invariant before allocation
inspection, without republishing those receipts. Earlier valid native seals
remain intact; all-candidate publication ordering is covered separately by the
allocated unit fixture, not claimed as original-runtime qualification. Independent
source review approved `9d0c6f3f`; a separate execution passed all 23 checkpoint
tests and both public CLI tests. The worker's stable-binary full qualification
passed 933 tests with 20 explicit optional skips, followed sequentially by fmt
and strict Clippy, with unchanged executable hashes. Final integrated source
qualification passed 1,041 tests with 21 optional skips, followed by strict
Clippy, formatting and build/docs checks. PR #287 merged as
`4224472f5fccb3a5651593fff8fbfd8f0b557a60`; exact-head hosted/package CI and
both post-merge jobs passed in run `39642527007`. **v0.2.22** was released
from that exact commit with a GitHub-verified signed tag. This does not close
other structural items.

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
at `4cb19e48`. Final integrated qualification passed 1,036 tests; delivery is
recorded in the released **v0.2.21** entry above. This is not a universal
non-Linux atomicity claim.
When the filesystem lacks exchange, a private no-replace probe precedes the
recoverable backup/commit/rollback fallback. This path honestly discloses a
temporary absent-destination/crash window, retains the old backup on failed
restore and never overwrites a racing foreign destination. Missing safe native
no-replace support fails closed without first moving user data. MacOS native
calls are selected explicitly; Linux fixtures do not qualify a macOS runtime.
Historical follow-up, resolved before **v0.2.21**: bars also required signed
baselines, rectangles, ticks and labels. The final signed-scalar implementation
and offline coordinate/extreme-value regressions cover those bar cases in
addition to the line-series axes; the earlier line-only candidate was not
treated as completion evidence.
The custom BPF scope validator is also a small function (about sixty lines),
not the reported 533; its confirmed correctness defect is tracked below.

Private task-controller contract candidate, staged patch **0.2.29**: typed Rust requests and event kinds
and Python operation/event identifiers preserve the existing JSON wire. Validated
ready and completed-execution views retain selected-instance/baseline/budget,
guest metadata, output and quiescence/delegated-owner obligations. A private
matching-completed-grade token replaces the official session's writable close
flag: every evaluator request clears it first, and normal close consumes it.
False grades remain completed grades. Separate normal-close/cancellation ack
validators preserve stopped-preservation versus destructive removal, while
cancellation still drains unknown/in-flight events. Generic untagged inspection,
extension evaluation kinds and structured/native outcome APIs stay unchanged.
Constructor matrices and actual Python-task-loop/Rust-session fixtures require
no Docker, installed SDK, real agent or provider. Original source `ac8c7753` is
independently approved; integration review, full qualification and delivery
remain pending. Qualified typed saved sweep modes are retained; internal command
identifiers remain separate work. Blocking controller request writes,
Python nominal-interval deadline accounting and controller Drop custody are not
repaired by those typed contracts and must not be claimed bounded there.

Separate Unix controller transport candidate: actual regressions first reproduced
a one-second blocked stdin write despite a 100 ms request budget, an escaped
stdout reader after Controller destruction with an owned writer retained, and
Python polling past its fake monotonic deadline. The Unix implementation now
uses nonblocking owned process pipes and a bounded caller-owned duplex pump,
without a reader thread. Write/response and acknowledgement/exit operations
share absolute deadlines; failed close cannot restart the cleanup clock.
Direct-child fallback custody is fallibly established before spawn. Explicit
cleanup and destruction poll within their deadline and hand uncertain children
to that existing keeper, never an unconditional caller wait or new Drop worker.
Python uses monotonic elapsed time and rejects late queued requests. Exact
wire/grade-token/normal-removal versus preserving-cancellation contracts remain.
Owned-child, pipe-pressure, framing, sentinel, pidfd, setup-failure and unwind
fixtures require no SDK, Docker, real agent, provider or host changes. Independent
review, full qualification and delivery remain pending. Non-Unix retains the
legacy blocking implementation and remains explicitly unqualified for this
bounded transport guarantee; this is not a completed portable lifecycle repair,
arbitrary descendant supervision, container recovery or a hard OS scheduling bound.

cli2key startup/session candidate, staged patch **0.2.23** after checkpoint
**0.2.22**: explicit external versus local ownership
keeps the shared fallibly pre-established `OwnedReaper` through successful
session transfer. Both children enter immediate custody; each nonblocking
output worker is created fallibly before its child. Readiness has byte, line,
message, channel and time bounds, followed by deliberate drain/discard for the
session lifetime. One cleanup deadline covers both children/readers; uncertain
children transfer to the existing worker, and startup errors retain cleanup
diagnostics. Finished readers alone are joined; stopped workers retain their
transport until they exit. Offline fixtures check limits, silence, creation
failure, both spawn error/unwind boundaries, pinned direct-child reaping,
deadline/poll-error handoff, retained output writers and sentinel survival.
External endpoint/key behavior and two-listener concurrent authenticated
requests retain their regressions. This repairs provider-owned direct-child
custody and readiness, not other provider/collector lifecycle paths, arbitrary
descendants or crash recovery. Independent review and delivery remain pending.
The shared provider-session interface now includes an explicit default no-op
finish contract. cli2key consumes its local owner exactly once and reports
uncertainty; generic model discovery always finishes after fetch/parse, while
the runner closes after grading/workload cleanup and before publication.
Provider cleanup failure is ancillary to client/grading failure, preserves the
official grade and prevents a continuation receipt. An early-error envelope
also finishes after preparation/execution errors. Private replay-only full-entry
fixtures and loopback catalog fixtures verify success, independent true/false
grades, client/preparation/fetch/parse failures and finish uncertainty without
duplicate cleanup or retry. Worker receipts distinguish cooperative reader exit
from releasing an unfinished handle after the caller's shared deadline.
Independent review rejected the initial candidate's last-child-only readiness
observation: the required bridge could exit while the compatibility hop was
starting. Final admission now checks every owned child, with a deterministic
two-child/pinned-identity regression. Self-review also retained the embedded
hop's cooperative outbound cleanup: stdin lifetime EOF starts shutdown, which
observes declared curl close events before success. Its at-most-two-second
grace consumes the existing shared five-second budget before direct-child
escalation. Real embedded-hop/held-loopback/curl and ignored-EOF fixtures verify
both paths; forced or unsuccessful cooperative exit reports uncertainty.
Follow-up review found that incoming-body collection previously escaped the
proxy handler's error envelope. The complete handler now catches body/URL/
forwarding errors, checks closing before forwarding and avoids late 502 writes
to aborted/destroyed responses. Actual partial-POST abort/shutdown fixtures
retain a sibling request and require cooperative exit/reaping. A full-suite
concurrent-proxy startup exit remains under investigation with bounded fixture
stderr; a passing isolated rerun is not claimed as its diagnosis.
Independent source review approved `bdf219d4`; root separately passed its 18
cli2key library tests, two provider-finish matrix entries (including five actual
runner cases and loopback catalog cases), and four actual CLI regressions.
The signed `ad5ce506` integration retains the structured native outcome/report
pipeline and bounded model transport from **0.2.19**; root semantic review
approved its generic finish/native cleanup wiring. Seven actual runner
finish cases now include completed numeric rewards with null resolution:
provider uncertainty preserves those rewards, legacy grades and execution
success, sets structured cleanup confirmation false and withholds receipts.
The approved structured checkpoint constructor from `3fa5239c` (**0.2.20**)
is retained without restoring Boolean-only native outcome requirements.
Final integrated source review, full stable-binary qualification, package and
hosted CI, merge and release remain pending. The earlier `93babfdd` full-suite
proxy-startup failure remains unattributed; bounded fixture stderr and 100
passing fresh concurrency fixtures are diagnostic evidence, not proof of its
cause or repair. No live-provider qualification is claimed.
Follow-up reporting review found that a healthy-native compatibility exception
also hid recorded cleanup failures. It now applies only without a recorded
failure kind/reason or explicit cleanup failure; unknown cleanup alone does not
invent a failure. Reports generated from the actual native finish fixtures
retain the numeric reward/null resolution and disclose provider uncertainty in
both row status and HTML, while healthy native completion stays completed.
Root source review approved `f7b7bb9d`; that immutable pre-planning integration
passed the full offline suite (1,033 passed, 21 optional skips), strict Clippy,
formatting and sequential build/documentation gates with unchanged binary bytes
through the suite. The provisional **0.2.21** integration now also includes
the exact planning/report checkpoint `c893de62`: admission policy, controller
budgets, persisted policy/task plans and computed protocol fingerprints remain
upstream-owned, while explicit provider finish still precedes publication and
gates cleanup confirmation and receipts. Actual finish fixtures verify saved
plan/policy/budget consistency and fingerprint propagation alongside native
reward preservation. That planning integration, `fbd0d69d`, subsequently
received independent source approval and passed 1,049 offline tests, strict
Clippy, formatting and sequential build/docs checks with stable executable
bytes. The current **0.2.23** candidate also retains the approved report and
checkpoint integration `1fb8c542`, including declared planning metadata outside
measurement charts; provider, runner and model-discovery source is unchanged
from `fbd0d69d`. Independent current-integration source review approved
`c291a808`; its full offline suite passed 1,052 tests with 21 optional skips,
followed by strict Clippy, formatting and build/docs checks. PR #289 merged as
`73f3ce4768ef9f6dec06a25884c19228683bbe26` after exact-head hosted/package CI.
Post-merge run `39644360265` passed both `checks` and `local-host-checks`.
Patch **0.2.23** is released at that merge commit; GitHub verifies annotated
tag `a073a80e836aa4911c5a11931dcdde57a26926a0` as valid.

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

Separate launch-descriptor repair shipped as **v0.2.20**: PR #286 merged as
`f2e805824c0815c31269b586ebca677c9a71218f`. Exact-head hosted/package CI and
both post-merge jobs passed in run `39638964433`; its released signed tag is
GitHub-verified at that exact commit. The adapter
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

Detached TUI installation custody candidate, provisional patch **0.2.24**:
the source-approved provider/report/checkpoint integration, including recorded
planning metadata, is merged with the independently qualified TUI custody repair.
All three progress-spinner paths
now share an owned helper process supervised through the existing native gate,
with bounded, credential-free fixed datagrams and explicit cancellation/cleanup
results. No blocking installer is assumed cancellable through a thread flag.
Successful helper status additionally requires native descendant quiescence
and refreshed readiness before selecting an item. A terminal guard independently
attempts restoration after startup/draw/input failures and unwinding, preserving
the original error alongside cleanup diagnostics. Offline actual-binary fixtures
exercise real EPIPE rendering with PTY input, Ctrl-C/Esc, owner/helper death,
slow TERM-resistant double-forked writers, unchanged unrelated sentinels and
absence of writes after confirmed cleanup. Other fixtures cover install success
and failure, helper framing, descriptor noninheritance, running-image pathname
replacement, keeper failure and unwind custody. Existing trial-wizard regression
coverage remains required. This repair does not establish
transactional installation rollback or bounded disappearance of unkillable
kernel tasks. The user-confirmed removal of the CI coverage gate is retained.
Independent source review approved the integrated `51e30654`; its full offline
suite passed 1,062 tests with 21 optional skips, followed by strict Clippy,
formatting and sequential build/private docs/diagrams/completions. Current main
ancestry changes only this ledger, not that qualified production/test tree.
Exact-head hosted/package CI passed in run `39646000398`; PR #290 merged as
`bb33f4007a6309c248239c6ee0080c1baeeddd3b`. Both post-merge jobs passed in run
`39647416921`. Patch **0.2.24** is released at that exact commit; GitHub verifies
annotated tag `effaad57f913077dc017eb5115e97606356a5c7c` as valid.

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

TUI choice-identity candidate: selections now bind typed agent/provider/model,
prompt, trial-set filename, collector, concurrency and trial-ID keys rather than
rendered labels. Colliding prompt/trial-set labels and literal repeated suffixes
retain distinct identities; reserved `all` is checked against the actual name.
Named pane kinds replace positional navigation/installation decisions, and the
concurrency pane uses the shared benchmark maximum of 64. Local regressions
cover every generated numeric concurrency, label changes across domain consumers,
saved trial-ID reopen and actual PTY publication of 64. The source-approved
identity/concurrency repair is integrated with the source-approved provider,
report, checkpoint and TUI-custody stream at provisional patch **0.2.25**.
Integration-specific source review and full delivery qualification remain
pending; installer custody and recorded planning metadata are preserved.

Replace remaining scattered protocol deadlines with named contracts and audit
shell-completion drift. Preserve signal-exit information
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
