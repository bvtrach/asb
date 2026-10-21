"""Read-only retained-outcome consistency checks, not historical attestation.

Owning adapters supply their own grader checks. No agent, provider, container,
test runner or reference solution is executed during metadata reconciliation.
"""
import argparse
import fcntl
import importlib
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import signal
import threading
import uuid

sys.modules.setdefault("asb_retained", sys.modules[__name__])


class RetainedError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise RetainedError(message)


class RetainedOutcome:
    def __init__(self, request, *, inspect=False):
        self.reference, self.inspect = request["reference"], inspect
        self.evidence = {}
        self.root = Path(self.reference["artifacts"])
        require(self.root.is_absolute() and self.root.is_dir() and not self.root.is_symlink(),
                "retained attempt artifacts are not a real directory")
        self.task = self.object("task.json")
        self.submission = self.object("submission.json")
        self.summary = ({"task": self.task.get("identity"), "workload_baseline": self.task.get("baseline"),
                         "workload_evaluation": "upstream-harness", "workload_success": None}
                        if inspect else request["summary"])
        self.identity = self.summary["task"]
        require(isinstance(self.identity,dict) and set(self.identity) == {"workload","task_id","attempt_id","dataset"},
                "retained task identity is malformed")
        require(self.task.get("identity") == self.identity and self.submission.get("task") == self.identity,
                "retained task and submission identities disagree")
        require(all(self.identity.get(key) == self.reference.get(key) for key in ["workload","task_id","dataset"]),
                "retained task differs from the original selected instance")
        require(self.task.get("baseline") == self.summary.get("workload_baseline"),
                "retained baseline evidence disagrees with the outcome")
        require(self.task.get("image") == self.submission.get("image"),
                "retained submission image differs from its task environment")
        cleanup = self.object("task-cleanup.json")
        self.recorded_cleanup = cleanup.get("status")
        require(cleanup.get("attempt_id") == self.identity["attempt_id"]
                and (self.recorded_cleanup == "removed" or inspect and self.recorded_cleanup == "stopped-preserved"),
                "retained task cleanup is not confirmed")
        require((inspect or type(self.summary.get("workload_success")) is bool)
                and self.summary.get("workload_evaluation") == "upstream-harness",
                "retained outcome has no completed upstream grade")
        resources = self.object("resources.json")
        require(resources.get("attempt") == self.identity["attempt_id"],
                "retained resource journal belongs to a different attempt")
        declarations = resources.get("resources",[])
        require(isinstance(declarations,list) and len(declarations) <= 64
                and all(isinstance(entry,dict) for entry in declarations),"retained resource declarations are malformed")
        if inspect:
            from asb_resources import resource_name
            require(resources.get("schema_version") == 1
                    and isinstance(resources.get("daemon"),str) and 1 <= len(resources["daemon"]) <= 256
                    and re.fullmatch(r"[0-9a-f]{32}",str(resources.get("token","")))
                    and all(set(entry) == {"kind","name"} and resource_name(entry["kind"],entry["name"],self.identity["attempt_id"])
                            for entry in declarations), "retained inspection resource declarations are malformed")
        images = [entry.get("name") for entry in declarations if entry.get("kind") == "images"]
        require(all(isinstance(name,str) and self.identity["attempt_id"] in name for name in images)
                and len(set(images)) == len(images),"retained build image declarations are malformed")
        if self.recorded_cleanup == "stopped-preserved":
            interruption = self.object("interruption.json")
            observed = interruption.get("resources")
            require(interruption.get("schema_version") == 1 and interruption.get("attempt") == self.identity["attempt_id"]
                    and interruption.get("scope") == "preserved-incomplete-attempt-not-grade-not-retry"
                    and isinstance(observed,list) and len(observed) == len(declarations)
                    and all(isinstance(entry,dict) for entry in observed), "retained interruption evidence is malformed")
            declared = [(entry.get("kind"),entry.get("name")) for entry in declarations]
            seen = [(entry.get("kind"),entry.get("name")) for entry in observed]
            require(len(set(declared)) == len(declared) and sorted(declared) == sorted(seen)
                    and all(entry.get("status") == "absent" or entry.get("status") == (
                        "stopped-preserved" if entry.get("kind") == "containers" else "preserved") for entry in observed)
                    and all(entry.get("status") == "absent" or entry.get("kind") != "images"
                            or re.fullmatch(r"sha256:[0-9a-f]{64}",str(entry.get("image_id",""))) for entry in observed),
                    "retained interruption does not cover its declared resources")
        elif images:
            cleanup = self.object("image-cleanup.json")
            outcomes = cleanup.get("resources")
            require(cleanup.get("schema_version") == 1 and cleanup.get("attempt") == self.identity["attempt_id"]
                    and cleanup.get("scope") == "owned-build-tags-only" and isinstance(outcomes,list)
                    and len(outcomes) == len(images)
                    and sorted(entry.get("name","") for entry in outcomes) == sorted(images)
                    and all(entry.get("kind") == "images" and entry.get("status") in ["removed","absent"]
                            and (entry.get("status") == "absent" or re.fullmatch(r"sha256:[0-9a-f]{64}",str(entry.get("image_id",""))))
                            for entry in outcomes), "retained build image cleanup is not confirmed")

    def select_grade(self, result):
        require(self.inspect and self.summary["workload_success"] is None,
                "retained inspection cannot replace an existing outcome")
        require(type(result.get("resolved")) is bool
                and result.get("outcome") == ("resolved" if result["resolved"] else "unresolved"),
                "retained evaluator did not complete an official grade")
        self.summary["workload_success"] = result["resolved"]

    def inspected_grade(self):
        require(type(self.summary["workload_success"]) is bool, "retained inspection has no completed grade")
        return {"schema_version":1, "scope":"retained-official-grade-not-execution-recovery",
                "task":self.identity, "workload_baseline":self.summary["workload_baseline"],
                "workload_evaluation":"upstream-harness", "workload_success":self.summary["workload_success"],
                "recorded_cleanup":self.recorded_cleanup,
                "evidence":dict(sorted(self.evidence.items())),
                "process_success":None, "exit_code":None, "elapsed_ms":None, "token_usage":None, "steps":None}

    def selected_evaluation(self):
        if not self.child("selected-evaluation.json").exists():
            return None
        selection = self.object("selected-evaluation.json")
        directory = selection.get("directory", "")
        require(selection.get("schema_version") == 1
                and selection.get("scope") == "selected-retained-evaluator-not-agent-retry"
                and selection.get("attempt_id") == self.identity["attempt_id"]
                and re.fullmatch(r"(?:evaluation|recovered-evaluation)-[a-f0-9]{32}",directory),
                "retained evaluator selection is malformed")
        require(self.child(directory).is_dir(), "selected retained evaluator directory is absent")
        return directory + "/"

    def verify_grading_cleanup(self, prefix):
        cleanup = self.object(prefix+"grading-cleanup.json")
        resources = cleanup.get("resources")
        declarations = self.object("resources.json")["resources"]
        require(cleanup.get("schema_version") == 1 and cleanup.get("attempt") == self.identity["attempt_id"]
                and cleanup.get("scope") == "retained-grading-resources-preserved"
                and isinstance(resources,list) and len(resources) == len(declarations)
                and all(isinstance(item,dict) and isinstance(item.get("kind"),str) and isinstance(item.get("name"),str)
                        for item in resources), "retained grading cleanup is malformed")
        declared = sorted((item["kind"],item["name"]) for item in declarations)
        observed = [(item.get("kind"),item.get("name")) for item in resources]
        require(len(set(observed)) == len(observed) and sorted(observed) == declared
                and all(item.get("status") == ("stopped-preserved" if item["kind"] == "containers" else "preserved")
                        or item["kind"] == "images" and item.get("status") == "absent" for item in resources)
                and any(item["kind"] == "containers" and item["name"] == self.identity["attempt_id"] for item in resources)
                and all(item["kind"] != "images" or item["status"] == "absent"
                        or re.fullmatch(r"sha256:[a-f0-9]{64}",str(item.get("image_id",""))) for item in resources),
                "retained grading did not confirm original resource preservation")

    def verify_environment_baseline(self):
        baseline = self.summary["workload_baseline"]
        require(baseline.get("kind") == "upstream-environment-definition"
                and baseline.get("source_revision") == self.identity["dataset"]["revision"]
                and re.fullmatch(r"[a-f0-9]{40}",str(baseline.get("source_revision","")))
                and re.fullmatch(r"[a-f0-9]{64}",str(baseline.get("definition_sha256",""))),
                "retained environment baseline does not identify its original task definition")

    def publish_selection(self, directory):
        require(getattr(self,"grading",False), "evaluation selection requires explicit grading")
        value = {"schema_version":1,"scope":"selected-retained-evaluator-not-agent-retry",
                 "attempt_id":self.identity["attempt_id"],"directory":directory}
        temporary = self.root / (".grade-selection-" + uuid.uuid4().hex)
        try:
            descriptor = os.open(temporary,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
            with os.fdopen(descriptor,"wb") as output:
                output.write(json.dumps(value,sort_keys=True).encode()+b"\n")
                output.flush()
                os.fsync(output.fileno())
            # Publish without replacing a previously selected grade.
            os.link(temporary,self.child("selected-evaluation.json"))
            descriptor = os.open(self.root,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
            try:os.fsync(descriptor)
            finally:os.close(descriptor)
        finally:
            temporary.unlink(missing_ok=True)

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
        entry = {"sha256":hashlib.sha256(value).hexdigest(), "bytes":len(value)}
        key = PurePosixPath(relative).as_posix()
        require(key not in self.evidence or self.evidence[key] == entry,
                "retained artifact changed while being inspected")
        self.evidence[key] = entry
        require(len(self.evidence) <= 256 and sum(item["bytes"] for item in self.evidence.values()) <= 512*1024**2,
                "retained evidence exceeds its inspection budget")
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
    parser.add_argument("--inspect-grade",action="store_true")
    parser.add_argument("--grade",action="store_true")
    arguments = parser.parse_args()
    try:
        require(re.fullmatch(r"asb_[a-z0-9_]+",arguments.module) is not None,
                "invalid owning verification module")
        raw = sys.stdin.buffer.readline(1024*1024+1)
        require(len(raw) <= 1024*1024 and raw.endswith(b"\n"), "retained verification request is not bounded")
        request = json.loads(raw)
        # Do not create a missing lock or change any evaluator evidence.
        root = Path(request["reference"]["artifacts"])
        require(root.is_absolute() and root.is_dir() and not root.is_symlink(), "retained attempt is not a real directory")
        descriptor = os.open(root / "resources.lock", os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            require(stat.S_ISREG(os.fstat(descriptor).st_mode), "retained owner lock is not a regular file")
            fcntl.flock(descriptor,fcntl.LOCK_EX | fcntl.LOCK_NB)
            retained = RetainedOutcome(request, inspect=arguments.inspect_grade or arguments.grade)
            module = importlib.import_module(arguments.module)
            if arguments.grade:
                cancelled = threading.Event()
                signal.signal(signal.SIGTERM,lambda *_:cancelled.set())
                def owner_lifetime():
                    # The Rust controller keeps stdin open until completion.
                    # EOF or any unexpected additional input cancels grading.
                    # Do not hold BufferedReader's lock in a daemon thread at
                    # interpreter shutdown after a completed grading response.
                    os.read(sys.stdin.fileno(),1)
                    cancelled.set()
                threading.Thread(target=owner_lifetime,daemon=True).start()
                retained.grading = True
                retained.owner_lock = descriptor
                module.grade_retained(retained,request["catalog"],cancelled)
                # Build an identical inspection report on future reads; old
                # failed grading attempts are retained but not silently merged.
                retained = RetainedOutcome(request,inspect=True)
                module.inspect_retained(retained)
                result = retained.inspected_grade()
            elif arguments.inspect_grade:
                module.inspect_retained(retained)
                result = retained.inspected_grade()
            else:
                module.verify_retained(retained)
                result = {"ok":True,"scope":"retained-artifact-consistency-not-historical-attestation"}
        finally:
            os.close(descriptor)
        print(json.dumps(result))
        return 0
    except Exception as error:
        from asb_resources import RecoveryError
        from asb_docker import DockerAccessError
        result = {"ok":False,"error_class":type(error).__name__}
        if isinstance(error,(RetainedError,RecoveryError,DockerAccessError)):
            result["reason"] = str(error)
        elif isinstance(error,FileNotFoundError):
            result["reason"] = "required retained artifact is missing"
        elif isinstance(error,BlockingIOError):
            result["reason"] = "retained attempt still has an active owner; wait for it to finish"
        print(json.dumps(result))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
