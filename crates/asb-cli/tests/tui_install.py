"""Only owned local fixtures. No network, package manager, agent or provider."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import termios
import time


INSTALLER = r'''#!/usr/bin/python3
import json, os, signal, sys, time
from pathlib import Path
root = Path.cwd()
descriptor = int(os.environ["ASB_INSTALL_PROGRESS_FD"])
try:
    target = os.readlink(f"/proc/self/fd/{descriptor}")
except FileNotFoundError:
    target = "closed"
assert not target.startswith("socket:"), "installer inherited private progress descriptor"
mode = (root / "mode").read_text()
if mode == "fail":
    print("fixture failure with secret credential-looking-value", file=sys.stderr)
    sys.exit(7)
if mode == "success":
    destination = Path(sys.argv[sys.argv.index("--prefix") + 1]) / "node_modules/.bin/pi"
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text("#!/bin/sh\nexit 99\n")
    destination.chmod(0o755)
    sys.exit(0)
(root / "principal.pid").write_text(str(os.getpid()))
child = os.fork()
if child == 0:
    os.setsid()
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    (root / "child.pid").write_text(str(os.getpid()))
    grandchild = os.fork()
    if grandchild == 0:
        (root / "grandchild.pid").write_text(str(os.getpid()))
        with (root / "writes").open("ab", buffering=0) as output:
            while True:
                output.write(b"x")
                time.sleep(.01)
    os._exit(0)  # double fork: only the gate can retain this descendant
if mode == "helper-loss":
    while not (root / "grandchild.pid").exists():
        time.sleep(.005)
    os.kill(os.getppid(), signal.SIGKILL)
while True:
    time.sleep(1)
'''


def wait_until(predicate, description, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.01)
    raise AssertionError(description)


def project(parent, name, mode):
    root = parent / name
    for directory in [".asb", "home", "workloads", "fixture-bin"]:
        (root / directory).mkdir(parents=True)
    (root / ".asb/project.toml").write_text("tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n")
    (root / "workloads/bug-fix.txt").write_text("Never run.\n")
    (root / "mode").write_text(mode)
    npm = root / "fixture-bin/npm"
    npm.write_text(INSTALLER)
    npm.chmod(0o755)
    node = root / "fixture-bin/node"
    node.write_text("#!/bin/sh\nprintf 'v22.0.0\\n'\n")
    node.chmod(0o755)
    return root


def environment(root):
    result = {"PATH": str(root / "fixture-bin"), "HOME": str(root / "home"),
              "TERM": "xterm-256color", "NO_COLOR": "1"}
    if "LLVM_PROFILE_FILE" in os.environ:
        result["LLVM_PROFILE_FILE"] = os.environ["LLVM_PROFILE_FILE"]
    return result


def tui(binary, root, action):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    # A real PTY owns input/raw mode while the actual backend renders to a pipe.
    # Closing only its reader causes EPIPE, retaining the PTY for restoration.
    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    process = subprocess.Popen([binary, "setup", "tui"], cwd=root,
        env=environment(root), stdin=slave, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, preexec_fn=controlling_terminal)
    transcript = bytearray()
    output_open = True
    started = False
    triggered = False
    deadline = time.monotonic() + 15
    try:
        while time.monotonic() < deadline and process.poll() is None:
            if output_open and select.select([process.stdout], [], [], .02)[0]:
                chunk = os.read(process.stdout.fileno(), 65536)
                transcript.extend(chunk)
                if not started and b"Space" in transcript:
                    if action == "replacement":
                        replacement = root / "replacement"
                        replacement.write_text("#!/bin/sh\nexit 77\n")
                        replacement.chmod(0o755)
                        os.replace(replacement, binary)
                    os.write(master, b"pi ")
                    started = True
            else:
                time.sleep(.01)
            ready = (root / "grandchild.pid").is_file() and (root / "writes").exists()
            if action in ["epipe", "cancel", "escape", "owner-loss"] and ready and not triggered:
                triggered = True
                if action == "epipe":
                    process.stdout.close()
                    output_open = False
                elif action == "owner-loss":
                    process.kill()  # exactly our owned ASB; its guardian survives
                else:
                    os.write(master, b"\x03" if action == "cancel" else b"\x1b")
            if action in ["success", "fail", "replacement", "helper-loss"] and not triggered:
                expected = b"Cannot install" if action in ["fail", "helper-loss"] else b"status is ready"
                if expected in transcript:
                    triggered = True
                    os.write(master, b"\x03")
        assert process.poll() is not None, (action, "TUI exceeded deadline", transcript[-2000:])
        assert started and triggered, (action, transcript[-2000:], process.stderr.read())
        diagnostic = process.stderr.read()
        if action != "owner-loss":
            flags = termios.tcgetattr(slave)[3]
            assert flags & termios.ICANON and flags & termios.ECHO, (action, "terminal not restored")
        if action == "epipe":
            assert b"cannot draw setup install status" in diagnostic, diagnostic
            assert b"cannot restore terminal" in diagnostic, diagnostic
        assert b"credential-looking-value" not in diagnostic + transcript
        if action in ["success", "replacement"]:
            assert (root / ".asb-tools/pi/node_modules/.bin/pi").is_file()
        if action == "fail":
            assert b"credential-looking-value" in (root / ".asb/install.log").read_bytes()
        if action == "helper-loss":
            assert b"missing final status" in transcript, transcript[-2000:]
        assert not list(root.glob("asb-sweep-*.toml")), "cancellation must not publish selection"
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=10)
        os.close(master)
        os.close(slave)


def assert_quiescent(root):
    pids = [int((root / name).read_text()) for name in ["principal.pid", "child.pid", "grandchild.pid"]]
    wait_until(lambda: all(not Path(f"/proc/{pid}").exists() for pid in pids),
               f"owned installer descendants survived: {pids}")
    before = (root / "writes").read_bytes()
    time.sleep(.2)
    assert (root / "writes").read_bytes() == before, "writes continued after confirmed exit"


def helper_protocol(binary, root):
    receiver, sender = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
    env = environment(root)
    env["ASB_INSTALL_PROGRESS_FD"] = str(sender.fileno())
    arguments = [binary, "__asb-install", "ASBINSTALL1", "tool", str(root),
                 str(root / ".asb-tools"), "pi"]
    child = subprocess.Popen(arguments, cwd=root, env=env, pass_fds=(sender.fileno(),),
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    sender.close()
    receiver.settimeout(5)
    final = None
    while final is None:
        frame = receiver.recv(9)
        assert len(frame) == 8 and frame[:4] == b"ASI1" and frame[6:] == b"\0\0", frame
        if frame[4] != 0:
            final = frame[4]
    stdout, stderr = child.communicate(timeout=5)
    assert final == 1 and child.returncode == 0 and stdout == stderr == b""
    receiver.close()
    invalid = subprocess.run([binary, "__asb-install", "old"], cwd=root,
                             env=environment(root), capture_output=True, timeout=5)
    assert invalid.returncode != 0


def startup_failure(binary, root):
    master, slave = pty.openpty()
    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    try:
        with open("/dev/full", "wb") as full:
            process = subprocess.Popen([binary, "setup", "tui"], cwd=root,
                env=environment(root), stdin=slave, stdout=full,
                stderr=subprocess.PIPE, preexec_fn=controlling_terminal)
            _, diagnostic = process.communicate(timeout=5)
        assert process.returncode != 0 and b"cannot start setup TUI" in diagnostic, diagnostic
        assert b"cannot restore terminal" in diagnostic, diagnostic
        flags = termios.tcgetattr(slave)[3]
        assert flags & termios.ICANON and flags & termios.ECHO, "partial startup retained raw mode"
        assert not (root / "principal.pid").exists()
    finally:
        os.close(master)
        os.close(slave)


def main():
    binary = os.path.abspath(sys.argv[1])
    with tempfile.TemporaryDirectory(prefix="asb-tui-install-") as temporary:
        parent = Path(temporary)
        sentinel = subprocess.Popen(["/usr/bin/sleep", "60"])
        try:
            for action in ["success", "fail", "epipe", "cancel", "escape", "owner-loss", "helper-loss"]:
                root = project(parent, action, action if action in ["success", "fail", "helper-loss"] else "slow")
                tui(binary, root, action)
                if action not in ["success", "fail"]:
                    assert_quiescent(root)
                assert sentinel.poll() is None, "unrelated process was signalled"
            helper_protocol(binary, project(parent, "helper", "success"))
            startup_failure(binary, project(parent, "startup", "slow"))
            root = project(parent, "replacement", "success")
            copied = root / "asb"
            shutil.copyfile(binary, copied)
            copied.chmod(0o755)
            tui(str(copied), root, "replacement")
            root = project(parent, "no-terminal", "slow")
            result = subprocess.run([binary, "setup", "tui"], cwd=root, env=environment(root),
                                    stdin=subprocess.DEVNULL, capture_output=True, timeout=5)
            assert result.returncode != 0 and not (root / "principal.pid").exists()
        finally:
            sentinel.terminate()
            sentinel.wait(timeout=5)


if __name__ == "__main__":
    main()
