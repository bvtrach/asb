"""Opt-in original service-bearing task qualification, no live LLM provider.

The known upstream reference is deliberately supplied only to a synthetic
client fixture. This checks the real guest adapter/supervisor and original
verifier, not whether an AI agent can solve the task.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
                     "requires explicit original SDK, Docker and compiled guest opt-in")
class WorkloadServiceQualification(unittest.TestCase):
    def test_original_jupyter_service_survives_client_exit_and_official_same_shell_grading(self):
        from asb_docker import DockerSession
        from asb_guest import execute_agent, upload
        from asb_runtime import bundle
        from asb_terminal_bench_1 import TerminalBenchTask
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="original-service-lifetime-", dir=root))
        print(f"Retained original service-lifetime qualification: {artifacts}", flush=True)
        with DockerSession() as client:
            with TerminalBenchTask(client, root / "workloads/terminal-bench-1.0/catalog.json",
                                   "jupyter-notebook-server", artifacts / "task") as task:
                self.assertTrue(task.handler.task.run_tests_in_same_shell)
                task.checked(["mkdir", "-p", "/opt/asb-runtime/bin", "/opt/asb-runtime/lib",
                    "/opt/asb-project/.asb", "/opt/asb-project/.asb-tools/pi/bin", "/opt/asb-private"])
                files, _ = bundle(os.environ["ASB_TEST_BENCHMARK_ASB"], task.workspace.image_identity["architecture"])
                upload(task.workspace.container, "/opt/asb-runtime", files, temporary_dir=artifacts)
                upload(task.workspace.container, "/opt/asb-project/.asb", [("project.toml",
                    b"tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n", 0o600)], temporary_dir=artifacts)
                upload(task.workspace.container, "/opt/asb-private", [("fixture-solution.sh",
                    (task.definition / "solution.sh").read_bytes(), 0o700)], temporary_dir=artifacts)
                fixture = b'''#!/bin/sh
set -eu
/bin/bash /opt/asb-private/fixture-solution.sh
printf '%s\\n' '{"type":"result","result":"known service fixture prepared"}'
'''
                upload(task.workspace.container, "/opt/asb-project/.asb-tools/pi/bin",
                       [("pi", fixture, 0o755)], temporary_dir=artifacts)
                guest = {"process_lifetime":"through-evaluation", "config":{
                    "agent":"pi", "provider":"openrouter", "model":"fixture", "workload":"terminal-bench-1.0",
                    "workspace":task.workspace.workdir, "concurrency":1},
                    "connection":{"interface":"open_ai_chat", "authentication":"bearer",
                        "base_url":"http://127.0.0.1:1/v1", "credential":"unused-fixture"},
                    "prompt":"explicit known service-lifetime fixture, no AI solving claim"}
                ended = []
                with patch("asb_guest.prepare_agent", return_value=(guest, {"PATH":"/usr/local/bin:/usr/bin:/bin"})):
                    result = execute_agent(task, {}, None, None, None, on_ended=ended.append)
                (artifacts / "driver.stdout").write_bytes(result["stdout"])
                (artifacts / "driver.stderr").write_bytes(result["stderr"])
                self.assertEqual(result["exit_code"], 0, result["stderr"].decode(errors="replace"))
                self.assertTrue(result["workload_processes_retained"])
                self.assertFalse(result["metadata"]["tree_quiesced"])
                self.assertEqual(len(ended), 1)
                self.assertTrue(task.workspace.held_executions[0][1].is_alive())
                task.checked(["python3", "-c", "import socket; s=socket.create_connection(('127.0.0.1',8888),5); s.close()"])
                observation = json.loads((task.artifacts / "execution.json").read_bytes())
                self.assertEqual(observation["agent_tree_cleanup"], "delegated-owned-workload")
                report = task.evaluate()
                self.assertTrue(report["resolved"], report)
                self.assertEqual(report["outcome"], "resolved")
                attempt = task.attempt
                workspace = task.workspace
            self.assertEqual(workspace.held_executions, [])
            self.assertEqual(json.loads((artifacts / "task/task-cleanup.json").read_text())["status"], "removed")
            self.assertEqual(client.containers.list(all=True, filters={"label":"asb.attempt=" + attempt}), [])


if __name__ == "__main__":
    unittest.main()
