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
from asb_material import MaterialError
import asb_material_root as subject


class RootTests(unittest.TestCase):
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
             patch("asb_guest.upload") as upload, \
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
        roots = calls[0].args[1]
        self.assertEqual(roots[0]["path"],"/")
        self.assertEqual(roots[0]["content_paths"],["outside/workdir"])
        self.assertIn("task/.git/index",roots[0]["exclusions"])
        self.assertEqual(roots[1],{"name":"shared-memory","path":"/dev/shm"})
        self.assertEqual((staged,uploaded),(9,1))
        repeated,_,staged,uploaded = self.capture(task)
        self.assertEqual(result,repeated)
        self.assertEqual((staged,uploaded),(8,0))
        task.workspace.container.diff = lambda:[{"Path":"/outside/workdir","Kind":1}]
        self.assertNotEqual(result,self.capture(task)[0],"deletion tombstones remain bound")

    def test_root_workspace_is_supported_and_second_round_mutation_is_rejected(self):
        task = self.fixture()
        task.workspace.workdir = "/"
        _,calls,_,_ = self.capture(task)
        self.assertIn(".git/index",calls[0].args[1][0]["exclusions"])
        with self.assertRaisesRegex(MaterialError,"between complete root"):
            self.capture(self.fixture(),[{"sha256":"a"*64},{"sha256":"b"*64}])

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
            with self.assertRaisesRegex(MaterialError,"topology coverage"):
                self.capture(task)
            run.assert_not_called()

    def test_staging_failure_does_not_upload_or_claim_a_helper(self):
        task = self.fixture()
        with patch("asb_guest.upload") as upload:
            with self.assertRaises(MaterialError):
                self.capture(task,stage=MaterialError("staging collision"))
            self.assertFalse(hasattr(task,"_material_runtime"))
            upload.assert_not_called()
        task._material_runtime = {"container":"another-container","binary":"/private/asb"}
        with self.assertRaisesRegex(MaterialError,"ownership changed"):
            self.capture(task)

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
                subject.tar_digest(chunks,"asb-runtime",expected_entries=expected)
            self.assertTrue(chunks.closed)
            self.assertEqual(chunks.reads,0)


if __name__ == "__main__":
    unittest.main()
