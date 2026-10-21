"""Provider-free owner-loss tests in an exclusive child-subreaper fixture.

Never mark the shared test runner as a subreaper or signal a process group.
"""
import ctypes
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import sys
import threading
import time


def until(predicate, description, seconds=8):
    deadline = time.monotonic() + seconds
    while not predicate():
        if time.monotonic() >= deadline:
            raise AssertionError(description)
        time.sleep(0.005)


def publish_pid(path, pid):
    """A pathname becomes ready only after the complete PID is visible."""
    temporary = path.with_name(f"{path.name}.{pid}.tmp")
    temporary.write_text(str(pid))
    temporary.replace(path)


def client(root):
    assert "ASB_GATE_OWNER_PID" not in os.environ
    first = os.fork()
    if first == 0:
        os.setsid()
        if os.fork() != 0:
            os._exit(0)
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        publish_pid(root / "helper", os.getpid())
        time.sleep(2)
        (root / "late-write").write_text("candidate mutated after owner loss")
        time.sleep(30)
        os._exit(0)
    os.waitpid(first, 0)
    publish_pid(root / "principal", os.getpid())
    time.sleep(30)


def owner(binary, root, threaded, receipts=False):
    for selected in (signal.SIGINT, signal.SIGHUP, signal.SIGTERM):
        signal.signal(selected, lambda *_: os._exit(0))
    def spawn():
        environment = {**os.environ, "ASB_GATE_OWNER_PID": str(os.getpid())}
        inherited = ()
        if receipts:
            reader, writer = socket.socketpair()
            environment["ASB_GATE_RECEIPT_FD"] = str(writer.fileno())
            inherited = (writer.fileno(),)
        gate = subprocess.Popen(
            [binary, "__asb-gate", sys.executable, __file__, "--client", str(root)],
            env=environment, pass_fds=inherited,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        if receipts:
            writer.close()
            reader.settimeout(5)
            hello = bytearray()
            while len(hello) < 16:
                chunk = reader.recv(16 - len(hello))
                assert chunk, "gate exited without Hello"
                hello.extend(chunk)
            assert hello == b"ASBGATE2" + bytes([1, 0, 0, 0]) + struct.pack("<I", gate.pid)
            # Publish this explicit protocol barrier only after validating the
            # gate-authored Hello. The owner retains its reader until exit.
            (root / "hello").touch()
            channels.append(reader)
        publish_pid(root / "gate", gate.pid)
        # Keep Popen alive without making a shared fixture process its owner.
        children.append(gate)
    children = []
    channels = []
    if threaded:
        thread = threading.Thread(target=spawn)
        thread.start()
        thread.join()
        (root / "thread-exited").touch()
    else:
        spawn()
    time.sleep(30)


def signal_pinned(pid, sig):
    descriptor = os.pidfd_open(pid)
    try:
        signal.pidfd_send_signal(descriptor, sig)
    finally:
        os.close(descriptor)


def cleanup_children():
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        children = Path(f"/proc/self/task/{os.getpid()}/children").read_text().split()
        if not children:
            return
        for raw in children:
            pid = int(raw)
            try:
                descriptor = os.pidfd_open(pid)
            except ProcessLookupError:
                continue
            try:
                try:
                    os.waitid(os.P_PIDFD, descriptor, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                except ChildProcessError:
                    continue
                signal.pidfd_send_signal(descriptor, signal.SIGKILL)
                os.waitid(os.P_PIDFD, descriptor, os.WEXITED | os.WNOHANG)
            except ProcessLookupError:
                pass
            finally:
                os.close(descriptor)
        time.sleep(.01)
    raise AssertionError("fixture could not reap its own children")


def guardian(binary, root):
    assert ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) == 0
    sibling = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(60)"])
    try:
        phases = ["startup"] * 8 + ["paused", "running", "threaded", "cancel", "cancel-paused", "cancel-int", "cancel-hup", "group-int", "group-paused"]
        phases += ["receipt-hello", "receipt-paused", "receipt-running", "receipt-threaded"]
        for index, original_phase in enumerate(phases):
            receipts = original_phase.startswith("receipt-")
            phase = original_phase.removeprefix("receipt-")
            case = root / f"{index}-{original_phase}"
            case.mkdir()
            process = subprocess.Popen([sys.executable, __file__, "--owner-receipt" if receipts else "--owner", binary, str(case), phase], start_new_session=True)
            until(lambda: (case / "gate").exists(), "owner did not publish its gate")
            gate = int((case / "gate").read_text())
            if receipts:
                assert (case / "hello").exists(), "owner failed to retain validated Hello"
            if phase not in ("startup", "hello"):
                until(lambda: "\nState:\tT" in Path(f"/proc/{gate}/status").read_text(),
                      "gate never paused")
            if phase in ("running", "threaded", "cancel", "cancel-int", "cancel-hup", "group-int"):
                signal_pinned(gate, signal.SIGCONT)
                until(lambda: (case / "principal").exists() and (case / "helper").exists(),
                      "client and daemonized helper never started")
            if phase == "threaded":
                assert (case / "thread-exited").exists()
                time.sleep(.15)
                assert Path(f"/proc/{gate}").exists(), "spawning thread exit killed gate"
                assert not (case / "late-write").exists()
            if phase.startswith("cancel"):
                selected = {"cancel-int": signal.SIGINT, "cancel-hup": signal.SIGHUP}.get(phase, signal.SIGTERM)
                signal_pinned(gate, selected)
                if phase == "cancel-paused":
                    # A stopped Linux process cannot deliver its handlers.
                    # The owning Rust cancellation API sends TERM + CONT.
                    signal_pinned(gate, signal.SIGCONT)
            elif phase.startswith("group"):
                # This fixture owner created its own session/group; no shared
                # test runner or unrelated process belongs to it.
                assert os.getpgid(process.pid) == process.pid
                os.killpg(process.pid, signal.SIGINT)
                process.wait(timeout=3)
            else:
                process.kill()
                process.wait(timeout=3)

            def reaped():
                try:
                    return os.waitpid(gate, os.WNOHANG)[0] == gate
                except ChildProcessError:
                    # A still-live owner keeps its child; check zombie state.
                    return not Path(f"/proc/{gate}").exists() or (
                        phase.startswith("cancel") and "\nState:\tZ" in Path(f"/proc/{gate}/status").read_text()
                    )
            until(reaped, f"gate survived {phase}")
            for name in ("principal", "helper"):
                if (case / name).exists():
                    pid = int((case / name).read_text())
                    assert not Path(f"/proc/{pid}").exists(), f"{name} survived {phase}"
            assert not (case / "late-write").exists(), f"candidate mutated during {phase}"
            if phase in ("startup", "hello", "paused"):
                assert not (case / "principal").exists(), f"client was admitted after owner loss in {phase}"
            assert sibling.poll() is None, "unrelated sibling was stopped"
            if phase.startswith("cancel"):
                process.kill()
                process.wait(timeout=3)
                until(reaped, "cancelled gate was not reaped after owner exit")
        # A failed gate can leave its wake helper adopted by the guardian;
        # require no such leftovers before the fixture's defensive cleanup.
        until(lambda: Path(f"/proc/self/task/{os.getpid()}/children").read_text().split()
              == [str(sibling.pid)], "owner-loss watcher survived its gate")
    finally:
        cleanup_children()


if __name__ == "__main__":
    if sys.argv[1] == "--client":
        client(Path(sys.argv[2]))
    elif sys.argv[1] in ("--owner", "--owner-receipt"):
        owner(sys.argv[2], Path(sys.argv[3]), sys.argv[4] == "threaded", sys.argv[1] == "--owner-receipt")
    else:
        guardian(sys.argv[1], Path(sys.argv[2]))
