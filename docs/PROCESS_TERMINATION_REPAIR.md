# Optional exact process termination (staged 0.2.42)

Initial implementation follows independently approved design `4e79cadd` on
reviewed setup40 source `f3883ca8`. Version 0.2.42 is reserved staging, not a
release. That initial checkpoint alone is not full qualification or hosted/package
evidence; later combined-source local qualification is recorded separately below.

One serde-validated ProcessTermination preserves full i32 codes and bounded
nonzero signals. A shared optional Observation carries that fact plus a fixed
ancillary diagnostic through capture, guest metadata, execution evidence,
immutable run/measurement records and comparison rows. Legacy carrier statuses,
CLI results and ENDED/READY framing remain unchanged. No signal is inferred
from a shell/Docker code or a requested cleanup signal. Grade-only reports and
historical missing fields remain unknown.

Actual native agents explicitly request observation-only admission using the
existing private optional READY/EXITED channel. Ordinary helper admission stays
ordinary. There is no active interrupt operation for observer admissions, no
collector acceptable-stop policy applied to agents and no new ABI/auth policy.
Post-admission observation errors are sticky unknown diagnostics independent
of base receipt validation. Matching Quiescent and guardian exit retain their
existing cleanup meaning. Optional channel failure on the guardian side does
not replace the actual principal wait status or owner/cancellation supervision.
The parent retains a failed observation socket through base settlement and
finalizes bounded framing without waiting for EOF. Controlled collector callers
still reject missing/invalid exact principal facts while retaining independently
confirmed cleanup; their existing adapter policy is unchanged.

No status-changing AttemptIssues, new completion condition or continuation gate
is added. Exact observation and observation diagnostics are metadata, not extra
permission or failure classifications. New known-fact report conflicts reject
updates without replacing previous reports; unknown is not a conflict or a new
receipt restriction. Guest metadata parsing isolates malformed optional facts
from independently valid quiescence/owner information. Artifact pending/final
publication and frozen native handoff remain untouched.

Owning tests distinguish actual exit137/143 from signals9/15; pair malformed,
missing and trailing/duplicate optional frames with base quiescence; exercise
native true/false/pending grade and observation-fault cases, repeated true/false
batches with/without optional faults, real guest entry plus normal grade-phase
composition, legacy/controller serialization and report agreement. The guest
grade-phase fixture uses the actual registered-adapter guest executable with a
local phase evaluator; it does not claim an original SDK/container/verifier.
The repeated fault switch is cfg(test) only and scoped to an isolated one-test
subprocess owning every worker; there is no production environment fault hook.

The original allocated CLI cache was idle, but the first compile admission
refused before starting cargo because available /srv space was below 3 GiB.
Root then copied only that verified-idle cache into a separate actual ASB
worktree at `/home/martin/projects/asb-build-cache-termination42`. Only its
`target` is allocated for this source checkout; the anchor's old source is not
used to build. Historical copied objects do not qualify current source. The
owned `target/debug` symlink points there; old caches and preserved qualification
binaries are unchanged. Every compilation uses one job, incremental disabled,
locked/offline inputs and a checked minimum 3 GiB available on that filesystem.

An actual ASB build passed before runtime testing. Initial library test
compilation identified a missing Observation initializer in an existing fixture;
`target/termination42-test-compile-initial.log` retains that failure. After the
owning initializer correction, all-target/all-feature test compilation passed.
Additional fixture/validation source changes still require a fresh frozen build
and focused runtime proof. No runtime result or strict-lint/full qualification
is claimed at this initial source checkpoint. No live provider, installed
client, SDK, Docker, privilege or host setting change is required. Native
Windows and other Unix kernels remain unqualified. Independent source review,
focused/strict-lint evidence and explicit full-suite authorization precede
hosted/package CI, main ancestry, merge, post-merge jobs and release.

## Focused checkpoint (not full qualification)

Signed implementation checkpoint `bcdeda35` and its signed follow-ups culminate
in source `1a9a445b41e657d17d5e082cac0b39a99af6e14f`. The last commit is solely
the approved Python-task-loop/Rust-session fixture extension. Production bytes
are unchanged from `7c4af151`; no source was changed during runtime tests.
Actual ASB was built first. Its SHA-256 before/after the broad runtime sequence,
after the final fixture sequence, and after strict lint is consistently:

`a73287e4d0c2883f73b7d66d6d2f32c94ef70f5cad6de5473d1927c511e9b3a2`.

Logs are under this source checkout's `target`, not the stale cache-anchor
checkout. `termination42-matrix-{before,after}.sha256` and
`termination42-final-{before,after,lint-after}.sha256` retain the hashes.
`termination42-matrix-build.log` and `termination42-final-build.log` both
finished successfully; the latter correctly reused the unchanged production
binary after a test-only edit.

| Focused log suffix | Passed | Ignored |
| --- | ---: | ---: |
| matrix-core | 7 | 0 |
| focus-process_gate | 26 | 0 |
| focus-runner-capture | 12 | 0 |
| focus-runner-phases | 34 | 0 |
| focus-measurements | 95 | 0 |
| focus-compare | 75 | 1 |
| focus-sweep-checkpoint | 34 | 0 |
| focus-sweep-recovered | 6 | 0 |
| focus-workloads-remote | 25 | 0 |
| focus-workloads-task_protocol | 7 | 0 |
| focus-guest_agent | 15 | 0 |
| focus-benchmark_protocol | 35 | 0 |
| focus-repeated_run | 30 | 0 |
| focus-capture_session | 2 | 0 |
| focus-agent_artifact_ownership | 2 | 0 |
| final-remote | 26 | 0 |
| final-collectors | 131 | 5 |
| final-workloads-controller | 29 | 0 |

Each filename is `termination42-<suffix>.log`. These total 591 passing test
executions, zero failures and six ignored executions; filters overlap, so this
is not a count of unique tests or a full-suite result. The artifact target's two
outer tests include ten Python scenarios. Native termination exercises 22
isolated full-entry/repeat cases; actual guest execution composed with grade
phases exercises 12 cases. The final actual Python task-loop/Rust-session test
has 18 declared-metadata cases, not 18 actual child terminations. It confirms
true/false and pending-grade preservation/settlement without claiming a real
SDK/container/verifier. Actual code/signal execution is independently checked
by native and guest fixtures. The collector family includes actual requested
SIGINT, strict exit130/control-fact requirements and offline decoder failures.

Strict `cargo clippy --locked --offline --workspace --all-targets --all-features
-- -D warnings`, formatting check and diff check all terminated successfully;
see `termination42-final-{clippy,fmt,diffcheck}.log`. All compilations used the
exclusive allocated root-filesystem target, one job, incremental disabled and
at least 3 GiB available. The latest check had approximately 208 GiB available.
Generated untracked `crates/asb-cli/target` fixture directories are retained as
owning test evidence; they are not source changes or an additional build cache.

### Projection review and failure provenance

The first actual `bcdeda35` core run failed at its code137 streamed-summary
assertion: manually built `render_json` omitted the new sidecar. Its four
other outer tests passed. The exact original failed log
`termination42-bcd-core.log` and before hash are retained; actual binary hash
`9ef39a1f68b5d5333b5589e874f162b7533e07c21fcbe85027eeefdef6e217ba`
was verified unchanged after failure. Signed `e2b07db3` fixes the renderer used
by both streamed and private completion summaries. The next owning native test
then reached comparison and correctly rejected its missing fixture cohort;
`termination42-focused-core.log` preserves that failure (four passed, one
failed), with unchanged actual hash
`2424dda81f8b3076438c455c592b0a71eeb52f82b503f1c2e534337c10cdaa14`.
Its fixture now supplies the same valid single-replica cohort as other owning
report tests. `termination42-report-fix-core.log` separately preserves a test
compilation failure from moved fixture Values, corrected by explicit clones.
These are diagnosed corrections, not unmodified retries or skipped assertions.

Manual projection review covered streamed/private completion, serialized
RunResult logs, full RunMeasurement/history records, guest pending/final
metadata, repeated outcome references and compare JSON/HTML. Tests now assert
private completion summary equality with streamed output. Grade-only import
and closed-slot projection explicitly null exact termination and drop its
observation diagnostic without mutating the original official-grade document.
Comparison coalesces known facts only from already-matched/agreed current
attempt records; unknown observations do not hide matching retained facts.
Known conflicts, malformed present facts and a different attempt object reject
updates while preserving both source bytes and existing JSON/HTML. Reserved
termination sidecars are excluded from generic numeric flattening at every
root, including host/component collector metadata, so codes/signals do not
become performance metrics or graphs. None of these changes adds a receipt,
status, grade, capture or continuation condition.

Independent final source/focused review and explicit authorization are still
required before any full suite. No make/package/hosted/full/new-main integration
qualification, native Windows execution or release is claimed here.

## Official41 integration: source and short focused proof

Root and the independent reviewer approved `c6bdb0bf` before this integration.
Before rebuilding, its actual executable was copied without replacement to
`target/qualification/termination42-c6bdb0bf-asb`; both original and copy matched
the full `a73287e4...` hash above. Historical hashes are not substituted for a
preserved executable.

Signed merge `71e72c372bcc2acddddea9cd6b47ce9b8b785232` combines `c6bdb0bf`
and reviewed official-engine checkpoint `31070b9a`. Only Cargo version lines
conflicted, resolved to staged 0.2.42. The complete incoming production, test and
documentation diff was inspected. Incoming adapter subtree, contracts/baselines
and official-engine documentation are byte-identical to `31070b9a`. All other
tracked paths, including Cargo42, termination, gates, capture, runner,
measurements, comparison, guest/Python/controller, publication and existing
tests, remain byte-identical to `c6bdb0bf`. Successful exact-diff proofs are
`termination42-integration-{incoming,protected}-proof.log`. Root independently
confirmed these protected-tree relationships.

The new combined actual ASB built first (`termination42-integration-build.log`)
on the same exclusive home-filesystem cache. Before runtime, after runtime and
after strict lint it matched SHA256
`a8876a2c2ced0174019dfe1a06dcc6012f4b78dd2cbe83e964e4d0b8b45e164e`;
see `termination42-integration-{before,after,final}.sha256`.

Fresh short focused logs `termination42-integration-<suffix>.log`:

| Suffix | Passed | Ignored |
| --- | ---: | ---: |
| workloads-adapters | 33 | 0 |
| workloads-contracts-baselines | 2 | 0 |
| adapter_layout | 1 | 0 |
| termination | 8 | 0 |
| process_gate | 26 | 0 |
| workloads-remote | 26 | 0 |
| workloads-task_protocol | 7 | 0 |
| collectors-adapters-perf | 50 | 3 |
| guest_agent | 15 | 0 |

Total: 168 passing executions, zero failures and three existing ignored
executions; filters overlap. The termination filter now also includes the
approved actual Python task-loop declared-metadata test. Root's full-suite
authorization is conditional on this combined focus and strict lint passing;
all these handles are now terminal zero. Strict workspace/all-target/all-feature
Clippy (`-D warnings`), formatting and diff checks also passed; logs have suffixes
`clippy`, `fmt` and `diffcheck`. This is new combined-source focused evidence,
not reuse of official41's 1401-test full qualification or a full result for42.
The newly generated untracked `crates/asb-cli/.asb` and prior fixture directories
are preserved. No unrelated source, cache or host state was changed.

## Fresh combined42 full local qualification

After independently reviewing protected-tree merge `71e72c37`, root explicitly
authorized a fresh full suite conditional on the successful combined focus and
strict lint above. Exact qualified source is signed documentation checkpoint
`9b843ee32ce22e2e4c240133aeaf536784b11556`; its production and tests are unchanged
from that reviewed merge. No source edit or executable relink occurred during
the full runtime suite. Post-test lint/build/documentation began only after the
full suite and separate doctest handle were terminal with exit zero.

| Gate at combined42 source 9b843ee3 | Actual result |
| --- | --- |
| Locked/offline workspace all-target/all-feature tests | **1413 passed / 0 failed / 21 existing opt-in skips**, 24 outer targets |
| Library portion | 1177 passed / 0 failed / 13 ignored, 24.31s |
| Other outer targets | 236 passed / 0 failed / 8 ignored |
| Python protocol integration within full run | 35 passed / 0 failed / 0 ignored, 99.87s |
| Separate workspace all-feature doctests | 0 tests / 0 failures |
| Strict workspace all-target/all-feature Clippy, `-D warnings` | passed |
| Formatting and whitespace diff checks | passed |
| Sequential offline `make -j1 build` | passed |
| Private Rustdoc, pinned AutoUML 0.7.2 DOT/Mermaid/SVG, flows and interactive HTML | passed |
| Bash/Zsh completions and separate Bash/Zsh syntax checks | passed |

`target/termination42-full-summary.log` retains the final summary for each
outer Cargo target, not nested one-test subprocess summaries. All 24 summaries
were inspected and summed; no opt-in test was enabled or removed. Fresh evidence
is `termination42-full.log`, `termination42-full-doctests.log`,
`termination42-full-{clippy,fmt,diffcheck,make,bash-syntax,zsh-syntax}.log`.
Both full/doctest and later gate handles terminated zero. No failure or retry
occurred in this fresh combined full qualification. Earlier diagnosed failures
and their original hashes/transcripts remain preserved as documented above.

Before full tests, after full tests, after make/shell syntax and in the preserved
frozen copy, SHA256 is identically
`a8876a2c2ced0174019dfe1a06dcc6012f4b78dd2cbe83e964e4d0b8b45e164e`.
Evidence: `termination42-full-{before,after,final,frozen}.sha256`; frozen copy:
`target/qualification/termination42-9b843ee3-asb`. The distinct pre-integration
`termination42-c6bdb0bf-asb` copy remains unchanged as well.

Build, test, lint, doctest and make used only the allocated home-filesystem
target, locked/offline Cargo, one job and incremental disabled. Every compiler
admission had at least 3 GiB available; final headroom was approximately 205 GiB.
Make used the verified existing pinned AutoUML executable without installing
anything. Graphviz's two ordinary straight-line-edge fallback warnings did not
fail generation. The source checkout's untracked fixture directories and
zero-byte `.asb/install.log` are retained, not silently staged or removed.

This is local offline Linux x86-64 qualification of the reviewed combined41+42
source, not a native Windows/ARM/other-kernel or live provider/SDK/container
qualification. Version 0.2.42 remains staged. Independent final proof review,
actual-main delivery ancestry, exact-head hosted/package CI, merge, both
post-merge jobs and release remain pending; no push or PR action was performed.

Root's independent final proof review approved checkpoint `9db664ef`: its delta
from qualified `9b843ee3` is documentation only, all 24 outer summaries support
1413/0/21 without recounting nested subprocesses, all four hash observations and
the frozen executable match, and separate doctests, strict lint/format and the
complete make/doc/diagram/completion gates passed. Root independently reran both
generated shell syntax checks. This does not replace actual-main ancestry
review, exact-head hosted/package CI, both post-merge jobs or the staged signed
release, and does not qualify native Windows/ARM or live SDK execution.
