# Recording raw-source consumption repair

Baseline: actual main `06c47a9e`, version 0.2.32; recording implementation was
introduced by peer main `cabaa576`. A separate reviewer identified a check-to-
unlink gap in `runner::recording::cleanup_raw`: it validates source metadata
and committed bytes, then unlinks the pathname without owning that final entry.
Existing tests replace entries before function entry, not in that gap.

This first source checkpoint adds a test-only per-thread replacement seam after
the last original identity check and proposes actual regular-file, symlink and
parent-directory replacement regressions. The seam does not exist in production
and has no environment or installed CLI surface. The observed-red checkpoint
below is later evidence; no production repair, green result, full qualification,
PR or release is claimed yet.

## Observed red

At signed source `248bf2ab`, the actual offline-built libtest exercised all
three replacements before asserting. It exited 101: zero passed, one failed,
zero ignored, in 0.03 seconds. Each variant actually removed the foreign
pathname and returned `Ok(())`: regular file, symlink and replaced parent
directory. Thus this is a reproduced ownership bug, not only a hypothetical
race or a failure to return a diagnostic. All data belonged to private fixtures;
no live producer, provider, external process or user file was involved.

The libtest SHA-256 stayed unchanged around the run:
`2c1c8eab1aef2fa03d9c1cd8e2761e84b84a412fdcc1728783be5fb0bd4c5849`.
Evidence is retained in `target/recording-raw-red.log`,
`target/recording-raw-red-build-final.log` and matching before/after hash files.
The initial source-only checkpoint/build remain preserved separately; only the
final test actually exercised all three cases. Independent review is next.

Required repair: keep archive publication, immutable track association and
observed producer cleanup prerequisites. Pin the producer namespace and original
file, transfer a candidate into private owned custody, recheck exact identity
and committed bytes before deletion, and restore without replacement or retain
with an inspection path when the moved entry is foreign. Never delete a new
arrival or recursively remove unknown entries. Directory synchronization after
successful unlink has its own truthful unconfirmed-durability outcome; do not
promise retention after deletion succeeded. Quiet private custody is not a
hostile same-UID sandbox guarantee.

Tests must preserve normal owned deletion, changed bytes/identity refusal,
foreign regular/symlink/parent replacements, failed movement/restoration/deletion,
raw retention after uncertain producer/archive publication, and exact original
recording/replay/grade/continuation contracts. Use offline local files and
synthetic clients only. Independent review, focused/full tests, strict lint,
build/docs/packaging, PR/merge, both post-merge jobs and a signed patch release
remain required. Version staging is deferred; retain the removed coverage gate.
