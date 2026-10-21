"""Canonical bridge allocation and explicit-intent tests, without Docker."""
import copy
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
from asb_material import MaterialError
from asb_material_network_identity import canonicalize
import benchmark_material_networks as fixtures


class IdentityTests(unittest.TestCase):
    fixture = fixtures.NetworkTests.fixture
    observe = fixtures.NetworkTests.observe

    def snapshot(self):
        task,network = self.fixture()
        value = self.observe(task)
        for endpoint in value["networks"]["private"]["endpoints"].values():
            endpoint["Gateway"] = "192.0.2.1"
        return value

    def test_automatic_pool_endpoint_mac_and_owned_names_normalize_across_fresh_attempts(self):
        first = self.snapshot()
        first["networks"]["private"]["attributes"]["Status"] = {"IPAM":{"Subnets":{
            "192.0.2.0/24":{"IPsInUse":5,"DynamicIPsAvailable":251}}}}
        changed = copy.deepcopy(first)
        network = changed["networks"]["private"]
        network["attributes"].update(Id="f"*64,Name="new-owned-network",Created="later")
        network["attributes"]["IPAM"]["Config"] = [{"Subnet":"198.51.100.0/24","Gateway":"198.51.100.1"}]
        network["attributes"]["Status"]["IPAM"]["Subnets"] = {
            "198.51.100.0/24":{"IPsInUse":5,"DynamicIPsAvailable":251}}
        for index,(role,endpoint) in enumerate(network["endpoints"].items()):
            identity = str(index+7)*64
            changed["services"][role] = identity
            changed["service_intent"][role]["container"] = "new-owned-"+role
            changed["modes"][role] = "new-owned-network"
            endpoint.update(NetworkID="f"*64,EndpointID=str(index+5)*64,
                IPAddress="198.51.100."+str(index+8),Gateway="198.51.100.1",
                MacAddress="02:00:00:00:00:0"+str(index+1),Aliases=[role,"new-owned-"+role])
        self.assertEqual(canonicalize(first)[0],canonicalize(changed)[0])
        self.assertNotEqual(canonicalize(first)[1],canonicalize(changed)[1],"private raw address maps stay scoped")

    def test_owned_status_requires_exact_pools_and_bounded_counters(self):
        value = self.snapshot()
        attrs = value["networks"]["private"]["attributes"]
        stats = {"IPsInUse":5,"DynamicIPsAvailable":251}
        attrs["Status"] = {"IPAM":{"Subnets":{"192.0.2.0/24":stats}}}
        baseline,_ = canonicalize(value)
        stats["IPsInUse"] = 6
        self.assertNotEqual(baseline,canonicalize(value)[0])
        for bad in [{"192.0.3.0/24":stats},{},
                    {"192.0.2.0/24":{"IPsInUse":True,"DynamicIPsAvailable":251}},
                    {"192.0.2.0/24":{"IPsInUse":-1,"DynamicIPsAvailable":251}},
                    {"192.0.2.0/24":{"IPsInUse":5,"DynamicIPsAvailable":2**64}},
                    {"192.0.2.0/24":{"IPsInUse":5,"DynamicIPsAvailable":251,"unknown":0}}]:
            attrs["Status"]["IPAM"]["Subnets"] = bad
            with self.subTest(bad=bad),self.assertRaises(MaterialError):
                canonicalize(value)

    def test_older_status_absence_is_not_fabricated_as_zero(self):
        value = self.snapshot()
        missing,_ = canonicalize(value)
        self.assertNotIn("Status",missing["networks"]["private"]["configuration"])
        value["networks"]["private"]["attributes"]["Status"] = None
        null,_ = canonicalize(value)
        self.assertIsNone(null["networks"]["private"]["configuration"]["Status"])
        self.assertNotEqual(missing,null)

    def test_explicit_status_pool_remains_literal(self):
        value = self.snapshot()
        network = value["networks"]["private"]
        network["definition"]["ipam"] = {"config":[{"subnet":"192.0.2.0/24"}]}
        network["attributes"]["Status"] = {"IPAM":{"Subnets":{
            "192.0.2.0/24":{"IPsInUse":5,"DynamicIPsAvailable":251}}}}
        public,_ = canonicalize(value)
        self.assertIn("192.0.2.0/24",str(public["networks"]["private"]["configuration"]["Status"]))

    def test_explicit_subnet_gateway_and_auxiliary_addresses_remain_literal_and_checked(self):
        value = self.snapshot()
        network = value["networks"]["private"]
        config = network["attributes"]["IPAM"]["Config"][0]
        config["AuxiliaryAddresses"] = {"reserved":"192.0.2.20"}
        network["definition"]["ipam"] = {"config":[{"subnet":"192.0.2.0/24",
            "gateway":"192.0.2.1","aux_addresses":{"reserved":"192.0.2.20"}}]}
        result,_ = canonicalize(value)
        self.assertEqual(result["networks"]["private"]["configuration"]["IPAM"]["Config"],[config])
        network["definition"]["ipam"]["config"][0]["gateway"] = "192.0.2.99"
        with self.assertRaises(MaterialError):
            canonicalize(value)

    def test_static_address_mac_alias_and_linklocal_intent_must_match_actual(self):
        value = self.snapshot()
        endpoint = value["networks"]["private"]["endpoints"]["main"]
        intent = {"ipv4_address":endpoint["IPAddress"],"mac_address":endpoint["MacAddress"],
            "aliases":["main"],"link_local_ips":["169.254.1.2"]}
        value["attachments"]["main"]["private"] = intent
        endpoint["IPAMConfig"] = {"IPv4Address":endpoint["IPAddress"],"LinkLocalIPs":["169.254.1.2"]}
        result,_ = canonicalize(value)
        selected = result["networks"]["private"]["endpoints"]["main"]
        self.assertEqual(selected["IPAddress"],endpoint["IPAddress"])
        self.assertEqual(selected["MacAddress"],endpoint["MacAddress"])
        self.assertEqual(selected["Aliases"][0],{"literal":"main"})
        for key,new in [("ipv4_address","192.0.2.99"),("mac_address","02:00:00:00:00:ff"),
                ("aliases",["missing"]),("link_local_ips",["169.254.1.3"]),("gw_priority",2)]:
            changed = copy.deepcopy(value)
            changed["attachments"]["main"]["private"][key] = new
            with self.subTest(key=key),self.assertRaises(MaterialError):
                canonicalize(changed)

    def test_options_internal_flags_routing_and_unknown_intents_do_not_disappear(self):
        value = self.snapshot()
        baseline = canonicalize(value)[0]
        changed = copy.deepcopy(value)
        network = changed["networks"]["private"]
        network["definition"]["internal"] = True
        with self.assertRaises(MaterialError):
            canonicalize(changed)
        network["attributes"]["Internal"] = True
        for endpoint in network["endpoints"].values():
            endpoint["Gateway"] = ""
        self.assertNotEqual(baseline,canonicalize(changed)[0])
        for target in [network["definition"],changed["attachments"]["main"]["private"]]:
            target["unknown_dependency"] = "do-not-drop"
            with self.assertRaises(MaterialError):
                canonicalize(changed)
            del target["unknown_dependency"]
        network["attributes"]["Options"] = {"com.docker.network.bridge.enable_icc":"false"}
        self.assertNotEqual(baseline,canonicalize(changed)[0])

    def test_global_mac_intent_and_explicit_generated_looking_hostname_are_not_erased(self):
        value = self.snapshot()
        endpoint = value["networks"]["private"]["endpoints"]["main"]
        intent = value["service_intent"]["main"]
        intent["mac_address"] = endpoint["MacAddress"]
        intent["hostname"] = value["services"]["main"][:12]
        endpoint["DNSNames"] = [intent["hostname"]]
        result,_ = canonicalize(value)
        selected = result["networks"]["private"]["endpoints"]["main"]
        self.assertEqual(selected["MacAddress"],intent["mac_address"])
        self.assertEqual(selected["DNSNames"],[{"literal":intent["hostname"]}])
        del intent["mac_address"]
        with self.assertRaises(MaterialError):
            canonicalize(value)

    def test_gateway_prefix_ipam_driver_and_malformed_evidence_fail_with_fixed_errors(self):
        for key,new in [("Gateway","192.0.2.99"),("IPPrefixLen",25),("IPAddress","not-an-address")]:
            value = self.snapshot()
            value["networks"]["private"]["endpoints"]["main"][key] = new
            with self.subTest(key=key),self.assertRaises(MaterialError):
                canonicalize(value)
        value = self.snapshot()
        value["networks"]["private"]["attributes"]["IPAM"]["Driver"] = "external-plugin"
        with self.assertRaises(MaterialError):
            canonicalize(value)
        value["networks"]["private"]["definition"]["ipam"] = {"driver":"external-plugin"}
        with self.assertRaises(MaterialError):
            canonicalize(value)
        with self.assertRaises(MaterialError):
            canonicalize({"networks":None,"service_intent":None})

    def dual_stack(self):
        value = self.snapshot()
        network = value["networks"]["private"]
        network["definition"]["enable_ipv6"] = network["attributes"]["EnableIPv6"] = True
        network["attributes"]["IPAM"]["Config"].append({"Subnet":"2001:db8::/64","Gateway":"2001:db8::1"})
        network["attributes"]["Status"] = {"IPAM":{"Subnets":{
            "192.0.2.0/24":{"IPsInUse":5,"DynamicIPsAvailable":251},
            "2001:db8::/64":{"IPsInUse":4,"DynamicIPsAvailable":2**64-4}}}}
        for index,endpoint in enumerate(network["endpoints"].values()):
            endpoint.update(GlobalIPv6Address="2001:db8::"+str(index+2),
                GlobalIPv6PrefixLen=64,IPv6Gateway="2001:db8::1")
        return value

    def test_pool_family_order_is_not_identity_and_equivalent_ipv6_spellings_collide(self):
        first = self.dual_stack()
        changed = copy.deepcopy(first)
        changed["networks"]["private"]["attributes"]["IPAM"]["Config"].reverse()
        statuses = changed["networks"]["private"]["attributes"]["Status"]["IPAM"]["Subnets"]
        changed["networks"]["private"]["attributes"]["Status"]["IPAM"]["Subnets"] = dict(reversed(list(statuses.items())))
        self.assertEqual(canonicalize(first)[0],canonicalize(changed)[0])
        invalid = copy.deepcopy(changed)
        invalid["networks"]["private"]["attributes"]["Status"]["IPAM"]["Subnets"] = {
            name:{"IPsInUse":4,"DynamicIPsAvailable":2**64-4}
            for name in ["2001:db8::/64","2001:0db8::/64"]}
        with self.assertRaises(MaterialError):
            canonicalize(invalid)
        endpoints = list(changed["networks"]["private"]["endpoints"].values())
        endpoints[0]["GlobalIPv6Address"] = "2001:db8::2"
        endpoints[1]["GlobalIPv6Address"] = "2001:0db8:0000:0000:0000:0000:0000:0002"
        with self.assertRaises(MaterialError):
            canonicalize(changed)

    def test_explicit_dual_stack_pool_order_and_duplicate_macs(self):
        first = self.dual_stack()
        first["networks"]["private"]["definition"]["ipam"] = {"config":[
            {"subnet":"192.0.2.0/24","gateway":"192.0.2.1"},
            {"subnet":"2001:db8::/64","gateway":"2001:db8::1"}]}
        changed = copy.deepcopy(first)
        changed["networks"]["private"]["definition"]["ipam"]["config"].reverse()
        changed["networks"]["private"]["attributes"]["IPAM"]["Config"].reverse()
        self.assertEqual(canonicalize(first)[0],canonicalize(changed)[0])
        endpoints = list(changed["networks"]["private"]["endpoints"].values())
        endpoints[1]["MacAddress"] = endpoints[0]["MacAddress"].upper()
        with self.assertRaises(MaterialError):
            canonicalize(changed)

    def test_generated_runtime_address_map_refuses_cross_network_ambiguity(self):
        value = self.snapshot()
        extra = copy.deepcopy(value["networks"]["private"])
        extra["attributes"].update(Id="f"*64,Name="other-owned-network")
        for role,endpoint in extra["endpoints"].items():
            endpoint["NetworkID"] = "f"*64
            value["attachments"][role]["other"] = copy.deepcopy(value["attachments"][role]["private"])
        value["networks"]["other"] = extra
        with self.assertRaises(MaterialError):
            canonicalize(value)


if __name__ == "__main__":
    unittest.main()
