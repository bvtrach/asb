# Material47 guardian wait-error diagnostics: source proposal

New isolated source from14668bd9. PR316 diagnostic CI39726088299 failed
ordinary library1234/1/13 at one_owned_signal_denial_does_not_starve_later_owned_siblings,
not the previously failing nested-probe test. The current generic wait error
does not expose actual guardian-error.json or child output, so root cause is
unknown; no timeout or production correction is inferred.

Only the explicit test Guardian.wait error path adds provisional snapshots AFTER
handoff to its pre-established wait-only keeper. Drop and retain remain free of
diagnostic filesystem reads. No signal, gate policy, joins, budget or custody
assertion changes. Five fixed known files (guardian-error.json, child.stderr,
child.stdout, guardian.stderr, guardian.stdout) are opened readonly NOFOLLOW and
NONBLOCK, checked regular, read with4096byte caps and32 attempts. One extra byte
detects truncation; retries/Interrupted count toward the attempt cap. Missing,
nonregular and I/O failures report categorical availability, never success.
Maximum captured raw content is20KiB, with bounded escaped UTF-8 presentation.
Snapshots may be incomplete while retained owners write: explicitly provisional,
NOT settlement evidence. This is not a hostile same-UID or arbitrary stalled
filesystem guarantee; paths belong to the existing owned local fixture root.

Pure fixture controls cover truncation, missing, directory, symlink/FIFO refusal,
read error and endless Interrupted. Existing actual finite Python uncertainty
case additionally requires provisional/fixed-file diagnostic context in the
returned error AFTER unchanged actual release/reap/sentinel assertions; its
existing final checks still parse the real CleanupDeadlineUnknown/custody record.
The live snapshot may precede a completed Python write and is never required to
be parseable. Unknown custody remains unknown. No production environment fault
hook or live resources.

Source-only: no build/runtime yet. Root review precedes actual-FIRST/exact Cargo
JSON/focused tests. Existing14668 full/cache/evidence remain immutable. No PR,
push, full suite, timeout increase or historical hosted-cause claim.
