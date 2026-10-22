# Executable fixture creation: inventory and proposed test-only isolation

Base `e62071fa3d36092b14171851e1a240b4f6282687`, staged0.2.44 unchanged.
DESIGN/READ-ONLY SOURCE INVENTORY ONLY: no implementation, build, runtime retry,
PR update or qualification of a fix. Class47 WIP/cache and class48/runner45
source/caches remain untouched. AGENTS/original report were read; development
and architecture are unchanged from the complete previously read documents.

## Observed failures and precise mechanism boundary

PR310 exact-head run39698910532 failed in perf_source_install.rs:71: its fake
curl could not start with ETXTBSY(os26). The outer library ended1197/1/13ignored
in63.30s. The complete failed transcript is preserved at the root coordinator's
checkpoint-transport target/pr310-failed-ci.log. Earlier local flamegraph and
the coordinator's separate runner gate failures are independently preserved.
No particular historical holder of a writable descriptor has been identified.

A shared parallel test parent opens/writes an eventual executable inode.
Another test thread can fork during that write; its child temporarily retains
the writable description until exec, despite CLOEXEC. Closing the writer in
the creating thread is not proof that every copied description has gone. A later
kernel exec of that SAME inode can return ETXTBSY. This source-level exposure
does not prove that every listed test failed, or that every executable-mode
file is executed. Sleeps, retries and serializing the whole suite are not fixes.

## Confirmed library parent -> isolated case exposures

All anchors below refer to immutable e620 source. Preserve exact script bytes,
paths, modes, cases, environment and assertions; move only executable setup
behind the existing child guard, before any tool admission.

| Owning file | Parent write/mode -> child boundary |
| --- | --- |
| tests/unit/perf_source_install.rs | 16-38 fake curl/tar/make/pkg-config/hasher ->40 exact child |
| tests/unit/perfetto_install.rs | 35-46 fake curl ->48 exact child |
| tests/unit/bpftrace_capture.rs | 70-101 fake tracer;106-114 sudo ->115 exact child |
| tests/unit/project_capabilities.rs | 23-42 sudo ->58 exact child |
| tests/unit/project_perfetto.rs | 204-217 sudo ->218 exact child |
| tests/unit/project_proc_boundaries.rs | 35-56 sudo -> per-case exact child |
| tests/unit/models.rs | 155-164 curl ->166 exact child |
| tests/unit/llmtape_boundaries.rs | 87-88 override ->89 exact child |
| tests/unit/cli2key_custody.rs | 360-369 configured runtime launcher ->374 exact child |
| src/install/operation_native.rs | 668-669 node/npm ->671 exact child |
| src/workloads/adapters/swe_bench/sources/coverage_tests.rs | 46-57 curl ->58 exact child |
| src/workloads/adapters/terminal_bench/catalog/download_tests.rs | 57-68 curl ->70 exact child |
| src/runtime_inputs.rs | 386-388 fake Python ->389 owned exact fixture; helper admission follows in child |
| tests/unit/checkpoint_transport.rs | 262-263 fake Python ->owned exact fixture; helper admission follows in child |
| src/install/coverage_tests.rs | Fixture::script25-28 called by outer host/npm/python/download tests ->Fixture::run32; retain script declarations as data or move each identical case setup into child |
| src/install/source/publication_tests.rs | Fixture::new25-54 fake curl and old tools ->command68; shared by publication/repair/lifecycle families; relocate actual executable creation, not intentional child mutation faults |

The last two constructors serve many tests. Changes must preserve any parent's
initial-byte/path assertions, and distinguish executable images from source
files copied by the product into a different inode. Do not blindly move every
write made by these constructors.

## Confirmed library direct execution exposures

These lack an existing one-case setup boundary. Prefer immutable committed
fixtures when content is fixed; otherwise use one bounded test-only writer
subprocess rather than duplicating supervision or changing product launch APIs.

- src/process_gate/coverage_tests.rs:14-29 mock gate. Reuse already reviewed
  signed98b2c643 unchanged (immutable shell asset plus control EOF fixture fix).
  That checkpoint DOES NOT cover the following ManagedGate launchers.
- src/process_gate/managed/tests.rs:145-146,270-278,307-315,367-368,392-393,
  418-419,449-450 dynamically configured Python carrier images.
- src/launch/tests.rs:145-146 client, then execute at149.
- tests/unit/perf_stop_contract.rs:138-139 installed decoder.
- src/collectors/adapters/perfetto/coverage_tests.rs:33-34 processor helper,
  used by complete/failed capture and timeline query cases.
- src/collectors/adapters/perfetto/service.rs:578-579 mock tracebox. Both direct
  service cases and the owner-loss child use it; the latter is created at908
  before its exact child. Preserve service/lease/socket/sibling assertions.
- src/runtime_inputs.rs:514-515 helper used by direct duplex/limit/fault cases.
- tests/unit/checkpoint_transport.rs:79-80 helper used by direct limit/status/
  schema/fault cases, distinct from the already-isolated deadline cases above.
- tests/unit/runner_phases.rs:1354-1355 guest client in actual code/signal and
  true/false/Pending cases, then actual ASB guest entry at1357 onward.

## Safe contrasts and exclusions actually inspected

- Already isolated before script creation: cli2key_lifecycle.rs:124-125,
  zen_endpoint_metadata.rs:69-70, bpftrace_install.rs:124-125, SWE catalog SDK
  fixture at124, Terminal SDK Python at137, runner full-entry/bundle/sweep phase
  fake clients, and source repair/lifecycle child-only probe mutation cases.
- flamegraph_cleanup.rs:238-240 already creates the same script inside the
  exact case child; preserve this reviewed isolation.
- tui.rs:2470 installer is passed as a DATA argument to /bin/sh at2473; no
  kernel exec of the generated script. Its flamegraph file is readiness-only.
- tools registry, codex/native preparation, declared tape override and launch
  framing negative fixtures only inspect/refuse generated files, not execute
  them. Keep their deliberate admission-negative assertions unchanged.
- OpenJiuwen's usage test writes a script but the owning process() explicitly
  invokes the real Python interpreter and passes the entry as data to its
  compatibility loader. It is not that generated inode's kernel exec.
- tools/runtime.rs:173 produces archive source data; extraction creates new
  runtime inodes before actual execution. Parent writes of the archive source
  do not establish an inherited writable FD for the extracted executable.
- Executable-policy source override files copied by installation and purely
  path/permission lookup fixtures are not automatically same-inode exposures.

## Proposed cohesive helper for remaining library scripts

Add a PRIVATE cfg(test) helper beside test_support, never a production CLI
entry. It reexecs the exact current libtest with one exact writer-child test.
Create OwnedReaper BEFORE spawn. Pass a bounded small request via argv/env
(raw OsString destination, bounded encoded bytes and mode); stdin/stdout/stderr
are null. There is no blocking pipe write/read, reader thread or join. The
writer child alone opens/writes/chmods; it starts no other processes. Its
confirmed successful exit precedes any dependent tool admission. An absolute
short caller deadline includes admission/polling; failure stops only that exact
owned child, with finite cleanup reserve and pre-established reaper retention
on uncertainty. No fallback to parent fs::write, no retries or guessed PIDs.
Attach a child-owning guard immediately after spawn, with nonblocking quiet
Drop adoption on unwind. Clear inherited environment, retaining only explicit
writer request and optional LLVM instrumentation. Helper creation and confirmed
completion must precede the test's intended fault/ObserverScope, because existing
OwnedReaper creation faults and observation counts are part of product tests.
Do not silently disable those hooks or include writer activity in assertions
which are meant to count the product's own admitted children.

Bound the serialized payload before spawn; preserve raw paths and exact bytes,
mode and create/truncate semantics at the selected INITIAL script setup sites.
The helper is not used to relocate intentional concurrent mutations inside
ownership fault callbacks. An uncertain writer must also retain its private
fixture namespace: add a test-only explicit retention decision to the affected
owning fixture cleanup path (including Drop), rather than allowing recursive
cleanup while the helper may still hold a writer. This lifetime detail requires
review before implementation; do not claim a reaper alone settles the path.

Test helper success with exact bytes/mode; refused/oversize request before any
writer; missing destination/permission failure; keeper refusal before spawn;
bounded delayed writer cancellation, failure/unwind and uncertainty handoff;
no dependent executable admission after helper failure; unrelated sentinel
survival. A deterministic fork-retained-writer fixture should prove the original
mechanism rather than infer its occurrence in historical CI. Then run every
owning family, strict lint and actual-first frozen qualification before PR/full.

## Integration-test audit boundary

Concrete analogous shared-parent script creation exists in offline_cli.rs
(many installer/provider/agent fixtures), guest_agent.rs:40-54 and309-323,
completion_diagnostics.rs:45-79, repeated_run.rs:43-86, compare_setup.rs:858-862
and890 (browser opener), planning_admission.rs:75-78 (actual native client).
Those targets have multiple concurrently executing subprocess tests, unlike
the single-real-test sweep/Goose targets plus their non-spawning support check.
These examples are source-confirmed, not an exhaustive individual call-site
audit of every integration test. Do not claim the private library helper fixes
integration targets: they have separate harnesses and cannot use private
production reaper APIs without an explicit test-only design. Existing Python/
Node fixtures launched as separate single-case processes are not equivalent to
writing an executable in the shared Rust parent. No integration/full repair is
claimed by this inventory. Root must approve final scope before implementation.

## Cross-harness implementation refinement submitted before code

Root approved implementation in this new worktree, but explicitly requires the
identified integration exposures to remain in final scope. Existing lib.rs
already includes tests/support/mod.rs as `crate::test_support`; integration
targets include that same file as `support`. No public product API is needed.
Proposed shared tests/support/fixture_writer.rs owns one generic bounded writer
protocol/caller guard, parameterized by a tiny private custody interface. The
library's cfg(test) wrapper supplies its existing OwnedReaper. Integration
harnesses supply a standalone one-slot pre-established keeper in that same
test-only module (Arc/Mutex/Condvar, one actual Child, no competing owner).
Only the keeper worker may wait for an adopted child; no caller wait/join or
new worker in Drop. Unconfirmed child custody is not fabricated completion.

Namespace retention must work across the existing outer/inner process boundary:
a process-local registry alone cannot stop the OUTER Fixture::Drop deleting a
failed INNER writer's files. Use an owned-root pending marker established before
writer admission; remove only after confirmed successful child exit/closed FDs.
Affected owning Drop/explicit cleanup paths inspect that marker and retain the
namespace quietly on any pending/failed writer. A failed marker creation or
bounded request/keeper preflight admits no writer; unknown child error/unwind
retains both actual Child and marker. No recursive cleanup is authorized by a
reaper handoff alone. This small sharing/retention tradeoff was sent to root for
confirmation before implementation; no helper or fixture migration exists yet.

## Observed mechanistic RED before helper implementation

Signed TEST-ONLY `69ff3730` deliberately retains the exact generated script's
writable description in one owned real fork/exec child (child-only fcntl on its
owned descriptor), then closes the parent's writer. It confirms matching full
dev/inode in that child's FD table before attempting the script. An independent
sleep sentinel is owned separately. Both children settle and the retainer's
pidfd confirms exit BEFORE the intended healthy-exec assertion.

Actual ASB built FIRST13.70s, locked/offline/jobs1/incremental0, in the new
worktree's own HOME target. Only verified-idle class48 debug artifacts were
copied beforehand; copied artifacts alone do not qualify this source and the
original cache was not changed. Frozen baseline
`target/qualification/fixture-writer-69ff3730-baseline-asb` SHA256
`637f32d2248cc0e1dc66dcd1b2c58c1ca784820e39651869e83c3ad6f8c5bbca`
matches actual before/after. `target/fixture-writer-mechanistic-red.log` is
0PASS/1intendedFAIL/0ignored, EXIT101,0.01s after23.32s compilation: actual
kernel ETXTBSY(errno26), no script body execution. Retained fixture marker
`target/executable-writer-controls/18e0b30ae7e1191e-1b6a6d-0/cleanup-confirmed`
records `retainer reaped; sentinel_alive=true; exec_errno=Some(26)`.
The actual failed libtest is preserved at
`target/qualification/fixture-writer-69ff3730-red-libtests` (post-run hash only).
This proves the forced mechanism, not the unknown historical CI FD holder.
The helper will retain this as an explicit negative control asserting ETXTBSY,
separate from new positive isolated-writer tests; that assertion change will be
visible in the reviewed diff, not described as an unchanged RED turning green.

## Cohesive migration checkpoint (qualification pending)

The shared std-only writer now serves both harness kinds. Library callers use
the existing OwnedReaper adapter; the six identified integration targets opt in
to the same file and use its one-slot pre-established keeper. No production API,
CLI dispatch, version, timeout retry or suite serialization is added. The shared
keeper retains the actual Child across observation errors until confirmed exit;
the caller remains bounded and never joins it. Root separately assigned the
existing production OwnedReaper wait-error prerequisite: this checkpoint does
not claim that independent library-reaper repair has already landed.

The perf source fixture's identical five script writes/chmods now occur inside
its existing exact child before tool admission. Other identified library parent
and direct-execution sites use the shared writer, retaining their script bytes,
chmods and ownership-fault mutations. ManagedGate initial helper activity occurs
before the intended product Scope, not by disabling product hooks. The exact
three reviewed gate/control paths from 98b2c643 are retained byte-for-byte,
including the executable asset's 100755 mode.

Integration migration includes offline_cli, guest_agent,
completion_diagnostics, repeated_run, compare_setup and planning_admission.
Generic Project.write routes only shebang script payloads through the writer;
data writes remain direct. Repeated-run initial script transformations happen
before admission through that writer, while the actual workload's intentional
input/material mutations remain unchanged. The offline flamegraph automatic
dependency no-clobber fixture's deliberate overwrite of the already-published
perf executable remains an intentional ownership-test mutation, not setup to
rewrite or remove. Existing inner-child script setup stays inner-child setup.
This is the identified inventory, not a claim to audit every integration target.

Affected fixture Drop paths now use conservative marker-aware cleanup (and
retain on unwind); non-Unix branches retain their existing fixture behavior.
The owning cleanup tests cover an enclosing parent root whose nested pending
marker was created by another real process, not the parent's in-process
registry; symlink no-follow preserves a foreign marked directory; exceeding
the bounded scan depth retains the owned namespace. Root approved the polling
and nested-marker corrections at d35e7eee. Their latest tests, all migrated
owning families and strict lint still require fresh actual-first qualification.
The earlier core Clippy failure from wiring the module into unrelated harnesses
is preserved; opt-in wiring replaces that accidental blanket inclusion.

## Observed migration focus before the shared-reaper prerequisite

Signed source d6e77ff5 built actual ASB FIRST (13.57s). Its frozen copy
`target/qualification/fixture-writer-d6e77ff5-asb` and the later
`fixture-writer-1892fbf4-asb` both match actual SHA256
`fed4a298914596699b2a1f46d5ee44f96ccd181aa8e15ba93fc0303b50463dc2`.
The actual-first followup build, before/after-runtime and final hash logs are
`target/fixture-writer-{followup-build.log,followup-before.sha256,followup-after.sha256,migration-final.sha256}`.

The first integration compile failed on the shared test-only UnknownCleanup
variant being unconstructed in that harness. The exact log is preserved as
`target/fixture-writer-integration-offline_cli-compile-failed-d6e.log`.
Signed 1892fbf4 extends the existing local fault-scope restoration test to all
six fault values: no lint suppression or product change. The mistaken
proxy::adapters::cli2key filter selected zero tests; its log is retained and is
NOT qualification. The corrected providers::adapters::cli2key filter passed22.

The 24 nonzero selected family summaries total677PASS/0FAIL/15ignored (family
executions, not a claim of full-suite coverage). They are the18 library family
logs `target/fixture-writer-focus-*.log` excluding that zero filter, and six
`fixture-writer-integration-{offline_cli,guest_agent,completion_diagnostics,repeated_run,compare_setup,planning_admission}.log`
summaries: respectively97/0/5ignored,17/0,8/0,32/0,19/0,9/0. The initial seven
core cases and repeated nine-case core run are not added again to that total.
The latest core run, including cross-process marker/no-follow/depth cases,
is `target/fixture-writer-followup-core.log`:9PASS/0FAIL/0ignored.
Strict workspace/all-target/all-feature Clippy (`-D warnings`), fmt and
diffcheck each finished EXIT0; logs use `fixture-writer-migration-*` names.
`fixture-writer-gate-parity.log` confirms exact98b three-path parity and100755.
The generated untracked `crates/asb-cli/.asb/install.log` is empty and preserved.

This proof does NOT qualify the shared production reaper's observation-error
path, which root assigned separately. No full suite, package promotion, hosted
CI retry, PR update or release is authorized by these focused results.

## Combined-reaper focus and stopped full qualification

Signed normal merge89990014 imports only the reviewed clean reaper prerequisite
73e2f43b (production process_cleanup.rs plus owning tests; no material50 ancestry).
Signed29bf5caf replaces the standalone unknown writer test's numeric PID
disappearance check with a captured OneSlotKeeper completion receipt. Release
is published only after actual Child.try_wait(Some), or no transferred child;
the test waits with Acquire under its existing bounded deadline.
Actual-FIRST combined build13.19s produced frozen
`target/qualification/fixture-writer-29bf5caf-combined-asb`, SHA256
`68f6b9a811174f1c00cddbc931d339710e4308b5106b17ec5c6f2e6de8db13b0`.
All14 requested combined focused summaries passed358/0/5ignored, followed by
strict workspace/all-target/all-feature Clippy15.49s, fmt and diffcheck EXIT0.

Authorized fresh full29bf then STOPPED EXIT101 at repeated_run: line31
create_dir(workloads) returned AlreadyExists(errno17) in
repeated_native_clients_have_fresh_home_xdg_and_temporary_state. Its other31
cases passed. The outer library passed1209/0/13ignored. Full evidence remains
`target/fixture-writer-combined-full.log`; actual/frozen68f6 remained equal.
The failed repeated_run binary is preserved at
`target/qualification/fixture-writer-29bf5caf-failed-repeated-run`, SHA256
`c5814115387ac33ab589807b2edae172976ed2b99ab26f9f3ac8da6cfb50f038`.
No unchanged-head rerun or subsequent gates occurred.

Source confirms the old pid+clock namespace was adopted by create_dir_all(.asb),
and config could be overwritten BEFORE the EEXIST diagnostic. A same-clock
collision is plausible, not a proven historical identity: the failed assertion
did not record the candidate pathname, and no surviving matching root was found.
The corrective test-only allocator claims the case root itself with create_dir,
adding an AtomicU64 sequence to PID/timestamp and allowing at most64 EEXIST
new-name attempts. Other IO errors stop immediately. All six opt-in integration
constructors share this std-only support file, retaining their original parent
locations/prefixes and all cases/payloads. The controlled same-clock concurrent
test compares actual directory dev/inode identities, preserves a preexisting
candidate sentinel, checks64-collision exhaustion/sentinels and one-attempt
non-AlreadyExists failure. This source awaits focused execution and review.
Untracked generated .asb and artifact target evidence remain preserved.
