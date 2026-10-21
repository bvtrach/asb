"""Offline shared infrastructure and qualified Harbor profile boundaries."""
import contextlib
import io
import json
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
import asb_harness as harness
import asb_harbor as harbor
import asb_terminal_bench_1 as terminal
from asb_execution import ExecutionError


class HarnessTests(unittest.TestCase):
    def test_both_adapters_use_the_same_infrastructure(self):
        for name in ["quiet_sdk", "write_json", "record_grading_start", "original_networks",
                     "network_options", "restore_task_networks"]:
            self.assertIs(getattr(harbor, name), getattr(harness, name))
            self.assertIs(getattr(terminal, name), getattr(harness, name))

    def test_sdk_stdout_cannot_corrupt_protocol_even_on_error(self):
        @harness.quiet_sdk
        def sdk_call():
            print("sdk diagnostic")
            raise RuntimeError("fixture failure")
        output, diagnostics = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(diagnostics):
            with self.assertRaisesRegex(RuntimeError, "fixture failure"):
                sdk_call()
            print("protocol")
        self.assertEqual(output.getvalue(), "protocol\n")
        self.assertEqual(diagnostics.getvalue(), "sdk diagnostic\n")
        self.assertEqual(sdk_call.__name__, "sdk_call")

    def test_private_metadata_and_nonrepeatable_verifier_start(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            identity = {"workload":"fixture", "task_id":"task", "attempt_id":"attempt"}
            harness.write_json(root/"task.json", identity)
            harness.record_grading_start(root, identity)
            start = root/"evaluation-started.json"
            original = start.read_bytes()
            for path in [start, root/"task.json"]:
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(json.loads(original), {"schema_version":1,
                "scope":"official-verifier-started-not-a-grade", "task":identity})
            with self.assertRaises(FileExistsError):
                harness.record_grading_start(root, {"task_id":"replacement"})
            self.assertEqual(start.read_bytes(), original)
            start.unlink()
            start.symlink_to(root/"task.json")
            with self.assertRaises(FileExistsError):
                harness.record_grading_start(root, {})
            self.assertEqual(json.loads((root/"task.json").read_bytes()), identity)

    def test_network_capture_restores_only_original_aliases_and_explicit_addresses(self):
        container = Mock(attrs={"NetworkSettings":{"Networks":{"original":{
            "Aliases":["main", "service"], "IPAddress":"dynamic-not-persisted",
            "IPAMConfig":{"IPv4Address":"172.20.0.5", "IPv6Address":"fd00::5",
                          "LinkLocalIPs":["169.254.1.1"]}}}}})
        original = harness.original_networks(container)
        self.assertEqual(original, {"original":{"aliases":["main", "service"],
            "ipv4_address":"172.20.0.5", "ipv6_address":"fd00::5",
            "link_local_ips":["169.254.1.1"]}})
        container.attrs["NetworkSettings"]["Networks"] = {"agent-override":{}}
        client = Mock()
        networks = {name:Mock() for name in ["original", "agent-override"]}
        client.networks.get.side_effect = networks.__getitem__
        harness.restore_task_networks(client, container, original)
        networks["original"].connect.assert_called_once_with(container, **original["original"])
        networks["agent-override"].disconnect.assert_called_once_with(container)
        for invalid in [{"network":{"driver":"host"}}, {"network":{"aliases":[0]}},
                        {str(i):{} for i in range(65)}]:
            client.reset_mock()
            with self.assertRaises(ExecutionError):
                harness.restore_task_networks(client, container, invalid)
            client.networks.get.assert_not_called()

    def test_unknown_profile_fails_before_sdk_import_or_artifact_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            catalog = root/"catalog.json"
            artifacts = root/"attempt"
            for workload in ["other-harbor", "terminal-bench-1.0", None, {}, ""]:
                catalog.write_text(json.dumps({"workload":workload, "sdk":{"package":"harbor"}}))
                task = harbor.HarborTask(Mock(), catalog, "task", artifacts)
                try:
                    with patch.object(harbor, "environment_class") as environment:
                        with self.assertRaises(ExecutionError):
                            task.__enter__()
                        environment.assert_not_called()
                    self.assertFalse(artifacts.exists())
                finally:
                    task.loop.close()
            with self.assertRaises(ExecutionError):
                harbor.HarborTask(Mock(), catalog, "task", artifacts, "other-harbor")

    def test_unsupported_retained_identity_fails_before_reconstruction_or_restart(self):
        retained = Mock(identity={"workload":"other-harbor"})
        with self.assertRaises(ExecutionError):
            harbor.grade_retained(retained, Path("unused"), Mock())
        retained.selected_evaluation.assert_not_called()


if __name__ == "__main__":
    unittest.main()
