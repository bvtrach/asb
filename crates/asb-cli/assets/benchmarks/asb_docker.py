"""Scoped Docker SDK transport through the existing Docker CLI.

The SDK receives a private socketpair connected to `docker system dial-stdio`.
No host listener, daemon configuration, socket chmod, or group change is used.
If Docker already needs passwordless sudo, only the installed Docker binary
is elevated; the benchmark controller and dataset code remain unprivileged.
"""

import os
import shutil
import signal
import socket
import subprocess
import threading
import time
from urllib.parse import urlsplit


class DockerAccessError(RuntimeError):
    pass


def docker_command():
    executable = shutil.which("docker")
    if not executable:
        raise DockerAccessError("Docker CLI is missing; install Docker Engine and retry")
    environment = os.environ.copy()
    # This is transport access, not an outbound provider request.
    def probe(command):
        try:
            return subprocess.run(command + ["version", "--format", "{{.Server.Version}}"],
                                  stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                  stderr=subprocess.DEVNULL, timeout=15, env=environment).returncode == 0
        except (OSError, subprocess.TimeoutExpired):
            return False
    direct = [executable]
    if probe(direct):
        return direct
    # Match the user's actual selected endpoint, not root's default context.
    try:
        endpoint = environment.get("DOCKER_HOST")
        if not endpoint:
            endpoint = subprocess.check_output(
                [executable, "context", "inspect", "--format", "{{.Endpoints.docker.Host}}"],
                stdin=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15,
                env=environment, text=True,
            ).strip()
    except (OSError, subprocess.SubprocessError):
        endpoint = None
    sudo = shutil.which("sudo")
    if sudo and endpoint and endpoint.startswith("unix://") and len(endpoint) <= 4096:
        elevated = [sudo, "-n", "--", executable, "--host", endpoint]
        if probe(elevated):
            return elevated
    raise DockerAccessError(
        "Docker daemon is unavailable or permission was denied, and scoped passwordless "
        "Docker access is unavailable; run `docker info` and repair access before retrying"
    )


class StdioConnections:
    """Own only the transport children created by this controller."""
    def __init__(self, command):
        self.command = list(command)
        self.children = []
        self.lock = threading.Lock()
        self.closed = False
        self._closing = False
        self._close_done = False
        self._close_error = None

    def connect(self, timeout):
        local, remote = socket.socketpair()
        local.settimeout(timeout)
        try:
            with self.lock:
                if self.closed:
                    raise DockerAccessError("Docker transport has already been closed")
                # Reap completed reconnects eagerly instead of retaining an
                # unbounded history of Popen owners until session exit.
                self.children = [child for child in self.children if child.poll() is None]
                process = subprocess.Popen(
                    self.command, stdin=remote, stdout=remote, stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )
                self.children.append(process)
            return local
        except BaseException:
            local.close()
            raise
        finally:
            remote.close()

    def close(self):
        with self.lock:
            if self._closing:
                raise DockerAccessError("Docker transport cleanup is already in progress; settlement is unconfirmed")
            if self._close_done:
                if self._close_error is not None:
                    raise DockerAccessError(self._close_error)
                return
            self.closed = True
            self._closing = True
            # Exactly one closer owns poll/reap/signal rights. Other callers
            # must not observe the detached empty list as settled, nor race a
            # cached PGID signal against this owner's actual child reap.
            children, self.children = self.children, []
        # One allowance for the entire population, not two seconds per child.
        # Python owns direct CLI children; the outer ManagedGate remains actual
        # descendant owner if signaling/observation is uncertain at this bound.
        started = time.monotonic()
        deadline = started + 3
        remaining = children
        term, force = set(), set()
        failed, interruption = False, None
        try:
            while remaining and time.monotonic() < deadline:
                for process in list(remaining):
                    if time.monotonic() >= deadline:
                        break
                    try:
                        if process.poll() is not None:
                            remaining.remove(process)
                            continue
                    except InterruptedError:
                        continue
                    except OSError:
                        failed = True
                        # An uncertain wait observation cannot authorize a
                        # cached numeric process-group signal.
                        continue
                    except BaseException as caught:
                        if interruption is None:
                            interruption = caught
                        continue
                    pending = force if time.monotonic()-started >= 2 else term
                    if process in pending:
                        continue
                    pending.add(process)
                    try:
                        # poll just confirmed the unreaped original leader;
                        # never signal this group after reaping its leader.
                        os.killpg(process.pid, signal.SIGKILL if pending is force else signal.SIGTERM)
                    except ProcessLookupError:
                        pass  # Only a subsequent actual reap proves exit.
                    except OSError:
                        failed = True  # Continue all independent siblings.
                    except BaseException as caught:
                        if interruption is None:
                            interruption = caught
                if remaining:
                    time.sleep(min(.01, max(0, deadline-time.monotonic())))
        except BaseException as caught:
            interruption = caught
        finally:
            # Retain exact pending owners; no Drop retry, unconditional wait,
            # per-child worker or new admission after unknown cleanup.
            with self.lock:
                self.children = remaining
                self._closing = False
                self._close_done = True
                self._close_error = (
                    "Docker transport exit is unconfirmed; owned controller supervision retains custody"
                    if remaining else
                    "Docker transport cleanup encountered an error; all owned transport children were reaped"
                    if failed or interruption is not None else None)
        if interruption is not None:
            raise interruption
        if remaining:
            raise DockerAccessError("Docker transport exit is unconfirmed; owned controller supervision retains custody")
        if failed:
            raise DockerAccessError("Docker transport cleanup encountered an error; all owned transport children were reaped")


class ClosingResponseStream:
    """Own an HTTP response even when its consumer stops before the first read.

    Docker SDK's default raw stream disables socket timeouts and does not close
    the response on early generator close. Archive inspection needs both the
    configured read timeout and deterministic release of its owned connection.
    """
    def __init__(self, response, chunk_size, decode):
        self.response = response
        self.closed = False
        try:
            self.iterator = iter(response.iter_content(chunk_size, decode))
        except BaseException:
            self.closed = True
            response.close()
            raise

    def __iter__(self):
        return self

    def __next__(self):
        if self.closed:
            raise StopIteration
        try:
            return next(self.iterator)
        except BaseException:
            self.close()
            raise

    def close(self):
        if not self.closed:
            self.closed = True
            try:
                close = getattr(self.iterator, "close", None)
                if close is not None:
                    close()
            finally:
                self.response.close()


class ArchiveStreamPolicy:
    """Change archive reads only; quiet TTY/attach streams retain SDK semantics."""
    def _stream_raw_result(self, response, chunk_size=1, decode=True):
        path = urlsplit(response.url).path
        if not ("/containers/" in path and path.endswith("/archive")):
            return super()._stream_raw_result(response, chunk_size, decode)
        try:
            self._raise_for_status(response)
        except BaseException:
            response.close()
            raise
        return ClosingResponseStream(response, chunk_size, decode)


class DockerSession:
    def __init__(self):
        self.connections = None
        self.client = None

    def __enter__(self):
        import docker
        from docker.transport.unixconn import UnixHTTPAdapter, UnixHTTPConnection, UnixHTTPConnectionPool

        connections = StdioConnections(docker_command() + ["system", "dial-stdio"])
        self.connections = connections

        class BoundedAPIClient(ArchiveStreamPolicy, docker.APIClient):
            pass

        class BoundedDockerClient(docker.DockerClient):
            def __init__(self, *args, **kwargs):
                self.api = BoundedAPIClient(*args, **kwargs)

        class Connection(UnixHTTPConnection):
            def connect(self):
                self.sock = connections.connect(self.timeout)

        class Pool(UnixHTTPConnectionPool):
            def _new_conn(self):
                return Connection(self.base_url, "unused", self.timeout)

        class Adapter(UnixHTTPAdapter):
            def get_connection(self, url, proxies=None):
                with self.pools.lock:
                    pool = self.pools.get(url)
                    if pool is None:
                        pool = Pool(url, "unused", self.timeout, maxsize=self.max_pool_size)
                        self.pools[url] = pool
                    return pool

        try:
            # Explicit version suppresses constructor-time I/O before mounting
            # the transport. The unversioned /version request then negotiates
            # the actual daemon version, including newer minimum API versions.
            self.client = BoundedDockerClient(base_url="unix:///unused", version="1.41", timeout=120)
            self.client.api.mount("http+docker://", Adapter("http+unix:///unused", timeout=120))
            version = self.client.version()["ApiVersion"]
            self.client.api._version = version
            self.client.ping()
            return self.client
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def __exit__(self, *_exception):
        client,self.client = self.client,None
        connections,self.connections = self.connections,None
        try:
            if client is not None:
                client.close()
        finally:
            if connections is not None:
                connections.close()
