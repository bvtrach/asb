"""Bind canonical owned topology into complete inspected runtime settings."""
import copy
import re

from asb_material import MaterialError


def validate_sandboxes(services):
    """Private service namespaces cannot share a Docker sandbox identity."""
    identities,keys = set(),set()
    for attributes in services.values():
        settings = attributes.get("NetworkSettings") or {}
        identity,key = settings.get("SandboxID"),settings.get("SandboxKey")
        if identity:
            if identity in identities or (key and key in keys):
                raise MaterialError("prepared private services share a network sandbox")
            identities.add(identity)
            if key:
                keys.add(key)


def integrate(value, attributes, role, network):
    """Transform exact generated identities, preserving other inspected fields."""
    raw,canonical = network
    def require(condition):
        if not condition:
            raise MaterialError("prepared runtime differs from verified owned network topology")
    require(attributes.get("Id") == raw["services"][role])
    require(attributes["HostConfig"]["NetworkMode"] == raw["modes"][role])
    settings = copy.deepcopy(attributes["NetworkSettings"])
    observed,selected = {},{}
    for network_role,item in raw["networks"].items():
        if role in item["endpoints"]:
            observed[item["attributes"]["Name"]] = item["endpoints"][role]
            selected[network_role] = canonical["networks"][network_role]["endpoints"][role]
    require(settings.get("Networks") == observed)
    settings["Networks"] = copy.deepcopy(selected)
    # Modern per-network fields supersede these deprecated aggregate fields,
    # but a nonempty legacy value must still match an observed endpoint.
    legacy = [key for key in ["EndpointID","Gateway","IPv6Gateway","IPAddress","GlobalIPv6Address","MacAddress"]
        if settings.get(key)]
    for prefix,address in [("IPPrefixLen","IPAddress"),("GlobalIPv6PrefixLen","GlobalIPv6Address")]:
        if settings.get(prefix) or settings.get(address):
            legacy.append(prefix)
    if legacy:
        matches = [network_role for network_role,item in raw["networks"].items()
            if role in item["endpoints"] and all(item["endpoints"][role].get(key,0 if "PrefixLen" in key else None)
                == settings.get(key,0 if "PrefixLen" in key else None) for key in legacy)]
        require(len(matches) == 1)
        for key in legacy:
            settings[key] = copy.deepcopy(selected[matches[0]][key])
    for key in ["LinkLocalIPv6Address","LinkLocalIPv6PrefixLen","SecondaryIPAddresses","SecondaryIPv6Addresses","Bridge"]:
        require(not settings.get(key))
    ports = settings.get("Ports") or {}
    require(isinstance(ports,dict) and all(not bindings for bindings in ports.values()))
    sandbox,key = settings.get("SandboxID"),settings.get("SandboxKey")
    require(bool(sandbox) or not observed)
    if sandbox:
        require(isinstance(sandbox,str) and re.fullmatch(r"[0-9a-f]{64}",sandbox) is not None)
        require(isinstance(key,str) and key in ["/var/run/docker/netns/"+sandbox[:12],"/run/docker/netns/"+sandbox[:12]])
        settings["SandboxID"] = {"owned_sandbox":role}
        settings["SandboxKey"] = {"owned_sandbox":role}
    else:
        require(not key)
    if value["config"].get("MacAddress"):
        mac = value["config"]["MacAddress"]
        matches = [selected[network_role]["MacAddress"] for network_role,item in raw["networks"].items()
            if role in item["endpoints"] and item["endpoints"][role].get("MacAddress") == mac]
        require(bool(matches) and all(item == matches[0] for item in matches))
        value["config"]["MacAddress"] = copy.deepcopy(matches[0])
    value["host"]["NetworkMode"] = copy.deepcopy(canonical["modes"][role])
    value["network"] = settings
    return value
