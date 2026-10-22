# Fair shared-reaper custody: design and unrun regression checkpoint

This isolated branch starts at reviewed73e2f43b, not the dirty material-controller
ancestry or fixture delivery branch. No release version is reserved. Applicable
AGENTS/development/architecture instructions are byte-identical to the already
read actual66fb baseline. No production fairness correction has been made yet;
this checkpoint adds only private cfg(test) fault selection and owning tests.

## Concrete caller requirement, not an observed leak

OwnedReaper's one pre-established worker currently completely waits/retries one
child before visiting the next. Actual production multi-adopt consumers are
Flamegraph PipelineChildren (three pipeline stages, flamegraph.rs67/286/301/330,
shared fallback at117) and CLI2Key Local (bridge+compatibility proxy, custody.rs38,
implementation.rs143/180, fallback194/207). Both perform cleanup attempts before
handoff. This source-level fairness limitation does not prove a never-signalled
child or an abandoned-child leak. Every unvisited Child remains owned by the
worker's iterator. Current production WaitOnly callers are single-child owners;
mixed-policy coverage guards the shared API, not a claimed deployed population.
Perfetto Daemons uses its own cleanup owner, not this reaper, and is separate.

## Bounded owning regressions, pending review and runtime

One keeper receives a first live WaitOnly actor and later Terminate actor. A
distinct pre-established keeper owns an unrelated sentinel WaitOnly actor. Real
stdin EOF releases held actors even on ordinary errors/unwind. The later actor
must exit and be reaped while first+sentinel remain live and worker completion
is false. Before the intended fairness assertion, release every actor and require
actual pinned exits, waitid(P_PIDFD) ECHILD and keeper completion. Old sequential
source should fail only the progress assertion after all actual cleanup.

The production-shaped second test hands two Terminate children to one keeper.
A controlled observation error selects only the first already-owned live Child
in that exact pre-established worker; no global PID lookup/signal authority is
introduced. The second actually exits before handoff closure, observed through
pidfd without reaping. It must be reaped despite the first's controlled fault.
The separate sentinel keeper does not inherit that fault. Release the fault and
real actors, then require actual reap before the intended progress assertion.
This is synthetic observation failure, not actual kernel-error qualification.

Each measurement/cleanup poll has a finite one-second caller allowance; held
actors release by closing tiny owned stdin handles, never signals to WaitOnly.
Uncertainty retains the pre-established actual owners, not a false completion.
No unbounded test stdout reads or unconditional waits are added. Existing five
reviewed tests remain unchanged. No tests/builds have been run on this checkpoint.

## Proposed minimal production correction after observed RED approval

Use fair nonblocking rounds across every retained (Child, ReapPolicy). Actual
try_wait success removes only that confirmed reaped child. Observation errors
retain it but do not stop independent sibling work. Terminate may request kill
only after successful live observation; WaitOnly never signals. Do not call a
blocking wait inside the worker. Sleep once between finite rounds if population
remains. Same API, worker, pre-spawn establishment and handoff mutex semantics;
no new supervisor/thread, retries by callers, completion status or release policy.
Worker completion receipt remains conditional on the population actually empty.
Permanent uncertainty can retain custody indefinitely; this is not crash-safe
process-level recovery or permission to kill an admitted supervisor.

Source review precedes actual-first build/frozen binary and the two expected RED
runs. Preserve red logs/hash/actual cleanup observations before production edits.
No full suite, push, PR, version bump or changes to the qualified73/material50
trees are authorized here.

## Observed RED checkpoint at source 29a37405

After independent full source/design review, actual ASB was built FIRST in the
new exclusive `/home/martin/projects/asb-build-cache-fair-reaper/target` cache,
offline/locked/jobs1/incremental0; no qualified73/material50/delivery cache was
modified. Build exited0 in37.54s. Actual was frozen read-only before runtime at
`target/qualification/fair-reaper-red-29a37405-asb`; before/after SHA256 matches:
`ed44e69de45ada85464dff64db7ded8cb00297aec1bc83d837f1da51eda07aa2`.
The exact reexecutible RED libtest was also preserved after both runs at
`target/qualification/fair-reaper-red-29a37405-libtests`; its digest is recorded
in `target/fair-reaper-red-frozen.sha256`.

Both separate runs exited101 at their intended final fairness assertion:

- Held WaitOnly/later Terminate: later_progress=false, first_survived=true,
  sentinel_survived=true, settled=true, actual_reaps=true;0pass/1intendedfail,
  1.01s. Every actual actor was released/reaped before that assertion.
- Two Terminate owners, first-child-local controlled observation error and later
  actual exited child: injected=true, later_progress=false, first_survived=true,
  sentinel_survived=true, settled=true, actual_reaps=true;0pass/1intendedfail,
  1.02s. Fault and actual actors were released, with confirmed reap before the
  assertion. This does not qualify a real kernel error or permission denial.

Evidence: `target/fair-reaper-red-{build,held,localfault}.log`, matching
`before/after.sha256`, and frozen actual/libtest copies. All runtime/build handles
are terminal; no production fairness edits, full suite, release, PR or push.
Independent observed-RED approval is required before the proposed correction.

## Minimal production correction, source-only pending review

After observed-RED approval, the same worker now retains its existing vector in
place and visits each Child with one nonblocking try_wait per round. Only actual
Some(status) removes a reaped entry. A successful live observation permits the
existing Terminate kill attempt; errors retain that exact Child without signal,
and WaitOnly never signals. One sleep separates nonempty rounds. No blocking
wait, extra worker, handoff API, release policy, deadline reset or caller retry
is introduced. Completion remains after actual empty population only. The
original five tests/assertions are unchanged. This implementation has NOT been
built or run; independent source review precedes new runtime qualification.

## Focused fixed-source qualification at 0bb3c480

After independent source approval, actual ASB was built FIRST (terminal0,13.77s)
in the exclusive fair-reaper HOME cache, then frozen read-only at
`target/qualification/fair-reaper-fixed-0bb3c480-asb`. Before/after/final actual
and frozen SHA256:
`2902c98ecdb396eda1873a72e7ef4c58dc8abd446dc7ab1f76655f461b97e663`.
The corresponding fixed libtest is preserved read-only alongside it; digests
are recorded in `target/fair-reaper-fixed-final.sha256`. RED actual/libtest,
signed source and both intended-failure transcripts remain immutable.

All seven reaper tests passed7/0/0 in0.02s, including the original five unchanged
cases and both unchanged former REDs. Held-first now reports later_progress=true
while first_survived/sentinel_survived remain true, then settled/actual_reaps=true.
The first-child-local synthetic observation-fault case reports injected=true,
later_progress=true with first/sentinel survival and actual final settlement/reap.
This establishes fairness through a controlled error, not real kernel-error or
permission-denial qualification. No source edit or actual-binary relink occurred
during runtime.

Strict offline/locked workspace/all-target/all-feature Clippy -Dwarnings passed
(27.00s), fmt and diff-check passed, and all handles are terminal. Evidence:
`target/fair-reaper-fixed-{build,seven,clippy,fmt}.log`, before/after/final digest
files and separately named readonly frozen executables. The source tree is clean
and this focused checkpoint awaits independent proof review. No full suite,
current-main integration, release version, push or PR is claimed or authorized.

## Reviewed functional-stack integration, awaiting new qualification

Normal immutable merge of reviewed runner45 ece0b2f6 retains all incoming
runner phases, optional termination, owned-listener and executable-fixture
contracts. No semantic conflict resolution was needed. This separate next patch
is coordinated as staged0.2.46, changing only the workspace/asb-cli package
version pair. The fair production helper and all seven owning tests remain
byte-identical to focused source0bb3c480. Outside process_cleanup.rs, its owning
test file, this document and the two version fields, the tree is byte-identical
to ece0b2f6. Historical RED/fixed source, logs and frozen executables remain
separate and immutable. No build/runtime has been performed on this integration;
source review precedes actual-first build and new combined qualification. The
seven-test old-base proof is not substituted for current-stack qualification.
No PR/push/release or ancestry reconciliation with future actual main is claimed.
