"""Opt-in real owned Docker transport lifecycle, no agents/providers/graders."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(all(os.environ.get(name) for name in
    ["ASB_TEST_MATERIAL_IMAGE","ASB_TEST_SWECATALOG_ROOT"]),"requires explicit cached image and artifact parent")
class TransportQualification(unittest.TestCase):
    def test_real_gate_output_exit_deadline_cancel_and_transport_settlement(self):
        import asb_execution
        from asb_docker import DockerSession
        from asb_resources import ResourceJournal, owned_resource
        source = Path(asb_execution.__file__)
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="execution-transport-",dir=root))
        print(f"Retained execution-transport qualification: {artifacts}",flush=True)
        with DockerSession() as client:
            image = client.images.get(os.environ["ASB_TEST_MATERIAL_IMAGE"])
            attempt = "asb-"+uuid.uuid4().hex
            journal = ResourceJournal(client,artifacts,attempt)
            workspace = asb_execution.ManagedDockerWorkspace(client,image,attempt,"/",journal=journal)
            success,targets = False,[]
            try:
                workspace.__enter__()
                def attach(target):
                    self.assertEqual(target["container_id"],workspace.container.id)
                    self.assertEqual(target["pid_scope"],"docker-daemon-host")
                    status = Path(f"/proc/{target['daemon_pid']}/status").read_text()
                    namespace = next(line.split()[1:] for line in status.splitlines() if line.startswith("NSpid:"))
                    self.assertEqual(namespace[0],str(target["daemon_pid"]))
                    self.assertEqual(namespace[-1],str(target["namespace_pid"]))
                    self.assertTrue(any(line.startswith("State:") and line.split()[1] == "T"
                        for line in status.splitlines()))
                    targets.append(target)
                result = workspace.execute(["/bin/sh","-c","printf stdout; printf stderr >&2; exit 7"],
                    before_resume=attach)
                self.assertEqual(result["exit_code"],7)
                self.assertEqual((result["stdout"],result["stderr"]),(b"stdout",b"stderr"))
                self.assertFalse(result["timed_out"] or result["cancelled"])
                result = workspace.execute(["/bin/sh","-c","sleep 30"],timeout=.1,
                    preparation_timeout=30,before_resume=attach)
                self.assertTrue(result["timed_out"])
                self.assertFalse(result["cancelled"])
                cancelled = threading.Event()
                timers = []
                def cancel_after_attach(target):
                    attach(target)
                    timer = threading.Timer(1,cancelled.set)
                    timers.append(timer);timer.start()
                try:
                    result = workspace.execute(["/bin/sh","-c","sleep 30"],timeout=60,
                        preparation_timeout=30,cancelled=cancelled,before_resume=cancel_after_attach)
                finally:
                    for timer in timers:
                        timer.cancel();timer.join(timeout=1)
                self.assertTrue(result["cancelled"])
                self.assertFalse(result["timed_out"])
                with self.assertRaises(asb_execution.ExecutionError):
                    workspace.execute(["/bin/sh","-c","printf 'invalid gate\\n'; sleep 30"],gate="application")
                result = workspace.execute(["/bin/sh","-c","printf recovered"],before_resume=attach)
                self.assertEqual(result["stdout"],b"recovered")
                self.assertEqual(result["exit_code"],0)
                self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(),digest)
                success = True
            finally:
                if success:
                    workspace.close()
                    self.assertEqual(workspace.cleanup_status,"removed")
                    self.assertIsNone(owned_resource(client,{"kind":"containers","name":attempt},journal.labels))
                else:
                    resource = owned_resource(client,{"kind":"containers","name":attempt},journal.labels)
                    if resource is not None:
                        resource.stop(timeout=2);resource.reload()
                        self.assertFalse(resource.attrs["State"]["Running"])
                journal.close()
        (artifacts/"qualification.json").write_text(json.dumps({"source_sha256":digest,
            "scope":"real-owned-docker-exec-transport-not-sdk-shell","targets":targets,
            "cleanup":"removed","agent_started":False,"provider_called":False,"grader_started":False},sort_keys=True)+"\n")


if __name__ == "__main__":unittest.main()
