"""Pre-creation role receipts and exact owned service resolution, without Docker."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
from asb_material import MaterialError
import asb_material_services as subject
from asb_material_declarations import read
from asb_resources import ResourceJournal
from benchmark_resources import client,Container,ATTEMPT


class ServiceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root,self.client = Path(self.temporary.name),client()
        self.journal = ResourceJournal(self.client,self.root,ATTEMPT)
        self.addCleanup(self.journal.close)
        self.config = {"services":{
            "database":{"container_name":ATTEMPT+"-database","networks":{"private":{"aliases":["db"]}}},
            "client":{"container_name":ATTEMPT}},
            "networks":{"private":{"name":ATTEMPT+"-private"}},
            "volumes":{"data":{"name":ATTEMPT+"-data"}}}
        self.journal.compose(self.config)
        for index,service in enumerate(self.config["services"].values()):
            resource = Container(service["container_name"],self.journal.labels)
            resource.id = str(index+1)*64
            self.client.containers.values[resource.name] = resource
        self.primary = self.client.containers.values[ATTEMPT]
        self.task = SimpleNamespace(journal=self.journal,client=self.client,cancelled=threading.Event(),
            workspace=SimpleNamespace(container=self.primary))

    def resolve(self):
        return subject.resolve(self.task,time.monotonic()+3)

    def test_roles_are_durable_sorted_and_primary_is_actual_identity_not_role_guess(self):
        receipt,resources,primary = self.resolve()
        self.assertEqual(primary,"client")
        self.assertEqual(list(resources),["client","database"])
        self.assertEqual(self.client.containers.lookups,[ATTEMPT,ATTEMPT+"-database"])
        self.assertEqual(receipt["services"]["database"]["networks"],{"private":{"aliases":["db"]}})
        self.assertEqual(receipt["volumes"],{"data":{"name":ATTEMPT+"-data"}})
        reordered = copy.deepcopy(self.config)
        reordered["services"] = dict(reversed(list(reordered["services"].items())))
        self.assertEqual(receipt,subject.declare(reordered,self.journal.record))
        receipt["services"]["client"]["container"] = "mutated-copy"
        self.assertEqual(self.resolve()[2],"client","caller mutation cannot alter cached authority")

    def test_missing_stopped_duplicate_and_foreign_primary_are_refused(self):
        database = self.client.containers.values[ATTEMPT+"-database"]
        del self.client.containers.values[database.name]
        with self.assertRaisesRegex(MaterialError,"missing"):
            self.resolve()

        self.client.containers.values[database.name] = database
        for state in [{"Running":False},{"Running":True,"Paused":True},{"Running":True,"Restarting":True}]:
            database.attrs["State"] = state
            with self.assertRaisesRegex(MaterialError,"running state"):
                self.resolve()
        database.attrs["State"] = {"Running":True}
        original = database.id
        database.id = self.primary.id
        with self.assertRaisesRegex(MaterialError,"identity"):
            self.resolve()
        database.id = original
        self.task.workspace.container = SimpleNamespace(id="f"*64)
        with self.assertRaisesRegex(MaterialError,"primary"):
            self.resolve()

    def test_nested_intents_and_later_compose_calls_cannot_rewrite_creation_authority(self):
        receipt = read(self.journal,"material_services","material-services.json")
        original = (self.root/"material-services.json").read_bytes()
        original_binds = (self.root/"material-binds.json").read_bytes()
        self.config["services"]["database"]["networks"]["private"]["aliases"].append("changed")
        self.config["services"]["database"]["networks"]["private"]["ipv4_address"] = "192.0.2.9"
        self.assertEqual(receipt,self.journal.material_services)
        self.journal.compose(self.config)
        self.assertEqual(receipt,self.journal.material_services)
        self.assertEqual(original,(self.root/"material-services.json").read_bytes())
        self.assertEqual(original_binds,(self.root/"material-binds.json").read_bytes())

    def test_all_journal_slots_are_required_before_resource_lookup(self):
        self.journal.declare("containers",ATTEMPT+"-extra")
        with self.assertRaisesRegex(MaterialError,"complete owner declarations"):
            self.resolve()
        self.assertEqual(self.client.containers.lookups,[])

    def test_verified_capture_resolves_all_roles_and_dispatches_the_environment_composer(self):
        from asb_material import capture_task
        expected = {"schema_version":1,"scope":"prepared-workload-material-v1","sha256":"a"*64}
        with patch("asb_material_root.capture") as capture, \
             patch("asb_material_environment.capture",return_value=expected) as compose:
            self.assertEqual(capture_task(self.task,{"fixture":"task"},binary="/not-started"),expected)
            capture.assert_not_called()
            self.assertIs(compose.call_args.args[0],self.task)
        self.assertEqual(self.client.containers.lookups,[ATTEMPT,ATTEMPT+"-database"])

    def test_shared_reader_requires_active_owner_even_for_cached_receipts(self):
        self.journal.close()
        with self.assertRaisesRegex(MaterialError,"active owner"):
            read(self.journal,"material_services","material-services.json")

    def test_cancelled_expired_and_wrong_daemon_never_inspect_services(self):
        self.task.cancelled.set()
        with self.assertRaisesRegex(MaterialError,"cancelled"):
            self.resolve()
        self.task.cancelled.clear()
        with self.assertRaisesRegex(MaterialError,"cancelled"):
            subject.resolve(self.task,time.monotonic()-1)
        self.client.info = lambda:{"ID":"another-daemon"}
        with self.assertRaisesRegex(MaterialError,"daemon"):
            self.resolve()
        self.assertEqual(self.client.containers.lookups,[])

    def test_reload_cannot_change_owner_or_expose_raw_transport_errors(self):
        database = self.client.containers.values[ATTEMPT+"-database"]
        database.reload = lambda:database.attrs["Config"]["Labels"].clear()
        with self.assertRaisesRegex(MaterialError,"ownership changed"):
            self.resolve()
        database.attrs["Config"]["Labels"] = self.journal.labels
        def fail():
            raise RuntimeError("fixture-secret-transport")
        database.reload = fail
        with self.assertRaises(MaterialError) as error:
            self.resolve()
        self.assertNotIn("fixture-secret",str(error.exception))

    def test_unavailable_declarations_do_not_block_legacy_compose(self):
        for change in ["invalid role","duplicate container","too many services"]:
            value = copy.deepcopy(self.config)
            if change == "invalid role":
                value["services"]["bad/control\n"] = value["services"].pop("database")
            elif change == "duplicate container":
                value["services"]["database"]["container_name"] = ATTEMPT
            else:
                value["services"] = {str(index):{"container_name":ATTEMPT+"-"+str(index)} for index in range(17)}
            receipt = subject.declare(value,self.journal.record)
            self.assertFalse(receipt["available"])
        self.config["services"]["database"]["hostname"] = "unsupported\n"
        # Receipt availability is not an authorization to run or claim material.
        # Legacy Compose remains independent of later verified capture policy.
        self.assertIs(self.journal.compose(self.config),self.config)
        self.assertFalse(subject.declare(self.config,self.journal.record)["available"])
        self.assertTrue(self.journal.material_services["available"],"first declaration remains immutable")

    def test_receipt_corruption_symlink_and_owner_change_are_fixed_private_errors(self):
        path = self.root/"material-services.json"
        original = path.read_bytes()
        for content in [b"fixture-secret-invalid-json",b"x"*(1024**2+1),json.dumps({"available":True}).encode()]:
            path.write_bytes(content)
            with self.assertRaises(MaterialError) as error:
                read(self.journal,"material_services",path.name)
            self.assertNotIn("fixture-secret",str(error.exception))
        path.write_bytes(original)
        retained = self.root/"original-receipt.json"
        path.rename(retained)
        path.symlink_to(retained)
        with self.assertRaises(MaterialError):
            self.resolve()
        path.unlink()
        retained.rename(path)
        self.journal.record["token"] = "another-owner"
        with self.assertRaises(MaterialError):
            self.resolve()


if __name__ == "__main__":
    unittest.main()
