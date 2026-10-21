# Exact process termination: proposed contract

Status: **design only; implementation and qualification are pending independent
review**. This branch starts at reviewed setup40 `f3883ca8b1a3c754e6122283d92072f67882b18e`
and leaves version 0.2.40 unchanged. It changes no production, tests, workflow or
cache. It does not qualify Windows, another Unix kernel, an SDK or a provider.

The original code-smell report identifies flattened termination as a low-severity
evidence defect. The source audit used actual main35
`8a115715e312b25b17385a0505c78d49994495b0` and the reviewed queued collector39
stack (`e5f30a86`, also inherited by setup40). The unrelated peer
`collector-execution-boundary` branch is not a design dependency or qualification.
The separately identified post-grade warning/publication bypass is not repaired
or bundled into this class.

## Existing facts that must remain distinct

- A guardian's synthetic exit code is not its principal's exact termination.
- A principal exit, a controller acknowledgement and EOF are not descendant
  quiescence. Base Hello/Quiescent receipts and matching guardian exit retain
  their existing ownership and completion requirements.
- Process execution, official grade, capture completeness, resource settlement,
  optional termination observation and publication are separate facts. Optional
  termination observation is not a new completion or continuation obligation.
- Requested TERM/KILL, a timeout, Docker code 137 or a shell code 143 cannot
  establish that an agent died from signal 9 or 15.
- Old records with numeric exit codes do not retroactively acquire exact
  principal termination. Grade-only imports/recovery have unknown execution.

At main35, `process_gate.rs:210-224` flattens the principal into the guardian's
code. `runner/capture.rs:153-158,499-506` carries that guardian status, while
direct-child capture already owns the original `ExitStatus`. Guest driver
`agents/driver.rs:416,506,541-550` retains only optional code in metadata and
synthetic codes in ENDED/READY/CLI returns. `workloads/task_protocol.rs:175-195`
reconstructs an ordinary status from the controller's numeric carrier. Run logs,
measurements and comparison diagnostics inherit that carrier value. Collector39
already has optional exact `PrincipalExit`, bounded READY/ACK/EXITED framing,
and separate collector `exit_code`/`exit_signal` publication; retain its existing
adapter-owned acceptable-stop policy.

## One typed fact, additive wire

Introduce one private shared `ProcessTermination` type, serde-validated at the
boundary, not independent enums for the gate, guest, runner and reports:

```json
{"kind":"code","code":143}
{"kind":"signal","signal":15}
```

`Code(i32)` preserves the exact value exposed by std, including negative i32
values on platforms whose API exposes them. It must not clamp, reinterpret an
unsigned platform bit pattern as a Unix signal, or restrict generic codes to
0..255. The existing Unix control wire still accepts only its actual code range
0..255. `Signal` accepts only a nonzero signal in the existing bounded wire range
1..127. A current-host signal name is optional display decoration, not a stored
portable name or additional fact. Core-dump status is not claimed by this class.
Reject unknown variants, wrong types, out-of-range numbers, duplicate/irrelevant
fields and ambiguous code-plus-signal objects. Do not deserialize malformed
present data as an absent observation.

Each carrier has `Option<ProcessTermination>` named `process_termination`.
Missing or explicit null means unknown for this new field; this normalization
does not change the established absent-versus-null resource-disposition policy.
Known observations originate only from a directly owned wait status or a
validated owner-authored principal observation. There is no code-to-signal
heuristic and no conversion of arbitrary carrier statuses into principal facts.

Keep every legacy `exit_code`, `Output.status`, CLI return and ENDED/READY numeric
value unchanged in this additive repair. In particular, legacy carrier code 1
and principal `Signal(15)` are **not contradictory**. Existing process/grade
classification is not recomputed from the new metadata. Reports prefer the new
typed fact for exact termination and label a legacy-only number as carrier/
historical evidence, not a factual signal. No schema-version bump is needed for
an optional additive field; unknown older fields retain their current policies.

## Optional native observation, not active interruption

Add an explicit `Admission::with_principal_observation` entry point for actual
native benchmark-agent launches, both measured and unmeasured. Ordinary
`Admission::new` remains ordinary: TUI installers, discovery helpers and unrelated
gates do not automatically gain observation or interruption requirements.
Reuse the existing bounded optional control socket, READY and EXITED wire;
there is no new authorization policy, identity requirement or ABI version unless
implementation exposes a concrete compatibility need. The observation-facing
API exposes no interrupt operation. It must not send INTERRUPT or apply collector
stop-status validation to agents. Existing controlled collectors retain their
explicit interrupt API and adapter policy.

Before CONT, requested observation requires the existing current-runtime READY,
Hello and stopped-state checks under the existing startup deadline. Missing or
malformed capability can refuse admission normally. The actual typed launch and
running-executable identity are retained; never reconstruct an opaque Command.
Descriptors stay private/CLOEXEC and the client/wake-only watcher cannot inherit
observation authority. Existing uncontrolled fixtures remain valid and continue
to exercise the unmodified ordinary route.

## Critical separation after admission

Today `ManagedGate::try_wait` propagates `control.poll` errors before finishing
the independent base receipt. Enabling that path for agents unchanged could
erase an available submission/grade despite valid quiescence. The implementation
must separate base completion from optional observation instead:

1. Poll and validate base receipts using their existing strict error semantics;
   retain direct guardian status independently. Only ordered matching Quiescent
   plus matching guardian exit can establish cleanup.
2. Poll optional observation into a sticky private state: pending, validated
   typed principal fact, or a bounded fixed diagnostic. After CONT, malformed,
   wrong-owner, duplicate, unexpected ACK, oversized, read-error or truncated
   observation cannot return an error *in place of* a valid base completion.
   A later protocol fault invalidates any earlier provisional observation.
3. On an observation fault, keep the owned socket alive until base settlement;
   do not close it early and thereby cause optional EOF to cancel the principal.
   Stop parsing that failed channel. Its finite READY/EXITED writes cannot create
   an unbounded drain/queue; no worker, EOF wait or new cleanup budget is added.
4. On the gate side, an optional observation write/read failure after admission
   must not replace the actual principal wait result with an I/O error. Disable
   the failed optional channel while the original owner/cancellation watchers
   and descendant custody remain authoritative. Reporting EXITED failure must
   not change a genuine principal code into guardian code 127. Before-admission
   capability failure remains an admission error.
5. After guardian exit, perform the final bounded poll and explicit framing
   finalization without waiting for EOF. Reject a trailing partial frame even
   when a duplicate writer holds the socket open. A requested missing EXITED is
   unknown plus a fixed diagnostic, not a synthetic observed exit.
6. `try_wait`/`stop` retain independently valid base quiescence. Observation state
   is retrieved separately. Base receipt failure still returns unconfirmed
   cleanup and uses the pre-established keeper exactly as before. Optional
   observation can never rescue absent/bad base receipts.

This separation also requires auditing the controlled collector callers: their
explicit observation/control obligation must still fail capture when observation
is malformed/missing or interruption cannot be confirmed, while retaining a
separately confirmed guardian-cleanup fact. Moving errors out of `try_wait` must
not silently turn collector failures into accepted metrics or stop requests
into successful termination. Existing completion and adapter validation tests
remain mandatory. No active-interruption semantics are weakened for agents.

For an admitted agent, a requested observation fault publishes unknown
`process_termination` and a safe fixed ancillary diagnostic. It must not create
a run error, change operational disposition/process success, or alter official
true/false/pending grade, artifact handoff, capture completeness, independently
confirmed workload quiescence, resource settlement or provider/measurement
publication. Route that diagnostic separately from status-changing AttemptIssues;
its presentation failure follows the existing presentation policy, not a new
termination-failure classification. Never retry the agent.

Missing, unrequested, malformed or failed optional observation creates/removes
no existing completion, replica/group receipt or repeat-admission permission.
With otherwise valid grade, capture, cleanup and publication, outcomes/receipts
and subsequent repeat batches behave exactly as they do with legacy unknown
termination. Existing actual capture failures, invalid protocol-owner/base
receipts, pending grades and unsafe cleanup retain their established policies;
the new sidecar can neither rescue those failures nor add a blanket withholding
rule. Reporting-only log fallback remains non-resumable for its existing reason.
Observation success is never an extra permission to seal.

If owner cancellation or a failed exec yields no actual principal wait
observation, record unknown. A cleanup signal request or guardian code 127 does
not fill that gap. Preserve the existing cancellation/deadline and grade-admission
boundaries; absence of first retained metadata still cannot invent READY owner
authority or authorize an official evaluation.

## Guest, controller and publication flow

Derive the typed fact from direct guest capture or `HeldProcess.output.status`
while its principal observation is still owned. Carry it in `GuestResult` before
the first publication, preserving artifact pending/final publication, current
instrumentation guard, workspace and namespace owner. A final ancillary update
must not erase that frozen fact. No agent-specific branch belongs in the runner.

The existing ENDED/READY framing and synthetic statuses can stay unchanged:
they establish their current timing/owner handoff, not exact termination.
The eventual private metadata carries the optional typed fact. Python validates
and preserves its shape/ranges, then includes it in the completed result and
execution journal only after the existing metadata/owner boundary. Early
ExecutionEnded without metadata reports no new fact. No new signal is inferred
from Docker `ExitCode`, timeout, SDK-return status or a container-reset operation.
Fallback metadata retains unknown termination, even when a synthetic code is
available. Missing/unconfirmed retained handoff retains its existing refusal
and preservation behavior.

Rust `CompletedExecution` decodes carrier and principal fields separately and
copies the latter through `ExecutionResult`, `ExecutionEvidence`, `RunResult`
and `RunMeasurement`. It must not manufacture a new ExitStatus from the typed
sidecar or confuse it with the controller/SDK process. Existing raw Output and
carrier conversion may remain for compatibility. Malformed optional evidence
after independently valid owner/quiescence metadata is an observation issue,
not a reason to skip the independently permitted grade. Decode those obligations
separately; metadata failure that actually prevents establishing owner/quiescence
still takes the existing unsafe-to-grade preservation path.

Collector39's known principal completion maps into the same ProcessTermination
type; retain its existing `exit_code`/`exit_signal`, acceptance policy and
unknown/unavailable records. Any additive typed collector field is collector
metadata, never the benchmark agent's termination or a numeric metric series.

Completion summaries already serialize RunResult; carry the optional field
under the existing integrity seals without adding receipt authority or a new
checkpoint eligibility condition. Reporting checks semantic agreement only
where both same-attempt publications have known exact facts: two different
known facts reject report updates before publication. Unknown/missing optional
observation is not a conflicting known fact or a new reason to reject an
otherwise valid receipt/admission. It must remain honestly unknown rather than
being filled from an older attempt. Malformed present report data rejects that
report update, never execution/grade recovery or permission to rerun the client.
Comparisons never compare principal termination against a legacy carrier code.
Standalone logs, measured/unmeasured sweeps, repeat and report-only fallback all
retain the typed per-attempt value without aggregating signals into a score.
Grade-only import/recovery continues to report unknown process termination;
grader/controller exits cannot populate it. Preserve prior report/evidence bytes
on conflicts and request inspection, never rerun a client to repair metadata.

## Required offline qualification before delivery

The matrix is planned, not executed evidence. Build the actual ASB first in a
separately allocated cache, freeze/hash its executable during runtime tests,
and review focused source before authorizing a full combined-tree suite.

| Boundary | Required checks |
| --- | --- |
| Typed value/serde | Every variant, all i32 code boundaries including negative values, signal 1/127 and rejected zero/negative/128, wrong types, duplicate/unknown fields, missing/null legacy unknown; no 128+n inference. |
| Actual gated principal | Actual exit137 and143 versus SIGKILL9 and SIGTERM15; ordinary exit0/7; unchanged guardian/carrier codes and distinct exact facts. |
| Optional observation | Fragmented READY/EXITED, wrong PID, duplicate/reordered/oversized/truncated/post-final frames, ACK without request, permanent read/write failure, EOF and missing EXITED. |
| Independent cleanup | Every post-CONT observation fault paired with real valid base Quiescent+matching exit still confirms cleanup and yields unknown+diagnostic; principal success without base proof never does. Include trailing fragments with a held duplicate writer and no EOF. |
| Real ownership | Double-forked TERM-resistant descendant, owner loss, observation failure, bounded custody, descriptor noninheritance, unrelated sentinel survival and no post-cleanup writes. No global/name/PID-parent kills. |
| Native full entry | True/false and pending official-grade seams with exit137/143 versus signals9/15; exactly-once client/evaluate/workload/provider/publish actions. Pair observation faults with valid quiescence: unknown+diagnostic, unchanged grade/process/disposition/capture/cleanup/publications and exactly the legacy-unknown outcome/receipt/repeat behavior. Pending-grade restrictions remain unchanged. |
| Actual guest entry | Quiescent and ThroughEvaluation routes, real code/signal children, direct metadata and immutable artifact handoff, synthetic helper return unchanged, pending/final publication faults and retained owner/READY obligations. |
| Python/controller | Legacy/no field, new variants, malformed/conflicting metadata, early ENDED without exact fact, Docker137/reset/timeout unknown, full actual Rust-controller/Python-task-loop true/false/pending grading seams. |
| Collector compatibility | Existing controlled interrupt/ACK/flush/quiescence tests and adapter acceptable-stop validation unchanged; control fault preserves confirmed cleanup but rejects incomplete capture/metrics. Ordinary uncontrolled helper routes still work. |
| Publication/reuse/report | Typed observations across log/measurement/outcome/standalone+repeated+sweep JSON/HTML; conflicting known facts preserve old reports and source bytes; unknown/fault adds/removes no receipt or repeat permission; malformed present report data rejects updates without borrowing older facts or rerunning agents; grade-only imports remain unknown. |
| Portability | Pure shared conversion/validation tests; retain direct non-Unix healthy behavior and full std code width. Native Windows compile/runtime remains explicitly unqualified without an allocated environment. |

Reuse owning tests under process_gate/control and managed, runner/capture,
runner/phases (existing true/false/native/pending and publication seams),
guest_agent, benchmark_execution.py, benchmark_guest.py, remote execution,
task_protocol, measurement publication, compare and checkpoint. Fault injection
must be private cfg(test)/fixture-owned, not a production environment switch.
No live provider, installed coding client, original SDK, privileged collector,
host configuration change or agent retry is necessary for this matrix.

## Planned source boundaries

One shared termination module and lib registration; process_gate control,
managed and owner handling; generic capture/MeasuredOutput/ExecutionEvidence;
native admission; guest result and held-output handoff; Python execution/guest/
task-server metadata forwarding; typed task completion; immutable run/measurement
publication; comparison and checkpoint agreement/rendering; owning offline
tests and documentation. Preserve setup40 prepared population/publication,
collector39 stop policies, runtime budgets, raw recording custody, frozen agent
artifacts, existing grade permissions and independent publisher ordering.
No implementation, build, cache allocation, version change, PR, push, merge to
main or release is authorized by this design checkpoint.
