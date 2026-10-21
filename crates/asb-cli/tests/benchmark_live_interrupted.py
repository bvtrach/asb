"""Opt-in original environments: interrupted teardown retains actual work.

Run with each selected adapter's installed SDK Python. No LLM is contacted.
"""
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


def cleanup_fixture(client, artifacts):
    """Remove only this completed qualification's positively-owned resources."""
    from asb_resources import lock_directory, open_directory, owned_resource, read_record
    if not (artifacts / "resources.json").exists():
        return
    directory, lock = open_directory(artifacts), None
    try:
        lock = lock_directory(directory)
        record = read_record(directory)
        assert record["daemon"] == client.info()["ID"]
        labels = {"asb.owner": "asb", "asb.attempt": record["attempt"], "asb.recovery": record["token"]}
        owned = [(item, owned_resource(client, item, labels)) for item in record["resources"]]
        for kind in ["containers", "networks", "volumes", "images"]:
            for item, resource in owned:
                if item["kind"] != kind or resource is None:
                    continue
                if kind == "containers":
                    resource.remove(force=True)
                elif kind == "images":
                    client.images.remove(item["name"], noprune=True)
                else:
                    resource.remove()
    finally:
        if lock is not None:
            os.close(lock)
        os.close(directory)


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT") and os.environ.get("ASB_TEST_INTERRUPTED_WORKLOADS"),
                     "requires explicit original project, workload selection and installed SDK Python")
class InterruptedOriginalQualification(unittest.TestCase):
    def test_original_environments_preserve_partial_files_and_existing_false_grades(self):
        from asb_docker import DockerSession
        from asb_swebench_task import SweBenchTask
        from asb_terminal_bench_1 import TerminalBenchTask
        from asb_harbor import HarborTask
        from asb_resources import quiesce
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        choices = os.environ["ASB_TEST_INTERRUPTED_WORKLOADS"].split(",")
        engines = {"swe-bench": (SweBenchTask, "astropy__astropy-12907"),
                   "swe-bench-lite": (SweBenchTask, "astropy__astropy-12907"),
                   "swe-bench-verified": (SweBenchTask, "astropy__astropy-12907"),
                   "terminal-bench-1.0": (TerminalBenchTask, "hello-world"),
                   "terminal-bench-2.1": (HarborTask, "cancel-async-tasks")}
        self.assertTrue(choices and set(choices) <= set(engines))
        for workload in choices:
            with self.subTest(workload=workload), DockerSession() as client:
                artifacts = Path(tempfile.mkdtemp(prefix="interrupted-original-", dir=root)) / "task"
                print(f"Interrupted original qualification {workload}: {artifacts}", flush=True)
                adapter, instance = engines[workload]
                try:
                    with adapter(client, root / "workloads" / workload / "catalog.json", instance, artifacts) as task:
                        command = task.workspace.execute(["/bin/sh", "-c", "printf 'retained partial work\\n' > asb-interrupted-partial.txt"])
                        self.assertEqual(command["exit_code"], 0)
                        # Capture and grade through the actual owning SDK, not
                        # a fixture grade. The unrelated change cannot fix it.
                        submission = task.capture_submission()
                        result = task.evaluate()
                        self.assertEqual(result["outcome"], "unresolved", result)
                        before = {path.relative_to(artifacts): path.read_bytes()
                                  for path in artifacts.rglob("evaluation.json")}
                        self.assertTrue(before)
                        candidate = (artifacts / "candidate.patch").read_bytes() if (artifacts / "candidate.patch").exists() else None
                        container = task.workspace.container
                        workdir = task.workspace.workdir.rstrip("/")
                        task.interrupted = True
                    container.reload()
                    self.assertIs(container.attrs["State"]["Running"], False)
                    archive, _ = container.get_archive(workdir + "/asb-interrupted-partial.txt")
                    with tarfile.open(fileobj=io.BytesIO(b"".join(archive))) as files:
                        self.assertEqual(files.extractfile(files.getmembers()[0]).read(), b"retained partial work\n")
                    for relative, contents in before.items():
                        self.assertEqual((artifacts / relative).read_bytes(), contents)
                    if candidate is not None:
                        self.assertEqual((artifacts / "candidate.patch").read_bytes(), candidate)
                    self.assertEqual(json.loads((artifacts / "submission.json").read_bytes())["task"], submission["task"])
                    self.assertEqual(json.loads((artifacts / "task-cleanup.json").read_bytes())["status"], "stopped-preserved")
                    self.assertEqual(json.loads((artifacts / "interruption.json").read_bytes())["attempt"], task.attempt)
                    resources = json.loads((artifacts / "resources.json").read_bytes())["resources"]
                    for resource in resources:
                        if resource["kind"] == "images":
                            self.assertTrue(client.images.get(resource["name"]).id.startswith("sha256:"))
                    self.assertEqual(quiesce(client, artifacts)["attempt"], task.attempt)
                finally:
                    cleanup_fixture(client, artifacts)


if __name__ == "__main__":
    unittest.main()
