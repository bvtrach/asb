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
