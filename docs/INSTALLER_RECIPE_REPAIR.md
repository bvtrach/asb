# Installer recipe preparation and publication repair

Source baseline: qualified publication stack `5516c4e9`, version 0.2.32.
The initial test checkpoint is `cce58fa4`. Its three regressions were actually
executed and failed as described below. No production fix, full qualification,
PR or release is claimed yet.

## Source-confirmed gaps and first regressions

The ordinary Host recipe and pinned Goose extraction copy directly into the
destination, unlike explicit source installation. Host copying follows an
existing destination symlink and can overwrite unrelated bytes and permissions.
The isolated Host fixture requires replacement of the directory entry while
preserving the foreign target. Automatic dependency installation has a separate
intentional no-clobber contract; that behavior must not be replaced with the
ordinary explicit reinstall policy.

The shared staged-source installer validates regular-file identity, bounds and
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
