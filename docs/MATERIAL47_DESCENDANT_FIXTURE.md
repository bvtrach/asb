# Material47 descendant fixture task-view correction: source checkpoint

New isolated source from67bd90a1; prior full qualification/cache unchanged.
PR316 CI39727416287 packaged library failed1236/1/13. New bounded diagnostic
shows child test failed immediately (0.01s) at descendants.rs291: observed
leader child population[] versus its two actual Child IDs. This preceded
DENIED_CHILD assignment and production quiesce. Guardian reported InnerFixtureFailed,
not a timeout or demonstrated production sibling starvation. Original two older
failure logs and this diagnostic remain preserved in the root delivery checkout.

## Primary-source explanation and limits

Linux v6.12 do_exit calls exit_mm before exit_notify. mm_release clears the
thread's child-TID futex; glibc pthread_join synchronizes on that clearing.
Reparenting/list splice occurs later in exit_notify/forget_original_parent.
Thus joining a spawning thread does not establish leader child-list readiness.
find_new_reaper also selects a live thread, not a fixture-promised leader.

- [Linux exit ordering and reparenting](https://github.com/torvalds/linux/blob/v6.12/kernel/exit.c#L653-L683)
- [Linux child-TID wake](https://github.com/torvalds/linux/blob/v6.12/kernel/fork.c#L1513-L1547)
- [glibc join synchronization](https://github.com/bminor/glibc/blob/glibc-2.40/nptl/pthread_join_common.c#L81-L100)

The hosted transcript proves the false immediate population assumption, not
which exact kernel handoff/thread choice occurred. No hosted kernel version or
real production cleanup failure is invented from these reference sources.

## Narrow test-only correction

The two exclusive guardian/reexec cases now create their actors directly on the
current libtest worker. A private thread-local RAII task-view captures only real
gettid and applies only when children(owner) is inspecting this actual process.
Default/non-test discovery still reads /proc/leader/task/leader/children. Other
owners cannot use the override. No process-wide task scan, arbitrary PID/signal,
new helper command, sleep, retry or timeout increase.

The exact original two-child population assertion remains; the reap fixture
adds an exact three-child assertion. Production pidfd+waitid ownership checks,
EPERM/EIO fault placement, fair sibling work, unchanged five-second allowance,
actual final reaping and unrelated guardian sentinel assertions remain. RAII
restores the view on panic, with a pure unwind control. Existing real native-gate
tests continue to validate the unchanged leader route; this private task view
does not claim libtest is the production supervisor process topology.

Source-only: no build/runtime yet. Root independently reviews full diff before
actual-FIRST/exact Cargo JSON, unchanged former failure and all owning focus.
No production child-scan change, full/PR/push/release or budget widening.
