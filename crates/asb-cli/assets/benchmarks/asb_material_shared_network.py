"""Relevant own endpoint on Docker's externally shared default bridge.

No global bridge ownership, membership completeness or external-state snapshot
is claimed. Foreign peers are never iterated, hashed, journaled or modified.
"""
import copy
import time

from asb_material import MaterialError, canonical_digest
from asb_material_networks import _address, _identity


def observe(task, receipt, services, primary, deadline):
    def check():
        if (time.monotonic() >= deadline or task.journal.lock is None
                or (task.cancelled is not None and task.cancelled.is_set())):
            raise MaterialError("shared network observation was cancelled or lost its owner")
    if (receipt.get("source") != "direct-container-v1" or receipt["networks"]
            or receipt.get("external_networks") != {"default":{"name":"bridge",
                "profile":"external-shared-default-bridge-v1"}}
            or set(services) != {"main"}):
        raise MaterialError("shared network creation profile is unsupported")
    service = services["main"]
    attrs = service.attrs
    mode = attrs.get("HostConfig",{}).get("NetworkMode")
    endpoints = attrs.get("NetworkSettings",{}).get("Networks")
    if mode not in ["default","bridge"] or not isinstance(endpoints,dict) or set(endpoints) != {"bridge"}:
        raise MaterialError("shared default bridge endpoint differs from creation intent")
    try:
        check()
        network = task.client.networks.get("bridge")
        check()
        network.reload()
        check()
        metadata = network.attrs
        if (network.name != "bridge" or metadata.get("Name") != "bridge"
                or not _identity(network.id) or metadata.get("Id") != network.id
                or metadata.get("Driver") != "bridge" or metadata.get("Scope") != "local"
                or (metadata.get("Options") or {}).get("com.docker.network.bridge.default_bridge") != "true"
                or (metadata.get("IPAM") or {}).get("Driver") != "default"
                or metadata.get("Ingress") or metadata.get("ConfigOnly")
                or (metadata.get("ConfigFrom") or {}).get("Network") or metadata.get("Peers")):
            raise MaterialError("named network is not the supported shared Docker default bridge")
        # Read precisely one named member. In particular, neither enumeration
        # nor a deep copy of the global Containers map is permitted.
        member = (metadata.get("Containers") or {}).get(service.id)
        endpoint = endpoints["bridge"]
        if (not isinstance(member,dict) or not isinstance(endpoint,dict)
                or endpoint.get("NetworkID") != network.id
                or not _identity(endpoint.get("EndpointID"))
                or member.get("EndpointID") != endpoint["EndpointID"]
                or member.get("Name") != receipt["services"]["main"]["container"]
                or member.get("MacAddress") != endpoint.get("MacAddress")):
            raise MaterialError("owned shared-bridge endpoint inspection disagrees")
        for family,address,prefix,member_key in [
                (4,"IPAddress","IPPrefixLen","IPv4Address"),
                (6,"GlobalIPv6Address","GlobalIPv6PrefixLen","IPv6Address")]:
            own = _address(endpoint.get(address),family)
            if own != _address(member.get(member_key),family,interface=True):
                raise MaterialError("owned shared-bridge endpoint addresses disagree")
            if own:
                import ipaddress
                if (type(endpoint.get(prefix)) is not int
                        or endpoint[prefix] != ipaddress.ip_interface(member[member_key]).network.prefixlen):
                    raise MaterialError("owned shared-bridge endpoint prefixes disagree")
        # Aggregate allocator counters describe all peers on this external
        # bridge. Like membership itself, they are outside own-endpoint scope.
        selected = {key:copy.deepcopy(value) for key,value in metadata.items() if key not in ["Containers","Status"]}
        canonical_digest(selected,"asb-private-shared-bridge-config-v1")
        result = {"primary":primary,"service_intent":copy.deepcopy(receipt["services"]),
            "services":{"main":service.id},"modes":{"main":mode},
            "attachments":{"main":{"default":None}},
            "networks":{"default":{"definition":{"driver":"bridge"},"attributes":selected,
                "endpoints":{"main":copy.deepcopy(endpoint)},
                "ownership":"external-shared-default-bridge-v1"}}}
        canonical_digest(result,"asb-private-shared-bridge-observation-v1")
        check()
        return result
    except MaterialError:
        raise
    except Exception:
        raise MaterialError("shared default bridge configuration or own endpoint could not be inspected") from None
