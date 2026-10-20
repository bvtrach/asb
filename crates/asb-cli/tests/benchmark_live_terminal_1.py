"""Opt-in real original Terminal-Bench environment and official verifier checks."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_TERMINAL_ROOT"), "requires installed original SDK, dataset and explicit Docker qualification")
class OriginalTerminalQualification(unittest.TestCase):
    def test_real_original_task_uses_its_official_verifier_and_scoped_cleanup(self):
        from asb_docker import DockerSession
        from asb_terminal_bench_1 import TerminalBenchTask
        root = Path(os.environ["ASB_TEST_TERMINAL_ROOT"]).absolute()
        for resolved in [False, True]:
            with self.subTest(resolved=resolved), DockerSession() as client:
                artifacts = Path(tempfile.mkdtemp(prefix="original-terminal-qualification-", dir=root)) / "task"
                print(f"Original SDK qualification artifacts: {artifacts}; known-good fixture={resolved}", flush=True)
                with TerminalBenchTask(client, root / "workloads/terminal-bench-1.0/catalog.json", "hello-world", artifacts) as task:
                    resources = json.loads((artifacts / "resources.json").read_bytes())
                    self.assertEqual(resources["attempt"], task.attempt)
                    self.assertIn({"kind":"containers","name":task.attempt}, resources["resources"])
                    self.assertEqual(task.workspace.container.attrs["Config"]["Labels"]["asb.recovery"], resources["token"])
                    result = task.workspace.execute(["/bin/sh", "-c", "printf 'Hello, world!\\n' > hello.txt" if resolved else "true"])
                    self.assertEqual(result["exit_code"], 0)
                    before = task.workspace.container.exec_run(["test", "!", "-e", "/tests/test_outputs.py"])
                    self.assertEqual(before.exit_code, 0, "verifier was exposed before grading")
                    report = task.evaluate()
                    self.assertEqual(report["outcome"], "resolved" if resolved else "unresolved", report)
                    self.assertEqual(report["resolved"], resolved)
                    self.assertTrue(report["test_results"])
                    attempt = task.attempt
                self.assertEqual(client.containers.list(all=True, filters={"label":"asb.attempt=" + attempt}), [])
                self.assertEqual(json.loads((artifacts / "task-cleanup.json").read_text())["status"], "removed")


if __name__ == "__main__":
    unittest.main()
