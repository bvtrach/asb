"""Canonical owned bridge semantics from positively verified private topology.

Generated-address mappings are evidence for Docker-generated runtime file
locations only, never permission to rewrite workload bytes or provider requests.
Raw observations must still agree across complete environment rounds.
"""
import copy
import ipaddress
import re

from asb_material import MaterialError, canonical_digest


def _require(condition):
    if not condition:
        raise MaterialError("prepared network intent differs from observed or supported semantics")


def _pools(role, definition, attributes):
    requested = definition.get("ipam") or {}
    _require(isinstance(requested,dict) and not set(requested)-{"driver","config","options"})
    actual = attributes.get("IPAM") or {}
    _require(actual.get("Driver") == requested.get("driver","default") == "default")
    _require((actual.get("Options") or {}) == (requested.get("options") or {}))
    configs = actual.get("Config") or []
    selected = requested.get("config") or []
    _require(isinstance(configs,list) and 1 <= len(configs) <= 2 and isinstance(selected,list))
    _require(not selected or len(selected) == len(configs))
    # Docker can enumerate IPv4/IPv6 pools in either order. Multiple explicit
    # pools require family-identifying subnets, not positional inference.
    configs = sorted(configs,key=lambda value:ipaddress.ip_network(value["Subnet"]).version)
    if len(selected) > 1:
        selected = sorted(selected,key=lambda value:ipaddress.ip_network(value["subnet"]).version)
    result,pools,families = [],[],set()
    for index,config in enumerate(configs):
        _require(isinstance(config,dict) and not set(config)-{"Subnet","Gateway","IPRange","AuxiliaryAddresses"})
        subnet = ipaddress.ip_network(config["Subnet"])
        _require(subnet.version not in families)
        families.add(subnet.version)
        gateway = config.get("Gateway")
        if gateway:
            _require(ipaddress.ip_address(gateway) in subnet)
        explicit = selected[index] if selected else None
        value = copy.deepcopy(config)
        if explicit is not None:
            _require(isinstance(explicit,dict) and not set(explicit)-{"subnet","gateway","ip_range","aux_addresses"})
            # Explicit pool definitions remain entirely literal. Even omitted
            # allocator-selected fields retain their actual value in this case.
            for source,target in [("subnet","Subnet"),("gateway","Gateway"),
                    ("ip_range","IPRange"),("aux_addresses","AuxiliaryAddresses")]:
                if source in explicit:
                    _require(explicit[source] == config.get(target))
        else:
            _require(not config.get("IPRange") and not config.get("AuxiliaryAddresses"))
            value["Subnet"] = {"automatic_pool":role,"family":subnet.version,"prefix":subnet.prefixlen}
            if gateway:
                value["Gateway"] = {"automatic_gateway":role,"family":subnet.version,
                    "offset":int(ipaddress.ip_address(gateway))-int(subnet.network_address)}
        result.append(value)
        pools.append((subnet,gateway,explicit is None))
    ipam = copy.deepcopy(actual)
    _require(not set(ipam)-{"Driver","Options","Config"})
    ipam["Config"] = result
    return ipam,pools


def _aliases(values, explicit, generated):
    if values is None:
        return None
    _require(isinstance(values,list) and len(values) <= 64 and all(isinstance(item,str) for item in values))
    return [{"literal":value} if value in explicit or value not in generated
        else generated[value] for value in values]


def canonicalize(snapshot):
    """Validate intent and derive logical topology plus scoped generated maps.

    Unsupported declared dependencies fail rather than being omitted. This is
    one component, not a complete environment equivalence check or snapshot.
    """
    try:
        return _canonicalize(snapshot)
    except MaterialError:
        raise
    except (KeyError,ValueError,TypeError,AttributeError,OverflowError):
        raise MaterialError("prepared network canonicalization received invalid evidence") from None


def _canonicalize(snapshot):
    canonical_digest(snapshot,"asb-private-network-identity-input-v1")
    networks,intents = snapshot["networks"],snapshot["service_intent"]
    _require(isinstance(networks,dict) and len(networks) <= 32 and isinstance(intents,dict) and len(intents) <= 16)
    public,mappings = {},{}
    for role,item in sorted(networks.items()):
        definition,attrs = item["definition"],item["attributes"]
        _require(not set(definition)-{"driver","driver_opts","internal","attachable","enable_ipv4","enable_ipv6","ipam"})
        _require(attrs.get("Driver") == definition.get("driver","bridge") == "bridge")
        _require(attrs.get("Scope") == "local")
        for source,target,default in [("internal","Internal",False),("attachable","Attachable",False),
                ("enable_ipv4","EnableIPv4",True),("enable_ipv6","EnableIPv6",False)]:
            requested = definition.get(source,default)
            _require(type(requested) is bool and attrs.get(target,default) == requested)
        options,requested_options = attrs.get("Options") or {},definition.get("driver_opts") or {}
        _require(isinstance(options,dict) and isinstance(requested_options,dict))
        _require(all(options.get(key) == value for key,value in requested_options.items()))
        selected = copy.deepcopy(attrs)
        for key in ["Name","Id","Created","Labels","Containers"]:
            selected.pop(key,None)
        selected["Options"] = copy.deepcopy(options)
        bridge_key = "com.docker.network.bridge.name"
        if bridge_key in options and bridge_key not in requested_options:
            _require(options[bridge_key] == "br-"+attrs["Id"][:12])
            selected["Options"][bridge_key] = {"automatic_bridge":role}
        selected["IPAM"],pools = _pools(role,definition,attrs)
        endpoints,used_addresses,used_macs = {},set(),set()
        for service,endpoint in sorted(item["endpoints"].items()):
            intent = snapshot["attachments"][service][role] or {}
            _require(not set(intent)-{"aliases","ipv4_address","ipv6_address","link_local_ips","mac_address","gw_priority","driver_opts"})
            service_intent = intents[service]
            _require("mac_address" in service_intent)
            aliases = intent.get("aliases") or []
            _require(isinstance(aliases,list) and all(isinstance(alias,str) for alias in aliases))
            _require(set(aliases) <= set(endpoint.get("Aliases") or []))
            value = copy.deepcopy(endpoint)
            value["NetworkID"] = {"owned_network":role}
            value["EndpointID"] = {"owned_endpoint":service,"network":role}
            generated = {service_intent["container"]:{"owned_container":service}}
            if service_intent.get("hostname") is None:
                generated[snapshot["services"][service][:12]] = {"generated_hostname":service}
            explicit_names = aliases+[service_intent["hostname"]] if service_intent.get("hostname") else aliases
            for key in ["Aliases","DNSNames"]:
                if key in value:
                    value[key] = _aliases(value[key],explicit_names,generated)
            _require(endpoint.get("GwPriority",0) == intent.get("gw_priority",0))
            _require((endpoint.get("DriverOpts") or {}) == (intent.get("driver_opts") or {}))
            ipam = endpoint.get("IPAMConfig") or {}
            _require(isinstance(ipam,dict) and not set(ipam)-{"IPv4Address","IPv6Address","LinkLocalIPs"})
            _require((ipam.get("LinkLocalIPs") or []) == (intent.get("link_local_ips") or []))
            for family,address_key,prefix_key,gateway_key,intent_key,ipam_key in [
                    (4,"IPAddress","IPPrefixLen","Gateway","ipv4_address","IPv4Address"),
                    (6,"GlobalIPv6Address","GlobalIPv6PrefixLen","IPv6Gateway","ipv6_address","IPv6Address")]:
                address = endpoint.get(address_key)
                requested = intent.get(intent_key)
                _require((ipam.get(ipam_key) or None) == (requested or None))
                if not address:
                    _require(not requested and not endpoint.get(gateway_key) and endpoint.get(prefix_key,0) == 0)
                    continue
                actual = ipaddress.ip_address(address)
                _require(actual.version == family and actual not in used_addresses)
                used_addresses.add(actual)
                matching = [(subnet,gateway,auto) for subnet,gateway,auto in pools if actual in subnet]
                _require(len(matching) == 1)
                subnet,gateway,automatic_pool = matching[0]
                # Internal bridges allocate a pool gateway but deliberately do
                # not install it as the container's default route. Preserve
                # that distinction rather than inventing a routed endpoint.
                expected_gateway = None if attrs.get("Internal") else gateway or None
                _require(endpoint.get(prefix_key) == subnet.prefixlen
                    and (endpoint.get(gateway_key) or None) == expected_gateway)
                if requested:
                    _require(ipaddress.ip_address(requested) == actual)
                else:
                    tag = {"automatic_address":service,"network":role,"family":family}
                    value[address_key] = tag
                    addresses = mappings.setdefault(service,{})
                    address_key_canonical = str(actual)
                    _require(address_key_canonical not in addresses or addresses[address_key_canonical] == tag)
                    addresses[address_key_canonical] = tag
                if endpoint.get(gateway_key) and automatic_pool:
                    value[gateway_key] = {"automatic_gateway":role,"family":family,
                        "offset":int(ipaddress.ip_address(gateway))-int(subnet.network_address)}
            requested_mac = intent.get("mac_address") or service_intent["mac_address"]
            _require(not intent.get("mac_address") or not service_intent["mac_address"]
                or intent["mac_address"].lower() == service_intent["mac_address"].lower())
            mac = endpoint.get("MacAddress")
            _require(isinstance(mac,str) and re.fullmatch(r"[0-9a-fA-F]{2}(?::[0-9a-fA-F]{2}){5}",mac) is not None)
            _require(mac.lower() not in used_macs)
            used_macs.add(mac.lower())
            if requested_mac:
                _require(mac.lower() == requested_mac.lower())
            else:
                value["MacAddress"] = {"automatic_mac":service,"network":role}
            endpoints[service] = value
        declared = copy.deepcopy(definition)
        if len((declared.get("ipam") or {}).get("config") or []) > 1:
            declared["ipam"]["config"].sort(key=lambda value:ipaddress.ip_network(value["subnet"]).version)
        public[role] = {"definition":declared,"configuration":selected,"endpoints":endpoints}
    names = {item["attributes"]["Name"]:role for role,item in networks.items()}
    modes = {service:{"owned_network":names[mode]} if mode in names else {"literal":mode}
        for service,mode in snapshot["modes"].items()}
    result = {"networks":public,"modes":modes}
    canonical_digest(result,"asb-owned-bridge-network-identity-v1")
    return result,mappings
