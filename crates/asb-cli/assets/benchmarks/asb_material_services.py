"""Stable Compose roles authorized before owned service/resource creation.

These receipts describe authority, not observed environment equivalence. They
do not authorize recovery retries, new resources, or unlisted daemon inventories.
"""
import copy
import re
import time

from asb_material import MaterialError, canonical_digest
from asb_material_declarations import read


def logical(value):
    return isinstance(value,str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}",value) is not None


def declare(configuration, record):
    """Prepare a nonblocking receipt; unsupported legacy inputs stay available."""
    from asb_resources import resource_name
    identity = {key:record[key] for key in ["attempt","daemon","token"]}
    try:
        services = configuration.get("services")
        if not isinstance(services,dict) or not 1 <= len(services) <= 16:
            raise MaterialError("prepared service population exceeds bounds")
        roles, names = {}, set()
        for role, service in services.items():
            if not logical(role) or not isinstance(service,dict):
                raise MaterialError("prepared service role is invalid")
            name = service.get("container_name")
            if not resource_name("containers",name,record["attempt"]) or name in names:
                raise MaterialError("prepared service identity is ambiguous")
            names.add(name)
            hostname = service.get("hostname")
            if hostname is not None and (not isinstance(hostname,str) or not 1 <= len(hostname) <= 253
                    or any(ord(char) < 32 or ord(char) == 127 for char in hostname)):
                raise MaterialError("prepared hostname declaration is invalid")
            roles[role] = {"container":name,"hostname":hostname,
                "networks":copy.deepcopy(service.get("networks"))}
        value = {"schema_version":1,**identity,"available":True,"services":roles}
        for kind in ["networks","volumes"]:
            resources = configuration.get(kind,{})
            if not isinstance(resources,dict) or len(resources) > 32:
                raise MaterialError("prepared resource role population exceeds bounds")
            value[kind] = {}
            for role, resource in resources.items():
                if (not logical(role) or not isinstance(resource,dict) or resource.get("external")
                        or not resource_name(kind,resource.get("name"),record["attempt"])):
                    raise MaterialError("prepared resource role is invalid or external")
                value[kind][role] = {"name":resource["name"]}
                if kind == "networks":
                    # Preserve creation intent, not a post-creation inspect
                    # guess. An omitted IPAM pool permits daemon allocation;
                    # an explicitly selected subnet/gateway does not. Retain
                    # unknown options too so topology coverage can refuse them
                    # rather than silently erasing a dependency.
                    definition = copy.deepcopy(resource)
                    definition.pop("name",None)
                    definition.pop("labels",None)
                    value[kind][role]["definition"] = definition
            if len({item["name"] for item in value[kind].values()}) != len(value[kind]):
                raise MaterialError("prepared resource roles identify duplicate resources")
        canonical_digest(value,"asb-private-service-declaration-v1")
        return value
    except (AttributeError,ValueError,KeyError,TypeError,MaterialError):
        return {"schema_version":1,**identity,"available":False}


def resolve(task, deadline):
    """Resolve every declared role to its exact live owned resource, never list."""
    from asb_resources import daemon_identity, owned_resource
    journal = task.journal
    def check():
        if (time.monotonic() >= deadline or journal.lock is None
                or (task.cancelled is not None and task.cancelled.is_set())):
            raise MaterialError("prepared service observation was cancelled or lost its owner")
    check()
    receipt = read(journal,"material_services","material-services.json")
    declarations = journal.record.get("resources")
    if not isinstance(declarations,list) or len(declarations) > 64:
        raise MaterialError("prepared service resource declarations exceed bounds")
    for kind, key in [("containers","container"),("networks","name"),("volumes","name")]:
        expected = [item[key] for item in receipt["services" if kind == "containers" else kind].values()]
        actual = [item["name"] for item in declarations if item.get("kind") == kind]
        if len(expected) != len(set(expected)) or sorted(expected) != sorted(actual):
            raise MaterialError("prepared service roles differ from the complete owner declarations")
    try:
        check()
        if daemon_identity(task.client) != journal.record["daemon"]:
            raise MaterialError("prepared services selected another resource daemon")
        check()
        resources, identities = {}, set()
        for role,item in sorted(receipt["services"].items()):
            resource = owned_resource(task.client,{"kind":"containers","name":item["container"]},journal.labels)
            check()
            if resource is None:
                raise MaterialError("prepared declared service is missing")
            resource.reload()
            check()
            state = resource.attrs.get("State",{})
            if (not isinstance(resource.id,str) or not re.fullmatch(r"[0-9a-f]{64}",resource.id)
                    or resource.id in identities or state.get("Running") is not True
                    or state.get("Paused") or state.get("Restarting")):
                raise MaterialError("prepared service identity or running state is invalid")
            # reload must not swap ownership evidence after the first lookup.
            labels = resource.attrs.get("Config",{}).get("Labels") or {}
            if (resource.name.lstrip("/") != item["container"]
                    or any(labels.get(key) != value for key,value in journal.labels.items())):
                raise MaterialError("prepared service ownership changed during inspection")
            identities.add(resource.id)
            resources[role] = resource
    except MaterialError:
        raise
    except Exception:
        raise MaterialError("prepared service ownership could not be confirmed") from None
    primary = [role for role,resource in resources.items() if resource.id == task.workspace.container.id]
    if len(primary) != 1:
        raise MaterialError("prepared service set does not identify the task primary")
    check()
    return receipt,resources,primary[0]
