# Installer recipe preparation and publication repair

Source baseline: qualified publication stack `5516c4e9`, version 0.2.32.
The initial test checkpoint is `cce58fa4`. Its three regressions were actually
executed and failed as described below. No production fix, full qualification,
PR or release is claimed by that checkpoint.

## Source-confirmed gaps and first regressions

The ordinary Host recipe and pinned Goose extraction copy directly into the
destination, unlike explicit source installation. Host copying follows an
existing destination symlink and can overwrite unrelated bytes and permissions.
The isolated Host fixture requires replacement of the directory entry while
preserving the foreign target. Automatic dependency installation has a separate
intentional no-clobber contract; that behavior must not be replaced with the
ordinary explicit reinstall policy.

The shared staged-source installer validates regular-file type, bounds and
checksum before invoking its optional version/probe callback. It then reopens
the pathname and renames it without checking those obligations again. Two
proposed regressions simulate a successful probe changing bytes or replacing
its staged executable with a symlink. Neither may publish or disturb the prior
ready installation. These tests require no network, package manager, provider,
host permission change or installed coding agent.

## Observed-red evidence

Built the baseline libtest offline with one compile job and incremental output
disabled, then ran that unchanged executable directly. Both source-probe tests
failed with exit 101 (zero passed, two failed, 0.02 seconds): changed bytes and
a replacement symlink were actually published. The separate ordinary Host
recipe test also exited 101 (zero passed, one failed): the foreign target's
bytes were replaced by the fixture host executable. Its isolated child uses
an empty environment with only fixture-owned paths and optional LLVM output.

The libtest hash was identical before and after both invocations:
`e2409209561c7c32d1f3040023d9c0bbd24f9f5a436c4e6bcf2969f61baafd1d`.
Retained local evidence: `target/installer36-red-build.log`,
`target/installer36-red-probe.log`, `target/installer36-red-host.log` and the
matching before/after SHA-256 records. This confirms these three precise
defects, not Goose failure behavior or whole-installer qualification.

## Intended coherent repair

1. Introduce cohesive prepared recipe inputs and a named resolved workload
   source, retaining project-root/runtime/environment authority and the existing
   NPM/Python install behavior. Do not claim rollback of package-manager changes.
2. Route ordinary Host and Goose executable publication through the shared
   private sibling staging and validated per-file commit. Keep local sources as
   `Path` values: non-UTF-8 host paths must not be lossy-round-tripped through a
   URL/string API. Preserve pinned Goose archive verification and adapter-owned
   extraction/version rules; no live download is needed for fault tests.
3. Pin staged-file identity around optional probe execution; recheck regular
   type, bounds, executable mode and supplied checksum afterward, before commit.
   Reject replacement or unexpected mutation without publishing unchecked bytes.
   Retain honest platform scope, atomic-replacement and durability semantics.
4. Explicitly settle only owned staging/extraction entries, with bounded safe
   diagnostics and retained paths on failure. Never recursively delete unknown
   files or remove a previously ready executable to make installation succeed.
5. Test source/probe/copy/mode/rename failures, symlink/replacement races, pinned
   local Goose archives, non-UTF-8 paths and automatic dependency no-clobber.
   Preserve primary errors and report retained scratch without inventing success.

Independent review, actual red/green evidence, focused/full offline tests,
Clippy/formatting/documentation gates, PR CI, merge, both post-merge jobs and a
signed patch release remain required. The removed 90% coverage gate stays removed.

## Independent review of the observed-red checkpoint

A separate agent reviewed `cce58fa4` and the documentation-only `55ef7683`,
confirmed all three source defects and approved the regressions and repair
direction. This is not approval of an unimplemented production fix. Additional
required coverage and design constraints:

- Reject a different staged regular file even with identical content, size,
  mode and matching checksum. Without a supplied checksum, still reject changed
  same-inode content, lost executable mode, empty output and oversized output.
- Exercise replacement between final validation and commit or explicit cleanup.
  Verify namespace/source identity; never publish or delete a foreign arrival.
  State private quiet-custody assumptions honestly, not a same-UID sandbox claim.
  Explicit failed settlement must not silently retry in Drop.
- Preserve old Host/Goose bytes and mode after copy, mode, probe, checksum,
  sync and rename failures. Automatic dependency no-clobber also needs a real
  concurrent-creator fixture, not only a preexisting destination.
- Preserve raw OS paths for both local sources and project/destination names.
  Goose checksum argv currently uses `to_string_lossy`; fix that path too,
  rather than only adding a typed local-source wrapper.
- Extraction failures, partial archives and unexpected extra entries must
  retain unknown scratch with actionable diagnostics, never recursive deletion.
- Include a real local successful version probe that mutates its own file,
  plus healthy probe and multicall-basename tests. Keep pre-spawn cleanup
  custody, existing bounded child waits and adapter-owned progress ordering.

## Implemented source contract (independent final review pending)

Prepared recipe inputs separate Host, pinned Goose, package-manager and custom
installation paths. Resolved workloads retain their project configuration and
typed internal URL-or-local-path source. Existing public `source: String`
interfaces stay unchanged; already-typed local paths, `--dir`, project/runtime
paths and checksum argv preserve their raw OS spelling. NPM/Python authority
and package-manager behavior are unchanged; no package rollback is promised.

Host and Goose use shared bounded staged-file publication. The admitted file
handle survives the optional probe. Identity, content digest, regular type,
size, mode and any supplied checksum are validated afterward. Unix sync uses
the admitted read-only descriptor, preserving readonly non-executable inputs.
Actual executable probes retain their basename and have no writable description
held open across exec. Existing source/archive limits remain 1 GiB / 256 MiB.

The probe/download-visible pathname is not the commit source. After its final
check, that entry is atomically detached into a fresh 0700 private quarantine
and compared to the admitted handle. A foreign arrival is restored with a
no-clobber hard link where possible; its quarantine entry remains evidence even
when recovery succeeds. Another creator at the original name is never replaced.
Only the admitted handle is copied, with bounded memory and byte count, into a
second private commit stage never supplied to a subprocess or validation
callback. The copy is checked and synced before atomic replacement or the
automatic-dependency no-clobber hard-link operation. This costs an additional
bounded disk copy, not a larger in-memory capture.

Explicit cleanup uses the same atomic detachment and identity check before
unlinking, including Goose extraction. Empty directory settlement also detaches
the exposed directory before verifying/removing it. Unknown entries are never
recursively removed. A foreign detached directory is retained with its exact
path and automatic recovery refused: ordinary rename could overwrite a second
arrival and directories cannot be restored with a no-clobber hard link. A failed
explicit settlement is not retried by Drop. Errors distinguish failure before
publication from successful publication with unconfirmed scratch cleanup, and
retain the primary failure. Unwinding may retain private quarantine evidence.

These are private quiet-custody rules under ordinary concurrency: only the
original exposed names are given to children/callbacks. They do not confine a
hostile same-UID process that searches private namespaces, override arbitrary
parent-directory mutation, or sandbox package managers. Windows uses complete
native file IDs rather than timestamp identity, but this work has no Windows
runtime qualification claim. Goose extraction errors, partial archives and
unexpected entries retain the extraction directory for inspection.

The independent review caught a final-check-to-rename/unlink gap in the first
implementation. Its deterministic regression actually published foreign bytes
(exit 101), retained in `target/installer36-final-check-red.log`; the quarantine
and quiet-copy path closes it. An implementation-audit regression also exposed
a readonly-file sync reopen incompatibility (exit 101), retained separately in
`target/installer36-readonly-red.log`; syncing the admitted Unix descriptor
preserves that previously supported input. These are implementation evidence,
not retrospective claims about an observed production incident.

## Focused source qualification

Built the actual ASB binary offline from the final source, then ran 57 installer
library tests and 10 actual `offline_cli` install-filter cases: 67 passed,
zero failed, zero skipped. The matrix includes the original three red cases,
identical-content/checksummed replacement, actual self-mutating successful
version probes, raw OS paths, real concurrent dependency creation, local pinned
Goose archives, partial/unknown extraction entries, final-check file/directory
arrivals, no-clobber recovery collision and quiet-commit copy/sync/rename faults.
Strict all-target/all-feature Clippy and formatting checks also passed.

Actual binary SHA-256 was unchanged before/after the runtime tests:
`312e975b436f8a5e7878a4f920faee0e33bd0d765d47fd8e33ec3ebdac55f9e0`.
Local evidence is retained in `target/installer36-qualified-build.log`,
`target/installer36-qualified-install.log`,
`target/installer36-qualified-actual-install.log`,
`target/installer36-qualified-clippy.log`,
`target/installer36-qualified-fmt.log` and matching before/after hash records.
The original red checkpoint and logs remain unchanged. This is focused Linux
source qualification on the historical 0.2.32 base, not a delivery slot or
whole-crate/full-stack qualification. Final independent source review, current
main integration, full qualification and all hosted/release gates remain pending.
