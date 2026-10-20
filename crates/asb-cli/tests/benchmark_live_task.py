"""Opt-in real task-environment qualification with synthetic agent commands only."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0, str(SOURCE))


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT"), "needs explicit upstream Docker qualification")
class TaskEnvironmentQualification(unittest.TestCase):
    def test_real_clean_base_gate_partial_and_committed_submission_and_cleanup(self):
        from asb_docker import DockerSession
        from asb_swebench_task import SweBenchTask
        import docker.errors
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="official-task-", dir=root))
        print(f"Retained task-environment qualification: {artifacts}", flush=True)
        with DockerSession() as client:
            for suite in ["swe-bench", "swe-bench-lite", "swe-bench-verified"]:
                with self.subTest(suite=suite):
                    with SweBenchTask(client, root / "workloads" / suite / "catalog.json",
                                      "astropy__astropy-12907", artifacts / suite) as task:
                        container_id = task.workspace.container.id
                        self.assertEqual(task.invocation["identity"]["workload"], suite)
                        self.assertEqual(task.invocation["baseline"]["kind"], "upstream-task-definition")
                        self.assertFalse(task.workspace.container.attrs.get("Mounts"))
                        self.assertNotIn("patch", task.invocation)
                        self.assertNotIn("test_patch", task.invocation)
                        self.assertEqual(task.capture_submission()["patch_bytes"], 0)
                        seen = []
                        def attach(target):
                            seen.append(target)
                            self.assertEqual(task.workspace.container.exec_run(
                                ["/bin/sh", "-c", "test ! -e /testbed/asb-committed.txt"]
                            ).exit_code, 0)
                            self.assertGreater(target["daemon_pid"], 1)
                            # This qualification uses the local daemon. Inspect
                            # NSpid as evidence, not an assumption about remote Docker.
                            status = Path(f"/proc/{target['daemon_pid']}/status").read_text()
                            namespace_ids = next(line for line in status.splitlines() if line.startswith("NSpid:"))
                            self.assertEqual(int(namespace_ids.split()[-1]), target["namespace_pid"])
                        result = task.workspace.execute(["/bin/sh", "-c",
                            "printf 'committed\\n' > asb-committed.txt; git add asb-committed.txt; "
                            "git -c user.name=ASB -c user.email=fixture@example.invalid commit -qm fixture; "
                            "printf 'untracked\\n' > asb-new.txt; printf 'stderr fixture\\n' >&2; exit 7"],
                            before_resume=attach, timeout=30)
                        self.assertEqual(len(seen), 1)
                        self.assertEqual(result["exit_code"], 7)
                        self.assertIn(b"stderr fixture", result["stderr"])
                        self.assertFalse(result["timed_out"])
                        submitted = task.capture_submission()
                        patch = (artifacts / suite / submitted["artifact"]).read_text()
                        self.assertIn("asb-committed.txt", patch)
                        self.assertIn("asb-new.txt", patch)
                        timeout = task.workspace.execute(["/bin/sh", "-c",
                            "printf 'partial\\n' > asb-timeout.txt; sleep 60 & wait"], timeout=2)
                        self.assertTrue(timeout["timed_out"])
                        submitted = task.capture_submission()
                        patch = (artifacts / suite / submitted["artifact"]).read_text()
                        self.assertIn("asb-timeout.txt", patch)
                        self.assertIn("asb-committed.txt", patch)
                        print(f"{suite}: clean base, stopped gate, exit-7 patch and timeout patch captured", flush=True)
                    with self.assertRaises(docker.errors.NotFound):
                        client.containers.get(container_id)
                    cleanup = json.loads((artifacts / suite / "task-cleanup.json").read_bytes())
                    self.assertEqual(cleanup["status"], "removed")


if __name__ == "__main__":
    unittest.main()
