# Collector custody integration (staged 0.2.39)

Source-only integration combines independently approved collector/perf
`07084a9a` with corrected runtime/installer/raw/artifact/publication stack
`2f30840d`. Version 0.2.39 is coordinated staging, not a published release.

The merge had no textual conflicts. Collector producer/control/snapshot,
owning perf/bpftrace/Perfetto lifecycle code and their tests are retained from
the approved collector source. The three shared runtime seams retain both sides:

- Collector usage still consumes the incoming already-frozen artifact document,
  not an agent-file pathname re-read; the incoming token-usage adapter and mock
  tests are unchanged.
- Measurements keep document-based token extraction alongside typed producer
  completion, shared stop deadlines, raw retention and truthful exit signals.
- ManagedGate retains the incoming owned-command stdin pipe API needed by the
  duplex runtime-discovery exchange, plus optional bounded principal control
  and the narrow proof-retaining cancellation operation for collector workers.

No runner/recording, agent-artifact, runtime-input or installer production file
changes relative to `2f30840d`. Thus bundled recording/replay associations,
selected private tracks, immutable result publication, resource dispositions,
typed grade permissions, native frozen artifact handoff and prepared installer
input/publication remain incoming code, not replacements from the older base.
The existing opt-in tests and removed coverage-threshold decision are preserved.

Root independently approved `07084a9a` after inspecting ordinary producer
ownership, fixed control protocol, actual pipe-pressure/dependency retention,
bounded readiness, owning perf deliberate-stop validation and focused evidence:
88 measurement, 126 collector and 23 guardian tests passed (five existing
collector opt-in skips). Those 237 passing tests belong to that exact historical
tree; they do not qualify this new stack. The earlier source, observed-red and
failed intermediate transcripts remain in the original collector worktree.

No build, test, cache relink, package/host mutation, push, PR or release accompanies
the initial source-only integration. Root independently approved `1d409705`.
The next source-only merge folds PR #300's reviewed fixture teardown/ledger head
`e284b2b7`: only the owned journaled directory permissions are restored after
scenario assertions; no recursive chmod, production, Cargo or workflow policy
changes are introduced. The staged version remains 0.2.39. Runtime38 full
qualification runs separately. Fresh combined focused proof is now authorized
in the exclusive diagnostics cache, with the historical collector executable
preserved first. Full execution still requires a subsequent parent review and
explicit clearance.

Linux offline normal-producer custody is the reviewed scope. This does not
claim live privileged collector qualification, native Windows/other kernels,
all one-shot preflight/postprocessor process lifetimes, a hard kernel-file-I/O
deadline or a hostile same-UID filesystem sandbox. Future proof must preserve
the service lease until matching guardian quiescence, not merely principal exit,
acknowledgement, caller timeout or generic reaper handoff.

## Fresh integrated focused qualification

At signed source `ec8015e3`, the reviewed permission-teardown fixture head is
folded and version remains 0.2.39. Before any build, the diagnostics cache was
confirmed idle and the old qualified collector executable was preserved as
`asb-collector-custody/target/qualification/collector-07084a9a-asb`, with its
original `e4c4072dd221e49c38da1c222f97f451729eae971c2644124546fbcf767e41a1`
SHA-256 verified. The new actual ASB executable was built first, then remained
unchanged throughout the focused runtime and lint gates at SHA-256
`d916dafa3c619940177308dad57094a30143218f90a72e2e9caac5acbfc15c10`.

All requested focused families passed: measurements 88, exact collector namespace
126, guardian protocol 23, runtime discovery eight, recording/raw 12, bundle
publication two, actual artifact integration two and repeated-run integration 30.
Five existing collector opt-in cases stay skipped. The shell batch accidentally
used the broader `collectors` substring instead of `collectors::`, so its outer
result is 131 passed: the 126 intended tests, two measurement tests already in
the 88-test family, and three additional planning/TUI/comparison regressions.
Thus the eight outer summaries total 296 passing executions but **294 distinct
target/test cases**, not 296 distinct tests. The common support test in the two
integration targets is counted once per actual Cargo target, as usual. There
were zero failures; no failed runtime test was retried. The two owning bundle
tests cover all their isolated record/replay/artifact/publication cases without
counting those child scenarios as extra outer tests.

Strict locked/offline workspace/all-target/all-feature Clippy (`-D warnings`),
formatting, whitespace checks and before/after actual executable hash comparison
passed. Only the exclusive diagnostics cache was used; incremental compilation
was disabled, subsequent compilation used one job after the already-admitted
two-job initial build finished. Disk headroom was checked before new stages and
stayed above the parent's 3-GiB admission floor. No other cache was cleaned or
relinked, and no live provider, privileged collector, host permission or SDK was
used. The owning `target/debug` symlink was created only in this new worktree.

Transcripts are `target/collector39-{build,measurements,collectors,process_gate,
runtime_inputs,runner-recording,runner-phases-tests-bundle_publication,
agent_artifact_ownership,repeated_run,clippy,fmt}.log`. The generated
`collector39-focused-summary.log` retains the eight actual outer summaries;
`collector39-{before,after}.sha256` retain matching executable hashes. All tool
handles are terminal. The packaged-layout fixture's generated untracked
`crates/asb-cli/target/` evidence is intentionally preserved, not committed.

This checkpoint changes documentation only. Fresh full execution remains held
for root review/clearance; build/docs/package gates, exact-head hosted CI,
merge/post-merge validation and release remain separate obligations. Earlier
collector, raw, installer and runtime qualifications are not substituted for
this new combined tree's remaining gates.

## Fresh full local qualification

Root independently approved focused proof `8ca779ca` and authorized the full
locked/offline workspace/all-target/all-feature suite at that exact signed
source. It passed **1,361 tests, zero failures, 21 existing opt-in skips** across
**24 outer Cargo targets**. The library passed 1,128 with 13 skips; the other
targets passed 233 with eight skips. The summary keeps the final outer result
per Cargo target and does not count the three nested one-test subprocess
summaries again. No failed full run was retried, and no source edit or binary
relink occurred while runtime tests were active.

After the full handle was terminal, separate doctests passed (zero cases),
strict workspace/all-target/all-feature Clippy (`-D warnings`) and formatting
passed, then sequential offline `make -j1 build` completed the actual build,
private Rustdoc, pinned AutoUML 0.7.2 type diagrams, data/control flow diagrams,
interactive HTML views and Bash/Zsh completions. Both generated shell syntax
checks passed. Graphviz emitted two normal straight-line layout fallback
warnings, not failed gates. The existing shared pinned UML executable was only
invoked; no tool installation, foreign-cache cleanup or host mutation occurred.

The actual executable remained SHA-256
`d916dafa3c619940177308dad57094a30143218f90a72e2e9caac5acbfc15c10`
before/after full tests and after every subsequent gate. A frozen copy is retained
at `target/qualification/collector39-8ca779ca-asb`. Full compilation used the
exclusive diagnostics cache, one job and disabled incremental compilation; the
admission disk floor was respected. Runtime and generated-output handles are
all terminal. Existing untracked fixture `.asb/` and `target/` directories beneath
the crate are retained as evidence, not committed or recursively removed.

Logs are `target/collector39-full.log`, `collector39-full-summary.log`,
`collector39-doctests.log`, `collector39-full-{clippy,fmt}.log`,
`collector39-make.log`, `collector39-{bash,zsh}-syntax.log` and
`collector39-full-{before,after}.sha256` / `collector39-final.sha256`.
This final qualification update changes documentation only; production, tests,
Cargo and CI remain identical to qualified `8ca779ca`. Earlier red/failure and
old-source qualifications remain separate and preserved. Final independent
proof review, exact-head hosted/package CI, PR/merge, both post-merge jobs and a
release remain pending. No push, PR, merge-to-main or release was performed.
