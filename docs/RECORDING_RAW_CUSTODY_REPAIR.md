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

## Candidate implementation boundary

Unix raw cleanup now pins the producer directory and the admitted original file,
verifies original identity/version and the bounded committed digest, then moves
the candidate relative to pinned handles into a fresh owner-only (0700) custody
directory. Only the moved candidate may be consumed. It must still match the
original inode/device, size, modification time and committed bytes; post-move
reads also check stable change time. Rename-induced change time is not mistaken
for a different original. No original source pathname is unlinked.

A foreign candidate is restored with relative `linkat` without following links
or replacing any existing entry, followed by private custody unlink. This is not
a multi-entry transaction: a failed/interrupted unlink can retain both links.
Unsupported hard-link restoration (including directories) or an occupied source
name retains the candidate in custody. Diagnostics distinguish restored,
restored-but-durability-unconfirmed, retained in custody, and actual successful
deletion followed by failed directory synchronization. Locations include the
custody component, original parent and pinned parent device/inode, so renaming
the parent does not falsely identify a replacement namespace as the owner.
Recorder finalization preserves the usable archive and limits detailed cleanup
warnings to 16 plus a total failure count; ancillary retention does not change
an official grade, track association, or producer-quiescence evidence.

The empty private directory intentionally remains after successful consumption;
removing its checked original name would recreate a namespace race. There is at
most one newly created directory per cleanup attempt (normal finalization visits
each track once); maximum recording population already bounds normal count.
Retained candidates can contain sensitive raw inference bytes. There is no
automatic expiry, project-wide disk quota or recursive recovery/cleanup. There
is no Drop cleanup, print, retry, worker or new continuation permission. Quiet
private custody is a concurrency boundary, not a hostile same-UID sandbox.
The existing non-Unix inability to confirm destructive raw identity remains an
honest retention warning, not an execution refusal or a new platform guarantee.
Native Windows and other Unix kernels are not runtime-qualified by Linux tests.

Private deterministic fixtures cover the observed three replacement windows,
foreign directories, arrivals/replaced namespaces after the move, committed-byte
mutation, move/unlink/restore faults, restore EEXIST, both-link retention and both
post-unlink synchronization distinctions. A real CLI test records true/false
local file grades using a bounded synthetic loopback provider, verifies immutable
track evidence after raw consumption and then replays with that provider closed.
This is not an original external SDK or live-provider qualification. Fresh
focused evidence is recorded below; current-main integration, full qualification
and delivery remain pending.

## Focused source qualification

Root independently source-reviewed signed `bffdfbaa` and the small OS-cause
follow-up `85f73781`. The latter retains the actual failed `renameat` error
alongside the source-not-consumed diagnostic; a real missing-source regression
checks ENOENT and original survival. Version remains the baseline 0.2.32, not a
reserved new release slot. The first implementation build failed on a
`CString::new` error-conversion type mismatch; the specific conversion was fixed
before the signed source candidate. Its failed transcript remains in
`target/recording-raw-green-build.log`, not recast as a passing build.

After building the actual ASB first at `85f73781`, the offline focused matrix
passed **196 tests**, zero failed, with three explicit optional runner skips:
runner 118, complete repeated-run target 30, cassette 46 and capture-session two.
This includes the original three-window red case now green, custody failure and
unwind fixtures, actual true/false record/replay and existing capture/report/
typed-disposition/grade/recording-association regressions. Strict workspace,
all-target, all-feature Clippy and formatting checks passed. All runtime handles
were terminal before linting; source and actual executable remained frozen.
The before/after actual ASB SHA-256 is
`622ef8a7b3537b38b237cd7eea4d0d5cd48c99652c5128a6f5a8d40eb4f21801`.
Logs and hash records are `target/recording-raw-cause-*`; earlier green `bffdfbaa`
checks remain separate in `target/recording-raw-final-*`. Original red logs and
source heads are retained. Only the allocated BPF target cache was used.

No full suite, current-main integration qualification, Windows/native-other-Unix
runtime qualification, hosted/package CI, PR, merge, push or release is claimed.
This documentation checkpoint changes no production or test bytes.
