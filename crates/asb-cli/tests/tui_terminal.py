"""Real bounded PTY fixture. No live provider, tool installation or agent run."""
import errno
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import sys
import tempfile
import termios
import time
import tomllib


def session(binary, root, arguments, keys, path="/usr/bin:/bin"):
    pid, master = pty.fork()
    if pid == 0:
        os.chdir(root)
        environment = {"PATH": path, "HOME": str(root / "home"),
                       "TERM": "xterm-256color", "NO_COLOR": "1"}
        if "LLVM_PROFILE_FILE" in os.environ:
            environment["LLVM_PROFILE_FILE"] = os.environ["LLVM_PROFILE_FILE"]
        os.execve(binary, [binary, *arguments], environment)
    transcript = bytearray()
    deadline = time.monotonic() + 15
    sent = False
    status = None
    try:
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
        while time.monotonic() < deadline:
            if select.select([master], [], [], .05)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    chunk = b""
                transcript.extend(chunk)
                if not sent and b"ASB" in transcript and b"Space" in transcript:
                    os.write(master, keys)
                    sent = True
            found, value = os.waitpid(pid, os.WNOHANG)
            if found:
                status = value
                break
        assert status is not None, f"TUI exceeded bounded deadline: {bytes(transcript[-3000:])!r}"
        assert sent, f"TUI did not draw its landing screen: {bytes(transcript)!r}"
        assert b"\x1b[?1049h" in transcript, "must enter alternate screen"
        assert b"\x1b[?1049l" in transcript, "must restore alternate screen"
        return os.waitstatus_to_exitcode(status), bytes(transcript)
    finally:
        if status is None:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        os.close(master)


def main():
    binary = str(Path(sys.argv[1]).resolve())
    target = Path(__file__).resolve().parents[3] / "target"
    target.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tui-pty-", dir=target) as temporary:
        root = Path(temporary)
        for directory in [".asb", "workloads/custom/source", "home", ".asb-tools/pi/node_modules/.bin"]:
            (root / directory).mkdir(parents=True, exist_ok=True)
        (root / ".asb/project.toml").write_text("tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n")
        for workload in ["bug-fix", "custom"]:
            (root / f"workloads/{workload}.txt").write_text("Offline task, never executed.\n")
        agent = root / ".asb-tools/pi/node_modules/.bin/pi"
        agent.write_text("#!/bin/sh\nprintf unexpected > invoked-agent\nexit 99\n")
        agent.chmod(0o755)

        # Real search/select/step/concurrency events, including backwards nav.
        code, transcript = session(binary, root, ["setup", "tui"],
            b"pi \t\x1b[Z\tlocal \t \tbug-fix \t\t\x1b[B \t")
        assert code == 0, transcript
        assert b"Generated 1 benchmark configurations" in transcript, transcript
        sweep_files = list(root.glob("asb-sweep-*.toml"))
        assert len(sweep_files) == 1
        manifest = tomllib.loads(sweep_files[0].read_text())
        assert len(manifest["configs"]) == 1
        config = tomllib.loads((root / manifest["configs"][0]).read_text())
        assert config["agent"] == "pi" and config["provider"] == "local"
        assert config["workload"] == "bug-fix" and config["concurrency"] == 2
        assert not (root / "invoked-agent").exists()

        # Both cancellation encodings exercise restoration and bounded exit.
        for arguments in [["setup", "tui"], ["workload", "tui"]]:
            for key in [b"\x03", b"\x1b"]:
                code, transcript = session(binary, root, arguments, key)
                assert code != 0, transcript
                assert b"cancelled" in transcript, transcript
        assert not (root / "invoked-agent").exists()

        # Trigger actual background install/error paths. Every external fetch
        # or package manager is an inert fixture and cannot forward requests.
        fixture_bin = root / "fixture-bin"
        fixture_bin.mkdir()
        invoked = root / "fixture-installer-invoked"
        for name in ["curl", "npm", "node", "sudo", "git", "make", "tar"]:
            tool = fixture_bin / name
            tool.write_text(f"#!/bin/sh\nprintf '{name}\\n' >> '{invoked}'\nexit 7\n")
            tool.chmod(0o755)
        agent.unlink()
        code, transcript = session(binary, root, ["setup", "tui"], b"pi \x03", str(fixture_bin))
        assert code != 0, transcript
        assert invoked.is_file(), "missing agent selection must attempt installation"
        assert not agent.exists(), "failed package install must not mark agent installed"
        assert len(list(root.glob("asb-sweep-*.toml"))) == 1, "failed selection must not publish a sweep"

        (root / "workloads/custom.txt").unlink()
        code, transcript = session(binary, root, ["workload", "tui"],
                                   b"custom \x03", str(fixture_bin))
        assert code != 0, transcript
        assert not (root / "workloads/custom.txt").exists(), "failed source install must not fabricate workload"
        assert not (root / "invoked-agent").exists()

        # A configured/discovered provider cannot be installed, and selecting
        # an installed collector must still prepare its missing dependency.
        agent.write_text("#!/bin/sh\nexit 99\n")
        agent.chmod(0o755)
        code, transcript = session(binary, root, ["setup", "tui"],
            b"pi \t\x1b[105;5u\x03", str(fixture_bin))
        assert code != 0, transcript
        flamegraph = root / ".asb-tools/flamegraph/bin/flamegraph"
        flamegraph.parent.mkdir(parents=True)
        flamegraph.write_text("#!/bin/sh\nexit 99\n")
        flamegraph.chmod(0o755)
        code, transcript = session(binary, root, ["setup", "tui"],
            b"pi \tlocal \t \tbug-fix \tflamegraph \x03", str(fixture_bin))
        assert code != 0, transcript
        assert not (root / ".asb-tools/perf/bin/perf").exists(), "failed dependency must not be marked installed"
        assert len(list(root.glob("asb-sweep-*.toml"))) == 1


if __name__ == "__main__":
    main()
