# Checkpoint helper exchange: class49 discovery and proposed repair

This is a source-evidenced extension of the original code-smell audit, not an
executed reproducer, finished repair, whole-project clearance or release claim.
Base: actual main `32329d76bfabef76ee4b9db1c32b5d82b5335c9a` (0.2.42).
AGENTS was read; DEVELOPMENT and ARCHITECTURE are byte-identical to the complete
documents previously read for the qualified official-engine source `31070b9a`.
The original supplied 52-line report was reread before this continuation.

## Concrete residual, not a grep-only cleanup allegation

`crates/asb-cli/src/sweep/checkpoint.rs::Snapshotter::fingerprint` currently:

1. Allocates the complete JSON request, then checks its 16 MiB limit.
2. Spawns Python with piped stdin/stdout/stderr and no pre-established keeper.
3. Performs blocking `stdin.write_all(&request)` before creating the deadline.
4. Starts a 130-second wait deadline only after stdin delivery has returned.
5. Waits for child exit without concurrently draining either output pipe.
6. Calls blocking kill/wait on delivery or polling failure; on the healthy path,
   calls `wait_with_output` and checks the 2 KiB response limit only afterward.

The fixed production helper normally reads stdin promptly and emits one small
digest, but those assumptions do not enforce the advertised boundary. An
unresponsive interpreter can block stdin delivery indefinitely outside the
deadline. Duplex pressure can prevent child exit even if data is ready, and a
malfunctioning helper's response can allocate beyond the claimed response cap.
These are separate from the already repaired model transport. The superficially
similar kill/wait calls in `sweep/lock.rs` are inside cfg(test), not another
production lock defect. No claim is made that a real user run has hit this gap.

## Preserve the actual checkpoint contract

The Python fingerprint algorithm, private key, exact JSON projection and keyed
digest bytes stay unchanged. Roots, environment inclusion/exclusions, file
identity/change checks, maximum nodes/bytes, HMAC policy and existing receipt /
continuation rules are not redesigned. No authentication requirement, stricter
authorization or new verification policy is introduced. A failed helper never
provides a partial digest or fallback receipt. Diagnostics may classify the
operation/failure stage but never include request bytes, key, paths, contents,
environment, helper stdout or helper stderr.

Keep request maximum 16 MiB, stdout maximum 2048 bytes and the existing
130-second production operation allowance. A test-only short allowance must
exercise the same production path rather than replace the helper implementation
with mocked success. Bounded request encoding should stop allocation at the
existing cap; deadline creation must precede exchange admission, not stdin I/O.

## Proposed implementation direction, awaiting independent review

Reuse the existing bounded Unix runtime-discovery exchange rather than copy a
third subprocess supervisor. It already materializes a ProcessLaunch, owns a
Linux ManagedGate before admission, uses nonblocking fair duplex turns, has a
pre-established non-Linux direct-child keeper, checks complete delivery/EOF /
exit and retains unknown cleanup. Its runtime-specific Python command policy
and 4 MiB response bound must become explicit call-site policy, not accidentally
applied unchanged to checkpoints.

Move the cohesive transport into a generic private local-helper module with
small explicit policy (response bound and existing operation allowance), leaving
runtime discovery as an owning wrapper. Preserve that caller's current 1 MiB
request / 4 MiB response / 60-second policy and unchanged successful result bytes.
Checkpoint construction still owns `-I -S -B -c HELPER` and response schema /
digest validation. Retain the original configured command; no reconstruction
from Command getters. Class44 installation budgets and publication permissions
are NOT imported into checkpointing.

Choose one absolute operation deadline before helper admission and retain it
through request writes, fair stdout/stderr draining, EOF and exit observation.
Stderr is diagnostic-free discard with bounded work per turn, not an unbounded
retained buffer and not a new nonempty-stderr failure policy (legacy successful
checkpoint helpers did not reject stderr). Exceeding required stdout capture,
partial input, read/write failure, timeout or unknown cleanup is an error.
Cleanup has an explicit finite reserve; never unconditional caller-thread wait
or join. A successful principal status alone is not Linux descendant quiescence.
Keep gate custody through failures/unwind, without killing an admitted guardian.

Do not silently change non-Unix legacy behavior or call it Linux-qualified.
Decide its compatibility route explicitly during design review; Windows
cancellable pipe handling must not be manufactured by a Unix-only test result.

## Required evidence before delivery

- First record actual built/frozen baseline, then isolated owned real-helper
  regressions for blocked stdin (request larger than pipe capacity), stdout
  before stdin, oversized output, malformed/partial JSON, nonzero exit, early
  stdin close and a descendant retaining stdout. A watchdog must own all test
  children; confirm cleanup and unrelated sentinel survival before assertions.
- Preserve initial failed logs. Distinguish observed RED cases from source-only
  risks; do not count synthetic timeouts as evidence of a historical user hang.
- Test request boundary at exact maximum and maximum+1, strict bounded encoding,
  no helper admission on preflight refusal, full request delivery and exact
  successful response; never parse a retained prefix after a transport error.
- Exercise setup/admission failure, actual cleanup receipt loss, unwind and
  finite unknown-custody handoff using existing owning gate fault mechanisms.
- Run unchanged runtime-discovery ownership/duplex tests and existing real
  fingerprint/context/replay/sweep continuation and checkpoint tests, including
  stable literal protocol expectations and private-value redaction.
- Independent source review of the signed implementation precedes fresh full
  offline CI qualification, exact-head hosted checks, merge by PR, both enabled
  post-merge jobs and a separately allocated signed patch release.

No production edits, runtime tests, version allocation, PR or merge are claimed
by this discovery/design checkpoint. The removed 90% coverage gate stays removed.
