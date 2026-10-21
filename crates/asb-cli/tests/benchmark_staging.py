"""Owned staging contract fixtures; no Docker, installed client or provider."""
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
from types import SimpleNamespace
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
import asb_staging as subject
from asb_execution import ExecutionError
from benchmark_material import Chunks


class StagingTests(unittest.TestCase):
    def fixture(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        state = SimpleNamespace(body=b"fixture",uid=1001,gid=1002,files=[],calls=[],roots=set(),inode=17,fail_upload=False)
        container = SimpleNamespace(id="owned-container",attrs={"Config":{"User":"worker"}})
        def put_archive(destination,source):
            if state.fail_upload:return False
            with tarfile.open(fileobj=source,mode="r") as archive:
                state.files.extend((destination,item.name,item.uid,item.gid,item.mode,archive.extractfile(item).read())
                    for item in archive.getmembers())
            return True
        def archive(*_args,**_options):
            output = io.BytesIO()
            with tarfile.open(fileobj=output,mode="w") as stream:
                for name in ["asb-runtime","asb-runtime/bin","asb-runtime/lib","asb-runtime/asb"]:
                    item = tarfile.TarInfo(name)
                    item.uid,item.gid = state.uid,state.gid
                    if name.endswith("/asb"):
                        item.mode,item.size = 0o755,len(state.body)
                        stream.addfile(item,io.BytesIO(state.body))
                    else:
                        item.type,item.mode = tarfile.DIRTYPE,0o700
                        stream.addfile(item)
            return Chunks(output.getvalue()),dict(name="asb-runtime")
        container.put_archive,container.get_archive = put_archive,archive
        task = SimpleNamespace(artifacts=Path(temporary.name),attempt="owned-attempt",
            invocation=dict(identity=dict(workload="fixture",attempt_id="allocated-attempt")),
            workspace=SimpleNamespace(container=container))
        def checked(arguments):
            state.calls.append(("configured",arguments))
            if arguments[:1] == ["id"]:return b"1001\n" if arguments[1] == "-u" else b"1002\n"
            if "__asb-staging" in arguments:
                return json.dumps(dict(schema_version=1,scope="owned-asb-instrumentation-prefix-v1",path=arguments[-1],
                    parent_device=1,parent_inode=2,device=1,inode=state.inode,uid=state.uid,gid=state.gid,mode=0o700)).encode()
            return b""
        def root_checked(arguments):
            state.calls.append(("root",arguments))
            path = arguments[-3]
            if path in state.roots:raise ExecutionError("owned staging destination already exists")
            state.roots.add(path)
        return task,state,checked,root_checked

    def stage(self,task,checked,root_checked):
        import time
        return subject.stage_runtime(task,"/private/frozen-asb",[("asb",b"fixture",0o755)],
            checked,root_checked,deadline=time.monotonic()+5)

    def test_nonroot_owner_applies_to_every_uploaded_file_and_verified_reuse_is_read_only(self):
        task,state,checked,root_checked = self.fixture()
        self.assertEqual(self.stage(task,checked,root_checked),(1001,1002))
        self.assertTrue(all(item[2:4] == (1001,1002) for item in state.files))
        initial = list(state.files)
        self.stage(task,checked,root_checked)
        self.assertEqual(state.files,initial)
        elevated = [args for kind,args in state.calls if kind == "root"]
        self.assertEqual(len(elevated),1)
        self.assertEqual(elevated[0][-3:],[subject.RUNTIME,"1001","1002"])
        self.assertNotIn("-R",elevated[0])
        task.workspace.container.attrs["Config"]["User"] = "other"
        with self.assertRaisesRegex(ExecutionError,"user or container changed"):
            self.stage(task,checked,root_checked)

    def test_reuse_rejects_changed_payload_owner_and_replaced_directory(self):
        for fault in ["body","owner","inode"]:
            task,state,checked,root_checked = self.fixture()
            self.stage(task,checked,root_checked)
            if fault == "body":state.body = b"changed"
            elif fault == "owner":state.uid = 0
            else:state.inode += 1
            with self.subTest(fault=fault),self.assertRaises(ExecutionError):
                self.stage(task,checked,root_checked)
            self.assertEqual(len([1 for kind,_ in state.calls if kind == "root"]),1)

    def test_fresh_roots_bind_attempt_and_private_nonce_without_changing_configured_environment(self):
        task,state,checked,root_checked = self.fixture()
        owner = self.stage(task,checked,root_checked)
        identity,receipt = subject.prepare_guest_roots(task,owner,checked,root_checked)
        self.assertEqual(receipt["attempt"],identity["attempt_id"])
        self.assertEqual([item["path"] for item in receipt["prefixes"]],list(subject.PREFIXES))
        self.assertEqual(len(receipt["nonce"]),32)
        entry = [item for item in state.files if item[1] == ".asb-owner.json"][0]
        self.assertEqual(entry[2:5],(1001,1002,0o600))
        self.assertEqual(json.loads(entry[5]),receipt)
        with self.assertRaisesRegex(ExecutionError,"already exists"):
            subject.prepare_guest_roots(task,owner,checked,root_checked)
        for kind,args in state.calls:
            if kind == "root":self.assertIn(args[-3],subject.PREFIXES)
            self.assertNotIn("HOME",repr(args))

    def test_collisions_partial_uploads_and_invalid_identity_never_adopt_foreign_paths(self):
        task,state,checked,root_checked = self.fixture()
        state.fail_upload = True
        with self.assertRaises(ExecutionError):self.stage(task,checked,root_checked)
        self.assertFalse(hasattr(task,"_material_runtime"))
        self.assertIn(subject.RUNTIME,state.roots)
        with self.assertRaisesRegex(ExecutionError,"already exists"):
            self.stage(task,checked,root_checked)
        for path in ["/opt","/","/tmp/workload","/opt/asb-private/child"]:
            with self.assertRaises(ExecutionError):subject.create_prefix(root_checked,path,(1001,1002))
        task,_,_,_ = self.fixture()
        for value in [b"",b"-1\n",b"true\n",b"4294967296\n",None]:
            with self.assertRaises(ExecutionError):subject.configured_owner(task,lambda _:value)


if __name__ == "__main__":unittest.main()
