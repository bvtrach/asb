"""Opt-in real Docker/guest-supervisor cleanup, no provider or real agent calls."""
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
                     "requires explicit Docker and installed SWE-bench fixture")
class QuiescenceQualification(unittest.TestCase):
    def test_guest_stops_and_reaps_agent_helpers_without_touching_namespace_peers(self):
        from asb_docker import DockerSession
        from asb_swebench_task import SweBenchTask
        from asb_guest import upload
        from asb_runtime import bundle
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="guest-quiescence-", dir=root))
        print(f"Retained Docker quiescence qualification: {artifacts}", flush=True)
        with DockerSession() as client:
            with SweBenchTask(client, root / "workloads/swe-bench-lite/catalog.json",
                              "astropy__astropy-12907", artifacts / "task") as task:
                task.checked(["mkdir", "-p", "/opt/asb-runtime/bin", "/opt/asb-runtime/lib",
                              "/opt/asb-project/.asb", "/opt/asb-project/.asb-tools/pi/bin"])
                files, _ = bundle(os.environ["ASB_TEST_BENCHMARK_ASB"], task.workspace.image_identity["architecture"])
                upload(task.workspace.container, "/opt/asb-runtime", files, temporary_dir=artifacts)
                upload(task.workspace.container, "/opt/asb-project/.asb", [("project.toml",
                    b"tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n", 0o600)], temporary_dir=artifacts)
                # This is a deliberately synthetic client, testing supervisor
                # semantics and not claiming to have solved an upstream task.
                fixture = b'''#!/usr/bin/env python3
import json,os,signal,sys,time
from pathlib import Path
first=os.fork()
if first==0:
    os.setsid()
    if os.fork()!=0:
        os._exit(0)
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    Path('helper.pid').write_text(str(os.getpid()))
    time.sleep(1.2)
    Path('late-write').write_text('submission changed after client exit')
    os._exit(0)
os.waitpid(first,0)
while not Path('helper.pid').exists():
    time.sleep(.005)
print(json.dumps(dict(type='result',result='fixture finished')),flush=True)
sys.exit(7)
'''
                upload(task.workspace.container, "/opt/asb-project/.asb-tools/pi/bin", [("pi", fixture, 0o755)], temporary_dir=artifacts)
                peer = client.api.exec_create(task.workspace.container.id, cmd=["/bin/sh", "-c",
                    'echo $$ > /tmp/asb-fixture-peer.pid; exec sleep 30'])["Id"]
                client.api.exec_start(peer, detach=True)
                request = {"config": {"agent": "pi", "provider": "openrouter", "model": "fixture",
                    "workload": "swe-bench-lite", "workspace": "/testbed", "concurrency": 1},
                    "connection": {"interface": "open_ai_chat", "authentication": "bearer",
                        "base_url": "http://127.0.0.1:1/v1", "credential": "dummy-not-used"},
                    "prompt": "offline supervision fixture", "project_root": "/opt/asb-project",
                    "runtime_dir": "/opt/asb-private", "run_dir": "/opt/asb-private/attempt"}
                result = task.workspace.execute(["/opt/asb-runtime/asb", "__asb-agent"], gate="application",
                    environment={"ASB_GUEST_AGENT_REQUEST": json.dumps(request)}, timeout=15)
                (artifacts / "driver.stdout").write_bytes(result["stdout"])
                (artifacts / "driver.stderr").write_bytes(result["stderr"])
                self.assertEqual(result["exit_code"], 7, result)
                metadata = json.loads(task.checked(["cat", "/opt/asb-private/attempt/agent-result.json"]))
                self.assertEqual(metadata["exit_code"], 7)
                self.assertTrue(metadata["tree_quiesced"])
                pid = task.checked(["cat", "helper.pid"]).strip().decode("ascii")
                self.assertTrue(pid.isdigit())
                task.checked(["/bin/sh", "-c", 'test ! -e "/proc/$1"', "asb-helper", pid])
                task.checked(["/bin/sh", "-c", 'kill -0 "$(cat /tmp/asb-fixture-peer.pid)"'])
                time.sleep(.8)
                task.checked(["/bin/sh", "-c", "test ! -e late-write"])
                submission = task.capture_submission()
                self.assertNotIn("late-write", (artifacts / "task" / submission["artifact"]).read_text())
                self.assertTrue(client.api.exec_inspect(peer)["Running"])
                # Missing supervisor evidence must stop the *old* owned PID
                # namespace, not merely signal the principal process. Preserve
                # its filesystem so partial submissions remain gradable.
                task.workspace.quiesce_unconfirmed_agent()
                self.assertFalse(client.api.exec_inspect(peer)["Running"])
                self.assertEqual(task.capture_submission()["patch_sha256"], submission["patch_sha256"])
            cleanup = json.loads((artifacts / "task/task-cleanup.json").read_text())
            self.assertEqual(cleanup["status"], "removed")


if __name__ == "__main__":
    unittest.main()
