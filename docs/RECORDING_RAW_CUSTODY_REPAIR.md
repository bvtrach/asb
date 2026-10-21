# Recording raw-source consumption repair

Baseline: actual main `06c47a9e`, version 0.2.32; recording implementation was
introduced by peer main `cabaa576`. A separate reviewer identified a check-to-
unlink gap in `runner::recording::cleanup_raw`: it validates source metadata
and committed bytes, then unlinks the pathname without owning that final entry.
Existing tests replace entries before function entry, not in that gap.

This first source checkpoint adds a test-only per-thread replacement seam after
the last original identity check and proposes actual regular-file, symlink and
parent-directory replacement regressions. The seam does not exist in production
and has no environment or installed CLI surface. No production repair, executed
red/green result, full qualification, PR or release is claimed yet.

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
