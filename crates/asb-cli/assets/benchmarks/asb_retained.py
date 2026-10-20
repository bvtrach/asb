"""Read-only retained-outcome consistency checks, not historical attestation.

Owning adapters supply their own grader checks. No agent, provider, container,
test runner or reference solution is executed during metadata reconciliation.
"""
import argparse
import fcntl
import importlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys

sys.modules.setdefault("asb_retained", sys.modules[__name__])


class RetainedError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise RetainedError(message)


class RetainedOutcome:
    def __init__(self, request):
        self.reference, self.summary = request["reference"], request["summary"]
        self.root = Path(self.reference["artifacts"])
        require(self.root.is_absolute() and self.root.is_dir() and not self.root.is_symlink(),
                "retained attempt artifacts are not a real directory")
        self.task = self.object("task.json")
        self.submission = self.object("submission.json")
        self.identity = self.summary["task"]
        require(self.task.get("identity") == self.identity and self.submission.get("task") == self.identity,
                "retained task and submission identities disagree")
        require(all(self.identity.get(key) == self.reference.get(key) for key in ["workload","task_id","dataset"]),
                "retained task differs from the original selected instance")
        require(self.task.get("baseline") == self.summary.get("workload_baseline"),
                "retained baseline evidence disagrees with the outcome")
        require(self.task.get("image") == self.submission.get("image"),
                "retained submission image differs from its task environment")
        cleanup = self.object("task-cleanup.json")
        require(cleanup.get("attempt_id") == self.identity["attempt_id"] and cleanup.get("status") == "removed",
                "retained task cleanup is not confirmed")
        require(type(self.summary.get("workload_success")) is bool
                and self.summary.get("workload_evaluation") == "upstream-harness",
                "retained outcome has no completed upstream grade")
        resources = self.object("resources.json")
        require(resources.get("attempt") == self.identity["attempt_id"],
                "retained resource journal belongs to a different attempt")

    def child(self, relative):
        path = PurePosixPath(relative)
        require(not path.is_absolute() and path.parts and ".." not in path.parts,
                "retained artifact path is outside its attempt")
        current = self.root
        for component in path.parts:
            current = current / component
            require(not current.is_symlink(), "retained artifact path contains a symlink")
        return current

    def bytes(self, relative, maximum=4*1024**2):
        descriptor = os.open(self.child(relative), os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor,"rb") as source:
            metadata = os.fstat(source.fileno())
            require(stat.S_ISREG(metadata.st_mode) and metadata.st_size <= maximum,
                    "retained artifact is not a bounded regular file")
            value = source.read(maximum+1)
        require(len(value) <= maximum, "retained artifact exceeded its size limit")
        return value

    def object(self, relative):
        try:
            value = json.loads(self.bytes(relative))
        except (UnicodeDecodeError,json.JSONDecodeError):
            raise RetainedError("retained artifact is not valid JSON") from None
        require(isinstance(value,dict), "retained artifact is not a JSON object")
        return value

    def grade(self, result):
        require(result.get("schema_version") == 1
                and all(result.get(key) == self.identity.get(key) for key in ["workload","task_id","dataset"]),
                "retained evaluator identity differs from its task")
        require(type(result.get("resolved")) is bool
                and result["resolved"] == self.summary["workload_success"]
                and result.get("outcome") == ("resolved" if result["resolved"] else "unresolved"),
                "retained evaluator grade disagrees with the outcome")
        require(result.get("sdk") == self.task.get("sdk"), "retained evaluator SDK differs from its task")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--module",required=True)
    arguments = parser.parse_args()
    try:
        require(re.fullmatch(r"asb_[a-z0-9_]+",arguments.module) is not None,
                "invalid owning verification module")
        raw = sys.stdin.buffer.readline(1024*1024+1)
        require(len(raw) <= 1024*1024 and raw.endswith(b"\n"), "retained verification request is not bounded")
        request = json.loads(raw)
        retained = RetainedOutcome(request)
        # Do not create a missing lock or change any evaluator evidence.
        descriptor = os.open(retained.child("resources.lock"), os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            require(stat.S_ISREG(os.fstat(descriptor).st_mode), "retained owner lock is not a regular file")
            fcntl.flock(descriptor,fcntl.LOCK_EX | fcntl.LOCK_NB)
            importlib.import_module(arguments.module).verify_retained(retained)
        finally:
            os.close(descriptor)
        print(json.dumps({"ok":True,"scope":"retained-artifact-consistency-not-historical-attestation"}))
        return 0
    except Exception as error:
        result = {"ok":False,"error_class":type(error).__name__}
        if isinstance(error,RetainedError):
            result["reason"] = str(error)
        elif isinstance(error,FileNotFoundError):
            result["reason"] = "required retained artifact is missing"
        print(json.dumps(result))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
