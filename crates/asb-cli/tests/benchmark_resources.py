"""Provider-free durable ownership and non-destructive recovery contracts."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0, str(SOURCE))
from asb_resources import RecoveryError, ResourceJournal, RetainedEnvironment, build_image_tag, quiesce

ATTEMPT = "asb-" + "a" * 32


class Missing(Exception):
    status_code = 404


class Collection:
    def __init__(self):
        self.values, self.lookups, self.removed = {}, [], []
    def get(self, name):
        self.lookups.append(name)
        if name not in self.values:
            raise Missing()
        return self.values[name]
    def remove(self,name,*,noprune):
        assert noprune is True
        self.removed.append(name)
        del self.values[name]


class Image:
    def __init__(self,name,labels):
        self.id = "sha256:" + "b" * 64
        self.attrs = {"Config":{"Labels":dict(labels)},"RepoTags":[name]}


class Container:
    def __init__(self, name, labels):
        self.name = name
        self.attrs = {"Config":{"Labels":dict(labels)}, "State":{"Running":True}}
        self.stops = 0
        self.starts = 0
        self.id = "c"*64
        self.attrs["Image"] = "sha256:"+"d"*64
        self.attrs["NetworkSettings"] = {"Networks":{}}
    def start(self):
        self.starts += 1
        self.attrs["State"]["Running"] = True
    def stop(self, timeout):
        assert timeout == 2
        self.stops += 1
        self.attrs["State"]["Running"] = False
    def reload(self):
        pass


def client():
    return SimpleNamespace(info=lambda: {"ID":"fixture-daemon"},
                           containers=Collection(), networks=Collection(), volumes=Collection(),images=Collection())


class ResourceTests(unittest.TestCase):
    def retained_environment(self):
        journal = self.journal()
        owned = Container(ATTEMPT,journal.declare("containers",ATTEMPT))
        self.client.containers.values[ATTEMPT] = owned
        journal.preserve(self.client)
        artifacts = self.root/"recovered-grading"
        artifacts.mkdir(mode=0o700)
        retained = SimpleNamespace(grading=True,owner_lock=journal.lock,root=self.root,
            child=lambda name:self.root/name,object=lambda name:json.loads((self.root/name).read_bytes()),
            identity={"attempt_id":ATTEMPT},submission={"container_id":owned.id},task={"image":{"image_id":owned.attrs["Image"]}})
        return retained,owned,artifacts

    def test_retained_environment_starts_exact_container_and_preserves_it_on_success_or_failure(self):
        for failure in [False,True]:
            # Each iteration has its own journal/attempt directory.
            if failure:
                self.temporary = tempfile.TemporaryDirectory()
                self.addCleanup(self.temporary.cleanup)
                self.root,self.client = Path(self.temporary.name),client()
            retained,owned,artifacts = self.retained_environment()
            try:
                with RetainedEnvironment(self.client,retained,artifacts,threading.Event()) as resumed:
                    self.assertIs(resumed.main,owned)
                    self.assertTrue(owned.attrs["State"]["Running"])
                    if failure:raise RuntimeError("SDK failed")
            except RuntimeError:
                self.assertTrue(failure)
            self.assertEqual(owned.starts,1)
            self.assertFalse(owned.attrs["State"]["Running"])
            self.assertIn(ATTEMPT,self.client.containers.values)
            self.assertEqual(json.loads((artifacts/"grading-cleanup.json").read_bytes())["resources"][0]["status"],"stopped-preserved")

    def test_retained_environment_rejects_missing_or_changed_resources_before_restart(self):
        retained,owned,artifacts = self.retained_environment()
        retained.submission["container_id"] = "wrong"
        with self.assertRaisesRegex(ValueError,"container or image"):
            with RetainedEnvironment(self.client,retained,artifacts,threading.Event()):pass
        self.assertEqual(owned.starts,0)
        retained.submission["container_id"] = owned.id
        owned.attrs["Config"]["Labels"]["asb.recovery"] = "wrong"
        with self.assertRaises(RecoveryError):
            with RetainedEnvironment(self.client,retained,artifacts,threading.Event()):pass
        self.assertEqual(owned.starts,0)

    def test_retained_environment_owner_cancellation_stops_only_its_original_container(self):
        retained,owned,artifacts = self.retained_environment()
        unrelated = Container("unrelated",{})
        self.client.containers.values["unrelated"] = unrelated
        cancelled = threading.Event()
        with RetainedEnvironment(self.client,retained,artifacts,cancelled):
            cancelled.set()
            deadline = time.monotonic()+2
            while owned.attrs["State"]["Running"] and time.monotonic()<deadline:time.sleep(.01)
            self.assertFalse(owned.attrs["State"]["Running"])
        self.assertEqual(unrelated.stops,0)
        self.assertEqual(unrelated.starts,0)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root, self.client = Path(self.temporary.name), client()

    def journal(self):
        journal = ResourceJournal(self.client, self.root, ATTEMPT)
        self.addCleanup(journal.close)
        return journal

    def test_intent_is_persisted_before_creation_and_is_idempotent(self):
        journal = self.journal()
        labels = journal.declare("containers", ATTEMPT)
        journal.declare("containers", ATTEMPT)
        record = json.loads((self.root / "resources.json").read_bytes())
        self.assertEqual(record["resources"], [{"kind":"containers", "name":ATTEMPT}])
        self.assertEqual(labels["asb.recovery"], record["token"])
        self.assertFalse(self.client.containers.lookups)
        for name in ["resources.lock", "resources.json"]:
            self.assertEqual((self.root / name).stat().st_mode & 0o777, 0o600)

    def test_active_owner_is_not_recovered_or_replaced(self):
        journal = self.journal()
        journal.declare("containers", ATTEMPT)
        with self.assertRaises(RecoveryError):
            quiesce(self.client, self.root)
        with self.assertRaises(RecoveryError):
            ResourceJournal(self.client, self.root, ATTEMPT)
        self.assertFalse(self.client.containers.lookups)

    def test_active_owner_can_preserve_without_releasing_lock_or_removing_resources(self):
        journal = self.journal()
        owned = Container(ATTEMPT, journal.declare("containers", ATTEMPT))
        self.client.containers.values[ATTEMPT] = owned
        tag = build_image_tag(ATTEMPT, "main")
        image = Image(tag, journal.declare("images", tag))
        self.client.images.values[tag] = image
        original = (self.root / "resources.json").read_bytes()
        real_stop = owned.stop
        def stop(timeout):
            with self.assertRaisesRegex(RecoveryError, "active owner"):
                quiesce(self.client, self.root)
            real_stop(timeout)
        owned.stop = stop
        result = journal.preserve(self.client)
        self.assertEqual([item["status"] for item in result["resources"]], ["stopped-preserved", "preserved"])
        self.assertEqual((self.root / "resources.json").read_bytes(), original)
        self.assertFalse(self.client.images.removed)
        self.assertIn(ATTEMPT, self.client.containers.values)
        record = json.loads((self.root / "interruption.json").read_bytes())
        self.assertEqual(record["attempt"], ATTEMPT)
        self.assertEqual(record["scope"], "preserved-incomplete-attempt-not-grade-not-retry")
        self.assertFalse((self.root / "evaluation.json").exists())
        journal.close()
        with self.assertRaisesRegex(RecoveryError, "active resource owner"):
            journal.preserve(self.client)

    def test_preservation_refuses_changed_journal_before_lookups_or_mutation(self):
        journal = self.journal()
        owned = Container(ATTEMPT, journal.declare("containers", ATTEMPT))
        self.client.containers.values[ATTEMPT] = owned
        path = self.root / "resources.json"
        record = json.loads(path.read_bytes())
        record["token"] = "b" * 32
        path.write_text(json.dumps(record))
        with self.assertRaisesRegex(RecoveryError, "changed"):
            journal.preserve(self.client)
        self.assertEqual(owned.stops, 0)
        self.assertFalse(self.client.containers.lookups)
        self.assertFalse((self.root / "interruption.json").exists())

    def test_unconfirmed_stop_cannot_publish_preserved_interruption(self):
        journal = self.journal()
        owned = Container(ATTEMPT, journal.declare("containers", ATTEMPT))
        self.client.containers.values[ATTEMPT] = owned
        # Simulate a daemon acknowledging stop but still reporting running.
        owned.stop = lambda timeout: None
        with self.assertRaisesRegex(RecoveryError, "stopped state"):
            journal.preserve(self.client)
        self.assertFalse((self.root / "interruption.json").exists())
        self.assertFalse((self.root / "resource-quiescence.json").exists())

    def test_quiescence_preserves_every_resource_and_ignores_unrelated_names(self):
        journal = self.journal()
        labels = journal.declare("containers", ATTEMPT)
        journal.declare("containers", ATTEMPT + "-missing")
        journal.declare("volumes", ATTEMPT + "-data")
        owned = Container(ATTEMPT, labels)
        unrelated = Container("unrelated", labels)
        self.client.containers.values = {ATTEMPT:owned, "unrelated":unrelated}
        volume = SimpleNamespace(name=ATTEMPT + "-data", attrs={"Labels":labels})
        self.client.volumes.values[volume.name] = volume
        journal.close()
        result = quiesce(self.client, self.root)
        self.assertEqual([entry["status"] for entry in result["resources"]],
                         ["stopped-preserved", "absent", "preserved"])
        self.assertEqual(owned.stops, 1)
        self.assertEqual(unrelated.stops, 0)
        self.assertNotIn("unrelated", self.client.containers.lookups)
        self.assertEqual(len(self.client.containers.values), 2)
        self.assertIn(volume.name, self.client.volumes.values)
        self.assertIn("not-a-retry-receipt", result["scope"])
        quiesce(self.client, self.root)

    def test_all_ownership_checks_precede_any_stop(self):
        journal = self.journal()
        owned = Container(ATTEMPT, journal.declare("containers", ATTEMPT))
        other = Container(ATTEMPT + "-other", journal.declare("containers", ATTEMPT + "-other"))
        other.attrs["Config"]["Labels"]["asb.recovery"] = "different-owner"
        self.client.containers.values = {owned.name:owned, other.name:other}
        journal.close()
        with self.assertRaises(RecoveryError):
            quiesce(self.client, self.root)
        self.assertEqual(owned.stops, 0)
        self.assertEqual(other.stops, 0)

    def test_different_daemon_is_refused_before_lookups(self):
        journal = self.journal()
        journal.declare("containers", ATTEMPT)
        journal.close()
        self.client.info = lambda: {"ID":"other-daemon"}
        with self.assertRaises(RecoveryError):
            quiesce(self.client, self.root)
        self.assertFalse(self.client.containers.lookups)

    def test_absent_resources_are_a_read_only_observation_not_new_evidence(self):
        journal = self.journal()
        journal.declare("containers", ATTEMPT)
        journal.close()
        previous = (self.root / "resources.json").read_bytes()
        result = quiesce(self.client, self.root)
        self.assertEqual(result["resources"][0]["status"], "absent")
        self.assertEqual((self.root / "resources.json").read_bytes(), previous)
        self.assertFalse((self.root / "resource-quiescence.json").exists())

    def test_unknown_duplicate_and_excessive_declarations_are_rejected(self):
        journal = self.journal()
        for kind, name in [("images", ATTEMPT), ("containers", "unrelated"),
                           ("containers", ATTEMPT + "/../host"),("prune",ATTEMPT),
                           ("images","../outside:" + ATTEMPT),("images","repo//other:" + ATTEMPT),
                           ("images","repo:other"),("images","repo:" + ATTEMPT + "/../other")]:
            with self.assertRaises(RecoveryError):
                journal.declare(kind, name)
        journal.declare("containers", ATTEMPT)
        journal.close()
        path = self.root / "resources.json"
        original = json.loads(path.read_bytes())
        for declarations in [[original["resources"][0]] * 2, [None], [original["resources"][0]] * 65]:
            record = {**original, "resources":declarations}
            path.write_text(json.dumps(record))
            with self.assertRaises(RecoveryError):
                quiesce(self.client, self.root)

    def test_existing_history_and_symlinks_are_never_overwritten(self):
        journal = self.journal()
        journal.close()
        previous = (self.root / "resources.json").read_bytes()
        with self.assertRaises(FileExistsError):
            ResourceJournal(self.client, self.root, ATTEMPT)
        self.assertEqual((self.root / "resources.json").read_bytes(), previous)
        (self.root / "resources.lock").unlink()
        (self.root / "resources.lock").symlink_to(self.root / "resources.json")
        with self.assertRaises(OSError):
            quiesce(self.client, self.root)
        self.assertEqual((self.root / "resources.json").read_bytes(), previous)

    def test_compose_declares_only_explicit_named_resources(self):
        journal = self.journal()
        config = {"services":{"main":{"container_name":ATTEMPT}},
                  "networks":{"default":{"name":ATTEMPT + "-default"}},
                  "volumes":{"data":{"name":ATTEMPT + "-data"}}}
        journal.compose(config)
        self.assertEqual(len(journal.record["resources"]), 3)
        self.assertEqual(config["volumes"]["data"]["labels"], journal.labels)
        with self.assertRaises(RecoveryError):
            journal.compose({"volumes":{"external":{"external":True}}})

    def test_compose_build_outputs_are_declared_and_labeled_before_creation(self):
        journal = self.journal()
        config = {"services":{
            "main":{"container_name":ATTEMPT,"image":"shared:original","build":{"context":"public","tags":["shared:other"]}},
            "secondary":{"container_name":ATTEMPT + "-secondary","build":"secondary-context"},
            "prebuilt":{"container_name":ATTEMPT + "-prebuilt","image":"upstream:prebuilt"}}}
        journal.compose(config)
        previous = (self.root / "resources.json").read_bytes()
        journal.compose(config)
        self.assertEqual((self.root / "resources.json").read_bytes(),previous)
        for name in ["main","secondary"]:
            service = config["services"][name]
            image = build_image_tag(ATTEMPT,name)
            self.assertEqual(service["image"],image)
            self.assertEqual(service["build"]["tags"],[image])
            self.assertEqual(service["build"]["labels"],journal.labels)
            self.assertIn({"kind":"images","name":image},json.loads(previous)["resources"])
        self.assertEqual(config["services"]["prebuilt"]["image"],"upstream:prebuilt")
        self.assertFalse(self.client.images.lookups)

    def test_recovery_preserves_exact_owned_image_identity_and_shared_aliases(self):
        journal = self.journal()
        tag = build_image_tag(ATTEMPT,"main")
        image = Image(tag,journal.declare("images",tag))
        image.attrs["RepoTags"].append("user:shared")
        self.client.images.values = {tag:image,"user:shared":image}
        journal.close()
        result = quiesce(self.client,self.root)
        self.assertEqual(result["resources"],[{"kind":"images","name":tag,"status":"preserved","image_id":image.id}])
        self.assertEqual(self.client.images.lookups,[tag])
        self.assertFalse(self.client.images.removed)
        self.assertEqual(self.client.images.values["user:shared"].id,image.id)

    def test_normal_image_cleanup_only_untags_declared_outputs_and_preserves_caches(self):
        journal = self.journal()
        tag = build_image_tag(ATTEMPT,"main")
        image = Image(tag,journal.declare("images",tag))
        image.attrs["RepoTags"].append("user:shared")
        self.client.images.values = {tag:image,"user:shared":image}
        result = journal.cleanup_images(self.client)
        self.assertEqual(result,[{"kind":"images","name":tag,"status":"removed","image_id":image.id}])
        self.assertEqual(self.client.images.removed,[tag])
        self.assertEqual(self.client.images.values,{"user:shared":image})
        self.assertEqual(journal.cleanup_images(self.client),[{"kind":"images","name":tag,"status":"absent"}])
        journal.close()
        with self.assertRaises(RecoveryError):
            journal.cleanup_images(self.client)

    def test_wrong_image_owner_or_tag_prevents_every_mutation(self):
        journal = self.journal()
        container = Container(ATTEMPT,journal.declare("containers",ATTEMPT))
        self.client.containers.values[ATTEMPT] = container
        tags = [build_image_tag(ATTEMPT,name) for name in ["main","other"]]
        for tag in tags:
            self.client.images.values[tag] = Image(tag,journal.declare("images",tag))
        image = self.client.images.values[tags[1]]
        labels = dict(image.attrs["Config"]["Labels"])
        for key,value in [("asb.recovery","another"),("asb.owner","another"),("asb.attempt","another")]:
            image.attrs["Config"]["Labels"] = dict(labels,**{key:value})
            with self.assertRaises(RecoveryError):
                journal.cleanup_images(self.client)
            self.assertFalse(self.client.images.removed)
        journal.close()
        with self.assertRaises(RecoveryError):
            quiesce(self.client,self.root)
        self.assertEqual(container.stops,0)
        image.attrs["Config"]["Labels"] = labels
        for tags_value, identity in [([],image.id),([tags[1]],"wrong-id")]:
            image.attrs["RepoTags"],image.id = tags_value,identity
            with self.assertRaises(RecoveryError):
                quiesce(self.client,self.root)
            self.assertEqual(container.stops,0)

    def test_absent_build_outputs_do_not_create_recovery_evidence_or_authorize_retry(self):
        journal = self.journal()
        journal.declare("images",build_image_tag(ATTEMPT,"main"))
        journal.close()
        result = quiesce(self.client,self.root)
        self.assertEqual(result["resources"][0]["status"],"absent")
        self.assertIn("not-a-retry-receipt",result["scope"])
        self.assertFalse((self.root / "resource-quiescence.json").exists())

    def test_world_readable_journals_and_nonregular_files_are_refused(self):
        journal = self.journal()
        journal.close()
        path = self.root / "resources.json"
        original = path.read_bytes()
        path.chmod(0o644)
        with self.assertRaises(RecoveryError):
            quiesce(self.client, self.root)
        path.unlink()
        outside = self.root / "outside"
        outside.write_bytes(original)
        path.symlink_to(outside)
        with self.assertRaises(OSError):
            quiesce(self.client, self.root)
        self.assertEqual(outside.read_bytes(), original)
        path.unlink()
        os.mkfifo(path, 0o600)
        with self.assertRaises(RecoveryError):
            quiesce(self.client, self.root)
        self.assertFalse(self.client.containers.lookups)

    def test_oversized_history_and_unconfirmed_stop_cannot_publish_success(self):
        journal = self.journal()
        owned = Container(ATTEMPT, journal.declare("containers", ATTEMPT))
        journal.close()
        path = self.root / "resources.json"
        original = path.read_bytes()
        path.write_bytes(b" " * (1024 * 1024 + 1))
        with self.assertRaises(RecoveryError):
            quiesce(self.client, self.root)
        self.assertFalse(self.client.containers.lookups)
        path.write_bytes(original)
        self.client.containers.values[ATTEMPT] = owned
        owned.stop = lambda timeout: None
        with self.assertRaises(RecoveryError):
            quiesce(self.client, self.root)
        self.assertFalse((self.root / "resource-quiescence.json").exists())

    def test_sigkill_releases_only_the_dead_controller_lock(self):
        code = """import sys,time
sys.path.insert(0,sys.argv[1])
from asb_resources import ResourceJournal
from types import SimpleNamespace
j=ResourceJournal(SimpleNamespace(info=lambda:{'ID':'fixture-daemon'}),sys.argv[2],sys.argv[3])
j.declare('containers',sys.argv[3])
print('ready',flush=True)
time.sleep(60)
"""
        process = subprocess.Popen([sys.executable,"-c",code,str(SOURCE),str(self.root),ATTEMPT],
                                   stdout=subprocess.PIPE, text=True,
                                   env={"PYTHONDONTWRITEBYTECODE":"1"})
        try:
            self.assertEqual(process.stdout.readline().strip(), "ready")
            with self.assertRaises(RecoveryError):
                quiesce(self.client, self.root)
            process.kill()
            process.wait(timeout=5)
            self.assertEqual(quiesce(self.client, self.root)["resources"][0]["status"], "absent")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            process.stdout.close()


if __name__ == "__main__":
    unittest.main()
