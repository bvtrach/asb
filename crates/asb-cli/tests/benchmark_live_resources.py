"""Opt-in actual SIGKILL resource quiescence; no agent or LLM provider."""
import io
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tarfile
import tempfile
import unittest
import uuid

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0, str(SOURCE))


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT"), "requires explicit installed Docker task opt-in")
class LiveResourceRecovery(unittest.TestCase):
    def test_killed_owner_can_be_quiesced_without_losing_partial_work_or_touching_sibling(self):
        from asb_docker import DockerSession
        from asb_execution import ManagedDockerWorkspace
        from asb_resources import quiesce
        from asb_swebench import read_task
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="resource-recovery-", dir=root))
        print("Retained SIGKILL resource qualification: " + str(artifacts), flush=True)
        attempt = "asb-" + uuid.uuid4().hex
        _, row = read_task(root / "workloads/swe-bench-lite/catalog.json", "astropy__astropy-12907")
        code = """import json,sys,time
sys.path.insert(0,sys.argv[1])
from asb_docker import DockerSession
from asb_execution import ManagedDockerWorkspace
from asb_resources import ResourceJournal
with DockerSession() as client:
    journal=ResourceJournal(client,sys.argv[2],sys.argv[3])
    image=client.images.get(sys.argv[4])
    with ManagedDockerWorkspace(client,image,sys.argv[3],'/tmp',journal=journal) as workspace:
        result=workspace.container.exec_run(['/bin/sh','-c','printf partial-work > /tmp/asb-partial.txt'])
        assert result.exit_code==0
        tag='asb-recovery:'+sys.argv[3]
        labels=journal.declare('images',tag)
        image=workspace.container.commit(repository='asb-recovery',tag=sys.argv[3],conf={'Labels':labels})
        print(json.dumps({'id':workspace.container.id,'image_id':image.id}),flush=True)
        time.sleep(120)
"""
        with DockerSession() as client:
            image = client.images.get(row["image"])
            with ManagedDockerWorkspace(client, image, "asb-" + uuid.uuid4().hex, "/tmp") as sibling:
                process = subprocess.Popen([sys.executable,"-c",code,str(SOURCE),str(artifacts),attempt,image.id],
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                    env={**os.environ,"PYTHONDONTWRITEBYTECODE":"1"})
                owned = None
                tag = "asb-recovery:" + attempt
                try:
                    with selectors.DefaultSelector() as selector:
                        selector.register(process.stdout, selectors.EVENT_READ)
                        self.assertTrue(selector.select(timeout=40), "owned container readiness timed out")
                    line = process.stdout.readline()
                    self.assertTrue(line, "controller exited before readiness")
                    ready = json.loads(line)
                    owned = client.containers.get(ready["id"])
                    self.assertEqual(owned.name, attempt)
                    process.kill()
                    process.wait(timeout=5)
                    self.assertEqual(process.returncode, -signal.SIGKILL)
                    result = quiesce(client, artifacts)
                    self.assertEqual(result["resources"], [{"kind":"containers","name":attempt,
                                                            "status":"stopped-preserved"},
                        {"kind":"images","name":tag,"status":"preserved","image_id":ready["image_id"]}])
                    self.assertEqual(client.images.get(tag).id,ready["image_id"])
                    self.assertEqual(client.images.get(row["image"]).id,image.id,"shared original task image changed")
                    owned.reload()
                    self.assertFalse(owned.attrs["State"]["Running"])
                    sibling.container.reload()
                    self.assertTrue(sibling.container.attrs["State"]["Running"])
                    chunks, _ = owned.get_archive("/tmp/asb-partial.txt")
                    with tarfile.open(fileobj=io.BytesIO(b"".join(chunks))) as archive:
                        self.assertEqual(archive.extractfile("asb-partial.txt").read(), b"partial-work")
                    self.assertTrue((artifacts / "resources.json").is_file())
                    self.assertTrue((artifacts / "resource-quiescence.json").is_file())
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait(timeout=5)
                    process.stdout.close()
                    process.stderr.close()
                    if owned is None:
                        try:
                            owned = client.containers.get(attempt)
                        except Exception as error:
                            if getattr(error,"status_code",None) != 404:
                                raise
                    if owned is not None:
                        labels = owned.attrs.get("Config",{}).get("Labels",{}) or {}
                        if owned.name != attempt or labels.get("asb.attempt") != attempt:
                            raise AssertionError("fixture refused cleanup of a different owner")
                        owned.remove(force=True)
                    try:
                        committed = client.images.get(tag)
                    except Exception as error:
                        if getattr(error,"status_code",None) != 404:
                            raise
                    else:
                        journal = json.loads((artifacts / "resources.json").read_bytes())
                        labels = committed.attrs.get("Config",{}).get("Labels",{}) or {}
                        if (tag not in committed.tags or labels.get("asb.attempt") != attempt
                                or labels.get("asb.recovery") != journal["token"]):
                            raise AssertionError("fixture refused image cleanup of a different owner")
                        client.images.remove(tag,noprune=True)


if __name__ == "__main__":
    unittest.main()
