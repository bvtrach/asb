# Owning workload installer49 runtime/source follow-up

SOURCE checkpoint from signed installer49 merge58dd035c, new isolated checkout
`/home/martin/projects/asb-workload-prerequisite49`. No build/runtime/cache/full/
version/push/PR has run here. Combined50 bc54 failed images, logs, raw fixture
roots and four retained .asb-install-step directories remain untouched.

## Observed contract failures

Fresh combined50 proxy56, gate32, tools15 and installer104 passed; workloads
then completed188/4/0. The four failures are retained in
target/llmtape50-bc54-focus-workloads__.log in the original50 checkout. Partial
395passed/4failed/2opt-ins is NOT combined qualification. Actual/frozen ASB accf8094
and libtest5d06616b stayed identical to their before manifests.

The SWE source fixture expected obsolete `--max-time 90` curl argv. Actual
controlled download intentionally has NO curl max-time: ProcessScope owns the
frozen operation/download deadline, with explicit curl connect-timeout10 and
size bound64MiB. The existing Source publication fixtures already enforce that
no independent fixed timer supersedes that budget, and actual Source/process
timeout controls supply lifecycle evidence. No variable remaining-time curl
argument is invented by this repair.

Terminal source-download fixtures expected pre-migration text rather than the
current source-specific curl failure/availability diagnostics. More importantly,
an absent absolute evaluator interpreter now returned gate target127, which was
reduced to Ok(false), indistinguishable from an existing but incomplete runtime.
Two original adapter/catalog tests correctly rejected that lost distinction.
The later existing SWE missing-interpreter case has the same source-evidenced
problem, though it was not reached after the earlier argv assertion failed.

## Narrow production correction

Before each Linux Terminal runtime probe or SWE source verification, the owning
adapter makes its already-selected tool-relative interpreter path absolute at
the same invocation cwd and factually checks that exact executable. It prepares
the SAME selected absolute program in the existing ProcessScope. It does not
search another interpreter or infer absence from status127. Missing/unusable
paths return the owning actionable diagnostic with exact path; existing runtime
commands returning7 or127 remain completed nonzero probes, not missing tools.
OperationContext checks before/after lookup preserve typed cancellation/deadline
Abort. The two private Terminal callers pass their existing frozen context;
no generic process API, authorization, producer, stage or publication changes.
Other hosts keep native direct/PATHEXT execution, not a Linux preflight or new
portability claim. This availability snapshot does not pin future exec identity;
a later execution failure remains an actual ordinary failure.

## Regression source awaiting review

All original source/catalog/adapter cases and their assertions remain, with
specific corrected diagnostic/argv expectations rather than generic error
acceptance. SWE adds genuine absent curl, marker-writing curl127 and existing
marker-writing interpreter127 controls. Terminal adds actual curl127 and an
interpreter127 step to its prior success/7/absent sequence. The existing missing
interpreter error additionally requires its exact selected path on Linux.

Missing/failed downloads must stop at the downloading phase, never call an
archive verifier/install phase, never publish the source or trial selection,
and distinguish absent curl from a real marker-producing executable127. SWE
absent interpreter must preserve the original cached bytes without any curl or
verifier call; real interpreter127 writes actual markers and keeps completed
validation-failure semantics. Success still verifies fixture bytes, source pin,
catalog/cache preservation and enabled selections. No real archive, SDK,
provider, Docker or network is used. Inherited fixture Command::output remains
synchronous/unbounded on exceptional fixture loss, not a newly repaired harness.

Only formatting/diff checks have run. Independent source review must precede
actual-FIRST/frozen owning focus/strict and any normal merge into49/50; no old
parent qualification substitutes for this source's execution.

## Owning49 focused qualification

Reviewed84e0cb80 actual-FIRST passed14.75s and libtest compilation27.14s in its
new exclusive HOME cache, seeded only from verified-idle own50 debug artifacts.
The exact executable was selected from Cargo compiler-artifact JSON and frozen.
Before/after actual/frozen ASB SHA256 is
`849b744aa6031f14529509defbdf62f5efad94cee7247732a3f09593129fcaa9`;
libtest/frozen SHA256 is
`c571237e5e0ddde164966624779594ce2391dcbf8a3bdf71bf4c620a0cc795b8`.

All four affected owning filters passed first (2 SWE source,2 Terminal adapter,
1 Terminal download,5 Terminal catalog outer tests). Complete workloads then
passed192/0/0(35.99s), tools15/0/2existing live opt-ins ignored(1.55s), and
installer104/0/0(63.20s). The10 repeated owning tests are included in workloads:
the deduplicated count is311 passed,0failed,2ignored, not321. All new marker127,
absent-tool, exact-path and no-later-phase assertions executed. Source/process
deadline and native/public-CLI ownership controls are included in installer104.
Strict workspace/all-target/all-feature Clippy passed17.86s; formatting/diff
checks passed. Complete logs and hash manifests are target/workload49-84e-*.

All handles are terminal and source stayed unchanged during execution. Fourteen
untracked .asb-install-step evidence directories created by the runtime failure
controls remain intact; they are not silently deleted or claimed clean. This is
focused owning49 proof only. The separately reviewed material fixture correction
was not imported during these runs. Current-stack50 integration and fresh full
qualification remain pending independent review/coordination, not covered by
these focused results.
