"""Whole-environment observation for explicitly covered owned service sets.

Owned default-IPAM local bridges and network-none environments are modeled.
External networks and unobservable cross-service bind aliases are refused.
"""
import time
from types import SimpleNamespace

from asb_material import MaterialError, canonical_digest
from asb_material_binds import beneath, verify as verify_binds
from asb_material_exec import ObservationBudget
from asb_material_mounts import plan, storage_groups, targets
from asb_material_runtime import configuration, normalize, read_files
from asb_material_services import resolve
from asb_material_root import RUNTIME, prepare
from asb_material_networks import observe as observe_networks
from asb_material_network_identity import canonicalize
from asb_material_network_runtime import validate_sandboxes


def capture(task, definition, exclusions, binary, deadline):
    from asb_docker import docker_command
    from asb_resources import owned_resource
    def check():
        if time.monotonic() >= deadline or (task.cancelled is not None and task.cancelled.is_set()):
            raise MaterialError("prepared environment observation was cancelled or timed out")
    receipt,resources,primary = resolve(task,deadline)
    def network_snapshot(live):
        if not receipt["networks"] and not receipt.get("external_networks"):
            return None,{}
        raw = observe_networks(task,deadline)
        if (raw["services"] != {role:resource.id for role,resource in live.items()}
                or raw["service_intent"] != receipt["services"]):
            raise MaterialError("prepared network service authority changed during observation")
        public,addresses = canonicalize(raw)
        validate_sandboxes({role:resource.attrs for role,resource in live.items()})
        return (raw,public),addresses
    network,_addresses = network_snapshot(resources)
    # Refuse unsupported runtime before any helper is staged.
    for role,resource in resources.items():
        configuration(resource.attrs,role,receipt["services"][role],network)
    contexts = getattr(task,"_material_service_contexts",None)
    if contexts is None:
        contexts = {}
        for role,resource in resources.items():
            workdir = task.workspace.workdir if role == primary else resource.attrs["Config"].get("WorkingDir") or "/"
            contexts[role] = SimpleNamespace(client=task.client,journal=task.journal,
                artifacts=task.artifacts,cancelled=task.cancelled,
                identity=resource.id,
                workspace=SimpleNamespace(container=resource,workdir=workdir))
        task._material_service_contexts = contexts
    if (set(contexts) != set(resources) or any(contexts[role].identity != resource.id
            for role,resource in resources.items())):
        raise MaterialError("prepared service identity changed after instrumentation")
    budget = ObservationBudget(observations=2*len(resources))
    command = docker_command()
    samplers = {}
    for role,context in contexts.items():
        check()
        samplers[role] = prepare(context,exclusions if role == primary else [],binary,deadline)
    role_names = {item["container"]:role for role,item in receipt["services"].items()}
    explicit_hostnames = {item["hostname"] for item in receipt["services"].values() if item["hostname"] is not None}
    role_names = {name:role for name,role in role_names.items() if name not in explicit_hostnames}

    def snapshot():
        check()
        current,live,current_primary = resolve(task,deadline)
        if current != receipt or current_primary != primary or any(
                resource.id != contexts[role].identity for role,resource in live.items()):
            raise MaterialError("prepared service set changed during environment observation")
        network,addresses = network_snapshot(live)
        volume_attributes = {}
        for item in receipt["volumes"].values():
            check()
            volume = owned_resource(task.client,{"kind":"volumes","name":item["name"]},task.journal.labels)
            check()
            if volume is None:
                raise MaterialError("prepared environment volume disappeared")
            volume_attributes[item["name"]] = volume.attrs
        public,private,names,source_roles = {},{},[],[]
        source_list = []
        used_volumes = set()
        for role,resource in live.items():
            check()
            attrs = resource.attrs
            runtime = configuration(attrs,role,receipt["services"][role],network)
            mounts = targets(attrs,RUNTIME)
            binds = verify_binds(task.journal,resource) if any(item["Type"] == "bind" for item in mounts) else {}
            topology = plan(attrs,binds,volume_attributes,RUNTIME)
            raw_files = read_files([*command,"exec","-i",resource.id,RUNTIME+"/asb","__asb-material","--runtime-files"],
                budget,deadline=min(deadline,time.monotonic()+30),cancelled=task.cancelled)
            files = normalize(raw_files,role,attrs,receipt["services"][role],role_names,addresses.get(role))
            canonical_mounts = topology["mounts"]
            for actual,mount in zip(mounts,canonical_mounts):
                target,kind = actual["Destination"],actual["Type"]
                if kind == "bind":
                    declared = binds[target]
                    source = (kind,declared["source"],declared["chain"][-1][:2])
                elif kind == "volume":
                    used_volumes.add(actual["Name"])
                    source = (kind,actual["Name"],mount["options"].get("VolumeOptions",{}).get("Subpath",""))
                else:
                    source = (kind,role+":"+target,"")
                source_list.append(source)
                names.append(role+"/"+mount["name"])
                source_roles.append(role)
            public[role] = {"runtime":runtime,"files":files,
                "mounts":{key:value for key,value in topology.items() if key != "raw_sha256"}}
            private[role] = {"id":resource.id,"image":attrs.get("Image"),
                "pid":attrs.get("State",{}).get("Pid"),"files":raw_files,
                "raw":canonical_digest({"config":attrs.get("Config"),"host":attrs.get("HostConfig"),
                    "networks":attrs.get("NetworkSettings"),"mounts":attrs.get("Mounts")},"asb-private-service-runtime-v1")}
        if used_volumes != set(volume_attributes):
            raise MaterialError("prepared environment has an unobserved declared volume")
        for left,a in enumerate(source_list):
            for right,b in enumerate(source_list[left+1:],left+1):
                if (source_roles[left] != source_roles[right] and a[0] == b[0] == "bind"
                        and a[2][0] == b[2][0] and a[2] != b[2]
                        and not beneath(a[1],b[1]) and not beneath(b[1],a[1])):
                    raise MaterialError("disjoint service binds require global inode coverage")
        groups,aliases = storage_groups(source_list,names)
        state = {"services":public,"storage":dict(zip(names,groups)),"aliases":aliases}
        if network is not None:
            state["networks"] = network[1]
        private = {"services":private,"networks":network[0] if network is not None else None}
        canonical_digest({"state":state,"private":private},"asb-private-environment-snapshot-v1")
        return state,private

    rounds,identities = [],[]
    for _ in range(2):
        before = snapshot()
        samples = {}
        for role,sample in samplers.items():
            check()
            samples[role] = sample(budget)
        after = snapshot()
        if before != after:
            raise MaterialError("prepared environment changed during a complete service round")
        rounds.append({"topology":before[0],"observations":samples})
        identities.append(before[1])
    if rounds[0] != rounds[1] or identities[0] != identities[1]:
        raise MaterialError("prepared environment changed between complete service rounds")
    check()
    return {"schema_version":1,"scope":"prepared-workload-material-v1",
        "sha256":canonical_digest({"scope":"shared-default-bridge-workload-environment-v1" if receipt.get("external_networks")
            else "owned-bridge-environment-v1" if receipt["networks"] else "owned-network-none-environment-v1",
            "definition":definition,"primary":primary,"observation":rounds[0]},"asb-prepared-workload-material-v1")}
