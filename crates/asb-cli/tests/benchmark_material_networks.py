"""Exact network membership tests; no Docker, provider, agent or credentials."""
from pathlib import Path
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
from asb_material import MaterialError
import asb_material_networks as subject
from asb_resources import ResourceJournal
from benchmark_resources import client,Container,ATTEMPT


class NetworkTests(unittest.TestCase):
    def fixture(self,implicit=False):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        api = client()
        journal = ResourceJournal(api,Path(temporary.name),ATTEMPT)
        self.addCleanup(journal.close)
        role = "default" if implicit else "private"
        name,identity = ATTEMPT+"-net","a"*64
        config = {"services":{key:{"container_name":ATTEMPT+"-"+key,
            **({} if implicit else {"networks":{role:{"aliases":[key]}}})} for key in ["main","database"]},
            "networks":{role:{"name":name,"driver":"bridge"}}}
        journal.compose(config)
        members = {}
        for index,(key,item) in enumerate(config["services"].items()):
            resource = Container(item["container_name"],journal.labels)
            resource.id = str(index+1)*64
            endpoint = str(index+3)*64
            address = "192.0.2."+str(index+2)
            mac = "02:42:c0:00:02:0"+str(index+2)
            resource.attrs["HostConfig"] = {"NetworkMode":name}
            resource.attrs["NetworkSettings"] = {"Networks":{name:{"NetworkID":identity,"EndpointID":endpoint,
                "IPAddress":address,"IPPrefixLen":24,"GlobalIPv6Address":"","MacAddress":mac,
                "Aliases":[key,item["container_name"]],"IPAMConfig":None}}}
            members[resource.id] = {"Name":item["container_name"],"EndpointID":endpoint,
                "IPv4Address":address+"/24","IPv6Address":"","MacAddress":mac}
            api.containers.values[resource.name] = resource
        network = SimpleNamespace(id=identity,name=name,reload=lambda:None,attrs={
            "Id":identity,"Name":name,"Labels":journal.labels,"Scope":"local","Driver":"bridge",
            "Containers":members,"IPAM":{"Driver":"default","Config":[{"Subnet":"192.0.2.0/24","Gateway":"192.0.2.1"}]}})
        api.networks.values[name] = network
        task = SimpleNamespace(client=api,journal=journal,cancelled=threading.Event(),
            workspace=SimpleNamespace(container=api.containers.values[ATTEMPT+"-main"]))
        return task,network

    def observe(self,task):
        return subject.observe(task,time.monotonic()+3)

    def test_exact_lookup_private_raw_fields_deep_copy_and_explicit_intent(self):
        task,network = self.fixture()
        result = self.observe(task)
        self.assertEqual(task.client.networks.lookups,[network.name])
        self.assertEqual(result["primary"],"main")
        self.assertEqual(result["modes"],{"main":network.name,"database":network.name})
        self.assertEqual(result["networks"]["private"]["definition"],{"driver":"bridge"})
        self.assertEqual(set(result["networks"]["private"]["endpoints"]),{"main","database"})
        result["networks"]["private"]["attributes"]["IPAM"]["Config"][0]["Subnet"] = "changed"
        self.assertEqual(network.attrs["IPAM"]["Config"][0]["Subnet"],"192.0.2.0/24")

    def test_implicit_default_requires_the_frozen_default_role(self):
        task,network = self.fixture(implicit=True)
        self.assertEqual(self.observe(task)["attachments"],{"database":{"default":None},"main":{"default":None}})
        network.attrs["Containers"]["f"*64] = {}
        with self.assertRaisesRegex(MaterialError,"members"):
            self.observe(task)

    def test_missing_foreign_and_extra_members_are_refused(self):
        for mutation in ["missing network","missing member","foreign member","extra endpoint"]:
            with self.subTest(mutation=mutation):
                task,network = self.fixture()
                if mutation == "missing network":
                    task.client.networks.values.clear()
                elif mutation == "missing member":
                    network.attrs["Containers"].pop("1"*64)
                elif mutation == "foreign member":
                    network.attrs["Containers"]["f"*64] = {}
                else:
                    task.workspace.container.attrs["NetworkSettings"]["Networks"]["foreign"] = {}
                with self.assertRaises(MaterialError):
                    self.observe(task)

    def test_endpoint_identity_name_address_and_mac_must_agree_in_both_views(self):
        mutations = {"NetworkID":"f"*64,"EndpointID":"e"*64,"IPAddress":"192.0.2.99","IPPrefixLen":25,
            "GlobalIPv6Address":"2001:db8::1","MacAddress":"02:00:00:00:00:ff"}
        for key,value in mutations.items():
            with self.subTest(key=key):
                task,network = self.fixture()
                task.workspace.container.attrs["NetworkSettings"]["Networks"][network.name][key] = value
                with self.assertRaises(MaterialError):
                    self.observe(task)
        task,network = self.fixture()
        network.attrs["Containers"]["1"*64]["Name"] = ATTEMPT+"-other"
        with self.assertRaisesRegex(MaterialError,"identities"):
            self.observe(task)

    def test_reload_cannot_change_owner_or_expose_transport_details(self):
        task,network = self.fixture()
        network.reload = lambda:network.attrs.update(Labels={})
        with self.assertRaisesRegex(MaterialError,"ownership"):
            self.observe(task)
        def fail():
            raise RuntimeError("fixture-secret")
        network.reload = fail
        with self.assertRaises(MaterialError) as error:
            self.observe(task)
        self.assertNotIn("fixture-secret",str(error.exception))

    def test_external_modes_drivers_and_dependencies_are_not_silently_covered(self):
        for key,value in [("Driver","overlay"),("Scope","swarm"),("ConfigOnly",True),
                ("ConfigFrom",{"Network":"foreign"}),("Ingress",True),("Peers",[{"Name":"remote"}])]:
            task,network = self.fixture()
            network.attrs[key] = value
            with self.subTest(key=key),self.assertRaises(MaterialError):
                self.observe(task)
        task,network = self.fixture()
        task.workspace.container.attrs["HostConfig"]["NetworkMode"] = "host"
        with self.assertRaisesRegex(MaterialError,"namespace"):
            self.observe(task)

    def test_deadline_cancel_and_released_owner_prevent_lookups(self):
        task,network = self.fixture()
        with self.assertRaises(MaterialError):
            subject.observe(task,time.monotonic()-1)
        task.cancelled.set()
        with self.assertRaises(MaterialError):
            self.observe(task)
        task.cancelled.clear()
        task.journal.close()
        with self.assertRaises(MaterialError):
            self.observe(task)
        self.assertEqual(task.client.networks.lookups,[])

    def test_malformed_or_oversized_attachment_lists_have_fixed_errors(self):
        for value in [[{}],["private"]*33,["private","private"],["unknown"],{"unknown":None}]:
            with self.subTest(value=value),self.assertRaises(MaterialError):
                subject._attachments({"networks":value},{"private":{"name":"owned"}},
                    {"HostConfig":{"NetworkMode":"owned"}})

    def test_ipv6_prefixes_and_duplicate_endpoint_ids_are_checked(self):
        task,network = self.fixture()
        endpoint = task.workspace.container.attrs["NetworkSettings"]["Networks"][network.name]
        endpoint.update(GlobalIPv6Address="2001:db8::2",GlobalIPv6PrefixLen=64)
        network.attrs["Containers"]["1"*64]["IPv6Address"] = "2001:db8::2/64"
        self.observe(task)
        endpoint["GlobalIPv6PrefixLen"] = 80
        with self.assertRaisesRegex(MaterialError,"prefix lengths"):
            self.observe(task)
        task,network = self.fixture()
        endpoints = [resource.attrs["NetworkSettings"]["Networks"][network.name]
            for resource in task.client.containers.values.values()]
        endpoints[1]["EndpointID"] = endpoints[0]["EndpointID"]
        network.attrs["Containers"]["2"*64]["EndpointID"] = endpoints[0]["EndpointID"]
        with self.assertRaisesRegex(MaterialError,"identities"):
            self.observe(task)

    def test_absent_optional_configfrom_is_not_an_external_dependency(self):
        task,network = self.fixture()
        network.attrs["ConfigFrom"] = None
        result = self.observe(task)
        self.assertIsNone(result["networks"]["private"]["attributes"]["ConfigFrom"])


if __name__ == "__main__":
    unittest.main()
