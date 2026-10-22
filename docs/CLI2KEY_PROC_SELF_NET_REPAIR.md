# Task-local procfs network-table test access

Base: actual main `c725266b77f047d6883f59c9fa466d14227852ef`, Cargo0.2.44.
This is a test-only postmerge correction; no product or host policy changes.

Postmerge run39708009003 local-host job119592487421 failed eight library
tests: each reached one of three unconditional `/proc/net/tcp` reads and
received ENOENT. The outer result was1203passed/8failed/13ignored. The original
complete log is retained at
`/home/martin/projects/asb-delivery-pr310/target/pr310-postmerge-local-failed.log`.
Exact-head hosted CI had passed; these are distinct qualification environments.

Read-only inspection of the actual `asb-local-ci.service` unit and live
`systemctl show` confirms `ProcSubset=pid`, `ProtectProc=invisible`, user
`gha-asb`. Linux subset=pid hides non-task top-level procfs entries, consistent
with the missing `/proc/net` alias. Current-user traversal through the running
service's `/proc/PID/root` is denied, so this inspection does not directly prove
the service's mounted view or attribute every historical detail. No namespace,
privilege, service configuration or host policy is changed to investigate it.

Linux documents `/proc/net` as a symbolic link to `/proc/self/net` since2.6.25;
both expose the calling process's network namespace, not just its own sockets.
The task-associated spelling avoids the hidden top-level alias. References:
[Linux proc_pid_net](https://man7.org/linux/man-pages/man5/proc_pid_net.5.html),
[kernel procfs mount options](https://docs.kernel.org/filesystems/proc.html#mount-options),
[systemd ProcSubset](https://www.freedesktop.org/software/systemd/man/258/systemd.exec.html#ProcSubset=).

The patch changes exactly three path literals. All eight original test cases,
exact socket inode checks, parent/child FD identity checks, high-FD exec proof,
bounded cleanup waits and unrelated-listener sentinels remain unchanged. No
skip, numeric-port substitution, weakened assertion, fallback or retry is added.
The namespace relationship is unchanged: these fixtures do not unshare their
network namespace between reservation and observation.

Planned qualification: after source review, build the actual ASB first in this
new owned HOME worktree/cache, freeze/hash it, run the two complete owning
cli2key custody/proxy families and offline CLI cases, then strict Clippy/fmt.
A bounded controlled loopback-listener probe will require the same exact inode
through both spellings while live and its absence after confirmed close.
That local probe does not substitute for the real hardened postmerge job;
release remains held until independent review and hosted/postmerge gates pass.
No runtime or corrected-source qualification is claimed by this source checkpoint.

## First local qualification and reviewed fixture prerequisite

Source37bd0568 built actual ASB FIRST13.14s; actual/frozen
`target/qualification/proc-self-net-37bd0568-asb` SHA256 is
`938009d6750c27d9da7975c337f47e49f45d750198893d5b4c20c8181c47e751`.
The owned HOME cache copied only verified-idle prior fixture-cache debug outputs;
those copied artifacts were not qualification evidence. The alias control
observed the same exact owned live inode through both paths, then bounded its
absence after close and confirmed the unrelated sentinel survived. It did not
enter the hardened service mount namespace.

Complete custody15/0/0, proxy4/0/0 and offline_cli98/0/5ignored passed, including
all eight previously failing cases. Fresh full workspace locked/offline all-target
all-feature suite passed1466/0/21ignored across24 outer targets. Complete logs
and compact outer-only summaries use `target/proc-self-net-*`; before/after full
and normal gates hashes matched938009d6. Doctests0/0, strict Clippy15.45s,
fmt/diffcheck, sequential make/private Rustdocs/pinned AutoUML0.7.2/flows/
interactive/completions and update tests7/0 passed. An operator shell-check
command first named nonexistent target/completions output paths; checking the
actual generated target/asb.bash and target/_asb then passed both syntax gates.
An archive-inspection command likewise initially used an incorrect support-file
path; the retained first inspection output is not a passing asset gate. Correct
tests/unit/executable_fixture_writer.rs and immutable gate asset parity passed.

After all runtime/build handles terminated, root approved preserving the exact
regular nonsymlink zero-byte generated install.log with source/dev/inode/mode
metadata under target/fixture-preservation; only its empty .asb parent was removed
with rmdir. Crate target diagnostics remain. Locked/offline cargo package passed,
511files, verification13.24s; package help and archive asset/mode checks passed.
Packaging legitimately replaced ordinary debug ASB with55a5f91e (help-only),
retained as qualification/proc-self-net-37bd0568-packaged-asb; the full-suite
frozen938009d6 remains unchanged. Archive SHA256 was
`fad601a08a4a6f16cd6c22a95ccdc8903be1a18af12c8ac32ed4a53369d9c8b0`.

Before promotion, root requested the independently reviewed test-only prerequisite
from ab877c31d60ec133e59264a85712b402b8b2636e: exact project_capabilities.rs
file delta only, NOT fair-reaper production or docs. The unchanged eleven consent
cases can legitimately close optional stdin before a parent pipe write; another
qualification observed inherited BrokenPipe there. Supplying the same exact input
bytes in a read-only file opened before child spawn removes that race without
ignoring EPIPE or changing status/output assertions. Exact file parity against
ab877c31 passed. No failure in the first37bd suite is attributed to this seam.
The first proof remains historical; this new signed candidate still requires an
actual-FIRST rebuild, consent focus and fresh full/final gates before promotion.

## Final local qualification, source `7cb6d428`

All previous handles were terminal before the exact reviewed consent-fixture
backport. Actual-FIRST cargo build completed exit0 (0.03s cached), restoring the
main-source image after package verification; it was frozen separately as
`target/qualification/proc-self-net-7cb6d428-asb`. SHA256 remains
`938009d6750c27d9da7975c337f47e49f45d750198893d5b4c20c8181c47e751`.
The consent owning test passed1outer/0/0, covering all eleven unchanged child
cases. Fresh full locked/offline workspace/all-target/all-feature suite completed
exit0: **1466passed/0failed/21ignored across24 outer targets**. The library is
1211/0/13, offline_cli98/0/5 and repeated_run33/0/0. No nested child summaries
are counted twice. All eight originally failing cases passed in this final run.

Complete final logs use `target/proc-self-net-final-*`: build, consent, full,
full-outer-summaries, doctests, clippy, fmt, diffcheck, make, update-tests,
completion-syntax, package, package-help, package-assets and package-hashes.
Before/full-after/normal-gates-after SHA256 logs agree for actual and frozen
938009d6. No source edit or relink occurred during runtime. Doctests0/0,
strict workspace/all-target/all-feature Clippy `-D warnings`10.34s, fmt/diffcheck,
sequential make/private Rustdocs/pinned AutoUML0.7.2/flows/interactive/completions,
both actual completion-file syntax checks and update tests7/0 passed. This is
the update-test gate, not mutation through make update. All handles are terminal.

The newly generated regular nonsymlink zero-byte install.log was again preserved
with exact metadata under target/fixture-preservation after runtime/builds ended;
only its empty .asb parent was removed, and crate target diagnostics remain.
Final locked/offline cargo package passed511files/13.86s verification. Actual
archived owning tests, writer support and mode0755 immutable mock gate match
checkout bytes; generated crate target artifacts are excluded. Package help
passed. Package verification legitimately replaced ordinary debug ASB with
`55a5f91e45fd235aeb2fb4b6158ace95a87e12b66b7eaaa3a0f0653212e142a3`,
retained as qualification/proc-self-net-7cb6d428-packaged-asb (help-only, not the
full-suite binary). Frozen full-suite938009d6 remains unchanged. Final archive
SHA256 is `14ba071e31240f89be74dd339ed54b19c3a8c621009faf98afdf72a960e97fa4`.

Scope is local Linux x86_64 offline qualification, not direct hardened-service
view, Windows, external SDK/client/provider or privileged tracing qualification.
No product source, Cargo version, workflow, original assertion, retry or skip
policy changed. The removed coverage gate stays removed. This signed docs-only
proof follows the frozen source; independent final review, new hotfix PR/CI,
actual postmerge hardened job and both release gates remain pending. Root owns
delivery; this worker has not pushed, opened a PR, merged or released anything.
