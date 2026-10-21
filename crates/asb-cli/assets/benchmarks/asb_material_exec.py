"""Bounded private material-helper transport, not guest lifetime ownership.

Only this invocation's Docker CLI child is stopped here. A failed observation
must escape to the owning task's stopped-preservation path: terminating Docker
exec's local transport alone does not prove its guest process has exited.
"""
import json
import subprocess
import threading
import time

from asb_material import MaterialError


def run(command, payload=b"", *, deadline, cancelled=None, output_limit=4096):
    if (not isinstance(payload, bytes) or len(payload) > 1024**2
            or not 0 < output_limit <= 65536):
        raise MaterialError("material helper transport exceeds its input/output bounds")
    if time.monotonic() >= deadline or (cancelled is not None and cancelled.is_set()):
        raise MaterialError("material helper observation was cancelled or timed out")
    try:
        child = subprocess.Popen(command, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    except OSError:
        raise MaterialError("material helper transport could not start") from None
    failed, stop = threading.Event(), threading.Event()
    buffers = [bytearray(), bytearray()]

    def drain(source, buffer):
        try:
            while True:
                chunk = source.read(4096)
                if not chunk:
                    return
                if len(buffer) + len(chunk) > output_limit:
                    failed.set()
                    # Continue draining until the supervisor stops the child;
                    # never retain overflow or block its output pipe.
                elif not failed.is_set():
                    buffer.extend(chunk)
        except (OSError, ValueError):
            if not stop.is_set():
                failed.set()
        finally:
            source.close()

    def write():
        try:
            with child.stdin:
                child.stdin.write(payload)
        except (BrokenPipeError, OSError, ValueError):
            if not stop.is_set():
                failed.set()

    workers = [threading.Thread(target=drain, args=(source, buffer), daemon=True)
               for source, buffer in zip([child.stdout, child.stderr], buffers)]
    workers.append(threading.Thread(target=write, daemon=True))
    try:
        for worker in workers:
            worker.start()
        while child.poll() is None or any(worker.is_alive() for worker in workers):
            if (failed.is_set() or time.monotonic() >= deadline
                    or (cancelled is not None and cancelled.is_set())):
                raise MaterialError("material helper observation failed, exceeded bounds or was cancelled")
            # Event wait is bounded; neither blocked stdin nor an output pipe
            # inherited by a descendant can defeat the enclosing deadline.
            stop.wait(min(0.02, max(0, deadline-time.monotonic())))
        if child.returncode or failed.is_set() or buffers[1]:
            raise MaterialError("material helper did not return a successful private observation")
        return bytes(buffers[0])
    finally:
        stop.set()
        if child.poll() is None:
            child.kill()
        unresolved = False
        try:
            child.wait(timeout=1)
        except subprocess.TimeoutExpired:
            unresolved = True
        # Do not synchronously close a Python buffered stream still owned by a
        # blocked worker: acquiring its IO lock could bypass our deadline. The
        # daemon worker owns final stream closure when its inherited writer ends.
        for worker in workers:
            worker.join(timeout=0.1)
        for source, worker in zip([child.stdout, child.stderr, child.stdin], workers):
            if not worker.is_alive():
                source.close()
        if unresolved:
            raise MaterialError("material helper transport could not be reaped; owned workload preservation is required")


class ObservationBudget:
    """One sequential composer's allowance across every service and both rounds.

    Limits charge actual reads/allocations, not declared file lengths. Each
    helper keeps its original maxima; successful calls debit shared remaining
    allowance. A failed call invalidates the whole capture, never earns a retry.
    """
    maxima = {"entries":100000,"names":16*1024**2,"bytes":2*1024**3,"attributes":16*1024**2}

    def __init__(self, limits=None, *, observations=2):
        if type(observations) is not int or not 1 <= observations <= 32:
            raise MaterialError("material observation population exceeds bounds")
        limits = {key:value*observations for key,value in self.maxima.items()} if limits is None else limits
        if (not isinstance(limits,dict) or set(limits) != set(self.maxima)
                or any(type(value) is not int or not 0 <= value <= self.maxima[key]*32
                    for key,value in limits.items())):
            raise MaterialError("material aggregate observation budget is invalid")
        self.remaining = dict(limits)
        self._pending = None

    def allowance(self):
        if self._pending is not None:
            raise MaterialError("material observation budget has an unresolved failed call")
        limits = {key:min(value,self.maxima[key]) for key,value in self.remaining.items()}
        if limits["entries"] == 0 or limits["names"] == 0:
            raise MaterialError("material aggregate observation budget is exhausted")
        self._pending = dict(limits)
        return limits

    def consume(self, usage, limits):
        if (limits != self._pending or not isinstance(usage,dict) or set(usage) != set(self.maxima)
                or any(type(value) is not int or not 0 <= value <= limits[key]
                    or value > self.remaining[key] for key,value in usage.items())):
            raise MaterialError("material helper returned invalid aggregate consumption")
        self.remaining = {key:value-usage[key] for key,value in self.remaining.items()}
        self._pending = None


def observe(command, roots, *, deadline, cancelled=None, budget=None):
    request = {"schema_version":1, "roots":roots}
    limits = budget.allowance() if budget is not None else None
    if limits is not None:
        request["limits"] = limits
    payload = json.dumps(request, separators=(",", ":")).encode()
    output = run(command, payload, deadline=deadline, cancelled=cancelled)
    try:
        value = json.loads(output)
    except (ValueError, UnicodeError):
        raise MaterialError("material helper returned invalid private JSON") from None
    fields = {"schema_version", "scope", "sha256", "entries", "bytes", "coverage"}
    if budget is not None:
        fields.add("usage")
    coverage = {"file_content", "posix_metadata", "visible_inode_equivalence",
        "extended_attributes", "visible_extended_attributes", "complete_namespace",
        "atomic_snapshot", "image_anchored_selection"}
    if (not isinstance(value, dict) or set(value) != fields
            or type(value["schema_version"]) is not int or value["schema_version"] != 3
            or value["scope"] != "declared-root-material-observation-v3"
            or not isinstance(value["sha256"], str) or len(value["sha256"]) != 64
            or any(char not in "0123456789abcdef" for char in value["sha256"])
            or type(value["entries"]) is not int or not 0 < value["entries"] <= 100000
            or type(value["bytes"]) is not int or not 0 <= value["bytes"] <= 2*1024**3
            or not isinstance(value["coverage"], dict) or set(value["coverage"]) != coverage
            or any(type(flag) is not bool for flag in value["coverage"].values())
            or not all(value["coverage"][key] for key in ["posix_metadata",
                "visible_inode_equivalence", "visible_extended_attributes"])
            or any(value["coverage"][key] for key in ["extended_attributes",
                "complete_namespace", "atomic_snapshot"])):
        raise MaterialError("material helper returned unsupported coverage or observation bounds")
    selective = any("content_paths" in root and root["content_paths"] is not None for root in roots)
    if (value["coverage"]["file_content"] != (not selective)
            or value["coverage"]["image_anchored_selection"] != selective):
        raise MaterialError("material helper coverage differs from the requested observation")
    if budget is not None:
        usage = value["usage"]
        if (not isinstance(usage,dict) or usage.get("entries") != value["entries"]
                or usage.get("bytes") != value["bytes"]):
            raise MaterialError("material helper returned contradictory aggregate consumption")
        budget.consume(usage,limits)
        # Private allocation/read counters are not content or public coverage.
        value.pop("usage")
    return value
