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


def container_task(root, scenario):
    from asb_resources import ResourceJournal
    from asb_execution import ExecutionError
    from asb_material import MaterialError
    from asb_material_exec import run
    from asb_docker import DockerSession, StdioConnections
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
                if pid == denied:
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
    except PermissionError:
        save(root, "transport-error.json", "PermissionError")
        return 1
    finally:
        for connection in sockets:
            connection.close()


def guardian(root, binary, test, scenario):
    # Only this exclusive fixture subprocess adopts children, never shared ASB.
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0):
        raise OSError(ctypes.get_errno(), "fixture subreaper")
    sentinel = subprocess.Popen(["/bin/sleep", "30"])
    env = {**os.environ, "ASB_MATERIAL_CUSTODY_CHILD": scenario,
           "ASB_MATERIAL_CUSTODY_ROOT": str(root)}
    with (root / "child.stdout").open("wb") as output, (root / "child.stderr").open("wb") as errors:
        child = subprocess.Popen([binary, "--exact", test, "--nocapture"],
                                 env=env, stdout=output, stderr=errors)
        try:
            status = child.wait(timeout=15)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=3)
            status = -999
    adopted_alive = []
    cleanup = []
    # The observed Rust child has exited, so orphans are now our exact children.
    pids = [int(value) for value in Path(f"/proc/{os.getpid()}/task/{os.getpid()}/children").read_text().split()]
    for pid in pids:
        if pid == sentinel.pid:
            continue
        result, _ = os.waitpid(pid, os.WNOHANG)
        if result == 0:
            adopted_alive.append(pid)
            # This unreaped exact child cannot have its PID recycled.
            os.kill(pid, signal.SIGKILL)
            end = time.monotonic() + 3
            while os.waitpid(pid, os.WNOHANG)[0] == 0:
                if time.monotonic() >= end:
                    raise RuntimeError("fixture child cleanup deadline")
                time.sleep(.005)
        cleanup.append(pid)
    sentinel_alive = sentinel.poll() is None
    sentinel.kill()
    sentinel.wait(timeout=3)
    save(root, "guardian.json", {"child_status": status,
        "adopted_alive_after_controller_exit": adopted_alive,
        "actually_reaped": cleanup, "sentinel_survived": sentinel_alive})
    return 0


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
        time.sleep(30)  # Guardian kills/reaps; finite fallback if harness fails.
    elif mode == "connection":
        save(root, f"connection-{os.getpid()}.json", True)
        time.sleep(30)
