# Raw capture custody integration (staged 0.2.36)

Initial source-only integration combines independently reviewed/focused raw repair
`bd2dea5e` with artifact/publication stack `b25d9f11`. It keeps the exact raw
implementation and its true/false record-to-selected-replay fixture, while
retaining the incoming shared provider fixture and all publication/artifact
fault cases. No collector/runtime/installer/setup repair is folded in.

Patch 0.2.35 belongs to artifact custody; its later local full qualification at
`b25d9f11` passed 1,303 tests (documentation proof `4b78a350`). Neither that result
nor raw repair's historical 196-test matrix qualifies the new 0.2.36 tree.
The removed coverage threshold remains removed.

## Fresh focused integration proof

At exact source `05ebf11d`, the integrated `runner/recording.rs` and
`runner/recording/raw.rs` are byte-identical to reviewed raw checkpoint
`bd2dea5e`. Against artifact parent `b25d9f11`, those are the only changed
production paths. All artifact/publication/capture/controller/grade/resource/
selection code is unchanged; Cargo only stages the package version at 0.2.36.
The added 90-line real recording/replay test retains the incoming shared provider
fixture rather than restoring the old duplicated implementation.

The actual 0.2.36 CLI was built first in the allocated BPF target cache. Fresh
locked/offline focused checks then passed **680 tests**, zero failed, with four
explicit optional skips (three runner, one comparison): recording/raw 12, bundle
publication two (all 15 isolated cases and their replay baselines), repeated-run
30, remaining runner 108, agents 65, layout one, actual artifact entry two,
workloads 169, comparison 70, checkpoint 34, benchmark protocol 35, guest agent
14, measurements 72, evidence 18, cassette 46 and capture-session two. Runner
filtering excludes the separately counted recording/bundle families; nested
child summaries are not counted again. No runtime failure was retried.

The matrix includes actual true/false raw consumption with confirmed ordinary
completion evidence, current selected-track replay, native frozen MiniSWE
handoff and commit/log/measurement faults. Unconfirmed publication/producer
cleanup continues to retain recording raw bytes; the custody repair does not
grant extra grade/continuation permission or alter independent agent evidence.
Replacement/restoration/unwind faults preserve their bounded locators and
truthful deleted-versus-retained outcomes. No live provider, original external
SDK, package installation or host mutation is involved.

After every runtime handle was terminal, strict workspace/all-target/all-feature
Clippy and formatting checks passed. The before/after/final actual executable
SHA-256 is
`dfc3f9dcd0a22a66eb0f91f3336c2dad15c1c9bf075b7e3f26c4b9846d6c542a`.
Source and executable were frozen throughout runtime qualification. Logs and
hashes are `target/raw36-*`; `raw36-focused-summary.log` records each family's
final outer result. Original red/old-source proof remains preserved separately,
including the frozen old 0.2.32 executable. No REPORT cache was touched.

This checkpoint changes documentation only; raw and incoming production/tests
remain as reviewed. All local handles are terminal. Fresh full qualification is
held for independent integration review; build/docs/packaging, hosted PR CI,
merge, both post-merge jobs and release remain pending. Native Windows/other
Unix kernels and live providers are not runtime-qualified by Linux fixtures.
No full suite, push or PR operation was performed here.

## Independent review and package-fixture ancestry

Independent review approved `98c51664` after reading the complete raw core,
coordinator/integration delta, replacement/restoration tests, actual true/false
selected replay and focused logs/hashes. Only the two raw-recording production
paths differ from the reviewed artifact parent, and they are byte-identical to
the reviewed `bd2dea5e` implementation. The 680-test focused proof remains tied
to its actual source and frozen binary, not relabelled as full qualification.

The subsequent source-only ancestry merge incorporates PR #300 fixture/ledger
head `0f0680a7`. It updates only the packaged artifact fixture's crate-relative
assets/fresh target directory and output-path validation plus historical proof.
Version stays 0.2.36; production and raw tests do not change. The documentation
conflict retains both the raw36 proof and artifact35 full/package-fixture history.
Root reported a second hosted failure AFTER all packaged tests passed: removal
of the extracted package encounters fixture-owned read-only directories. The
artifact author is preparing narrowly scoped post-assertion permission
restoration; that later reviewed fixture is not yet folded here. Fresh full
execution remains held for that correction and explicit root clearance, even
though the allocated BPF cache currently has no live matching process. No build,
test, cache mutation, push, PR or release accompanied this source-only merge.
