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
this source-only integration. The new combined source needs independent review
before qualification. Runtime38 focused qualification is running separately;
the PR #300 packaged-fixture post-test read-only-directory cleanup correction
also remains pending its reviewed delivery head. Do not start full qualification
until the parent explicitly clears these boundaries and assigns an idle cache.

Linux offline normal-producer custody is the reviewed scope. This does not
claim live privileged collector qualification, native Windows/other kernels,
all one-shot preflight/postprocessor process lifetimes, a hard kernel-file-I/O
deadline or a hostile same-UID filesystem sandbox. Future proof must preserve
the service lease until matching guardian quiescence, not merely principal exit,
acknowledgement, caller timeout or generic reaper handoff.
