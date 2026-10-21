"""Provider-free SDK launch protocol fixtures; not an installed-SDK proof."""
import json
from pathlib import Path
import shlex
import subprocess
import sys
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
from benchmark_execution import Container as BaseContainer, API
from asb_execution import ExecutionError, ManagedDockerWorkspace
from asb_material import MaterialError
from asb_terminal_session import SDKSessionLauncher, _publish_receipt


class Container(BaseContainer):
    def __init__(self):
        super().__init__()
        self.attrs = {"Config":{"User":"worker","Labels":{"asb.owner":"asb","asb.attempt":"asb-"+"a"*32}}}
        self.image = SimpleNamespace(id="sha256:"+"f"*64,attrs={"Os":"linux","Architecture":"amd64"})
        self.frames,self.archives,self.nonce = {},[],None
        self.candidates = {"Titles":["PID"],"Processes":[["222"],["9001"]]}
    def exec_run(self,argv,user=None):
        result = SimpleNamespace(exit_code=0,output=b"")
        if argv[0] == "id":result.output = b"1001\n" if argv[1] == "-u" else b"1002\n"
        elif "asb-session" in argv:self.nonce = argv[-1].split("asb-session-")[-1]
        elif "asb-session-receipt" in argv:
            name = argv[-1].rsplit("/",1)[1]
            fields = {"started":"42","parent":"9","exit":"42 7","returned":"9"}
            result.output = self.frames.get(name,(self.nonce+" "+fields[name]+"\n").encode())
        elif argv[0] == "/opt/asb-runtime/asb":
            result.output = json.dumps({"boot_id":"01234567-89ab-cdef-0123-456789abcdef",
                "namespace_pid":42,"start_time_ticks":123}).encode()
        elif argv[0] != "timeout":return super().exec_run(argv)
        return result
    def put_archive(self,destination,source):
        with tarfile.open(fileobj=source,mode="r") as archive:
            for entry in archive.getmembers():
                self.archives.append((destination,entry.uid,entry.gid,entry.mode,archive.extractfile(entry).read()))
        return True
    def top(self,ps_args):
        assert ps_args == "-eo pid"
        return self.candidates


class SDKSessionTests(unittest.TestCase):
    def test_receipts_publish_complete_private_bytes_without_overwriting(self):
        with tempfile.TemporaryDirectory() as root:
            for name,variables,expected in [
                    ("parent",("asb_parent",),b"nonce 9\n"),
                    ("started",("asb_child_pid",),b"nonce 42\n"),
                    ("exit",("asb_child_pid","asb_child_status"),b"nonce 42 7\n"),
                    ("returned",("asb_parent",),b"nonce 9\n")]:
                with self.subTest(receipt=name):
                    final = Path(root)/name
                    temporary = Path(root)/(name+".tmp")
                    # Intercept exactly the publication boundary: final must
                    # still be absent and the staged file already complete.
                    script = ('asb_parent=9; asb_child_pid=42; asb_child_status=7; '
                        'ln() { test "$1" = -T && test "$2" = -- && '
                        'test ! -e "$4" && test "$(cat "$3")" = '+
                        shlex.quote(expected.decode().strip())+
                        ' && command ln "$@"; }; '+_publish_receipt(root,name,"nonce",*variables))
                    subprocess.run(["/bin/sh","-c",script],check=True,capture_output=True)
                    self.assertEqual(final.read_bytes(),expected)
                    self.assertEqual(final.stat().st_mode & 0o777,0o600)
                    # A second publication cannot replace either staged or
                    # published bytes, including an existing symlink target.
                    failed = subprocess.run(["/bin/sh","-c",script],capture_output=True)
                    self.assertNotEqual(failed.returncode,0)
                    self.assertEqual(final.read_bytes(),expected)
                    temporary.unlink()
                    sentinel = Path(root)/(name+"-sentinel")
                    sentinel.write_bytes(b"preserve")
                    temporary.symlink_to(sentinel)
                    failed = subprocess.run(["/bin/sh","-c",script],capture_output=True)
                    self.assertNotEqual(failed.returncode,0)
                    self.assertEqual(sentinel.read_bytes(),b"preserve")
                    self.assertEqual(final.read_bytes(),expected)

    def test_receipt_final_target_is_exact_and_never_followed(self):
        for kind in ["file","directory","dangling-symlink","directory-symlink"]:
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as root:
                final = Path(root)/"receipt"
                destination = Path(root)/"destination"
                if kind == "file":final.write_bytes(b"preserve")
                elif kind == "directory":final.mkdir()
                elif kind == "dangling-symlink":final.symlink_to(destination)
                else:
                    destination.mkdir()
                    final.symlink_to(destination,target_is_directory=True)
                before = final.lstat()
                result = subprocess.run(["/bin/sh","-c",
                    "asb_pid=42; "+_publish_receipt(root,"receipt","nonce","asb_pid")],capture_output=True)
                self.assertNotEqual(result.returncode,0)
                self.assertEqual(final.lstat().st_ino,before.st_ino)
                if kind == "file":self.assertEqual(final.read_bytes(),b"preserve")
                if kind == "directory":self.assertEqual(list(final.iterdir()),[])
                if kind == "directory-symlink":self.assertEqual(list(destination.iterdir()),[])
                if kind == "dangling-symlink":self.assertFalse(destination.exists())

    def fixture(self,require_host=True):
        temporary = tempfile.TemporaryDirectory();self.addCleanup(temporary.cleanup)
        container = Container();api = API(container)
        original = api.exec_inspect
        def inspect(exec_id):
            self.assertTrue(api.completed,"bridge inspection must not identify the client")
            return original(exec_id)
        api.exec_inspect = inspect
        client = SimpleNamespace(api=api)
        workspace = ManagedDockerWorkspace.attach(client,container,"asb-"+"a"*32,"/task")
        keys = []
        session = SimpleNamespace(container=container,_user="worker",is_session_alive=lambda:True,
            send_keys=lambda values,**kwargs:keys.append(values))
        launcher = SDKSessionLauncher(session,"/frozen/asb",Path(temporary.name),require_host_target=require_host)
        return workspace,container,launcher,keys

    def resolve(self,command,payload,**kwargs):
        self.assertEqual(command,["/frozen/asb","__asb-target","--resolve"])
        value = json.loads(payload)
        self.assertEqual(value.pop("candidates"),[222,9001])
        return json.dumps(dict(value,daemon_pid=222,pid_scope="docker-daemon-host")).encode()

    def test_private_launcher_preserves_user_and_never_types_credentials_into_sdk(self):
        workspace,container,launcher,keys = self.fixture()
        with patch("asb_material_exec.run",side_effect=self.resolve):
            result = workspace.execute(["agent","literal ; $value"],
                {"FIXTURE_KEY":"private-key-value"},transport_factory=launcher)
        self.assertEqual(result["exit_code"],7)
        self.assertEqual(result["target"]["daemon_pid"],222)
        self.assertEqual((result["stdout"],result["stderr"]),(b"stdout",b"stderr"))
        self.assertNotIn("private-key-value",str(keys))
        self.assertEqual(len(container.archives),1)
        _,uid,gid,mode,script = container.archives[0]
        self.assertEqual((uid,gid,mode),(1001,1002,0o600))
        self.assertIn(b"private-key-value",script)
        self.assertIn(b'if wait "$asb_child_pid"',script)
        self.assertTrue(script.endswith(b"exit 0\n"))
        self.assertTrue(launcher.transports[0].closed)
        launcher.close();workspace.close()

    def test_host_resolution_failure_is_explicit_and_never_uses_bridge_pid(self):
        for require_host in [False,True]:
            workspace,container,launcher,_ = self.fixture(require_host)
            with patch("asb_material_exec.run",side_effect=MaterialError("remote host")):
                if require_host:
                    with self.assertRaisesRegex(ExecutionError,"no verified local host PID"):
                        workspace.execute(["agent"],transport_factory=launcher)
                    self.assertFalse(container.resumed)
                else:
                    result = workspace.execute(["agent"],transport_factory=launcher)
                    self.assertEqual(result["target"]["pid_scope"],"container-namespace")
                    self.assertIsNone(result["target"]["daemon_pid"])
            launcher.close();workspace.close()

    def test_malformed_or_contradictory_resolver_output_never_resumes(self):
        for output in [b"not-json",b"null",b"[]",
                b'{"container_id":"foreign","exec_id":"bridge","namespace_pid":42,"daemon_pid":222,"pid_scope":"docker-daemon-host"}']:
            with self.subTest(output=output):
                workspace,container,launcher,_ = self.fixture()
                with patch("asb_material_exec.run",return_value=output),self.assertRaises(ExecutionError):
                    workspace.execute(["agent"],transport_factory=launcher)
                self.assertFalse(container.resumed)
                self.assertIn("kill",container.events)
                launcher.close();workspace.close()

    def test_bad_nonce_pid_or_parent_identity_cannot_admit_or_complete(self):
        for name,data in [("started",b"wrong 42\n"),("started",b"x"*129),
                ("parent",b"{nonce} 42\n"),("exit",b"{nonce} 43 7\n"),
                ("exit",b"{nonce} 42 256\n"),("returned",b"{nonce} 10\n")]:
            with self.subTest(name=name,data=data):
                workspace,container,launcher,_ = self.fixture()
                run = container.exec_run
                def read(argv,user=None):
                    result = run(argv,user)
                    if "asb-session-receipt" in argv and argv[-1].endswith("/"+name):
                        result.output = data.replace(b"{nonce}",container.nonce.encode())
                    return result
                container.exec_run = read
                with patch("asb_material_exec.run",side_effect=self.resolve),self.assertRaises(ExecutionError):
                    workspace.execute(["agent"],transport_factory=launcher)
                if name in ["started","parent"]:self.assertFalse(container.resumed)
                self.assertIn("kill",container.events)
                launcher.close();workspace.close()

    def test_partial_failed_sdk_launch_remains_owned_and_does_not_resume(self):
        workspace,container,launcher,_ = self.fixture()
        def fail(*args,**kwargs):raise ValueError("SDK launch failed")
        launcher.session.send_keys = fail
        with self.assertRaisesRegex(ExecutionError,"SDK launch worker failed"):
            workspace.execute(["agent"],transport_factory=launcher)
        self.assertFalse(container.resumed)
        self.assertEqual(len(launcher.transports),1)
        with self.assertRaises(ExecutionError):launcher.close()
        workspace.close()

    def test_failed_factory_construction_settles_registered_transport(self):
        workspace,container,launcher,_ = self.fixture()
        with patch("asb_terminal_session.SessionTransport.start",side_effect=ExecutionError("start failed")):
            with self.assertRaisesRegex(ExecutionError,"start failed"):
                workspace.execute(["agent"],transport_factory=launcher)
        self.assertFalse(container.resumed)
        self.assertEqual(len(launcher.transports),1)
        self.assertTrue(launcher.transports[0].closed)
        launcher.close();workspace.close()

    def test_bridge_construction_failure_cannot_claim_launched_child_settled(self):
        workspace,container,launcher,_ = self.fixture()
        with patch("asb_terminal_session.DockerExecTransport",side_effect=ExecutionError("bridge failed")):
            with self.assertRaisesRegex(ExecutionError,"settlement remains unconfirmed"):
                workspace.execute(["agent"],transport_factory=launcher)
        self.assertFalse(container.resumed)
        self.assertIn("kill",container.events)
        self.assertEqual(len(launcher.transports),1)
        with self.assertRaisesRegex(ExecutionError,"settlement remains unconfirmed"):launcher.close()
        workspace.close()

    def test_upload_rejects_invalid_private_owners_before_staging(self):
        from asb_guest import upload
        container = Container()
        for owner in [(True,1),(-1,1),(1,4294967296),(1,),[1,2],("1",2)]:
            with self.subTest(owner=owner),self.assertRaisesRegex(ExecutionError,"invalid private guest file owner"):
                upload(container,"/tmp",[("private",b"fixture",0o600)],owner=owner)
        self.assertEqual(container.archives,[])


if __name__ == "__main__":unittest.main()
