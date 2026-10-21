# Optional exact process termination (staged 0.2.42)

Initial implementation follows independently approved design `4e79cadd` on
reviewed setup40 source `f3883ca8`. Version 0.2.42 is reserved staging, not a
release. This checkpoint is not full qualification or hosted/package evidence.

One serde-validated ProcessTermination preserves full i32 codes and bounded
nonzero signals. A shared optional Observation carries that fact plus a fixed
ancillary diagnostic through capture, guest metadata, execution evidence,
immutable run/measurement records and comparison rows. Legacy carrier statuses,
CLI results and ENDED/READY framing remain unchanged. No signal is inferred
from a shell/Docker code or a requested cleanup signal. Grade-only reports and
historical missing fields remain unknown.

Actual native agents explicitly request observation-only admission using the
existing private optional READY/EXITED channel. Ordinary helper admission stays
ordinary. There is no active interrupt operation for observer admissions, no
collector acceptable-stop policy applied to agents and no new ABI/auth policy.
Post-admission observation errors are sticky unknown diagnostics independent
of base receipt validation. Matching Quiescent and guardian exit retain their
existing cleanup meaning. Optional channel failure on the guardian side does
not replace the actual principal wait status or owner/cancellation supervision.
The parent retains a failed observation socket through base settlement and
finalizes bounded framing without waiting for EOF. Controlled collector callers
still reject missing/invalid exact principal facts while retaining independently
confirmed cleanup; their existing adapter policy is unchanged.

No status-changing AttemptIssues, new completion condition or continuation gate
is added. Exact observation and observation diagnostics are metadata, not extra
permission or failure classifications. New known-fact report conflicts reject
updates without replacing previous reports; unknown is not a conflict or a new
receipt restriction. Guest metadata parsing isolates malformed optional facts
from independently valid quiescence/owner information. Artifact pending/final
publication and frozen native handoff remain untouched.

Owning tests distinguish actual exit137/143 from signals9/15; pair malformed,
missing and trailing/duplicate optional frames with base quiescence; exercise
native true/false/pending grade and observation-fault cases, repeated true/false
batches with/without optional faults, real guest entry plus normal grade-phase
composition, legacy/controller serialization and report agreement. The guest
grade-phase fixture uses the actual registered-adapter guest executable with a
local phase evaluator; it does not claim an original SDK/container/verifier.
The repeated fault switch is cfg(test) only and scoped to an isolated one-test
subprocess owning every worker; there is no production environment fault hook.

The original allocated CLI cache was idle, but the first compile admission
refused before starting cargo because available /srv space was below 3 GiB.
Root then copied only that verified-idle cache into a separate actual ASB
worktree at `/home/martin/projects/asb-build-cache-termination42`. Only its
`target` is allocated for this source checkout; the anchor's old source is not
used to build. Historical copied objects do not qualify current source. The
owned `target/debug` symlink points there; old caches and preserved qualification
binaries are unchanged. Every compilation uses one job, incremental disabled,
locked/offline inputs and a checked minimum 3 GiB available on that filesystem.

An actual ASB build passed before runtime testing. Initial library test
compilation identified a missing Observation initializer in an existing fixture;
`target/termination42-test-compile-initial.log` retains that failure. After the
owning initializer correction, all-target/all-feature test compilation passed.
Additional fixture/validation source changes still require a fresh frozen build
and focused runtime proof. No runtime result or strict-lint/full qualification
is claimed at this initial source checkpoint. No live provider, installed
client, SDK, Docker, privilege or host setting change is required. Native
Windows and other Unix kernels remain unqualified. Independent source review,
focused/strict-lint evidence and explicit full-suite authorization precede
hosted/package CI, main ancestry, merge, post-merge jobs and release.
