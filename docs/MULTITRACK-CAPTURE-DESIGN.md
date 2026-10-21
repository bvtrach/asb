# Recording finalization and run integration

Status: reviewed implementation design, not a claim of available CLI behavior.
The archive/journal backend is implemented; recorder evidence and run integration
are the next feature boundary. See MULTITRACK.md for the full delivery scope.

The separate `capture-session.mjs` component now implements create-new output,
exact-byte and handler accounting, sticky uncertain finalization/close state, and
atomic no-clobber receipt publication with separate cleanup evidence. Its
`capture_session` Cargo integration runs the extensible Node fault matrix. This
does not yet qualify HTTP upload/fetch/delivery lifetime handling: the component
is not yet wired into the proxy. Its caller must bound input before serialization;
the post-serialization byte limit does not prevent that initial allocation.

## Authority and separation

The run coordinator owns bundle detection, compatibility, persisted selection,
private extraction, the single recording journal and immutable track receipts.
Provider adapters never parse ZIP files or select tracks. Proxy adapters expose
ordinary capture observations, not archive types. Resource cleanup, complete
recording, client process outcome and official workload grade remain independent.

Embedded recording receives a private invocation nonce and receipt path. Scrub
these variables for external overrides; missing evidence is unknown/incomplete,
not proof inferred from an exit code or a nonempty output file. This is a local
consistency protocol, not production authentication or a signing prerequisite.

## Handler accounting

Admission is counted at the HTTP callback, before reading an upload. Each handler
has exactly one terminal outcome, maintained through a finally path:

`accepted = completed + failed + inflight`

Completed means the full request and upstream response were captured, their exact
JSONL line was appended successfully, and downstream delivery finished. Observe
request upload failures, upstream body failures and downstream finish/close/error;
an upstream fetch registry alone does not cover all these lifetimes. A complete
HTTP error response is a recorded interaction, not a transport failure. A client
process may fail after all handlers completed and still produce a complete trace.

Track appended line count, exact UTF-8 byte length and incremental SHA-256 only
after each successful append. Keep inference request count separate from known
health/model/capability probes. Missing observations remain null. A partial write
or other ambiguous storage outcome poisons positive completeness; the host must
independently verify the retained bytes before committing them to the journal.

## Bounded shutdown

1. Transition admission to draining exactly once; close the listener and idle
   connections. Newly arriving handlers cannot become accepted recording work.
2. Allow already admitted uploads, upstream reads, appends and downstream writes
   to finish within an explicit grace budget. Do not discard their append merely
   because shutdown has started, and do not immediately destroy all connections.
3. Abort remaining owned uploads, fetches and responses, quiesce owned outbound
   subprocesses, and await every handler's terminal accounting within the remaining
   parent stop budget. Unconfirmed quiescence cannot produce a complete receipt.
4. Sync/close the recording and atomically publish a bounded versioned receipt.
   Bind PID, invocation nonce, mode, output identity, counts, bytes and checksum.
   Repeated shutdown cannot publish conflicting receipts. A receipt-write failure,
   fatal exception or SIGKILL never implies successful finalization.

The parent currently grants sidecars two seconds after TERM before escalation.
Grace, forced cancellation, outbound-child quiescence and final publication must
fit that bound or introduce an explicit recorder-owned bound consistently on both
sides; a longer uncoordinated Node timeout would be ineffective.

After reaping, the host reads a bounded no-follow regular receipt, validates the
schema, nonce/PID/output binding, integer bounds and accounting invariant, and
checks actual payload size/hash. Zero-inference/probe-only recordings remain
inspectable, but must not silently become usable automatic inference samples.
Represent eligibility honestly rather than falsifying recording completeness.

## Run integration ordering

Freeze replay selection in stable execution order before launching resources;
persist the seed, algorithm, source identity and selected IDs. Decode only selected
unique archive entries and give each execution a private JSONL cursor before the
existing model checks, usage accounting and proxy startup. Retain legacy JSONL
and sparse concurrent layouts without fabricating bundle completeness evidence.

For recording, refuse a preexisting destination early and again at authoritative
no-replace publication. Create journal intent only after actual compatible prompt,
task and initial material are established; never use placeholder fingerprints.
Bind replay-relevant harness/module/protocol semantics separately from the whole
execution-plan fingerprint, which also contains permitted repeat/concurrency and
measurement differences. One coordinator serializes immutable track commits;
client and evaluator execution must not hold a bundle writer lock unnecessarily.

Commit immutable track evidence before sealing execution completion. Do not seal
a growing ZIP or scratch paths removed after confirmed publication. Ordinary
client/grade failures finalize readable results; uncertain cleanup stops later
admission and preserves diagnostics. Explicit recovery never reruns a client.

## Extensible qualification matrix

- Clean success, complete HTTP 500, failed client with complete captured calls.
- Empty capture, probe-only capture, probes plus inference, unknown statistics.
- Slow/aborted/oversized upload; truncated/oversized upstream response.
- Concurrent handlers and deterministic identity independent of completion order.
- Shutdown before upload completes, during upstream streaming, after append but
  before downstream finish, and while a new connection is admitted.
- Disconnected downstream, repeated shutdown, forced drain and deadline expiry.
- Append/short-write/sync/receipt-publication faults and exact-byte verification.
- Missing, malformed, oversized, symlinked, wrong-PID/nonce or reused receipts.
- External override without private receipt capability remains unknown.
- Real CLI bundled record/replay, failure continuation, compatibility mismatch,
  immutable completion evidence after publication cleanup, and legacy replay.

Use bounded loopback providers and fake clients for the ordinary suite. Retain
separate opt-in original SDK qualifications and exact frozen executable evidence;
neither a skipped opt-in nor an exit code substitutes for required assertions.
