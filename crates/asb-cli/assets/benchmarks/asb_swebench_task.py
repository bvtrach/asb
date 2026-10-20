"""SWE-bench-owned preparation and submission capture, using generic execution."""
import hashlib
from pathlib import Path
import uuid

from asb_execution import ExecutionError, ManagedDockerWorkspace
from asb_swebench import (CatalogError, MAX_PATCH_BYTES, agent_instruction,
                          atomic_write, encoded, evaluate_submission, read_task,
                          remove_evaluation_container, run_harness, sdk_identity)
from asb_swebench_sources import build_image, remove_owned_image, resolve_image
from asb_resources import ResourceJournal


class SweBenchTask:
    def __init__(self, client, catalog_path, task_id, artifacts):
        self.client, self.catalog_path, self.task_id = client, Path(catalog_path), task_id
        self.artifacts = Path(artifacts).absolute()
        self.workspace = None
        self.invocation = None
        self.row = None
        self.catalog = None
        self.cancelled = None
        self.attempt = "asb-" + uuid.uuid4().hex
        self.source_image_tag = None
        self.journal = None

    def __enter__(self):
        from swebench.harness.utils import make_test_spec
        from swebench.image_builder.constants import CONTAINER_WORKDIR
        sdk = sdk_identity()
        self.catalog, self.row = read_task(self.catalog_path, self.task_id)
        spec = make_test_spec(self.row)
        self.artifacts.mkdir(mode=0o700, parents=True, exist_ok=False)
        try:
            self.journal = ResourceJournal(self.client, self.artifacts, self.attempt)
            image, source = resolve_image(self.client, spec.image, lambda: build_image(
                self.client, self.row, self.catalog["dataset"], self.artifacts, self.attempt,
                cancelled=self.cancelled))
            self.source_image_tag = source.get("tag")
            if self.source_image_tag is not None:
                atomic_write(self.artifacts / "source-image.json", encoded(source))
            self.workspace = ManagedDockerWorkspace(self.client, image, self.attempt, CONTAINER_WORKDIR,
                                                    journal=self.journal)
            self.workspace.__enter__()
            base = self.row["base_commit"]
            self.checked(["git", "cat-file", "-e", base + "^{commit}"])
            # This is an ASB-owned container, never the user's host checkout.
            self.checked(["git", "reset", "--hard", base])
            self.checked(["git", "clean", "-fd"])
            observed = self.checked(["git", "rev-parse", "HEAD"]).strip().decode("ascii")
            if observed != base:
                raise CatalogError("prepared repository does not match the official base commit")
            if self.checked(["git", "status", "--porcelain", "--untracked-files=all"]).strip():
                raise CatalogError("prepared task repository is not clean")
            self.invocation = {
                "identity": {"workload": self.catalog["workload"], "task_id": self.task_id,
                             "attempt_id": self.attempt, "dataset": self.catalog["dataset"]},
                "prompt": agent_instruction(self.row), "workspace": str(CONTAINER_WORKDIR),
                "baseline": {"kind": "upstream-task-definition", "base_commit": observed},
                "image": self.workspace.image_identity, "sdk": sdk,
                "resources": self.workspace.limits,
                "environment_source": source,
            }
            # No gold patch/test script is copied into the task environment.
            atomic_write(self.artifacts / "task.json", encoded(self.invocation))
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def checked(self, argv, output_limit=256 * 1024):
        result = self.workspace.execute(argv, timeout=60, output_limit=output_limit, cancelled=self.cancelled)
        if result["timed_out"] or result["cancelled"] or result["exit_code"] != 0 or result["output_truncated"]:
            # Command output can contain agent-controlled data. Keep diagnostics
            # structured and do not echo it into human/measurement records.
            raise ExecutionError(f"task repository operation {argv[0]} {argv[1]} failed (exit {result['exit_code']}, timeout {result['timed_out']}, truncated {result['output_truncated']})")
        return result["stdout"]

    def capture_submission(self):
        if self.invocation is None:
            raise ExecutionError("task must be prepared before capturing a submission")
        untracked = self.checked(["git", "ls-files", "--others", "--exclude-standard", "-z"], MAX_PATCH_BYTES)
        paths = [path.decode("utf-8") for path in untracked.split(b"\x00") if path]
        if len(paths) > 10000 or sum(len(path.encode()) for path in paths) > 512 * 1024:
            raise ExecutionError("submission contains too many untracked paths")
        if paths:
            self.checked(["git", "add", "--intent-to-add", "--", *paths])
        patch = self.checked(["git", "-c", "core.fsmonitor=false", "diff", "--binary", "--no-ext-diff",
                              "--no-textconv", self.row["base_commit"], "--"], MAX_PATCH_BYTES)
        path = self.artifacts / "candidate.patch"
        atomic_write(path, patch)
        submission = {"task": self.invocation["identity"], "base_commit": self.row["base_commit"],
                      "image": self.workspace.image_identity, "patch_sha256": hashlib.sha256(patch).hexdigest(),
                      "patch_bytes": len(patch), "artifact": "candidate.patch"}
        atomic_write(self.artifacts / "submission.json", encoded(submission))
        return submission

    def __exit__(self, *_exception):
        try:
            if self.workspace is not None:
                try:
                    self.workspace.close()
                finally:
                    atomic_write(self.artifacts / "task-cleanup.json", encoded({
                        "attempt_id": self.attempt, "status": self.workspace.cleanup_status,
                    }))
        finally:
            try:
                if self.source_image_tag is not None:
                    status = remove_owned_image(self.client, self.source_image_tag, self.attempt)
                    atomic_write(self.artifacts / "source-image-cleanup.json", encoded({
                        "attempt_id": self.attempt, "tag": self.source_image_tag, "status": status,
                    }))
                    self.source_image_tag = None
            finally:
                if self.journal is not None:
                    self.journal.close()

    def evaluate(self, timeout=1800):
        submission = self.capture_submission()
        relative = "evaluation-" + uuid.uuid4().hex
        def executor(arguments, directory, log, deadline):
            run_id = arguments[arguments.index("--run_id") + 1]
            def cancel():
                cleanup = remove_evaluation_container(self.client, self.task_id, run_id)
                atomic_write(directory / "parent-cleanup.json", encoded(cleanup))
                if cleanup["status"] == "error":
                    raise ExecutionError("could not stop the owned evaluator container")
            return run_harness(arguments, directory, log, deadline,
                               cancelled=self.cancelled, on_cancel=cancel)
        result = evaluate_submission(
            self.catalog_path, self.task_id, self.artifacts / submission["artifact"],
            self.artifacts / relative, timeout, image_identity=self.workspace.image_identity,
            executor=executor,
        )
        result["artifact"] = relative + "/evaluation.json"
        return result
