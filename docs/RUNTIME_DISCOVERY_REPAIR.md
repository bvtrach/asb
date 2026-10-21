# Runtime-discovery bounded exchange design

This is the design and red-reproduction checkpoint for the next repair class,
based on qualified `8dabf284`. It is not a production fix or qualification.
The version remains 0.2.29 until later integration stages this class.

## Observed defects and first reproductions

`runtime_inputs::discover` starts a blocking stdin writer and stdout reader,
polls only the direct helper until its 60-second deadline, unconditionally waits
and joins both threads. A zero-exit helper skips `stop_owned`. Its descendants
can therefore retain stdout or stdin after direct-child exit and block either
join without any remaining deadline. `stop_owned` also starts external
`/bin/kill` with a blocking status wait. Its initial `try_wait` can reap the
group leader: no future repair may signal that numeric group after reaping.

Two actual tests call the production discovery exchange with its unchanged
algorithm and an injected 150 ms budget. Private fake Python executables fork
a new-session child, publish a valid schema-1 response and exit zero. One
descendant retains stdout; the other retains stdin while the caller sends a
valid roughly half-MiB declaration. An outer actual ASB `ManagedGate` supplies
a two-second watchdog, then stops and reaps its entire fixture tree before
asserting failure. A separately owned sentinel must survive. The fixture does
not leave a hung libtest worker or kill processes by unverified PID/PGID.

The timeout-parameter extraction keeps the production 60-second policy and
wire format unchanged. These reproductions do not yet test the proposed pump.

## Proposed production boundaries

- Keep the dependency schema, stable sorting/deduplication, count and response
  validation, secret-free public error and optional input-seal semantics.
- Poll request writes, response reads and child state from one owner; use no
  pipe-reader/sender threads, unbounded queue or blocking caller join. Retry
  Interrupted, distinguish permanent read errors from EOF, close stdin only
  after all request bytes were delivered, and reject an early exit that leaves
  request delivery incomplete.
- Preserve the current 1 MiB request and 4 MiB response limits while bounding
  allocations during capture, not after an unlimited `communicate`. A fixed
  read buffer and per-tick read/write quanta must leave deadlines observable.
  Responses are complete protocol messages, not diagnostic tails. Stderr stays
  discarded unless a fixed, non-secret diagnostic classification is required.
- Use one absolute exchange deadline plus a separate bounded cleanup reserve;
  neither active output nor repeated short writes restart it. Production keeps
  a 60-second exchange limit. Short private budgets qualify the same algorithm.
- On Linux reuse `ProcessLaunch`/`CommandRoute`/`Admission`/`ManagedGate` and the
  actual invoking ASB identity. Configure the original opaque command once;
  never reconstruct environment, stdio or callbacks from getters. A minimal
  private owned-stdin accessor is sufficient. Establish fallback custody before
  spawn and require the gate's quiescence proof, not just helper exit.
- Cancellation and unwind must use existing proof-aware bounded stop and
  wait-only keeper handoff. Remove external `kill` and raw group signalling.
  The native supervisor, not process-name scans, owns detached descendants.
- Preserve healthy non-Linux behavior. Use cancellable Unix/Windows pipe
  operations where available, and pre-established direct-child custody outside
  Linux. Such a path must disclose its direct-only ownership rather than claim
  Linux descendant quiescence or introduce a new blanket platform refusal.

## Python-owned nested probes

`Discovery.python` and `asb_runtime.resolve_dependencies` currently use
`subprocess.run(capture_output=True)` and only check stdout size afterward.
They need a bounded, selector-driven owned subprocess exchange, stdout/stderr
limits enforced while reading, a fixed buffer and an absolute deadline capped
by the discovery's remaining allowance. Loader-list parsing and no-agent-entry
execution semantics remain unchanged; no raw probe output enters diagnostics.

The outer Linux discovery gate must retain all of these helper descendants,
including detached processes and inherited pipe endpoints. Python stops only
its actual direct child, never a possibly reused numeric process group, and
must report uncertain settlement. Reuse outside that outer gate (notably SDK
runtime bundling) requires an explicit direct-child custody policy and honest
limits; this repair cannot claim whole-SDK descendant cleanup from a bounded
local wait. The exact helper ownership implementation remains a review point
before production changes.

## Required qualification matrix after the first red checkpoint

1. Both zero-exit inherited-endpoint regressions turn green without releasing
   unowned pipe threads; real child/descendant reaping and sentinel checks stay.
2. Healthy duplex pressure and a maximum valid request/response complete; one
   byte over each bound fails before another allocation or input seal.
3. Permanent read failure is not EOF; Interrupted retries. Broken stdin and
   premature exit cannot become a successful partial request.
4. Active-output and silent timeouts share the same absolute exchange deadline;
   cleanup has its own finite reserve and cannot eagerly call raw `wait`.
5. Fallible keeper creation occurs before admission. Inject exhaustion, cleanup
   poll uncertainty and unwind after real spawn; retain custody and compose the
   original error without touching the unrelated sentinel.
6. Nested Python and loader probes exercise oversized stdout/stderr, inherited
   writers, nonreading stdin where applicable, early failure, timeout and
   remaining-budget exhaustion with actual local fake executables.
7. Existing real interpreter/ELF/library discovery, native extension handling,
   PATH replacement, input-seal/resume refusal and error-redaction tests remain.

No provider, credential, installed coding client, SDK environment, host tracing
setting or network operation is required by this matrix. Full-suite and hosted
delivery qualification remain separate later steps.

## First red-checkpoint evidence

The actual ASB binary was built offline before the final real-child run.
`runtime34-red-final-tests.log` records three existing tests passing and the two
new regressions failing at the outer watchdog in 2.06 seconds (exit 101).
Both failures occur after the zero-exit marker, owned descendant reaping and
unrelated-sentinel survival assertions; they are not uncontrolled test hangs.
The actual binary SHA-256 was unchanged before and after that run:
`59ecde66077b329ffad46fe6f9039fbf2f36e51867fb5cc196d1be6f4ca30c14`.
The initial reproduction log is retained separately as `runtime34-red-tests.log`.
Strict all-target/all-feature offline Clippy and formatting passed; those checks
do not convert the deliberately red behavioral regressions into qualification.
Logs/build products stay under this worktree's ignored `target/`; its debug
symlink uses only the reserved BPF cache. Production behavior is not fixed at
this checkpoint, and the rest of the proposed qualification matrix is pending.
