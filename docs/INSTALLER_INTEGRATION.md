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

No fresh combined-tree build/test/full qualification is claimed. That remains
required before PR CI, merge, both post-merge jobs and release. No runtime,
collector or setup-publication work is included. Existing unbounded inspection
command output and the unnecessary string-serialization fallback remain separate
audit items, not a claim of complete installer-process coverage.
