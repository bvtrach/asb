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
