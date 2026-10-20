"""Provider-free durable ownership and non-destructive recovery contracts."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0, str(SOURCE))
from asb_resources import RecoveryError, ResourceJournal, quiesce

ATTEMPT = "asb-" + "a" * 32


class Missing(Exception):
    status_code = 404


class Collection:
    def __init__(self):
        self.values, self.lookups = {}, []
    def get(self, name):
        self.lookups.append(name)
        if name not in self.values:
            raise Missing()
        return self.values[name]


class Container:
    def __init__(self, name, labels):
        self.name = name
        self.attrs = {"Config":{"Labels":dict(labels)}, "State":{"Running":True}}
        self.stops = 0
    def stop(self, timeout):
        assert timeout == 2
        self.stops += 1
        self.attrs["State"]["Running"] = False
    def reload(self):
        pass


def client():
    return SimpleNamespace(info=lambda: {"ID":"fixture-daemon"},
                           containers=Collection(), networks=Collection(), volumes=Collection())


class ResourceTests(unittest.TestCase):
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
                           ("containers", ATTEMPT + "/../host")]:
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
