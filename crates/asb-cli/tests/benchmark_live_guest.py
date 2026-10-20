"""Real ASB adapter + installed Pi in a real SWE task, with an offline provider.

Public package caches are prepared with ASB on the host first. The task uses
only copied project caches, not host mounts, and loses external networking
before the real client is launched. No provider key or subscription is needed.
"""
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import time
import unittest

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0, str(SOURCE))


def upload(container, destination, files):
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w") as tar:
        for name, content, mode in files:
            entry = tarfile.TarInfo(name)
            entry.size, entry.mode = len(content), mode
            tar.addfile(entry, io.BytesIO(content))
    if not container.put_archive(destination, archive.getvalue()):
        raise AssertionError("could not stage owned guest files")


@unittest.skipUnless(os.environ.get("ASB_TEST_GUEST_BINARY"), "needs explicit static guest binary and cached public tools")
class RealGuestQualification(unittest.TestCase):
    def test_actual_registered_pi_adapter_uses_offline_tools_and_reports_usage_in_task_container(self):
        from asb_docker import DockerSession
        from asb_swebench_task import SweBenchTask
        import docker.errors
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_GUEST_BINARY"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="official-real-guest-", dir=root))
        print(f"Retained real-agent guest qualification: {artifacts}", flush=True)
        cache = root / ".asb-cache/npm"
        self.assertTrue(cache.is_dir(), "prime Pi package cache using asb tool install pi")
        # A fixture may copy public artifacts, never a registry-auth cache.
        for entry in (cache / "_cacache/index-v5").rglob("*"):
            if entry.is_file():
                for line in entry.read_text().splitlines():
                    if "\t" in line:
                        metadata = json.loads(line.split("\t", 1)[1]).get("metadata") or {}
                        self.assertFalse({key.lower() for key in metadata.get("reqHeaders", {})} & {"authorization", "proxy-authorization"})
        with DockerSession() as client:
            with SweBenchTask(client, root / "workloads/swe-bench-lite/catalog.json", "astropy__astropy-12907", artifacts / "task") as task:
                container = task.workspace.container
                task.checked(["mkdir", "-p", "/opt/asb/bin", "/opt/asb-project/.asb", "/opt/asb-fixture"])
                upload(container, "/opt/asb/bin", [("asb", binary.read_bytes(), 0o755)])
                configuration = b"tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n"
                upload(container, "/opt/asb-project/.asb", [("project.toml", configuration, 0o600)])
                # Stage only public download caches, then run the real Rust
                # installers inside this image rather than copying a host venv.
                task.checked(["mkdir", "-p", "/opt/asb-project/.asb-tools/node/cache", "/opt/asb-project/.asb-cache"])
                node_archive = next((root / ".asb-tools/node/cache").glob("*.tar.xz"))
                upload(container, "/opt/asb-project/.asb-tools/node/cache", [(node_archive.name, node_archive.read_bytes(), 0o600)])
                archive = io.BytesIO()
                with tarfile.open(fileobj=archive, mode="w") as tar:
                    tar.add(cache, arcname="npm")
                self.assertTrue(container.put_archive("/opt/asb-project/.asb-cache", archive.getvalue()))
                for tool in ["node", "pi"]:
                    result = task.workspace.execute(["/bin/sh", "-c", f"cd /opt/asb-project && /opt/asb/bin/asb tool install {tool}"],
                        environment={"NPM_CONFIG_OFFLINE": "true"}, timeout=180, output_limit=1024 * 1024)
                    (artifacts / f"{tool}-install.stdout").write_bytes(result["stdout"])
                    (artifacts / f"{tool}-install.stderr").write_bytes(result["stderr"])
                    self.assertEqual(result["exit_code"], 0, result)
                upload(container, "/opt/asb-fixture", [("provider.py", (Path(__file__).parent / "guest_provider_fixture.py").read_bytes(), 0o600)])
                container.exec_run(["python3", "/opt/asb-fixture/provider.py", "/opt/asb-fixture"], detach=True)
                for _ in range(50):
                    port = container.exec_run(["cat", "/opt/asb-fixture/port"])
                    if port.exit_code == 0:
                        break
                    time.sleep(0.1)
                self.assertEqual(port.exit_code, 0)
                port = int(port.output.strip())
                # Only this test's container is disconnected. The provider
                # remains reachable on its own loopback interface.
                for network in list(container.attrs["NetworkSettings"]["Networks"]):
                    client.networks.get(network).disconnect(container)
                request = {"config": {"agent": "pi", "workload": "swe-bench-lite", "provider": "openrouter",
                    "model": "fixture-model", "workspace": "/testbed", "concurrency": 1},
                    "connection": {"interface": "open_ai_chat", "authentication": "bearer",
                        "base_url": f"http://127.0.0.1:{port}/v1", "credential": "fixture-token"},
                    "project_root": "/opt/asb-project", "runtime_dir": "/opt/asb-private", "run_dir": "/opt/asb-private/attempt",
                    "prompt": "For this offline qualification, create asb-real-agent.txt containing fixture using the shell tool, then finish."}
                seen = []
                result = task.workspace.execute(["/opt/asb/bin/asb", "__asb-agent"],
                    environment={"ASB_GUEST_AGENT_REQUEST": json.dumps(request)}, gate="application", timeout=60,
                    before_resume=lambda target: seen.append(target), output_limit=1024 * 1024)
                (artifacts / "agent.stdout").write_bytes(result["stdout"])
                (artifacts / "agent.stderr").write_bytes(result["stderr"])
                self.assertEqual(result["exit_code"], 0, result)
                self.assertEqual(len(seen), 1)
                self.assertEqual(task.checked(["cat", "asb-real-agent.txt"]).strip(), b"fixture")
                meta = json.loads(task.checked(["cat", "/opt/asb-private/attempt/agent-result.json"]))
                calls = json.loads(task.checked(["cat", "/opt/asb-fixture/calls.json"]))["calls"]
                self.assertGreaterEqual(calls, 2)
                self.assertEqual(meta["token_usage"]["input_tokens"], calls * 17)
                self.assertEqual(meta["token_usage"]["output_tokens"], calls * 5)
                self.assertEqual(meta["token_usage"]["total_tokens"], calls * 22)
                submission = task.capture_submission()
                self.assertIn("asb-real-agent.txt", (artifacts / "task" / submission["artifact"]).read_text())
                name = container.name
            with self.assertRaises(docker.errors.NotFound):
                client.containers.get(name)
        print("Real Pi adapter/tool execution, loopback provider, usage, patch capture and cleanup passed", flush=True)


if __name__ == "__main__":
    unittest.main()
