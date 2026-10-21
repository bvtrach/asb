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

## Fresh full qualification after fixture teardown repair

Signed merge `807012d9` folds independently reviewed artifact checkpoint
`e284b2b7`: only the owning Python permission-fault teardown and its historical
ledger change. Cargo remains 0.2.36. Production is byte-identical to `089efcbc`;
the two raw-recording production paths remain byte-identical to `bd2dea5e`.
The fixture restores only directory identities journaled before its deliberate
chmod, after scenario assertions and on exceptional exits; it does not relax
the protected-period cleanup/publication failures. Both failed hosted artifact
runs and their distinct owning fixture corrections remain recorded separately.

The actual CLI was built first in the exclusively allocated BPF cache, and its
old focused executable was preserved under `target/qualification/` before that
build. Fresh actual artifact entry checks passed two tests (all ten native/guest
scenarios); recording/raw checks passed 12 tests. The subsequent exact-source
command `cargo test --locked --offline --workspace --all-targets --all-features`
completed with **1,314 passed, zero failed and 21 explicit optional skips across
24 outer targets**, including 1,081 library passes and 13 library skips. Three
nested child-test summaries are not counted a second time. No runtime failure
occurred and none was retried. No other build/relink or source mutation occurred
while the full runtime handle was active; externally running peer work is not
claimed to be globally absent.

After full handle 65101 was terminal with exit zero, strict workspace/all-target/
all-feature Clippy and formatting passed. Sequential offline `make build` passed,
including private Rust documentation, pinned AutoUML 0.7.2, Graphviz diagrams,
function/data/control flows, interactive HTML and actual Bash/Zsh completions.
Both completion shell syntax checks passed. Graphviz emitted only its ordinary
node-margin fallback warnings. All generated documentation remains under the
project target tree. No original external SDK, live provider, host installation
or host credential change was used.

Before/after/full-gates actual executable SHA-256 remained
`dfc3f9dcd0a22a66eb0f91f3336c2dad15c1c9bf075b7e3f26c4b9846d6c542a`.
Logs are `target/raw36-final-{build,artifact,recording,full,full-summary,clippy,fmt,make,shell-syntax}.log`;
before/after/final hashes are retained alongside them. The final proof changes
documentation only, not production, tests, Cargo or workflows. Generated fixture
directories and the test-produced `.asb/install.log` remain preserved, untracked.
Native Windows and other Unix kernels remain source-only, not runtime-qualified
by these Linux checks. Final independent qualification review, hosted/package
CI, merge, both post-merge jobs and release remain pending; no push/PR/release
operation was performed here.
