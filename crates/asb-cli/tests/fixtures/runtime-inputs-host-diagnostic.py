#!/usr/bin/env python3
"""Temporary provider-free hosted diagnostics; never used by ASB execution."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import time
import traceback


CRATE, ASB, ROOT = [Path(value) for value in sys.argv[1:]]
ASSETS = CRATE / "assets/benchmarks"
DEADLINE = time.monotonic() + 90
OPERATION = "initialization"
SEQUENCE = 0
RETAINED = 0


def record(value):
    # Only fixed fixture labels, categorical errors and public binary metadata.
    descriptor = os.open(ROOT / "summary.jsonl", os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        output.write(json.dumps(value, sort_keys=True) + "\n")
    print(json.dumps(value, sort_keys=True), flush=True)


def load(name):
    spec = importlib.util.spec_from_file_location(name, ASSETS / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


load("asb_execution")
runtime = load("asb_runtime")
subject = load("asb_runtime_inputs")
original_probe = runtime.bounded_probe
original_libraries = runtime.library_paths


def capture_libraries(text):
    global SEQUENCE, RETAINED
    SEQUENCE += 1
    data = text.encode("utf-8")
    assert len(data) <= 1024 * 1024
    if SEQUENCE > 256 or RETAINED + len(data) > 16 * 1024 * 1024:
        raise RuntimeError("private diagnostic retention bound reached")
    RETAINED += len(data)
    # Exact loader output, from hardcoded benign executables and a clean
    # environment, remains under the private fixture; never print environment.
    path = ROOT / f"loader-{SEQUENCE:03}.txt"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        output.write(data)
    record({"operation": OPERATION, "stage": "loader-output", "file": path.name,
            "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    return original_libraries(text)


def probe(arguments, *, environment=None, deadline=None):
    # Production bounded IO, output caps and pre-established child custody.
    return original_probe(arguments, environment=environment,
                          deadline=min(DEADLINE, deadline or DEADLINE))


runtime.library_paths = capture_libraries
runtime.bounded_probe = probe


def inspect(label, action):
    global OPERATION
    OPERATION = label
    if time.monotonic() >= DEADLINE:
        record({"operation": label, "result": "not-admitted", "reason": "shared-deadline"})
        return
    try:
        action()
    except Exception as error:
        descriptor = os.open(ROOT / f"{label}.traceback.txt",
                             os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            output.write(traceback.format_exc(limit=32)[:65536])
        # The public row deliberately omits arbitrary exception text/path/env.
        record({"operation": label, "result": "error", "class": type(error).__name__})
    else:
        record({"operation": label, "result": "ok"})


def discovery():
    value = subject.Discovery()
    value.deadline = DEADLINE
    return value


script = ROOT / "fixture-agent"
script.write_text("#!/bin/sh\nexit 97\n", encoding="utf-8")
script.chmod(0o700)
applet = ROOT / "unshare"
shutil.copyfile("/bin/true", applet)
applet.chmod(0o700)
def image_digest(path):
    digest = hashlib.sha256()
    total = 0
    with path.open("rb") as source:
        if os.fstat(source.fileno()).st_size > 256 * 1024 * 1024:
            raise ValueError("diagnostic ASB image exceeds its byte bound")
        while chunk := source.read(65536):
            total += len(chunk)
            if total > 256 * 1024 * 1024:
                raise ValueError("diagnostic ASB image grew beyond its byte bound")
            digest.update(chunk)
    return digest.hexdigest()


record({"stage": "image", "python": sys.version.split()[0],
        "asb_sha256": image_digest(ASB)})

inspect("copied-true-program", lambda: discovery().program(applet))
inspect("shell-program", lambda: discovery().program("/bin/sh"))
inspect("python-program", lambda: discovery().program("python3"))
inspect("python-no-site", lambda: discovery().python("python3", False))
inspect("shebang-program", lambda: discovery().program(script))
inspect("missing-file", lambda: discovery().file(ROOT / "missing-configuration"))
inspect("current-python-process", lambda: discovery().process(os.getpid()))
inspect("built-asb-program", lambda: discovery().program(ASB))

# Run original definitions and preserve their categorical boundary, not main's
# catch/redaction wrapper. No coding client, provider or workload is executed.
inspect("combined-python-process", lambda: subject.discover([
    {"kind": "process", "pid": os.getpid()},
    {"kind": "python", "path": "python3", "site": False},
    {"kind": "program", "path": str(script), "search": None},
    {"kind": "file", "path": str(ROOT / "missing-configuration")},
]))
record({"stage": "complete", "fixture": ROOT.name,
        "scope": "Python-process-and-built-ASB-file-not-a-running-Rust-libtest"})
