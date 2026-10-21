"""Actual-root composition seams; no Docker or provider execution."""
import hashlib
import io
from pathlib import Path
import sys
import threading
import tarfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
from asb_material import MaterialError, tar_digest
import asb_material_root as subject


class RootTests(unittest.TestCase):
    def setUp(self):
        # Ownership transport is tested independently; these are composition
        # fixtures with synthetic TARs, not real container-user qualification.
        owner = patch("asb_staging.configured_owner",return_value=(0,0))
        owner.start()
        self.addCleanup(owner.stop)
        identity = patch("asb_staging.prefix_identity",return_value={"fixture":"owned-prefix"})
        identity.start()
        self.addCleanup(identity.stop)
    def runtime(self, content=b"fixture", extra=False, mode=0o755):
        output = io.BytesIO()
        with tarfile.open(fileobj=output,mode="w") as archive:
            for name in ["asb-runtime","asb-runtime/bin","asb-runtime/lib"]:
                entry = tarfile.TarInfo(name)
                entry.type, entry.mode = tarfile.DIRTYPE, 0o700
                archive.addfile(entry)
            for name,body in [("asb",content)]+([("extra",b"unexpected")] if extra else []):
                entry = tarfile.TarInfo("asb-runtime/"+name)
                entry.mode, entry.size = mode,len(body)
                archive.addfile(entry,io.BytesIO(body))
        from benchmark_material import Chunks
        return Chunks(output.getvalue()), {"name":"asb-runtime"}

    def fixture(self):
        image = SimpleNamespace(reload=lambda:None, attrs={"Os":"linux","Architecture":"amd64",
            "RootFS":{"Layers":["sha256:"+"a"*64]}})
        container = SimpleNamespace(id="owned-fixture", reload=lambda:None, image=image,
            attrs={"Mounts":[],"Config":{}}, diff=lambda:[{"Path":"/outside/workdir","Kind":1},
                {"Path":"/deleted","Kind":2}])
        container.get_archive = lambda *_args,**_kwargs:self.runtime()
        return SimpleNamespace(workspace=SimpleNamespace(container=container,workdir="/task"),
            cancelled=threading.Event(), artifacts=Path("/not-used-in-pure-fixture"))

    def capture(self,task, observations=None, stage=None):
        observations = observations or [{"sha256":"b"*64,"entries":3,"bytes":7}]*2
        with patch("asb_docker.docker_command",return_value=["fixture-docker"]), \
             patch("asb_staging.upload") as upload, \
             patch("asb_runtime.bundle",return_value=([("asb",b"fixture",0o755)],
                 {"files":[{"sha256":hashlib.sha256(b"fixture").hexdigest()}]})), \
             patch.object(subject,"run",side_effect=stage) as run, \
             patch.object(subject,"observe",side_effect=observations) as observe:
            result = subject.capture(task, {"task":"fixture"}, [".git/index"],
                "/private/asb",time.monotonic()+3)
            return result, observe.call_args_list, run.call_count, upload.call_count

    def test_nonempty_and_deleted_diff_binds_complete_rounds_and_reuses_owned_staging(self):
        task = self.fixture()
        result, calls, staged, uploaded = self.capture(task)
        self.assertEqual(result["scope"],"prepared-workload-material-v1")
        self.assertEqual(len(calls),2)
        for call in calls:
            self.assertIn("--user",call.args[0])
            self.assertEqual(call.args[0][call.args[0].index("--user")+1],"0","only whole-root observations are elevated")
        roots = calls[0].args[1]
        self.assertEqual(roots[0]["path"],"/")
        self.assertEqual(roots[0]["content_paths"],["outside/workdir"])
        self.assertIn("task/.git/index",roots[0]["exclusions"])
        self.assertEqual(roots[1],{"name":"shared-memory","path":"/dev/shm","exclusions":[]})
        self.assertEqual((staged,uploaded),(12,1))
        repeated,_,staged,uploaded = self.capture(task)
        self.assertEqual(result,repeated)
        self.assertEqual((staged,uploaded),(10,0))
        task.workspace.container.diff = lambda:[{"Path":"/outside/workdir","Kind":1}]
        self.assertNotEqual(result,self.capture(task)[0],"deletion tombstones remain bound")

    def test_preparing_all_services_precedes_interleaved_whole_environment_rounds(self):
        events = []
        tasks = {role:self.fixture() for role in ["main","database"]}
        for role,task in tasks.items():
            task.workspace.container.id = "owned-"+role
        with patch("asb_docker.docker_command",return_value=["fixture-docker"]), \
             patch("asb_staging.upload",side_effect=lambda container,*args,**kwargs:events.append(("stage",container.id))), \
             patch("asb_runtime.bundle",return_value=([("asb",b"fixture",0o755)],
                 {"files":[{"sha256":hashlib.sha256(b"fixture").hexdigest()}]})), \
             patch.object(subject,"run"), \
             patch.object(subject,"observe",side_effect=lambda command,*args,**kwargs:
                 events.append(("sample",command[command.index("-i")+1])) or {"sha256":"b"*64,"entries":3,"bytes":7}):
            samplers = {role:subject.prepare(task,[],"/private/asb",time.monotonic()+3)
                for role,task in tasks.items()}
            self.assertEqual(events,[("stage","owned-main"),("stage","owned-database")])
            rounds = [{role:sample() for role,sample in samplers.items()} for _ in range(2)]
        self.assertEqual(rounds[0],rounds[1])
        self.assertEqual(events[2:],[('sample','owned-main'),('sample','owned-database')]*2)

    def test_root_workspace_is_supported_and_second_round_mutation_is_rejected(self):
        task = self.fixture()
        task.workspace.workdir = "/"
        _,calls,_,_ = self.capture(task)
        self.assertIn(".git/index",calls[0].args[1][0]["exclusions"])
        with self.assertRaisesRegex(MaterialError,"between complete root"):
            self.capture(self.fixture(),[{"sha256":"a"*64},{"sha256":"b"*64}])

    def test_bind_source_witness_is_sent_privately_to_each_guest_census(self):
        task = self.fixture()
        task.journal = SimpleNamespace()
        task.workspace.container.attrs["Mounts"] = [{"Type":"bind","Source":"/owned/input",
            "Destination":"/input","RW":True,"Mode":"","Propagation":"rprivate"}]
        declaration = {"/input":{"source":"/owned/input","chain":[[12,34,16384]]}}
        with patch("asb_material_binds.verify",return_value=declaration), \
             patch("asb_material_mounts.require_local_kernel") as local:
            result,calls,_,_ = self.capture(task)
        self.assertEqual(local.call_count,4)
        for call in calls:
            root = next(root for root in call.args[1] if root["path"] == "/input")
            self.assertEqual(root["expected_identity"],[12,34,16384])
        self.assertEqual(set(result),{"schema_version","scope","sha256"})

    def test_host_bind_views_require_current_guest_witnesses_and_exact_policy(self):
        task = self.fixture()
        task.journal = SimpleNamespace()
        task.workspace.container.attrs["Mounts"] = [{"Type":"bind","Source":"/owned/input",
            "Destination":"/task","RW":True,"Mode":"","Propagation":"rprivate"}]
        declaration = {"/task":{"source":"/owned/input","chain":[[12,34,16384]]}}
        with patch("asb_docker.docker_command",return_value=["fixture-docker"]), \
             patch("asb_staging.upload"), \
             patch("asb_runtime.bundle",return_value=([("asb",b"fixture",0o755)],
                 {"files":[{"sha256":hashlib.sha256(b"fixture").hexdigest()}]})), \
             patch("asb_material_binds.verify",return_value=declaration), \
             patch("asb_material_mounts.require_local_kernel") as local, \
             patch.object(subject,"run"),patch.object(subject,"observe",return_value={"sha256":"a"*64}) as observe:
            sampler = subject.prepare(task,[".git/index"],"/private/asb",time.monotonic()+3)
            with self.assertRaises(MaterialError):sampler.bind_views()
            observe.assert_not_called()
            observe.side_effect = MaterialError("guest source inode mismatch")
            with self.assertRaisesRegex(MaterialError,"guest source inode mismatch"):sampler()
            with self.assertRaisesRegex(MaterialError,"current guest inode coverage"):sampler.bind_views()
            observe.side_effect = None
            sampler()
            self.assertEqual(sampler.bind_views(),[{"name":"mount-001","source":"/owned/input",
                "identity":[12,34,16384],"exclusions":[".git/index"]}])
            observe.side_effect = MaterialError("later guest census failed")
            with self.assertRaisesRegex(MaterialError,"later guest census failed"):sampler()
            with self.assertRaisesRegex(MaterialError,"current guest inode coverage"):sampler.bind_views()
            observe.side_effect = None
            sampler()
            local.side_effect = MaterialError("remote daemon")
            with self.assertRaisesRegex(MaterialError,"remote daemon"):sampler.bind_views()
            local.side_effect = None
            with self.assertRaisesRegex(MaterialError,"current guest inode coverage"):sampler.bind_views()
            sampler()
            task.workspace.container.attrs["Mounts"][0]["RW"] = False
            with self.assertRaisesRegex(MaterialError,"current guest inode coverage"):sampler.bind_views()

    def test_inventory_change_and_unsupported_mounts_never_fall_back(self):
        task = self.fixture()
        counter = [0]
        def changes():
            counter[0] += 1
            return [{"Path":"/a" if counter[0] == 1 else "/b","Kind":1}]
        task.workspace.container.diff = changes
        with self.assertRaisesRegex(MaterialError,"change inventory changed"):
            self.capture(task)
        task = self.fixture()
        task.workspace.container.attrs["Mounts"] = [{"Destination":"/task"}]
        with patch.object(subject,"run") as run:
            with self.assertRaises(MaterialError):
                self.capture(task)
            run.assert_not_called()

    def test_staging_failure_does_not_upload_or_claim_a_helper(self):
        task = self.fixture()
        with patch("asb_staging.upload") as upload:
            with self.assertRaises(MaterialError):
                self.capture(task,stage=MaterialError("staging collision"))
            self.assertFalse(hasattr(task,"_material_runtime"))
            upload.assert_not_called()
        task._material_runtime = {"container":"another-container","binary":"/private/asb"}
        with self.assertRaisesRegex(MaterialError,"ownership changed"):
            self.capture(task)

    def test_actual_tmpfs_and_nested_mounts_join_the_same_root_observation(self):
        task = self.fixture()
        task.workspace.container.attrs["HostConfig"] = {"IpcMode":"private","ShmSize":67108864,
            "Tmpfs":{"/scratch":"size=1m","/scratch/child":"size=2m"}}
        first,calls,_,_ = self.capture(task)
        roots = calls[0].args[1]
        self.assertEqual([root["path"] for root in roots],["/","/scratch","/scratch/child","/dev/shm"])
        self.assertEqual(roots[1]["exclusions"],["child"])
        self.assertIn("scratch",roots[0]["exclusions"])
        task.workspace.container.attrs["HostConfig"]["Tmpfs"]["/scratch"] = "size=3m"
        self.assertNotEqual(first,self.capture(task)[0])

    def test_reuse_verifies_bytes_modes_and_no_extra_entries_before_execution(self):
        task = self.fixture()
        self.capture(task)
        for kwargs in [{"content":b"changed"}, {"extra":True}, {"mode":0o700}]:
            task.workspace.container.get_archive = lambda *_args, **_unused:self.runtime(**kwargs)
            with self.assertRaisesRegex(MaterialError,"exact owned declaration"):
                self.capture(task, observations=AssertionError("must not execute substituted helper"))

    def test_invalid_expected_tree_closes_archive_before_reading_payloads(self):
        for expected in [{}, {"../escape":{"kind":"5","mode":0o700}},
                         {"asb":{"kind":"file","mode":True,"bytes":1,"sha256":"a"*64}}]:
            chunks,_ = self.runtime()
            with self.assertRaises(MaterialError):
                tar_digest(chunks,"asb-runtime",expected_entries=expected)
            self.assertTrue(chunks.closed)
            self.assertEqual(chunks.reads,0)


if __name__ == "__main__":
    unittest.main()
