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
