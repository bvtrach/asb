"""Offline contracts for original Terminal-Bench task isolation and teardown."""
import json
from pathlib import Path
import stat
import sys
import tempfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
import asb_terminal_bench_1 as subject
from asb_execution import ExecutionError, ManagedDockerWorkspace


class TerminalExecutionTests(unittest.TestCase):
    def test_verifier_networks_remove_agent_override_and_preserve_original_services(self):
        container = Mock(attrs={"NetworkSettings": {"Networks": {"fixture-only": {}, "service": {}}}})
        client = Mock()
        networks = {name: Mock() for name in ["fixture-only", "service", "original"]}
        client.networks.get.side_effect = networks.__getitem__
        subject.restore_task_networks(client, container, ["original", "service"])
        networks["original"].connect.assert_called_once_with(container)
        networks["fixture-only"].disconnect.assert_called_once_with(container)
        networks["service"].connect.assert_not_called()
        networks["service"].disconnect.assert_not_called()

    def test_build_inputs_hide_gold_and_verifier_but_keep_application_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "task"
            (source / "tests").mkdir(parents=True)
            (source / "tests/test_gold.py").write_text("private verifier")
            (source / "app/tests").mkdir(parents=True)
            (source / "app/tests/test_app.py").write_text("public project test")
            for name in ["solution.sh", "solution.yaml", "task.yaml", "run-tests.sh"]:
                (source / name).write_text("private")
            (source / "Dockerfile").write_text("FROM public")
            subject.copy_task_inputs(source, root / "input", source)
            actual = {str(path.relative_to(root / "input")) for path in (root / "input").rglob("*") if path.is_file()}
            self.assertEqual(actual, {"Dockerfile", "app/tests/test_app.py"})
            with self.assertRaises(ExecutionError):
                subject.copy_task_inputs(source / "solution.sh", root / "leaked", source)
            self.assertFalse((root / "leaked").exists())

    def test_compose_environment_is_private_whitelisted_and_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "compose.env"
            subject.write_compose_environment(path, {"T_BENCH_TASK": "a${b}\nnext", "OPENROUTER_API_KEY": "secret", "HOME": "/private"})
            self.assertEqual(path.read_text(), 'T_BENCH_TASK="a$${b}\\nnext"\n')
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            with self.assertRaises(FileExistsError):
                subject.write_compose_environment(path, {"T_BENCH_TASK": "replacement"})

    def test_attach_requires_exact_attempt_labels_and_records_actual_limits(self):
        attempt = "asb-" + "a" * 32
        image = SimpleNamespace(id="sha256:" + "b" * 64, attrs={})
        container = SimpleNamespace(image=image, attrs={"Config": {"Labels": {"asb.owner": "asb", "asb.attempt": attempt}},
            "HostConfig": {"Memory": 123456789, "NanoCpus": 1500000000, "PidsLimit": 64}})
        workspace = ManagedDockerWorkspace.attach(None, container, attempt, "/app")
        self.assertEqual(workspace.limits, {"memory_bytes": 123456789, "cpus": 1.5, "pids": 64})
        for labels in [{}, {"asb.owner": "someone", "asb.attempt": attempt}, {"asb.owner": "asb", "asb.attempt": "another"}]:
            container.attrs["Config"]["Labels"] = labels
            with self.assertRaises(ExecutionError):
                ManagedDockerWorkspace.attach(None, container, attempt, "/app")

    def test_cleanup_failure_is_not_reported_as_never_created(self):
        modules = {name: ModuleType(name) for name in ["docker", "terminal_bench", "terminal_bench.terminal", "terminal_bench.terminal.terminal"]}
        modules["terminal_bench"].terminal = modules["terminal_bench.terminal"]
        modules["terminal_bench.terminal"].terminal = modules["terminal_bench.terminal.terminal"]
        with tempfile.TemporaryDirectory() as directory, patch.dict(sys.modules, modules):
            task = subject.TerminalBenchTask(Mock(), "catalog", "task", directory)
            manager = Mock(asb_config="owned-compose.json")
            manager._run_docker_compose_command.side_effect = ExecutionError("down failed")
            task.terminal = SimpleNamespace(_sessions={}, _compose_manager=manager)
            task.workspace = SimpleNamespace(cleanup_status="pending")
            with self.assertRaisesRegex(ExecutionError, "down failed"):
                task.__exit__(None, None, None)
            self.assertEqual(task.workspace.cleanup_status, "cleanup_failed")
            self.assertEqual(json.loads((Path(directory) / "task-cleanup.json").read_text())["status"], "cleanup_failed")
            manager._run_docker_compose_command.assert_called_once_with(["down", "--timeout", "2", "--volumes", "--remove-orphans"])


if __name__ == "__main__":
    unittest.main()
