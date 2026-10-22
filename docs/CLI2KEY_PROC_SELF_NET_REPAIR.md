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
