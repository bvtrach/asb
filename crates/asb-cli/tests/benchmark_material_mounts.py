"""Owned bind declarations and canonical mount topology, without Docker."""
import copy
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
from asb_material import MaterialError
import asb_material_binds as binds
import asb_material_mounts as subject
from asb_resources import ResourceJournal


RUNTIME = "/opt/asb-runtime"


def mounted(kind,target,source="",**values):
    return {"Type":kind,"Destination":target,"Source":source,"RW":True,"Mode":"","Propagation":"",**values}


def attributes(mounts,**host):
    return {"Mounts":mounts,"HostConfig":{"IpcMode":"private","ShmSize":67108864,**host}}


def canonical(value):
    return {key:item for key,item in value.items() if key != "raw_sha256"}


class MountTests(unittest.TestCase):
    def test_bind_aliases_parent_subdir_and_reordered_rebased_sources_are_canonical(self):
        def plan(root,reverse=False):
            points = [mounted("bind","/a",root+"/input"),mounted("bind","/b",root+"/input/sub")]
            declarations = {"/a":{"source":root+"/input","chain":[[1,2,16384]]},
                            "/b":{"source":root+"/input/sub","chain":[[1,3,16384]]}}
            return subject.plan(attributes(list(reversed(points)) if reverse else points),declarations,{},RUNTIME)
        first = plan("/owned/asb-first")
        second = plan("/owned/asb-second",True)
        self.assertEqual(canonical(first),canonical(second))
        self.assertNotEqual(first["raw_sha256"],second["raw_sha256"])
        self.assertEqual(first["aliases"],[{"parent":"mount-001","child":"mount-002","relative":"sub"}])
        self.assertNotIn("/owned/",json.dumps(canonical(first)))

    def test_same_inode_bind_sources_share_even_when_pathnames_differ(self):
        declarations = {target:{"source":"/owned/"+target[1:],"chain":[[1,9,16384]]} for target in ["/a","/b"]}
        points = [mounted("bind",target,item["source"]) for target,item in declarations.items()]
        first = subject.plan(attributes(points),declarations,{},RUNTIME)
        self.assertEqual(first["aliases"][0]["relative"],"")
        declarations["/b"]["chain"] = [[1,10,16384]]
        self.assertEqual(subject.plan(attributes(points),declarations,{},RUNTIME)["aliases"],[])

    def test_volume_subpaths_and_independent_storage_do_not_collapse(self):
        volumes = {name:{"Driver":"local","Scope":"local","Options":None} for name in ["owned-one","owned-two"]}
        points = [mounted("volume","/a","/daemon/one",Name="owned-one"),
                  mounted("volume","/b","/daemon/one",Name="owned-one")]
        host = {"Mounts":[{"Type":"volume","Source":"owned-one","Target":"/b","VolumeOptions":{"Subpath":"child","NoCopy":True}}]}
        first = subject.plan(attributes(points,**host),{},volumes,RUNTIME)
        self.assertEqual(first["aliases"][0]["relative"],"child")
        points[1]["Name"] = "owned-two"
        host["Mounts"][0]["Source"] = "owned-two"
        second = subject.plan(attributes(points,**host),{},volumes,RUNTIME)
        self.assertEqual(second["aliases"],[])
        self.assertNotEqual(canonical(first),canonical(second))
        host["Mounts"][0]["VolumeOptions"]["NoCopy"] = False
        self.assertNotEqual(canonical(second),canonical(subject.plan(attributes(points,**host),{},volumes,RUNTIME)))

    def test_tmpfs_nested_roots_private_shared_memory_and_access_are_bound(self):
        value = attributes([],Tmpfs={"/scratch":"size=1m","/scratch/child":"size=2m","/dev/shm":"size=3m"})
        first = subject.plan(value,{}, {},RUNTIME)
        self.assertEqual(sum(root["path"] == "/dev/shm" for root in first["roots"]),1)
        parent = next(root for root in first["roots"] if root["path"] == "/scratch")
        self.assertEqual(parent["exclusions"],["child"])
        value["HostConfig"]["Tmpfs"]["/scratch"] = "ro,size=1m"
        self.assertNotEqual(canonical(first),canonical(subject.plan(value,{}, {},RUNTIME)))
        for mode in ["host","shareable","container:external"]:
            value["HostConfig"]["IpcMode"] = mode
            with self.assertRaisesRegex(MaterialError,"IPC"):
                subject.plan(value,{}, {},RUNTIME)

    def test_disjoint_subpaths_keep_shared_backing_even_without_overlap(self):
        points = [mounted("volume","/a","/daemon/one",Name="owned-one"),
                  mounted("volume","/b","/daemon/one",Name="owned-one")]
        host = {"Mounts":[{"Type":"volume","Source":"owned-one","Target":target,
            "VolumeOptions":{"Subpath":path}} for target,path in [("/a","left"),("/b","right")]]}
        volumes = {name:{"Driver":"local","Scope":"local","Options":{}} for name in ["owned-one","owned-two"]}
        shared = subject.plan(attributes(points,**host),{},volumes,RUNTIME)
        self.assertEqual(shared["aliases"],[])
        self.assertEqual([mount["backing"] for mount in shared["mounts"]],["mount-001","mount-001"])
        points[1]["Name"] = "owned-two"
        host["Mounts"][1]["Source"] = "owned-two"
        independent = subject.plan(attributes(points,**host),{},volumes,RUNTIME)
        self.assertEqual(independent["aliases"],[])
        self.assertEqual([mount["backing"] for mount in independent["mounts"]],["mount-001","mount-002"])
        self.assertNotEqual(canonical(shared),canonical(independent))

    def test_structured_tmpfs_without_mountpoint_is_observed_and_options_are_bound(self):
        option = {"Type":"tmpfs","Target":"/scratch","ReadOnly":False,
            "TmpfsOptions":{"SizeBytes":1048576,"Mode":0o700,"Options":[["noexec"],["uid","1000"]]}}
        value = attributes([],Mounts=[option])
        first = subject.plan(value,{}, {},RUNTIME)
        self.assertIn("/scratch",[root["path"] for root in first["roots"]])
        self.assertEqual(first["mounts"][0]["kind"],"tmpfs")
        option["TmpfsOptions"]["SizeBytes"] *= 2
        self.assertNotEqual(canonical(first),canonical(subject.plan(value,{}, {},RUNTIME)))
        for invalid in [{"SizeBytes":True},{"SizeBytes":-1},{"Mode":0o10000},
                        {"Options":[["uid","1000","extra"]]},{"Options":[["bad\u001b"]]}, {"Unknown":1}]:
            option["TmpfsOptions"] = invalid
            with self.assertRaises(MaterialError):
                subject.plan(value,{}, {},RUNTIME)
        option["TmpfsOptions"] = {}
        value["HostConfig"]["Tmpfs"] = {"/scratch":"size=1m"}
        with self.assertRaisesRegex(MaterialError,"conflicting"):
            subject.plan(value,{}, {},RUNTIME)

    def test_invalid_sources_ownership_propagation_shadowing_and_bounds_are_rejected(self):
        for target in ["/","/opt","/opt/asb-runtime","/opt/asb-runtime/lib","/a/../b","//a"]:
            with self.assertRaises(MaterialError):
                subject.plan(attributes([mounted("tmpfs",target)]),{}, {},RUNTIME)
        for points in [[mounted("tmpfs","/a")]*2, [mounted("tmpfs",f"/a{i}") for i in range(33)],
                       [mounted("bind","/a","/unowned")], [mounted("volume","/a",Name="unowned")],
                       [mounted("tmpfs","/a",Propagation="rshared")]]:
            with self.assertRaises(MaterialError):
                subject.plan(attributes(points),{}, {},RUNTIME)
        with self.assertRaises(MaterialError):
            subject.plan(attributes([],Mounts=[{"Type":"volume","Target":"/missing"}]),{}, {},RUNTIME)

    def test_local_daemon_proof_requires_exact_container_cgroup_and_guest_kernel(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            process = proc/"4242"
            process.mkdir()
            boot = "11111111-1111-1111-1111-111111111111"
            (proc/"sys/kernel/random").mkdir(parents=True)
            (proc/"sys/kernel/random/boot_id").write_text(boot+"\n")
            (process/"status").write_text("NSpid:\t4242\t1\n")
            (process/"stat").write_text("4242 (fixture init) S "+"0 "*18+"42\n")
            identity = "a"*64
            (process/"cgroup").write_text("0::/docker/"+identity+"\n")
            value = {"Id":identity,"State":{"Pid":4242,"Running":True}}
            proof = json.dumps({"boot_id":boot,"namespace_pid":1234,"start_time_ticks":456}).encode()
            subject.require_local_kernel(value,proof,proc)
            for changed,namespace in [(value,b"other"),(value,proof.replace(boot.encode(),b"22222222-2222-2222-2222-222222222222")),
                                      ({**value,"Id":"b"*64},proof)]:
                with self.assertRaises(MaterialError):
                    subject.require_local_kernel(changed,namespace,proc)


class BindDeclarationTests(unittest.TestCase):
    def test_creation_receipt_verifies_exact_owned_chain_and_rejects_substitution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root/"input"
            source.mkdir()
            client = SimpleNamespace(info=lambda:{"ID":"fixture-daemon"})
            attempt = "asb-"+"a"*32
            journal = ResourceJournal(client,root,attempt)
            try:
                configuration = {"services":{"main":{"container_name":attempt,
                    "volumes":[{"type":"bind","source":str(source),"target":"/input"}]}}}
                journal.compose(configuration)
                self.assertTrue(journal.material_binds["available"])
                container = SimpleNamespace(name=attempt)
                self.assertEqual(binds.verify(journal,container)["/input"]["source"],str(source))
                original_receipt = (root/"material-binds.json").read_bytes()
                source.rename(root/"original")
                source.mkdir()
                journal.compose(configuration)
                self.assertEqual((root/"material-binds.json").read_bytes(),original_receipt,
                    "later Compose operations cannot authorize a replacement bind inode")
                with self.assertRaises(MaterialError):
                    binds.verify(journal,container)
                source.rmdir()
                source.symlink_to(root/"original",target_is_directory=True)
                with self.assertRaises(MaterialError):
                    binds.verify(journal,container)
            finally:
                journal.close()

    def test_unsupported_bind_declaration_is_unavailable_not_a_legacy_execution_gate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            client = SimpleNamespace(info=lambda:{"ID":"fixture-daemon"})
            attempt = "asb-"+"b"*32
            journal = ResourceJournal(client,root,attempt)
            try:
                configuration = {"services":{"main":{"container_name":attempt,
                    "volumes":[{"type":"bind","source":"/outside-owned-artifacts","target":"/input"}]}}}
                self.assertIs(journal.compose(configuration),configuration)
                self.assertFalse(journal.material_binds["available"])
                with self.assertRaisesRegex(MaterialError,"creation declaration"):
                    binds.verify(journal,SimpleNamespace(name=attempt))
            finally:
                journal.close()


if __name__ == "__main__":
    unittest.main()
