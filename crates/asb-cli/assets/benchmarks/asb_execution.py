"""Agent/provider/benchmark-neutral execution inside an owned Docker workspace.

No host mounts, host networking or privileged container mode. A stopped shell
gate reports both namespace and daemon-host PIDs before the caller attaches
collectors and resumes execution. Docker-daemon PIDs are deliberately not
claimed to be local-host PIDs: the caller must establish that separately.
"""
import re
import threading
import time


class ExecutionError(RuntimeError):
    pass


GATE = 'printf "__ASB_GATE__:%s\\n" "$$"; kill -STOP "$$"; exec "$@"'
STOPPED = 'while IFS=: read -r key value; do case "$key" in State) case "$value" in *T*) exit 0;; esac;; esac; done < "/proc/$1/status"; exit 1'
MAX_ARGUMENT_BYTES = 1024 * 1024


class ManagedDockerWorkspace:
    @classmethod
    def attach(cls, client, container, attempt, workdir):
        """Use an official harness-created container with verified ASB ownership.

        Its owning workload session remains responsible for Compose/sidecar
        teardown; generic command execution never enumerates host resources.
        """
        labels = container.attrs.get("Config", {}).get("Labels", {}) or {}
        if labels.get("asb.owner") != "asb" or labels.get("asb.attempt") != attempt:
            raise ExecutionError("cannot attach to a task environment outside this attempt")
        workspace = cls(client, container.image, attempt, workdir)
        host = container.attrs.get("HostConfig", {})
        if host.get("PidMode") not in [None, "", "private"]:
            raise ExecutionError("task execution requires a private container PID namespace")
        workspace.limits = {"memory_bytes": host.get("Memory") or None,
                            "cpus": (host.get("NanoCpus") or 0) / 10**9 or None,
                            "pids": host.get("PidsLimit") if (host.get("PidsLimit") or 0) > 0 else None}
        workspace.container, workspace.cleanup_status = container, "pending"
        return workspace

    def __init__(self, client, image, attempt, workdir, memory_bytes=4 * 1024**3,
                 cpus=2, pids=512):
        if not re.fullmatch(r"asb-[0-9a-f]{32}", attempt):
            raise ExecutionError("workspace requires a unique ASB attempt identity")
        if not isinstance(workdir, str) or not workdir.startswith("/") or "\x00" in workdir:
            raise ExecutionError("workspace working directory must be absolute")
        if not 128 * 1024**2 <= memory_bytes <= 1024**4 or not 0 < cpus <= 1024 or not 16 <= pids <= 65536:
            raise ExecutionError("invalid container resource limits")
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", image.id):
            raise ExecutionError("workspace image must resolve to a content identity")
        self.client, self.image, self.attempt, self.workdir = client, image, attempt, workdir
        self.limits = {"memory_bytes": memory_bytes, "cpus": cpus, "pids": pids}
        self.container = None
        self.active = threading.Lock()
        self.cleanup_status = "not_created"
        self.image_identity = {"image_id": image.id, "repository_digests": image.attrs.get("RepoDigests", []),
                               "architecture": image.attrs.get("Architecture"), "os": image.attrs.get("Os")}

    def __enter__(self):
        if self.container is not None:
            raise ExecutionError("workspace has already been created")
        self.container = self.client.containers.create(
            image=self.image.id, name=self.attempt, command=["/bin/sh", "-c", "exec tail -f /dev/null"],
            detach=True, working_dir=self.workdir,
            labels={"asb.owner": "asb", "asb.attempt": self.attempt},
            mem_limit=self.limits["memory_bytes"], nano_cpus=int(self.limits["cpus"] * 10**9),
            pids_limit=self.limits["pids"],
        )
        self.cleanup_status = "pending"
        try:
            self.container.start()
            return self
        except BaseException:
            self.close()
            raise

    def execute(self, argv, environment=None, timeout=900, output_limit=256 * 1024, before_resume=None,
                on_output=None, cancelled=None, gate="shell", preparation_timeout=None):
        if self.container is None:
            raise ExecutionError("workspace is not running")
        if not self.active.acquire(blocking=False):
            raise ExecutionError("one command is already running in this task workspace")
        try:
            return self._execute(argv, environment or {}, timeout, output_limit, before_resume, on_output, cancelled, gate, preparation_timeout)
        finally:
            self.active.release()

    def _execute(self, argv, environment, timeout, output_limit, before_resume, on_output, cancelled, gate, preparation_timeout):
        if (not isinstance(argv, list) or not argv or len(argv) > 10000
                or any(not isinstance(value, str) or "\x00" in value for value in argv)
                or not isinstance(environment, dict)
                or any(not isinstance(key, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key)
                       or not isinstance(value, str) or "\x00" in value for key, value in environment.items())):
            raise ExecutionError("invalid execution arguments or environment")
        if sum(len(value.encode()) for value in argv) + sum(len(key) + len(value.encode()) for key, value in environment.items()) > MAX_ARGUMENT_BYTES:
            raise ExecutionError("execution arguments and environment exceed 1 MiB")
        if not 0 < timeout <= 86400 or not 1 <= output_limit <= 16 * 1024**2:
            raise ExecutionError("invalid execution timeout or output limit")
        preparation_timeout = min(timeout, 60) if preparation_timeout is None else preparation_timeout
        if not 0 < preparation_timeout <= 300:
            raise ExecutionError("invalid execution preparation timeout")
        if cancelled is not None and cancelled.is_set():
            raise ExecutionError("execution was cancelled before starting")
        if gate not in ["shell", "application"]:
            raise ExecutionError("unknown execution gate protocol")
        container, api = self.container, self.client.api
        done, expired, interrupted = threading.Event(), threading.Event(), threading.Event()
        reset_needed = threading.Event()
        watchdog_error = []
        started = time.monotonic()
        execution_started = None
        phase_deadline, phase_lock = [started + preparation_timeout], threading.Lock()

        def deadline():
            while not done.wait(min(0.05, timeout)):
                with phase_lock:
                    if cancelled is not None and cancelled.is_set():
                        interrupted.set()
                        break
                    if time.monotonic() >= phase_deadline[0]:
                        expired.set()
                        break
            else:
                return
            reset_needed.set()
            try:
                # Killing this owned container terminates the entire tree,
                # including daemonized helpers, without host-wide signals.
                container.kill()
            except Exception as error:
                watchdog_error.append(type(error).__name__)

        watchdog = threading.Thread(target=deadline, daemon=True)
        watchdog.start()
        stream = None
        result = None
        try:
            command = argv if gate == "application" else ["/bin/sh", "-c", GATE, "asb-gate", *argv]
            created = api.exec_create(container.id, cmd=command,
                                      environment=environment, workdir=self.workdir, stdout=True, stderr=True)
            exec_id = created["Id"]
            stream = api.exec_start(exec_id, stream=True, demux=True)
            ready = bytearray()
            for out, err in stream:
                if err:
                    raise ExecutionError("container execution gate failed before the agent started")
                ready.extend(out or b"")
                if len(ready) > 64:
                    raise ExecutionError("container execution gate returned an invalid readiness frame")
                if ready.endswith(b"\n"):
                    break
            match = re.fullmatch(rb"__ASB_GATE__:(\d+)\n", ready)
            if match is None or expired.is_set() or interrupted.is_set():
                raise ExecutionError("container execution gate did not become ready before its deadline")
            namespace_pid = int(match.group(1))
            if namespace_pid <= 1:
                raise ExecutionError("container gate did not identify a distinct agent process")
            # The readiness bytes precede SIGSTOP by a few instructions. Do not
            # attach or resume until the process is actually stopped.
            for _ in range(100):
                stopped = container.exec_run(["/bin/sh", "-c", STOPPED, "asb-status", str(namespace_pid)])
                if stopped.exit_code == 0:
                    break
                if expired.is_set() or interrupted.is_set():
                    raise ExecutionError("container gate expired while waiting for SIGSTOP")
                time.sleep(0.01)
            else:
                raise ExecutionError("container gate never reached its stopped state")
            inspection = api.exec_inspect(exec_id)
            daemon_pid = inspection.get("Pid")
            if not inspection.get("Running") or not isinstance(daemon_pid, int) or daemon_pid <= 1:
                raise ExecutionError("Docker did not identify the running execution target")
            target = {"container_id": container.id, "exec_id": exec_id,
                      "daemon_pid": daemon_pid, "namespace_pid": namespace_pid,
                      "pid_scope": "docker-daemon-host"}
            if before_resume is not None:
                before_resume(target)
            with phase_lock:
                if (expired.is_set() or interrupted.is_set() or time.monotonic() >= phase_deadline[0]
                        or (cancelled is not None and cancelled.is_set())):
                    raise ExecutionError("container execution preparation was cancelled or exceeded its deadline")
                execution_started = time.monotonic()
                phase_deadline[0] = execution_started + timeout
            # Minimal official task images may have only the POSIX shell's
            # kill builtin, not a standalone /bin/kill executable.
            resumed = container.exec_run(["/bin/sh", "-c", 'kill -CONT "$1"', "asb-resume", str(namespace_pid)])
            if resumed.exit_code != 0:
                raise ExecutionError("container execution gate could not be resumed")
            stdout, stderr = bytearray(), bytearray()
            truncated = False
            for out, err in stream:
                for name, destination, chunk in [("stdout", stdout, out), ("stderr", stderr, err)]:
                    if chunk:
                        if on_output is not None:
                            for start in range(0, len(chunk), 32 * 1024):
                                on_output(name, chunk[start:start + 32 * 1024])
                        room = max(0, output_limit - len(destination))
                        truncated |= len(chunk) > room
                        destination.extend(chunk[:room])
            inspection = api.exec_inspect(exec_id)
            status = inspection.get("ExitCode")
            if not isinstance(status, int) or inspection.get("Running"):
                raise ExecutionError("Docker returned no final execution status")
            result = {"exit_code": status, "stdout": bytes(stdout), "stderr": bytes(stderr),
                      "timed_out": expired.is_set(), "cancelled": interrupted.is_set(), "output_truncated": truncated,
                      "elapsed_seconds": time.monotonic() - execution_started,
                      "preparation_seconds": execution_started - started, "target": target}
        except BaseException:
            # A failed collector callback, malformed gate or protocol failure
            # must not leave a paused/live agent behind. Preserve its filesystem
            # by restarting only PID 1 after the complete old tree is gone.
            reset_needed.set()
            try:
                container.kill()
            except Exception:
                pass
            raise
        finally:
            done.set()
            watchdog.join(timeout=125)
            try:
                if stream is not None:
                    try:
                        stream.close()
                    finally:
                        # Docker's CancellableStream shuts down the raw socket
                        # but does not close its HTTP response/file wrappers.
                        # Release both, including partially consumed gates.
                        response = getattr(stream, "_response", None)
                        if response is not None:
                            response.close()
            finally:
                if reset_needed.is_set() and not watchdog.is_alive() and not watchdog_error:
                    container.wait(timeout=125)
                    container.start()
        if watchdog.is_alive() or watchdog_error:
            raise ExecutionError("Docker could not stop the timed-out execution tree")
        return result

    def quiesce_unconfirmed_agent(self):
        """Stop every process in this owned PID namespace, retaining its files.

        A failed/old guest supervisor cannot publish a trustworthy completion
        acknowledgement. Container exit must be observed before restarting its
        original entry point and considering any partial submission for grading.
        """
        if not self.active.acquire(blocking=False):
            raise ExecutionError("cannot stop an unconfirmed agent during another task operation")
        try:
            container = self.container
            if container is None:
                raise ExecutionError("unconfirmed agent workspace is no longer available")
            container.reload()
            labels = container.attrs.get("Config", {}).get("Labels", {}) or {}
            if (labels.get("asb.owner") != "asb" or labels.get("asb.attempt") != self.attempt
                    or container.attrs.get("HostConfig", {}).get("PidMode") not in [None, "", "private"]):
                raise ExecutionError("refused process-tree reset outside this owned PID namespace")
            if container.attrs.get("State", {}).get("Running", True):
                container.kill()
            container.wait(timeout=125)
            container.start()
        except ExecutionError:
            raise
        except Exception as error:
            raise ExecutionError("could not confirm stopped agent tree before submission capture") from error
        finally:
            self.active.release()

    def close(self):
        if self.container is None:
            return
        labels = self.container.attrs.get("Config", {}).get("Labels", {}) or {}
        if labels.get("asb.owner") != "asb" or labels.get("asb.attempt") != self.attempt:
            self.cleanup_status = "ownership_mismatch"
            raise ExecutionError("refused cleanup of a container outside this attempt")
        try:
            self.container.remove(force=True)
            self.cleanup_status = "removed"
            self.container = None
        except Exception:
            self.cleanup_status = "error"
            raise

    def __exit__(self, *_exception):
        self.close()
