"""Whole-environment ordering, exact runtime semantics and global storage tests."""
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
from asb_material_exec import ObservationBudget
from asb_resources import ResourceJournal
import asb_material_runtime as runtime
import asb_material_environment as subject
from benchmark_resources import client,Container


def files(hostname="fixture",name="fixture-container"):
    return [{"name":key,"text":text,"mode":0o644,"uid":0,"gid":0,
        "inode_group":key,"attributes_sha256":"a"*64} for key,text in [
        ("hosts",f"127.0.0.1 localhost\n127.0.1.1 {hostname} {name}\n"),
        ("hostname",hostname+"\n"),("resolv.conf","nameserver 127.0.0.53\n")]]


class RuntimeTests(unittest.TestCase):
    def test_only_exact_generated_tokens_are_normalized_and_comments_literals_remain(self):
        def observe(identity,name,explicit=None,comment=""):
            attrs = {"Id":identity,"Config":{"Hostname":identity[:12]},"HostConfig":{}}
            raw = files(identity[:12],name)
            raw[0]["text"] += comment
            return runtime.normalize(raw,"main",attrs,{"hostname":explicit},{name:"main"})
        a,b = "1"*64,"2"*64
        self.assertEqual(observe(a,"owned-first"),observe(b,"owned-second"))
        self.assertNotEqual(observe(a,"owned-first",a[:12]),observe(b,"owned-second",b[:12]))
        self.assertNotEqual(observe(a,"owned-first",comment="# "+a[:12]+"\n"),
            observe(b,"owned-second",comment="# "+b[:12]+"\n"))
        literal = files(a[:12],"literal-generated_hostname:main")
        attrs = {"Id":a,"Config":{"Hostname":a[:12]},"HostConfig":{}}
        self.assertNotEqual(runtime.normalize(literal,"main",attrs,{"hostname":None},{}),observe(a,"owned-first"))
        original = files(a[:12],"owned-first")
        for key in ["text","mode","attributes_sha256"]:
            changed = copy.deepcopy(original)
            changed[2][key] = {"text":"nameserver 192.0.2.1\n","mode":0o600,"attributes_sha256":"b"*64}[key]
            self.assertNotEqual(runtime.normalize(original,"main",attrs,{"hostname":None},{}),
                runtime.normalize(changed,"main",attrs,{"hostname":None},{}))

    def test_private_runtime_file_transport_debits_shared_budget_and_rejects_invalid_dtos(self):
        raw = files()
        usage = {"entries":3,"names":39,"bytes":sum(len(item["text"]) for item in raw),"attributes":0}
        budget = ObservationBudget({key:value*2 for key,value in usage.items()})
        with patch.object(runtime,"run",return_value=json.dumps({"files":raw,"usage":usage}).encode()) as run:
            for _ in range(2):
                self.assertEqual(runtime.read_files(["fixture"],budget,deadline=time.monotonic()+3,cancelled=None),raw)
            self.assertEqual(run.call_args.kwargs["output_limit"],65536)
            with self.assertRaisesRegex(MaterialError,"exhausted"):
                runtime.read_files(["fixture"],budget,deadline=time.monotonic()+3,cancelled=None)
        for changed in [{"files":raw[:2],"usage":usage},{"files":raw,"usage":{**usage,"names":38}},
                        {"files":raw,"usage":{**usage,"attributes":65537}}]:
            with patch.object(runtime,"run",return_value=json.dumps(changed).encode()):
                with self.assertRaises(MaterialError):
                    runtime.read_files(["fixture"],ObservationBudget(),deadline=time.monotonic()+3,cancelled=None)


class EnvironmentTests(unittest.TestCase):
    def fixture(self,letter="a",volumes=None):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        owner = "asb-"+letter*32
        api = client()
        journal = ResourceJournal(api,root,owner)
        self.addCleanup(journal.close)
        config = {"services":{role:{"container_name":owner+"-"+role} for role in ["main","database"]}}
        if volumes:
            config["volumes"] = {role:{"name":owner+"-"+role} for role in (["data"] if volumes == "shared" else ["left","right"])}
        journal.compose(config)
        for index,(role,item) in enumerate(sorted(config["services"].items())):
            resource = Container(item["container_name"],journal.labels)
            resource.id = str(index+1 if letter == "a" else index+3)*64
            resource.attrs.update({"Id":resource.id,"HostConfig":{"NetworkMode":"none","IpcMode":"private"},
                "Mounts":[],"NetworkSettings":{"Networks":{"none":{}}}})
            resource.attrs["Config"].update(Hostname=resource.id[:12],Domainname="",WorkingDir="/",Cmd=["sleep","infinity"])
            resource.attrs["State"]["Pid"] = 1000+index
            if volumes:
                volume_role = "data" if volumes == "shared" else ["left","right"][index]
                name = config["volumes"][volume_role]["name"]
                api.volumes.values[name] = SimpleNamespace(name=name,attrs={"Labels":journal.labels,"Driver":"local","Scope":"local","Options":{}})
                resource.attrs["Mounts"] = [{"Type":"volume","Name":name,"Source":"/private/"+name,
                    "Destination":"/data","Driver":"local","RW":True,"Mode":"","Propagation":""}]
            api.containers.values[resource.name] = resource
        main = api.containers.values[owner+"-main"]
        return SimpleNamespace(journal=journal,client=api,cancelled=threading.Event(),artifacts=root,
            workspace=SimpleNamespace(container=main,workdir="/"))

    def capture(self,task,mutate=None):
        events = []
        roles = {item["container"]:role for role,item in task.journal.material_services["services"].items()}
        identities = {resource.id:roles[name] for name,resource in task.client.containers.values.items()}
        budgets = []
        def prepare(context,*args):
            role = identities[context.workspace.container.id]
            events.append(("prepare",role))
            def sample(budget):
                events.append(("sample",role)); budgets.append(budget)
                usage = {"entries":1,"names":1,"bytes":0,"attributes":0}
                budget.consume(usage,budget.allowance())
                if mutate:
                    mutate(task,role,events)
                return {"files":{"sha256":"f"*64,"entries":1,"bytes":0}}
            return sample
        def read(command,budget,**kwargs):
            identity = command[3]
            role = identities[identity]
            events.append(("runtime",role)); budgets.append(budget)
            resource = next(item for item in task.client.containers.values.values() if item.id == identity)
            result = files(resource.attrs["Config"]["Hostname"],resource.name)
            budget.consume({"entries":3,"names":39,"bytes":sum(len(item["text"]) for item in result),"attributes":0},budget.allowance())
            return result
        with patch.object(subject,"prepare",side_effect=prepare),patch.object(subject,"read_files",side_effect=read), \
             patch("asb_docker.docker_command",return_value=["fixture-docker"]):
            result = subject.capture(task,{"task":"fixture"},[],"/not-run",time.monotonic()+3)
        self.assertEqual(len({id(budget) for budget in budgets}),1)
        return result,events

    def test_whole_rounds_follow_all_preparation_and_rebased_ids_names_are_canonical(self):
        first,events = self.capture(self.fixture())
        second,_ = self.capture(self.fixture("b"))
        self.assertEqual(first,second)
        self.assertEqual(events[:2],[("prepare","database"),("prepare","main")])
        self.assertEqual([item for item in events if item[0] == "sample"],
            [("sample","database"),("sample","main")]*2)

    def test_cross_service_shared_storage_is_not_independent_equal_content(self):
        shared,_ = self.capture(self.fixture(volumes="shared"))
        independent,_ = self.capture(self.fixture("b",volumes="independent"))
        self.assertNotEqual(shared,independent)

    def test_runtime_mutation_and_owner_replacement_cannot_be_hidden_by_normalization(self):
        def mutate(task,role,events):
            if role == "main":
                task.workspace.container.attrs["Config"]["Cmd"] = ["changed"]
        with self.assertRaisesRegex(MaterialError,"complete service round"):
            self.capture(self.fixture(),mutate)
        def replace(task,role,events):
            if role == "main":
                task.workspace.container.id = "e"*64
                task.workspace.container.attrs["Id"] = "e"*64
        with self.assertRaises(MaterialError):
            self.capture(self.fixture("b"),replace)

    def test_external_network_namespaces_and_devices_fail_before_staging(self):
        for key,value in [("NetworkMode","bridge"),("PidMode","host"),("CgroupnsMode","host"),
                          ("Privileged",True),("VolumesFrom",["external"]),("DeviceRequests",[{}])]:
            task = self.fixture()
            task.workspace.container.attrs["HostConfig"][key] = value
            with patch.object(subject,"prepare") as prepare:
                with self.assertRaises(MaterialError):
                    subject.capture(task,{},[],"/not-run",time.monotonic()+3)
                prepare.assert_not_called()


if __name__ == "__main__":
    unittest.main()
