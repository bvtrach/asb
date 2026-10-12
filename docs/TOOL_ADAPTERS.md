# Source-to-executable adapter contract

ASB never treats a source archive, package, or unknown catalog row as an
executable. A tool becomes usable only when the checked-in source-executable
matrix identifies an adapter, immutable source revision, package strategy,
runtime closure, launcher identity, and offline fixture. The matrix is
`crates/asb-cli/fixtures/v1/source-executable-support-matrix.json` and is
validated before a project install can use it.

Each adapter uses a finite argument vector in a project-local staging area.
Installers and runtime downloads, ambient package-manager state, shell syntax,
floating revisions, and credential-bearing environment variables are rejected.
The launcher must be a regular executable file or an explicitly approved
relative symlink whose target remains inside the installation root. Unknown
tools, unsupported rows, archive-only output, missing or substituted
entrypoints, runtime mismatch, and unsafe modes fail closed before activation.

Materialization is transactional. A new output is inspected and structurally
smoke-checked before its receipt is written. A failed attempt removes only its
transaction-owned staging directory and retains the last known-good receipt and
executable. Receipts contain only bounded identities and digests: source
revision, adapter/package strategy, runtime closure, launcher, smoke result,
rollback target, and retention decision. They never contain command output,
host paths, credentials, prompts, or transcripts.

The support matrix deliberately includes unsupported rows. Unsupported means
that ASB will not guess a generic build recipe; a future adapter must add a
reviewed row and offline positive and hostile negative fixtures before the tool
can be selected. The current source-package rows remain unsupported because a
wheel/npm package is not itself a verified launcher and ASB has no permitted
runtime package download or ambient package-manager installation path. The
prebuilt rows are the only supported executable adapters until those package
closures receive their own bounded local adapters.
