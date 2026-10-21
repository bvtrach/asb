"""Opt-in owned local bridge membership qualification, without agent traffic."""
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
    "ASB_TEST_SWECATALOG_ROOT"]),"requires explicit cached Docker fixture opt-in")
class NetworkQualification(unittest.TestCase):
    def test_owned_internal_bridge_observation_and_missing_endpoint_refusal(self):
        from asb_docker import DockerSession
        from asb_material import MaterialError, canonical_digest
        from asb_material_networks import observe
        from asb_resources import ResourceJournal, owned_resource
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="network-material-",dir=root))
        print(f"Retained network-material qualification: {artifacts}",flush=True)
        attempt = "asb-"+uuid.uuid4().hex
        containers,network,success = {},None,False
        with DockerSession() as client:
            image = client.images.get(os.environ["ASB_TEST_MATERIAL_IMAGE"])
            journal = ResourceJournal(client,artifacts,attempt)
            try:
                config = {"services":{role:{"container_name":attempt+"-"+role,
                    "networks":{"private":None}} for role in ["main","database"]},
                    "networks":{"private":{"name":attempt+"-private","driver":"bridge","internal":True}}}
                journal.compose(config)
                network = client.networks.create(config["networks"]["private"]["name"],
                    driver="bridge",internal=True,labels=journal.labels)
                for role,item in config["services"].items():
                    containers[role] = client.containers.run(image.id,["sleep","infinity"],detach=True,
                        name=item["container_name"],labels=journal.labels,network=network.name,
                        mem_limit="1g",pids_limit=64)
                task = SimpleNamespace(client=client,journal=journal,cancelled=threading.Event(),
                    workspace=SimpleNamespace(container=containers["main"],workdir="/"))
                first = observe(task,time.monotonic()+30)
                self.assertEqual(set(first["networks"]["private"]["endpoints"]),{"main","database"})
                self.assertEqual(first,observe(task,time.monotonic()+30))
                network.disconnect(containers["database"])
                with self.assertRaises(MaterialError):
                    observe(task,time.monotonic()+30)
                network.connect(containers["database"])
                restored = observe(task,time.monotonic()+30)
                self.assertNotEqual(first,restored,"replacement endpoint must change the private witness")
                (artifacts/"qualification.json").write_text(json.dumps({
                    "scope":"private-owned-internal-bridge-membership-only",
                    "first_sha256":canonical_digest(first,"qualification-v1"),
                    "restored_sha256":canonical_digest(restored,"qualification-v1"),
                    "missing_endpoint_refused":True,"agent_started":False,"provider_called":False,
                    "evaluator_started":False},sort_keys=True)+"\n")
                success = True
            finally:
                try:
                    for resource in containers.values():
                        declaration = {"kind":"containers","name":resource.name}
                        owned = owned_resource(client,declaration,journal.labels)
                        if owned is not None:
                            owned.stop(timeout=2)
                            if success:
                                owned.remove()
                                self.assertIsNone(owned_resource(client,declaration,journal.labels))
                    if success and network is not None:
                        declaration = {"kind":"networks","name":network.name}
                        owned = owned_resource(client,declaration,journal.labels)
                        if owned is not None:
                            owned.remove()
                            self.assertIsNone(owned_resource(client,declaration,journal.labels))
                finally:
                    journal.close()


if __name__ == "__main__":
    unittest.main()
