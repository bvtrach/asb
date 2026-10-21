"""Opt-in actual two-service material qualification, without agents/providers.

Uses an explicitly selected cached image. All fixtures are journal-owned;
successful fixtures are removed, failed fixtures stopped and preserved.
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
class EnvironmentQualification(unittest.TestCase):
    def test_two_service_rounds_normalize_only_generated_identity_and_bind_shared_storage(self):
        from docker.types import Mount
        from asb_docker import DockerSession
        from asb_material import capture_task
        from asb_resources import ResourceJournal, owned_resource
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="environment-material-",dir=root))
        print(f"Retained environment-material qualification: {artifacts}",flush=True)
        observations = []
        with DockerSession() as client:
            image = client.images.get(os.environ["ASB_TEST_MATERIAL_IMAGE"])
            for index,shared in enumerate([True,True,False]):
                directory = artifacts/f"attempt-{index+1}"
                directory.mkdir(mode=0o700)
                attempt = "asb-"+uuid.uuid4().hex
                journal = ResourceJournal(client,directory,attempt)
                containers,volumes = {},{}
                success = False
                try:
                    volume_roles = ["data"] if shared else ["left","right"]
                    config = {"services":{"main":{"container_name":attempt},
                        "database":{"container_name":attempt+"-database"}},
                        "volumes":{role:{"name":attempt+"-"+role} for role in volume_roles}}
                    journal.compose(config)
                    for role,item in config["volumes"].items():
                        volumes[role] = client.volumes.create(name=item["name"],labels=journal.labels)
                    for number,(role,item) in enumerate(sorted(config["services"].items())):
                        volume = volumes["data" if shared else volume_roles[number]]
                        containers[role] = client.containers.run(image.id,["sleep","infinity"],detach=True,
                            name=item["container_name"],labels=journal.labels,network_mode="none",
                            mem_limit="1g",pids_limit=64,mounts=[Mount("/data",volume.name,type="volume")])
                        containers[role].reload()
                    task = SimpleNamespace(client=client,journal=journal,artifacts=directory,cancelled=threading.Event(),
                        workspace=SimpleNamespace(container=containers["main"],workdir="/"))
                    def observe():
                        return capture_task(task,{"fixture":"two-service-network-none"},
                            binary=os.environ["ASB_TEST_BENCHMARK_ASB"],deadline=time.monotonic()+420)
                    first = observe()
                    observations.append(first)
                    if index == 0:
                        result = containers["main"].exec_run(["/bin/sh","-c","printf changed > /data/changed.txt"])
                        self.assertEqual(result.exit_code,0)
                        self.assertNotEqual(first,observe(),"shared service volume mutation must be visible")
                    elif index == 1:
                        self.assertEqual(first,observe(),"repeat observations must not restage instrumentation")
                    success = True
                finally:
                    for resource in containers.values():
                        declaration = {"kind":"containers","name":resource.name}
                        owned = owned_resource(client,declaration,journal.labels)
                        if owned is not None:
                            owned.stop(timeout=2)
                            if success:
                                owned.remove()
                                self.assertIsNone(owned_resource(client,declaration,journal.labels))
                    if success:
                        for resource in volumes.values():
                            declaration = {"kind":"volumes","name":resource.name}
                            owned = owned_resource(client,declaration,journal.labels)
                            if owned is not None:
                                owned.remove()
                                self.assertIsNone(owned_resource(client,declaration,journal.labels))
                    journal.close()
        self.assertEqual(observations[0],observations[1],"fresh generated names/IDs must normalize equally")
        self.assertNotEqual(observations[0],observations[2],"independent equal-content volumes must differ")
        (artifacts/"qualification.json").write_text(json.dumps({"observations":observations,
            "scope":"two-services-network-none-shared-versus-independent-volumes",
            "agent_started":False,"provider_called":False,"evaluator_started":False},sort_keys=True)+"\n")


if __name__ == "__main__":
    unittest.main()
