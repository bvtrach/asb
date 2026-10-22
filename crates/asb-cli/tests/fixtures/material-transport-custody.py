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


def guardian(root, binary, test, scenario, actual_asb, control="normal"):
    if control not in ["normal", "deferred", "short-report"]:
        raise ValueError("unknown private guardian fixture control")
    deferred = control != "normal"
    report_seconds = .04 if control == "short-report" else 35
    # Only this exclusive fixture subprocess adopts children, never shared ASB.
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0):
        raise OSError(ctypes.get_errno(), "fixture subreaper")
    actual_asb = Path(actual_asb)
    image = actual_asb.stat()
    output = (root / "child.stdout").open("wb")
    errors = (root / "child.stderr").open("wb")
    sentinel = subprocess.Popen(["/bin/sleep", "90"])
    env = {**os.environ, "ASB_MATERIAL_CUSTODY_CHILD": scenario,
           "ASB_MATERIAL_CUSTODY_ROOT": str(root)}
    spawn_error = None
    try:
        child = subprocess.Popen([binary, "--exact", test, "--nocapture"],
                                 env=env, stdout=output, stderr=errors)
    except BaseException as caught:
        child, spawn_error = None, type(caught).__name__
    adopted_alive = []
    cleanup = []
    identities, gates = {}, []
    cleanup_error, gate_record, gate_identity = None, None, None
    status = -999 if child is None else None
    cleanup_deadline = time.monotonic()+report_seconds if child is None else None
    owner_deadline, owner_kill_requested = time.monotonic()+15, False

    def report(category):
        nonlocal cleanup_error
        if cleanup_error is None:
            cleanup_error = category
            try:
                save(root,"guardian-error.json",{"error":category,
                    "custody":"retained-not-settled","namespace":str(root)})
            except OSError:
                # Rust's independent caller deadline still transfers this exact
                # guardian wait-only if even finite diagnostic publication fails.
                pass

    def read_record(name):
        try:return json.loads((root/name).read_text())
        except (FileNotFoundError,ValueError):return None

    def gate_exited():
        if gate_identity is None:
            # A failed pre-ack fixture cannot claim an admitted gate receipt.
            # After actual inner-owner reap and an empty kernel child namespace
            # it may exit FAILED, never success, without retaining a dead PID.
            return gate_record is None or (cleanup_error is not None and status not in [None,0])
        import select
        return bool(select.select([gate_identity],[],[],0)[0])

    # A finite error report is NOT an ownership deadline. On uncertainty retain
    # every exact pidfd/Popen and keep polling/reaping released children. Only
    # actual namespace settlement permits closing those owners and exiting.
    if spawn_error is not None:report(spawn_error)
    while True:
        try:
            if gate_identity is None and (record:=read_record("gate.json")) is not None:
                try:
                    gate_record = record
                    if record["image"] != [image.st_dev,image.st_ino]:
                        raise RuntimeError("admission image differs from explicit actual ASB")
                    descriptor = os.pidfd_open(record["pid"])
                    # Inner Rust waits for this ack before progressing. Its
                    # unreaped live gate cannot be recycled underneath the pin.
                    try:
                        if not os.path.samefile(f"/proc/{record['pid']}/exe",actual_asb):
                            raise RuntimeError("known gate has a different executable")
                    except BaseException:
                        os.close(descriptor)
                        raise
                    gate_identity = descriptor
                    save(root,"gate-identity-ack.json",True)
                except BaseException as caught:
                    report(type(caught).__name__)
            if status is None:
                status = child.poll()
                if status is None:
                    if (root/"request-release").exists():
                        (root/"release-fixtures").touch()
                        save(root,"release-ack.json",True)
                    if time.monotonic() >= owner_deadline and not owner_kill_requested:
                        report("InnerFixtureDeadline")
                        # Only the direct inert TEST OWNER, never the admitted
                        # ASB gate. Keep its Popen until actual nonblocking reap.
                        child.kill()
                        owner_kill_requested = True
                    time.sleep(.005)
                    continue
                if status != 0:report("InnerFixtureFailed")
                cleanup_deadline = time.monotonic()+report_seconds
                if not deferred:
                    (root/"release-fixtures").touch()
                output.close()
                errors.close()
            pids = [int(value) for value in Path(f"/proc/{os.getpid()}/task/{os.getpid()}/children").read_text().split()]
            pids = [pid for pid in pids if pid != sentinel.pid]
            if not pids:
                if gate_exited():break
                # A single empty adoption snapshot is never a receipt for a
                # still-live pinned actual guardian. Keep owning/polling.
            for pid in pids:
                if pid not in identities:
                    descriptor = os.pidfd_open(pid)
                    # Non-reaping kernel check pins exact ownership, including
                    # a live child; no discovered PID becomes a signal target.
                    event = os.waitid(os.P_PIDFD, descriptor, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                    identities[pid] = descriptor
                    if event is None:
                        adopted_alive.append(pid)
                    if gate_record is not None and pid == gate_record["pid"]:
                        gates.append(pid)
                        if event is None:
                            # Positive direct kernel ownership while the exact
                            # admitted gate is still live, before parent release.
                            save(root,"adopted-live-gate.json",{"pid":pid})
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
            if time.monotonic() >= cleanup_deadline:
                report("CleanupDeadlineUnknown")
                # No new deadline, KILL, handle closure or exit on uncertainty.
            time.sleep(.005)
        except BaseException as caught:
            report(type(caught).__name__)
            try:(root/"release-fixtures").touch()
            except OSError:pass
            time.sleep(.01)
    # The loop exits ONLY after actual inner-owner reap, an empty repeatedly
    # observed child population, and pinned known-gate exit (if any).
    for descriptor in identities.values():os.close(descriptor)
    if gate_identity is not None:os.close(gate_identity)
    sentinel_alive = sentinel.poll() is None
    if sentinel_alive:
        try:sentinel.kill()  # Exact direct sentinel only, never an ASB guardian.
        except OSError as caught:report(type(caught).__name__)
    sentinel_deadline = time.monotonic()+3
    while True:
        try:
            if sentinel.poll() is not None:break
        except OSError as caught:report(type(caught).__name__)
        if time.monotonic() >= sentinel_deadline:report("SentinelSettlementUnknown")
        time.sleep(.01)  # Retain direct ownership rather than exiting on error.
    output.close()
    errors.close()
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
