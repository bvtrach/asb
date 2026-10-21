# Installer integration (staged 0.2.37)

Source-only integration combines independently source-reviewed installer
`ccf56450` with recording/artifact/publication stack `05ebf11d`.
The original installer core, prepared input, Goose, native-path and fault-test
files are unchanged by the automatic merge. Incoming recording, capture,
artifact, controller and CI/package assets are retained unchanged.

The root reviewed all installer production changes, fault tests and the actual
67-pass focused evidence; strict Clippy/formatting and stable binary hashes
were verified. The original focused binary is preserved at
`asb-installer-recipes/target/qualification/installer-ccf56450-asb` with SHA-256
`312e975b436f8a5e7878a4f920faee0e33bd0d765d47fd8e33ec3ebdac55f9e0`.
Only idle rebuildable installer package outputs were subsequently cleaned;
source, test logs, retained failures and that frozen binary remain available.

No fresh combined-tree build/test/full qualification was claimed at that initial
checkpoint. The later qualification below does not replace its retained evidence.
PR CI, merge, both post-merge jobs and release remain required. No runtime,
collector or setup-publication work is included. Existing unbounded inspection
command output and the unnecessary string-serialization fallback remain separate
audit items, not a claim of complete installer-process coverage.

## Integrated qualification and observed fixture repairs

The first combined focus passed 101 tests at `9e1089d4`: 57 owning installer,
10 actual CLI installation, two extended artifact/recording, 30 repeated-run
and two TUI-installation tests. The actual binary remained unchanged.

Two subsequent full-suite failures were diagnosed, not silently retried:

- `target/installer37-full.log` exposed an old Perfetto fixture expecting a
  missing external SHA utility. The installer correctly computes SHA internally
  and refused the fixture's invalid payload. Independently reviewed `2b1fb9ed`
  asserts the actual mismatch with and without that external utility; neither
  route may publish an executable. Its five isolated cases then passed.
- `target/installer37-full-v2.log` exposed the old perf source fixture's forged
  external checksum output. Independently reviewed `9aa20068` retains the
  unchanged official URL/hash production wrapper, adds a private helper tested
  with an actual synthetic-payload SHA, and requires the actual CLI to refuse
  the forged hash before extraction/build. The positive owning helper test and
  complete actual offline CLI target passed (94 passed, five opt-in skips).
  This is synthetic source-build orchestration, not an actual official archive
  download/build qualification.

Signed integrated source `1ea1adde1101d512dfa76c3b539d96705e0e4e8e` also includes
PR #300's checkout/package-layout fixture correction, without changing the
reviewed installer behavior. Its fresh locked/offline/all-target/all-feature
suite passed **1,332 tests, zero failures, 21 existing opt-in skips**, across
24 outer Cargo targets. Nested child-test summaries are not counted twice.
Separate doctests passed (zero cases). Strict all-target/all-feature Clippy,
formatting, sequential `make -j1 build`, private Rustdoc, pinned AutoUML 0.7.2,
data/control flows, interactive diagrams, Bash/Zsh completion generation and
both shell syntax checks passed.

The actual ASB executable was built before runtime qualification and its SHA-256
remained unchanged before/after full tests and the subsequent build/doc gates:
`e2ac782da85bd124770a51d538bd0904d89eb9ae870d3098f3e808131a4b4349`.
Evidence: `target/installer37-full-v3.log`, `installer37-doctests.log`,
`installer37-v3-clippy.log`, `installer37-v3-fmt.log`, `installer37-make.log`,
and `installer37-v3-full-{before,after}.sha256` / `installer37-final.sha256`.
Both failed full-suite transcripts and all earlier focused evidence remain.
Only owned CLI-cache build outputs were used; no live provider, privileged
collector, host permission, credential or external SDK was exercised. This
qualifies the local combined source, not hosted package CI or a released patch.
