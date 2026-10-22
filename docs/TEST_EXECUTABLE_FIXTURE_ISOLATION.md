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

## Stopped allocator full and response-bound fixture separation

Source f8e11fdb actual-first build and six integration targets passed188/0/5ignored;
the same-clock allocator test ran in each. Strict Clippy/fmt/diff passed.
The next authorized full stopped EXIT101 in the library1208/1/13ignored:
the valid exact4MiB response returned "local helper exchange deadline exceeded"
under the table's300ms budget. Evidence is `target/fixture-allocator-full.log`;
actual/frozen68f6 stayed equal. Frozen failed library
`target/qualification/fixture-allocator-f8e11fdb-failed-libtests` SHA256
`fa59f706c7b59d944acd0aa2c34f9564f1caab33144a1449cb726ff582089baf`.
No unchanged-head rerun or subsequent gates occurred; no historical scheduling
cause is claimed.

The approved test-only split separates byte-limit correctness from deadline
rejection: exact4MiB acceptance and one-byte-over rejection get fixed2s (the
neighboring healthy request budget), with the overflow required to report the
actual response byte bound, not any deadline error. All original three negative
controls remain300ms and must return within2s. A new continuously active child
emits at most300 one-byte writes with10ms cadence, append-only records actual
progress, and must fail specifically on the absolute deadline, not byte bound.
Actual reaper receipts settle before marker assertions. No production limits,
admission/cleanup policy, retry, skip or suite serialization changed. This
signed source checkpoint precedes its next focused execution/review.

Root's pre-run followup gives only the NEW slow-activity proof the same fixed2s
healthy startup allowance and4s outer return bound. Requiring actual repeated
child output inside a new300ms budget would conflate startup scheduling again.
Its unchanged at-most300 one-byte writes/10ms cadence take at least3s naturally,
so a deadline reset by activity would reach schema failure rather than the
required deadline error. All original three300ms/<2s negative controls remain
unchanged. No production timer or qualification claim changes.

## Final local qualification at c93d0653 (hosted promotion pending)

Root reviewed every functional delta before the fresh runtime sequence. Exact
signed source c93d0653 built actual ASB FIRST13.90s, then froze
`target/qualification/fixture-deadlines-c93d0653-asb` at SHA256
`68f6b9a811174f1c00cddbc931d339710e4308b5106b17ec5c6f2e6de8db13b0`.
Runtime/checkpoint/helper/reaper/gate focus passed92/0/0; strict workspace
all-target/all-feature Clippy16.76s, fmt and diffcheck finished EXIT0. The six
allocator integration targets had already passed188/0/5ignored at f8e11fdb;
their source remains unchanged. No skip/retry/serialization was introduced.

The authorized NEW locked/offline workspace/all-target/all-feature full suite
at c93d0653 completed EXIT0: **1466PASS/0FAIL/21ignored across24 outer targets**.
`target/fixture-deadlines-full.log` is complete; the compact
`fixture-deadlines-full-outer-summaries.log` selects only the last summary for
each outer Running section, not nested child summaries. Library1211/0/13ignored,
offline_cli98/0/5ignored and repeated_run33/0 all passed. Before/after-full and
after-normal-gates SHA256 logs confirm actual and frozen68f6 remained equal.
All source/executable inputs stayed frozen during runtime; each handle reached
terminal status before the next build/gate. Stopped29bf and f8 full logs and
failed binaries remain immutable evidence, not erased or counted as passes.

After full terminal: separated doctests0/0, strict Clippy, fmt/diffcheck,
sequential make build (private Rustdocs, pinned AutoUML0.7.2, Graphviz type/flow
diagrams, interactive HTML and bash/zsh completions), both shell syntax checks,
and all7 checkout-update tests completed EXIT0. This is the update-test gate,
NOT make update or mutation of the real checkout. Logs use
`target/fixture-deadlines-{doctests,final-clippy,final-fmt,final-diffcheck,make,update-tests,completion-syntax}.log`.

With all runtime/gates terminal, root-approved preservation verified the exact
generated `crates/asb-cli/.asb/install.log` was a regular nonsymlink zero-byte
file, recorded source path/device/inode/mode, then moved it recoverably to
`target/fixture-preservation/install-c93d0653.log`; its metadata record is beside
it. Only the now-empty .asb directory was removed with rmdir. Generated artifact
diagnostics under untracked `crates/asb-cli/target` remain untouched.

Locked/offline cargo package verified compilation EXIT0:511 files,6.0MiB
uncompressed/1.3MiB compressed. Archive source/assets checks include both shared
support files and the immutable mock gate (executable mode and matching digest);
no generated crate target artifacts are archived. Package --help passed.
Packaging legitimately replaced ordinary target/debug/asb with the packaged
source build, SHA256
`e48b5bc834ee8ee1151db38d82cae1837971b4d241f08115b7d87e7d5895da49`,
preserved as `target/qualification/fixture-deadlines-c93d0653-packaged-asb`.
That help-only package binary is NOT the full-suite68f6 binary, which remains
unchanged. Archive SHA256 is
`5e7e02b602f9a66b0ba9b6aeaaa10426aad6947576885e12ea25d9f637cd55e7`.
Exact98 gate/control and73e reaper parity checks still pass.

Scope: local Linux x86-64 offline fixtures; no installed clients, live providers,
SDK/Docker/privileged collectors, Windows runtime or packaged full-test suite
qualification is claimed. Prior optional/live ignores remain. The known
historical CI errno26 holder and historical timestamp-collision identity remain
unproven; only the controlled mechanisms are established. Source/functionality
is frozen for independent root review. Hosted CI/PR update/merge/release remain
pending; no worker push or promotion occurred.
