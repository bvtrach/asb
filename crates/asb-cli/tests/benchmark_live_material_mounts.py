"""Opt-in actual mounted-root qualification; no provider, agent or evaluator.

Requires an explicit cached Linux image, built ASB and artifact parent. Creates
only journal-declared fixture resources; failed fixtures are stopped/preserved.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(all(os.environ.get(name) for name in ["ASB_TEST_MATERIAL_IMAGE",
    "ASB_TEST_BENCHMARK_ASB","ASB_TEST_SWECATALOG_ROOT"]),"requires explicit cached Docker fixture opt-in")
class MountedMaterialQualification(unittest.TestCase):
    def test_owned_bind_nested_tmpfs_and_volume_observation(self):
        from docker.types import Mount
        from asb_docker import DockerSession
        from asb_material_root import capture
        from asb_resources import ResourceJournal, owned_resource
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="mounted-material-",dir=root))
        print(f"Retained mounted-material qualification: {artifacts}",flush=True)
        observations = []
        with DockerSession() as client:
            image = client.images.get(os.environ["ASB_TEST_MATERIAL_IMAGE"])
            for index in range(2):
                directory = artifacts/f"attempt-{index+1}"
                directory.mkdir(mode=0o700)
                source = directory/"input"
                source.mkdir()
                (source/"input.txt").write_bytes(b"fixture-input\n")
                (source/"child").mkdir()
                (source/"child/nested.txt").write_bytes(b"nested-input\n")
                attempt = "asb-"+uuid.uuid4().hex
                journal = ResourceJournal(client,directory,attempt)
                container = volume = None
                success = False
                try:
                    journal.compose({"services":{"main":{"container_name":attempt,"volumes":[
                        {"type":"bind","source":str(source),"target":"/fixture-input"},
                        {"type":"bind","source":str(source/"child"),"target":"/fixture-alias"}]}}})
                    volume_name = attempt+"-data"
                    volume = client.volumes.create(name=volume_name,labels=journal.declare("volumes",volume_name))
                    container = client.containers.run(image.id,["sleep","infinity"],detach=True,
                        name=attempt,labels=journal.labels,network_mode="none",mem_limit="1g",pids_limit=64,
                        mounts=[Mount("/fixture-input",str(source),type="bind"),
                            Mount("/fixture-alias",str(source/"child"),type="bind"),
                            Mount("/fixture-data",volume_name,type="volume"),
                            Mount("/fixture-input/scratch",None,type="tmpfs",tmpfs_size=1048576)],
                        tmpfs={"/fixture-tmp":"size=1m"})
                    container.reload()
                    task = SimpleNamespace(client=client,journal=journal,artifacts=directory,
                        cancelled=threading.Event(),workspace=SimpleNamespace(container=container,workdir="/"))
                    def observe():
                        return capture(task,{"fixture":"owned-mounted-material"},[],
                            os.environ["ASB_TEST_BENCHMARK_ASB"],time.monotonic()+420)
                    first = observe()
                    observations.append(first)
                    self.assertEqual(first,observe())
                    if index == 0:
                        result = container.exec_run(["/bin/sh","-c","printf changed > /fixture-data/changed.txt"])
                        self.assertEqual(result.exit_code,0)
                        self.assertNotEqual(first,observe(),"mounted volume changes must affect material")
                    success = True
                finally:
                    if container is not None:
                        owned = owned_resource(client,{"kind":"containers","name":attempt},journal.labels)
                        if owned is not None:
                            owned.stop(timeout=2)
                            if success:
                                owned.remove()
                    if success and volume is not None:
                        owned = owned_resource(client,{"kind":"volumes","name":volume_name},journal.labels)
                        if owned is not None:
                            owned.remove()
                    journal.close()
            self.assertEqual(observations[0],observations[1],"fresh mount sources must canonicalize equally")
        (artifacts/"qualification.json").write_text(json.dumps({"observations":observations,
            "scope":"single-container-bind-volume-nested-tmpfs-not-multiservice",
            "provider_called":False,"agent_started":False,"evaluator_started":False},sort_keys=True)+"\n")


if __name__ == "__main__":
    unittest.main()
