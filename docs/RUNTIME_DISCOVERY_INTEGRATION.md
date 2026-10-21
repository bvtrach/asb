# Runtime discovery integration (staged 0.2.38)

Source-only integration combines reviewed runtime discovery `f287d0e3` with
installer/recording/artifact/publication stack `9e1089d4`. Runtime Rust exchange,
Python helper/input code and focused protocol tests are unchanged by the merge.
The shared ManagedGate change remains the reviewed narrow API accepting its
owned command's pipe inputs; no collector custody work is included.

The historical 98-test focused result and red reproductions remain tied to
their original source and executable. This combined source was unbuilt at the
initial checkpoint; fresh qualification is recorded below. PR CI, merge, both
post-merge jobs and a signed release remain required.
Non-Unix legacy Rust/Python fallback routes remain outside the bounded Unix
runtime qualification. No coverage threshold or live-provider requirement is
introduced.

## Fresh integrated qualification

Signed merge `2f30840d0765e9619a63272436f9c2366ca0662d` folds the fully qualified
installer checkpoint `b757bd99`, including its two diagnosed source-checksum
fixture repairs. An independent agent inspected the integration: runtime Rust
exchange/Python/input/protocol files remain byte-identical to reviewed `f287d0e3`;
all protected incoming production remains intact, with only the narrow owned
stdin accessor added to the shared gate. It approved the exact integration.

Built the actual binary first, then passed runtime discovery eight, protocol 35,
checkpoint 34, native gate 18, partial sweep two and replay sweep two: **99 passed,
zero failures/skips**. These wrappers include their Python scenarios rather than
counting them again. The subsequent fresh locked/offline/all-target/all-feature
suite passed **1,337 tests, zero failures, 21 existing opt-in skips** across
24 outer Cargo targets. Strict all-target/all-feature Clippy and formatting
passed; separate doctests passed (zero cases).

After that full run, signed merge `86daccdb3810e30e86f051e866545dd935c55c61`
folds only PR #300's independently reviewed permission-teardown fixture and
ledger correction `e284b2b7`. Production, Rust tests, Cargo and workflows are
unchanged. The fresh owning artifact target passed both tests/all ten scenarios
at that source. This is not claimed as another full run. Sequential
`make -j1 build`, private Rustdoc, pinned AutoUML 0.7.2, data/control-flow and
interactive diagrams, generated Bash/Zsh completions and shell syntax checks
all passed afterward.

Actual executable SHA-256 remained identical before/after focused and full
runtime tests, the test-only merge and the build/documentation gates:
`db062f7653ab15ae08bd02a9c62101fb5a80d418ff8391429a50edac1e60639f`.
Logs are `target/runtime38-{build,rust-focused,protocol-focused,
checkpoint-focused,gate-focused,sweep-focused,clippy,fmt,full,doctests,
fixture-build,fixture,make}.log`; matching SHA files retain the observations.
No failed runtime test was retried. All root test/build handles are terminal.
Only the owned CLI cache was used, without a live provider, credential,
privileged collector, external SDK or host permission change. This is Linux
combined-source qualification, not native non-Unix qualification, hosted package
CI, a merge or a release. Historical evidence remains preserved separately.

An independent final proof review approved docs checkpoint `25a85e9b`: it checked
all 24 outer summaries, source/merge identity, focused/full/owning-fixture logs,
all five hash observations and the preserved executable, lint and complete
build/documentation gates; independent completion shell syntax checks passed.
After all local handles were terminal, root verified the CLI cache idle and
preserved the exact executable at `target/qualification/runtime38-2f30840d-asb`.
A scoped `cargo clean --package asb-cli` then reclaimed 2 GiB of rebuildable CLI
cache outputs. Source, transcripts, retained failures and benchmark data were
not removed; loss of the cache's derived binary is not a qualification failure.
