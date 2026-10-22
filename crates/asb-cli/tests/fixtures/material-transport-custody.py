"""Class50 inert whole-path fixtures; no Docker daemon, SDK, or privilege.

The outer guardian is an exclusive subreaper. It cleans real adopted children
before returning observations to the Rust test's intended contract assertion.
"""
import ctypes
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve()
sys.path.insert(0, str(HERE.parents[2] / "assets/benchmarks"))


def save(root, name, value):
    (root / name).write_text(json.dumps(value))


def wait_file(path):
    end = time.monotonic() + 5
    while not path.exists():
        if time.monotonic() >= end:
            raise RuntimeError("owned fixture handshake deadline")
        time.sleep(.005)


def await_release(root):
    # Owned fixture actors accept a real release before guardian cleanup waits.
    # A finite natural-exit fallback also covers assertion/unwind before the
    # release request. The guardian never kills an admitted ASB supervisor.
    end = time.monotonic() + 30
    while not (root / "release-fixtures").exists() and time.monotonic() < end:
        time.sleep(.005)


def container_task(root, scenario):
    from asb_resources import ResourceJournal
    from asb_execution import ExecutionError
    from asb_material import MaterialError
    from asb_material_exec import run
    from asb_docker import DockerAccessError, DockerSession, StdioConnections
    from asb_task_server import serve

    class Container:
        def __init__(self, name, labels):
            self.name = name
            self.attrs = {"Config": {"Labels": labels}, "State": {"Running": True}}
        def stop(self, timeout):
            if timeout != 2:
                raise RuntimeError("wrong preservation timeout")
            self.attrs["State"]["Running"] = False
            save(root, "stopped.json", {"container": self.name})
        def reload(self):
            save(root, "reloaded.json", {"running": self.attrs["State"]["Running"]})

    class Client:
        def info(self):
            return {"ID": "inert-owned-fixture-daemon"}
        def close(self):
            save(root, "sdk-closed.json", True)

    client = Client()
    save(root, "scenario.json", scenario)
    if scenario in ["retained", "ownerloss"]:
        # Only this inert principal delays termination until actual release;
        # no host signal disposition or production process is changed.
        signal.signal(signal.SIGTERM, lambda *_: None)
    journal = ResourceJournal(client, root, "asb-" + "5" * 32)
    name = "asb-" + "5" * 32 + "-container"
    labels = journal.declare("containers", name)
    container = Container(name, labels)
    class Containers:
        def get(self, selected):
            if selected != name:
                raise RuntimeError("foreign fixture container")
            return container
    client.containers = Containers()
    transport = StdioConnections([sys.executable, "-I", "-S", "-B", str(HERE),
                                  "connection", str(root)])
    session = DockerSession()
    session.client, session.connections = client, transport
    sockets = []

    class Task:
        invocation = {"prompt": "inert material fixture", "workspace": "/fixture"}
        cleanup_status = "pending"
        def __enter__(self):
            return self
        def __exit__(self, *_error):
            if not self.interrupted:
                raise RuntimeError("fixture expected preserving cancellation")
            try:
                journal.preserve(client)
                self.cleanup_status = "stopped-preserved"
            finally:
                journal.close()
        def initial_material(self):
            if scenario == "inherited":
                before = set(threading.enumerate())
                try:
                    run([sys.executable, "-I", "-S", "-B", str(HERE),
                         "material", str(root)], deadline=time.monotonic() + 1)
                except MaterialError:
                    save(root, "workers.json", {
                        "live_after_error": sum(t.is_alive() for t in
                            set(threading.enumerate()) - before)})
                    raise ExecutionError("fixture material observation failed") from None
                raise RuntimeError("inherited pipe unexpectedly became material")
            for index in range(3):
                sockets.append(transport.connect(1))
                wait_file(root / f"connection-{transport.children[-1].pid}.json")
                save(root, "connections.json", [child.pid for child in transport.children])
            # Test-local API denial, not actual root privilege or a kernel hang.
            original = os.killpg
            denied = transport.children[0].pid
            def deny_first(pid, sig):
                if pid == denied and not (root / "release-fixtures").exists():
                    raise PermissionError("inert fixture signal denial")
                return original(pid, sig)
            os.killpg = deny_first
            raise ExecutionError("fixture material observation failed")

    try:
        try:
            status = serve(Task(), sys.stdin, sys.stdout)
        finally:
            # Exactly the real DockerSession teardown outside real serve.
            session.__exit__(None, None, None)
        return status
    except (PermissionError, DockerAccessError) as error:
        save(root, "transport-error.json", type(error).__name__)
        return 1
    finally:
        for connection in sockets:
            connection.close()


def guardian(root, binary, test, scenario):
    # Only this exclusive fixture subprocess adopts children, never shared ASB.
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0):
        raise OSError(ctypes.get_errno(), "fixture subreaper")
    sentinel = subprocess.Popen(["/bin/sleep", "90"])
    env = {**os.environ, "ASB_MATERIAL_CUSTODY_CHILD": scenario,
           "ASB_MATERIAL_CUSTODY_ROOT": str(root)}
    with (root / "child.stdout").open("wb") as output, (root / "child.stderr").open("wb") as errors:
        child = subprocess.Popen([binary, "--exact", test, "--nocapture"],
                                 env=env, stdout=output, stderr=errors)
        deadline = time.monotonic() + 15
        while child.poll() is None and time.monotonic() < deadline:
            if (root / "request-release").exists() and not (root / "release-ack.json").exists():
                (root / "release-fixtures").touch()
                save(root, "release-ack.json", True)
            time.sleep(.005)
        if child.poll() is None:
            # This is the test owner, NOT an admitted gate. Owner death must
            # leave its real independent gate supervising the exact children.
            child.kill()
            child.wait(timeout=3)
            status = -999
        else:
            status = child.returncode
    adopted_alive = []
    cleanup = []
    identities, gates, cleanup_error = {}, [], None
    gate_record = json.loads((root / "gate.json").read_text()) if (root / "gate.json").exists() else None
    expected_gate = gate_record["pid"] if gate_record else None
    actual_asb = Path(binary).parent.parent / "asb"
    # Release every known actor/fault before waiting for actual supervisor
    # completion. Unlike the baseline RED guardian there is NO KILL of adopted
    # children. Repeated snapshots handle late reparenting after owner loss.
    (root / "release-fixtures").touch()
    end = time.monotonic() + 35
    try:
        while True:
            pids = [int(value) for value in Path(f"/proc/{os.getpid()}/task/{os.getpid()}/children").read_text().split()]
            pids = [pid for pid in pids if pid != sentinel.pid]
            if not pids:
                settled = (json.loads((root / "gate-settled.json").read_text())
                           if (root / "gate-settled.json").exists() else {})
                reported = (json.loads((root / "controller.json").read_text())
                            if (root / "controller.json").exists() else {})
                if (expected_gate is not None and expected_gate not in cleanup
                        and not settled.get("actual_exit_and_reap")
                        and not reported.get("cancel_confirmed")):
                    # An empty first snapshot is not a receipt for a known
                    # admitted gate. Require actual owner/keeper reap evidence
                    # or our own later adoption+pidfd reap, never KILL it.
                    if time.monotonic() >= end:
                        raise RuntimeError("known gate has no actual settlement observation")
                    time.sleep(.005)
                    continue
                break
            for pid in pids:
                if pid not in identities:
                    descriptor = os.pidfd_open(pid)
                    # Non-reaping kernel check pins exact ownership, including
                    # a live child; no discovered PID becomes a signal target.
                    event = os.waitid(os.P_PIDFD, descriptor, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                    identities[pid] = descriptor
                    if event is None:
                        adopted_alive.append(pid)
                    if pid == expected_gate:
                        image = actual_asb.stat()
                        if [image.st_dev, image.st_ino] != gate_record["image"]:
                            raise RuntimeError("actual ASB changed during fixture")
                        if event is None:
                            try:
                                matches = os.path.samefile(f"/proc/{pid}/exe", actual_asb)
                            except FileNotFoundError:
                                # A zombie may lose /proc/exe after the pinned
                                # ownership observation; known admission image
                                # plus actual exit is not a different live PID.
                                matches = os.waitid(os.P_PIDFD, descriptor,
                                    os.WEXITED | os.WNOHANG | os.WNOWAIT) is not None
                            if not matches:
                                raise RuntimeError("adopted fixture guardian is not the exact actual ASB")
                        gates.append(pid)
                descriptor = identities[pid]
                event = os.waitid(os.P_PIDFD, descriptor, os.WEXITED | os.WNOHANG)
                if event is not None:
                    # Verify actual kernel reap of that pinned process.
                    try:
                        os.waitid(os.P_PIDFD, descriptor, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                    except ChildProcessError:
                        cleanup.append(pid)
                        os.close(identities.pop(pid))
                    else:
                        raise RuntimeError("fixture guardian child was not reaped")
            if time.monotonic() >= end:
                raise RuntimeError("fixture release did not produce actual descendant/gate settlement")
            time.sleep(.005)
    except BaseException as caught:
        cleanup_error = type(caught).__name__
    finally:
        for descriptor in identities.values():
            os.close(descriptor)
    sentinel_alive = sentinel.poll() is None
    sentinel.kill()
    sentinel.wait(timeout=3)
    save(root, "guardian.json", {"child_status": status,
        "adopted_alive_after_controller_exit": adopted_alive,
        "actually_reaped": cleanup, "sentinel_survived": sentinel_alive,
        "adopted_actual_gates": gates, "adopted_gate_kills": 0,
        "cleanup_error": cleanup_error})
    return 1 if cleanup_error else 0


if __name__ == "__main__":
    mode, root = sys.argv[1], Path(sys.argv[2])
    if mode == "guardian":
        raise SystemExit(guardian(root, *sys.argv[3:]))
    if mode == "controller":
        raise SystemExit(container_task(root, sys.argv[3]))
    if mode == "material":
        holder = subprocess.Popen([sys.executable, "-I", "-S", "-B", str(HERE),
                                   "holder", str(root)])
        wait_file(root / "holder-ready.json")
        raise SystemExit(0)
    if mode == "holder":
        save(root, "holder-ready.json", {"pid": os.getpid()})
        await_release(root)
    elif mode == "connection":
        if json.loads((root / "scenario.json").read_text()) in ["retained", "ownerloss"]:
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
        save(root, f"connection-{os.getpid()}.json", True)
        await_release(root)
