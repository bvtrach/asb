# Coordinated development

This is the canonical process for the ASB repository. Use the configured remote
development host and second-drive storage for source, worktrees, toolchains,
caches, artifacts and all build/test activity. Host details belong only in
ignored runtime configuration, not public repository state.

Native ARM64 capacity is optional future qualification and never blocks development or release.
Use the pinned QEMU AArch64 lane for applicable userspace portability checks; keep native kernel,
PMU, eBPF, timing, contention, performance, and support-cell claims explicitly unqualified until
separate native evidence exists. See [the native ARM64 policy](NATIVE_AARCH64_POLICY.md).

Development TUI source builds keep ambient `PATH` absent. The router copies the
opened, validated linker into a fresh private prefix containing exactly one
regular `ld`, makes that prefix non-writable, and passes GCC an inherited
directory-descriptor path. It never supplies the validated linker source
directory to the broad GCC `-B` helper and library search. The prefix contents
and retained file identities are checked immediately before child creation.

This boundary excludes other users, unvalidated sibling tools and libraries,
and pathname replacement. A malicious process running as the same effective
user is inside the development-tool trust boundary: it can already ptrace or
mutate the build process. Directory modes and retained descriptors still limit
accidental mutation, and deterministic tests reject any substitution observed
before spawn. This development boundary is not production release attestation.

## Start and recover

Read PLAN, ARCHITECTURE and QUALITY and preserve all existing work. Reconcile
Git refs, processes, CI and worktrees before repeating any action after an
interruption.

Read the complete selected task and plan, dependencies, revision and checkpoint.
Only open tasks with completed dependencies may be claimed. Planned work becomes
open through a reviewed state change when dependencies and resources are ready.
One worker owns one active task and one named branch/worktree at a time.

Use the coordinator wrapper for product/Git/build/review mutations. For worktree
creation, run the command from the canonical checkout.

Development merge review identity is defined in
[`DEVELOPMENT_REVIEW_POLICY.md`](DEVELOPMENT_REVIEW_POLICY.md). A separate
technical review worker remains mandatory, but a same-account GitHub approval is
acceptable for development-only integration after that review and all exact-head
checks pass. This does not change signed merge integrity or release gates.

Publish concise evidence after every material result, failure or changed next
action. Never copy prompts, raw logs, credentials or private machine details
into public history. No forced updates or destructive checkouts to resolve
concurrent work.

## Integration

Treat shared schema and Cargo workspace changes as coordinator-owned integration
points. Freeze contracts before parallel adapter work; each worker modifies only
its own paths. Serialize merges and benchmark-host reservations while allowing
independent source work. A failed required gate preempts further feature work.

Use focused commits with git commit -S -s, Martin Beck's configured gmx.de identity,
and Linux-kernel-style matching Signed-off-by trailers. New product work uses Rust.
Retain and test the reused Python coordinator; do not dilute its fault coverage.

Product changes go through pull requests after the initial repository bootstrap.
Schema/tooling/policy changes need review and CI. Do not manually edit generated
project state views. The repository rejects unrelated private project history and
runtime configuration.

Reviewed product changes use the [signed merge integrity](MERGE_INTEGRITY.md) procedure. GitHub
requires one pull-request merge method to remain enabled, so only merge-commit is exposed; the
protected-main ruleset requires the portable provenance context, which rejects GitHub Web Flow and
noreply committers while validating the exact signed-DCO publication. On supported Enterprise
organization capabilities, the ruleset retains the metadata restriction as defense in depth.
Web merge, squash, rebase, and auto-merge remain unauthorized. Integration records the exact base,
reviewed head and tree, constructs a
signed DCO two-parent commit locally, and publishes only through an exact target-ref lease.
Before finishing, verify the GitHub merge commit is signed, its tree is the independently
reviewed tree, and the reviewed commit retains its raw matching DCO trailer. A mismatch remains an
explicit in-progress recovery; never rewrite protected `main` to conceal it.

A task is done only when its acceptance criteria, applicable exact-head CI and
post-merge evidence pass and the durable state matches the actual result.
