"""Whole-environment observation for explicitly covered owned service sets.

Owned default-IPAM local bridges and network-none environments are modeled.
Cross-service bind aliases use a joint census after positive guest-root proof.
"""
import time
from types import SimpleNamespace

from asb_material import MaterialError, canonical_digest
from asb_material_binds import verify as verify_binds
from asb_material_bind_graph import plan as bind_graph_plan
from asb_material_exec import ObservationBudget, observe
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
    has_binds = any(item["Type"] == "bind" for resource in resources.values()
        for item in targets(resource.attrs,RUNTIME))
    budget = ObservationBudget(observations=min(32,(3*len(resources)+4) if has_binds else 2*len(resources)))
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
        public,private,names = {},{},[]
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
            public[role] = {"runtime":runtime,"files":files,
                "mounts":{key:value for key,value in topology.items() if key != "raw_sha256"}}
            private[role] = {"id":resource.id,"image":attrs.get("Image"),
                "pid":attrs.get("State",{}).get("Pid"),"files":raw_files,
                "raw":canonical_digest({"config":attrs.get("Config"),"host":attrs.get("HostConfig"),
                    "networks":attrs.get("NetworkSettings"),"mounts":mounts},"asb-private-service-runtime-v1")}
        if used_volumes != set(volume_attributes):
            raise MaterialError("prepared environment has an unobserved declared volume")
        groups,aliases = storage_groups(source_list,names)
        state = {"services":public,"storage":dict(zip(names,groups)),"aliases":aliases}
        if network is not None:
            state["networks"] = network[1]
        private = {"services":private,"networks":network[0] if network is not None else None}
        canonical_digest({"state":state,"private":private},"asb-private-environment-snapshot-v1")
        return state,private

    def bind_views():
        check()
        current,live,current_primary = resolve(task,deadline)
        if current != receipt or current_primary != primary or any(
                resource.id != contexts[role].identity for role,resource in live.items()):
            raise MaterialError("prepared bind service authority changed")
        views = []
        for role,sampler in samplers.items():
            check()
            if not any(item["Type"] == "bind" for item in targets(live[role].attrs,RUNTIME)):
                continue
            for view in sampler.bind_views():
                views.append(dict(view,name=role+"/"+view["name"]))
        return bind_graph_plan(views)

    def host_sample():
        before = bind_views()
        if not before["roots"]:
            raise MaterialError("prepared bind roots disappeared before joint observation")
        result = observe([str(binary),"__asb-material"],before["roots"],budget=budget,
            deadline=min(deadline,time.monotonic()+90),cancelled=task.cancelled)
        if before != bind_views():
            raise MaterialError("prepared bind witnesses changed during joint observation")
        return {"bindings":before["bindings"],"files":result}

    initial_samples, initial_state = None,None
    if has_binds:
        # Prove every guest-visible bind inode before the first host content
        # read; same-kernel evidence alone does not prove Source resolution.
        initial_state = snapshot()
        initial_samples = {role:sample(budget) for role,sample in samplers.items()}
        if initial_state != snapshot():
            raise MaterialError("prepared environment changed during initial guest bind validation")
    rounds,identities = [],[]
    for _ in range(2):
        before = snapshot()
        if initial_state is not None and before != initial_state:
            raise MaterialError("prepared environment differs from initial guest bind validation")
        host_before = host_sample() if has_binds else None
        samples = {}
        for role,sample in samplers.items():
            check()
            samples[role] = sample(budget)
        host_after = host_sample() if has_binds else None
        after = snapshot()
        if before != after or host_before != host_after or (initial_samples is not None and samples != initial_samples):
            raise MaterialError("prepared environment changed during a complete service round")
        observation = {"topology":before[0],"observations":samples}
        if has_binds:
            observation["bind_graph"] = host_before
        rounds.append(observation)
        identities.append(before[1])
    if rounds[0] != rounds[1] or identities[0] != identities[1]:
        raise MaterialError("prepared environment changed between complete service rounds")
    check()
    return {"schema_version":1,"scope":"prepared-workload-material-v1",
        "sha256":canonical_digest({"scope":"shared-default-bridge-workload-environment-v1" if receipt.get("external_networks")
            else "owned-bridge-environment-v1" if receipt["networks"] else "owned-network-none-environment-v1",
            "definition":definition,"primary":primary,"observation":rounds[0]},"asb-prepared-workload-material-v1")}
