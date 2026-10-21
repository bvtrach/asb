"""All-five adapter interrupted teardown; no SDK/daemon/provider required."""
import json
from pathlib import Path
import sys
import tempfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
from asb_resources import RecoveryError, ResourceJournal
from asb_swebench_task import SweBenchTask
from asb_terminal_bench_1 import TerminalBenchTask
from asb_harbor import HarborTask


class Collection:
    def __init__(self):
        self.values = {}
    def get(self, name):
        return self.values[name]


class Container:
    def __init__(self, name, labels):
        self.name = name
        self.attrs = {"Config": {"Labels": dict(labels)}, "State": {"Running": True}}
        self.stops = 0
    def stop(self, timeout):
        assert timeout == 2
        self.stops += 1
        self.attrs["State"]["Running"] = False
    def reload(self):
        pass


class InterruptedTests(unittest.TestCase):
    def test_every_original_adapter_preserves_partial_work_and_existing_grades(self):
        for workload, adapter in [("swe-bench", SweBenchTask), ("swe-bench-lite", SweBenchTask),
                                  ("swe-bench-verified", SweBenchTask), ("terminal-bench-1.0", TerminalBenchTask),
                                  ("terminal-bench-2.1", HarborTask)]:
            for mismatch, unwind in [(False, False), (True, False), (False, True)]:
                with self.subTest(workload=workload, mismatch=mismatch, unwind=unwind), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    client = SimpleNamespace(info=lambda: {"ID": "fixture-daemon"},
                        containers=Collection(), images=Collection(), volumes=Collection(), networks=Collection())
                    task = adapter(client, root / "catalog", "original-task", root)
                    task.interrupted = not unwind
                    task.journal = ResourceJournal(client, root, task.attempt)
                    journal = task.journal
                    self.addCleanup(journal.close)
                    owned = Container(task.attempt, journal.declare("containers", task.attempt))
                    unrelated = Container("unrelated", {})
                    client.containers.values = {owned.name: owned, unrelated.name: unrelated}
                    volume = SimpleNamespace(name=task.attempt + "-data", attrs={"Labels": journal.declare("volumes", task.attempt + "-data")})
                    client.volumes.values[volume.name] = volume
                    tag = "owned:" + task.attempt
                    image = SimpleNamespace(id="sha256:" + "a" * 64,
                        attrs={"Config": {"Labels": journal.declare("images", tag)}, "RepoTags": [tag]})
                    client.images.values[tag] = image
                    if mismatch:
                        owned.attrs["Config"]["Labels"]["asb.recovery"] = "foreign"
                    workspace = SimpleNamespace(container=owned, cleanup_status="pending", close=Mock(), join_held_executions=Mock())
                    task.workspace = workspace
                    task.terminal = Mock()
                    task.environment = Mock()
                    # A completed false grade must remain false, byte for byte.
                    evidence = {"candidate.patch": b"private partial candidate\n",
                                "evaluation.json": b'{"resolved":false,"outcome":"unresolved"}\n'}
                    for name, contents in evidence.items():
                        (root / name).write_bytes(contents)
                    modules = {name: ModuleType(name) for name in ["docker", "terminal_bench",
                        "terminal_bench.terminal", "terminal_bench.terminal.terminal"]}
                    modules["terminal_bench"].terminal = modules["terminal_bench.terminal"]
                    modules["terminal_bench.terminal"].terminal = modules["terminal_bench.terminal.terminal"]
                    with patch.dict(sys.modules, modules):
                        if mismatch:
                            with self.assertRaises(RecoveryError):
                                task.__exit__(None, None, None)
                        else:
                            task.__exit__(RuntimeError if unwind else None, None, None)
                    expected = "preservation_failed" if mismatch else "stopped-preserved"
                    self.assertEqual(workspace.cleanup_status, expected)
                    self.assertTrue(task.interrupted)
                    self.assertEqual(json.loads((root / "task-cleanup.json").read_bytes())["status"], expected)
                    self.assertIsNone(journal.lock)
                    self.assertEqual(owned.stops, 0 if mismatch else 1)
                    self.assertEqual(unrelated.stops, 0)
                    self.assertIs(client.containers.values[owned.name], owned)
                    self.assertIs(client.images.values[tag], image)
                    self.assertIs(client.volumes.values[volume.name], volume)
                    self.assertFalse(workspace.close.called)
                    self.assertFalse(task.terminal.mock_calls)
                    self.assertFalse(task.environment.mock_calls)
                    self.assertFalse((root / "image-cleanup.json").exists())
                    self.assertEqual((root / "interruption.json").exists(), not mismatch)
                    for name, contents in evidence.items():
                        self.assertEqual((root / name).read_bytes(), contents)
                    if isinstance(task, HarborTask):
                        self.assertTrue(task.loop.is_closed())


if __name__ == "__main__":
    unittest.main()
