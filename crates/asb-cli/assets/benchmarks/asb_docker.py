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

    def connect(self, timeout):
        local, remote = socket.socketpair()
        local.settimeout(timeout)
        try:
            with self.lock:
                if self.closed:
                    raise DockerAccessError("Docker transport has already been closed")
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
            self.closed = True
            children, self.children = self.children, []
        for process in children:
            if process.poll() is None:
                # No task/container is killed here: these are only our own
                # short-lived CLI connections, never a global Docker process.
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait()
                except ProcessLookupError:
                    process.wait()


class DockerSession:
    def __init__(self):
        self.connections = None
        self.client = None

    def __enter__(self):
        import docker
        from docker.transport.unixconn import UnixHTTPAdapter, UnixHTTPConnection, UnixHTTPConnectionPool

        connections = StdioConnections(docker_command() + ["system", "dial-stdio"])
        self.connections = connections

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
            self.client = docker.DockerClient(base_url="unix:///unused", version="1.41", timeout=120)
            self.client.api.mount("http+docker://", Adapter("http+unix:///unused", timeout=120))
            version = self.client.version()["ApiVersion"]
            self.client.api._version = version
            self.client.ping()
            return self.client
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def __exit__(self, *_exception):
        if self.client is not None:
            self.client.close()
            self.client = None
        if self.connections is not None:
            self.connections.close()
            self.connections = None
