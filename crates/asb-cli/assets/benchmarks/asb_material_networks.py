"""Exact owned bridge membership observation, not canonical replay identity.

Raw endpoint/network identities remain private. Callers must additionally model
allocation, runtime files and explicit intent before claiming equivalence. This
module never lists resources or creates/connects/disconnects network endpoints.
"""
import copy
import ipaddress
import re
import time

from asb_material import MaterialError, canonical_digest
from asb_material_services import logical, resolve


def _identity(value):
    return isinstance(value,str) and re.fullmatch(r"[0-9a-f]{64}",value) is not None


def _attachments(declaration, networks, attributes):
    selected = declaration.get("networks")
    if selected is None:
        selected = {"default":None} if "default" in networks else {}
    if isinstance(selected,list):
        if len(selected) > 32 or any(not logical(role) for role in selected) or len(selected) != len(set(selected)):
            raise MaterialError("prepared network attachments contain duplicate roles")
        selected = {role:None for role in selected}
    if (not isinstance(selected,dict) or len(selected) > 32 or any(not logical(role) for role in selected)
            or set(selected)-set(networks)
            or any(value is not None and not isinstance(value,dict) for value in selected.values())):
        raise MaterialError("prepared network attachment intent is invalid or undeclared")
    mode = attributes.get("HostConfig",{}).get("NetworkMode")
    allowed = {networks[role]["name"] for role in selected}
    if (selected and mode not in allowed) or (not selected and mode != "none"):
        raise MaterialError("prepared service uses an uncovered network namespace")
    return selected


def _address(value, family, *, interface=False):
    if value in [None,""]:
        return None
    if not isinstance(value,str):
        raise MaterialError("prepared endpoint address is invalid")
    try:
        address = ipaddress.ip_interface(value) if interface else ipaddress.ip_address(value)
    except ValueError:
        raise MaterialError("prepared endpoint address is invalid") from None
    if address.version != family:
        raise MaterialError("prepared endpoint address family is contradictory")
    return str(address.ip if interface else address)


def observe(task, deadline):
    """Return a bounded private role-indexed snapshot with exact membership.

    Both container and network inspect must agree with the frozen role receipt.
    Every service is freshly resolved and ownership is checked again after each
    network reload. A later whole-round comparison must detect intervening
    topology mutation; this read sequence is not an atomic Docker snapshot.
    """
    from asb_resources import owned_resource
    receipt,services,primary = resolve(task,deadline)
    if receipt.get("external_networks"):
        from asb_material_shared_network import observe as shared
        return shared(task,receipt,services,primary,deadline)
    journal = task.journal
    def check():
        if (time.monotonic() >= deadline or journal.lock is None
                or (task.cancelled is not None and task.cancelled.is_set())):
            raise MaterialError("prepared network observation was cancelled or lost its owner")
    declarations = receipt["networks"]
    # Old receipts that lack explicit creation intent cannot authorize dynamic
    # address normalization. Reject them rather than guessing absent IPAM.
    if any(not isinstance(item.get("definition"),dict) for item in declarations.values()):
        raise MaterialError("prepared network creation intent is unavailable")
    attachments = {role:_attachments(receipt["services"][role],declarations,resource.attrs)
        for role,resource in services.items()}
    for role,resource in services.items():
        endpoints = resource.attrs.get("NetworkSettings",{}).get("Networks")
        if not isinstance(endpoints,dict) or len(endpoints) > 32:
            raise MaterialError("prepared service endpoint population exceeds bounds")
        expected = {declarations[key]["name"] for key in attachments[role]}
        actual = set(endpoints)
        if not expected and actual == {"none"}:
            actual = set()
        if actual != expected:
            raise MaterialError("prepared service endpoints differ from declared attachments")
    result,identities = {},set()
    try:
        for network_role,item in sorted(declarations.items()):
            check()
            resource = owned_resource(task.client,{"kind":"networks","name":item["name"]},journal.labels)
            check()
            if resource is None:
                raise MaterialError("prepared declared network is missing")
            resource.reload()
            check()
            attrs = resource.attrs
            canonical_digest(attrs,"asb-private-network-inspect-v1")
            labels = attrs.get("Labels") or {}
            if (resource.name != item["name"] or attrs.get("Name") != item["name"]
                    or not _identity(resource.id) or attrs.get("Id") != resource.id
                    or resource.id in identities
                    or any(labels.get(key) != value for key,value in journal.labels.items())):
                raise MaterialError("prepared network ownership or identity changed")
            if (attrs.get("Driver") != "bridge" or attrs.get("Scope") != "local"
                    or attrs.get("Ingress") or attrs.get("ConfigOnly")
                    or (attrs.get("ConfigFrom") or {}).get("Network") or attrs.get("Peers")):
                raise MaterialError("prepared network has uncovered external driver dependencies")
            identities.add(resource.id)
            members = attrs.get("Containers")
            expected = {service.id:role for role,service in services.items() if network_role in attachments[role]}
            if not isinstance(members,dict) or len(members) > 16 or set(members) != set(expected):
                raise MaterialError("prepared network has missing or undeclared endpoint members")
            endpoints = {}
            endpoint_ids = set()
            for identity,role in sorted(expected.items(),key=lambda pair:pair[1]):
                member = members[identity]
                endpoint = services[role].attrs["NetworkSettings"]["Networks"][item["name"]]
                if not isinstance(member,dict) or not isinstance(endpoint,dict):
                    raise MaterialError("prepared endpoint inspection is invalid")
                endpoint_id = endpoint.get("EndpointID")
                if (not _identity(endpoint_id) or endpoint_id in endpoint_ids
                        or member.get("EndpointID") != endpoint_id
                        or endpoint.get("NetworkID") != resource.id
                        or member.get("Name") != receipt["services"][role]["container"]):
                    raise MaterialError("prepared endpoint identities disagree across inspections")
                endpoint_ids.add(endpoint_id)
                for family,container_key,member_key,prefix_key in [
                        (4,"IPAddress","IPv4Address","IPPrefixLen"),
                        (6,"GlobalIPv6Address","IPv6Address","GlobalIPv6PrefixLen")]:
                    address = _address(endpoint.get(container_key),family)
                    if address != _address(member.get(member_key),family,interface=True):
                        raise MaterialError("prepared endpoint addresses disagree across inspections")
                    if address is not None:
                        prefix = endpoint.get(prefix_key)
                        if (type(prefix) is not int or prefix != ipaddress.ip_interface(member[member_key]).network.prefixlen):
                            raise MaterialError("prepared endpoint prefix lengths disagree across inspections")
                if member.get("MacAddress") != endpoint.get("MacAddress"):
                    raise MaterialError("prepared endpoint hardware addresses disagree")
                endpoints[role] = copy.deepcopy(endpoint)
            result[network_role] = {"definition":copy.deepcopy(item["definition"]),
                "attributes":copy.deepcopy(attrs),"endpoints":endpoints}
    except MaterialError:
        raise
    except Exception:
        raise MaterialError("prepared network ownership or topology could not be confirmed") from None
    check()
    value = {"primary":primary,"networks":result,
        "service_intent":copy.deepcopy(receipt["services"]),
        "attachments":copy.deepcopy(attachments),"services":{role:resource.id for role,resource in services.items()},
        "modes":{role:resource.attrs["HostConfig"]["NetworkMode"] for role,resource in services.items()}}
    canonical_digest(value,"asb-private-network-topology-observation-v1")
    return value
