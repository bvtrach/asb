"""Prepare one owned root sampler; compose whole-environment rounds above it.

Unsupported declarations fail; no definition-only fallback exists. Preparation
stages instrumentation before any environment sampling begins.
"""
import time

from asb_material import MaterialError, RUNTIME_FILES, RUNTIME_ROOTS, canonical_digest, change_inventory, image_digest, path_name
from asb_material_exec import ObservationBudget, observe, run

RUNTIME = "/opt/asb-runtime"


class PreparedSampler:
    """One prepared service's census and guest-validated bind views."""
    def __init__(self, sample, bind_views):
        self._sample, self._bind_views = sample,bind_views

    def __call__(self, budget=None):
        return self._sample(budget)

    def bind_views(self):
        return self._bind_views()


def prepare(task, exclusions, binary, deadline):
    """Stage one service, then return a callable taking one complete observation.

Callers must compare two whole-environment rounds. Two observations of each
service in isolation cannot establish a stable multi-service environment.
"""
    from asb_docker import docker_command
    from asb_staging import configured_owner, stage_runtime, verify_runtime as verify_owned_runtime
    from asb_execution import ExecutionError
    from asb_runtime import bundle
    container = task.workspace.container
    configured_command = [*docker_command(), "exec", "-i", container.id]
    # The read-only whole-root observer needs access to root-owned material.
    # This changes only observation privilege inside this exact owned container,
    # never the configured installer/client/SDK-shell identity.
    command = [*docker_command(), "exec", "--user", "0", "-i", container.id]
    cancelled = task.cancelled

    def check():
        if time.monotonic() >= deadline or (cancelled is not None and cancelled.is_set()):
            raise MaterialError("prepared root observation was cancelled or timed out")

    def checked(arguments):
        return run([*command, *arguments], deadline=deadline, cancelled=cancelled)

    def configured_checked(arguments):
        return run([*configured_command,*arguments],deadline=deadline,cancelled=cancelled)

    from asb_material_binds import beneath, verify as verify_binds
    from asb_material_mounts import plan as mount_plan, targets, require_local_kernel
    kernel_identity = None

    def mounted(validate_kernel=True):
        from asb_resources import owned_resource, RecoveryError
        mounts = targets(container.attrs,RUNTIME)
        binds, volumes = {}, {}
        if any(item["Type"] == "bind" for item in mounts):
            if validate_kernel:
                if kernel_identity is None:
                    raise MaterialError("prepared bind topology changed before local daemon verification")
                require_local_kernel(container.attrs,kernel_identity)
            binds = verify_binds(task.journal,container)
        for item in mounts:
            if item["Type"] != "volume":
                continue
            matches = [entry for entry in task.journal.record["resources"]
                if entry.get("kind") == "volumes" and entry.get("name") == item.get("Name")]
            if len(matches) != 1:
                raise MaterialError("prepared mounted volume is not declared by its task owner")
            check()
            try:
                volume = owned_resource(task.client,matches[0],task.journal.labels)
            except RecoveryError:
                raise MaterialError("prepared mounted volume ownership changed") from None
            check()
            if volume is None:
                raise MaterialError("prepared mounted volume disappeared")
            volumes[item["Name"]] = volume.attrs
        return mount_plan(container.attrs,binds,volumes,RUNTIME)

    check(); container.reload(); check()
    initial_mounts = mounted(False)
    def root_checked(arguments):
        return run([*docker_command(), "exec", "--user", "0", "-i", container.id, *arguments],
                   deadline=deadline, cancelled=cancelled)
    try:
        check(); files, _receipt = bundle(binary, container.image.attrs.get("Architecture")); check()
        configured_owner(task,configured_checked)
        stage_runtime(task,binary,files,configured_checked,root_checked,deadline=deadline,cancelled=cancelled)
    except ExecutionError as error:
        raise MaterialError(str(error)) from None

    def verify_runtime():
        check()
        try:
            verify_owned_runtime(task,binary,checked,deadline=deadline,cancelled=cancelled)
        except ExecutionError as error:
            raise MaterialError(str(error)) from None
        check()
    workdir = task.workspace.workdir.rstrip("/")
    if not workdir:
        workdir = "/"
    if not workdir.startswith("/") or (workdir != "/" and path_name(workdir[1:]) != workdir[1:]):
        raise MaterialError("prepared material workspace is invalid")
    policy = [workdir.rstrip("/")+"/"+path_name(name) for name in exclusions]

    def snapshot():
        check(); container.reload(); check()
        topology = mounted()
        hidden = sorted({*RUNTIME_ROOTS, *RUNTIME_FILES, RUNTIME, *policy,
            *(item["target"] for item in topology["mounts"])})
        image = container.image
        check(); image.reload(); check()
        anchor = image_digest(image.attrs, container.attrs.get("Config", {}))
        changes = [[path, kind] for path, kind in change_inventory(container.diff())
            if not any(beneath(path,name) for name in hidden)]
        check()
        return {"image":anchor,"changes":changes,"topology":topology,"hidden":hidden}

    validated_mounts = None

    def mounted_roots(topology):
        roots = []
        binds = verify_binds(task.journal,container) if any(
            item["kind"] == "bind" for item in topology["mounts"]) else {}
        for root in topology["roots"]:
            root = dict(root,exclusions=list(root.get("exclusions",[])))
            if root["path"] in binds:
                root["expected_identity"] = binds[root["path"]]["chain"][-1]
            for path in policy:
                if beneath(root["path"],path):
                    raise MaterialError("semantic material exclusion hides a mounted root")
                if beneath(path,root["path"]):
                    root["exclusions"].append(path[len(root["path"].rstrip("/"))+1:])
            roots.append(root)
        return roots

    def bind_views():
        nonlocal validated_mounts
        witness, validated_mounts = validated_mounts,None
        if witness is None:
            raise MaterialError("prepared bind roots lack current guest inode coverage")
        # A successful guest census must positively match source/root inodes
        # before any host observer may read the daemon's declared source paths.
        check(); container.reload(); check()
        verify_runtime()
        topology = mounted()
        if topology["raw_sha256"] != witness:
            raise MaterialError("prepared bind roots lack current guest inode coverage")
        binds = verify_binds(task.journal,container)
        views = [{"name":root["name"],"source":binds[root["path"]]["source"],
            "identity":root["expected_identity"],"exclusions":root["exclusions"]}
            for root in mounted_roots(topology) if root["path"] in binds]
        validated_mounts = witness
        return views

    def sample(budget=None):
        nonlocal kernel_identity
        nonlocal validated_mounts
        validated_mounts = None
        verify_runtime()
        if kernel_identity is None and any(item["kind"] == "bind" for item in initial_mounts["mounts"]):
            kernel_identity = checked(["/bin/sh","-c",'exec /opt/asb-runtime/asb __asb-target "$$"'])
        before = snapshot()
        roots = [{"name":"filesystem", "path":"/", "image_sha256":before["image"],
            "content_paths":[path[1:] for path, kind in before["changes"] if kind != 2],
            "exclusions":[name[1:] for name in before["hidden"]]}]
        roots.extend(mounted_roots(before["topology"]))
        result = observe([*command, RUNTIME+"/asb", "__asb-material"], roots,
            deadline=min(deadline, time.monotonic()+90), cancelled=cancelled, budget=budget)
        after = snapshot()
        verify_runtime()
        if before != after:
            raise MaterialError("prepared image or change inventory changed during root observation")
        validated_mounts = after["topology"]["raw_sha256"]
        topology = {key:value for key,value in before["topology"].items() if key != "raw_sha256"}
        return {"image_sha256":before["image"], "changes":before["changes"],
            "topology":topology,"files":result}
    return PreparedSampler(sample,bind_views)


def capture(task, definition, exclusions, binary, deadline):
    sample = prepare(task,exclusions,binary,deadline)
    budget = ObservationBudget(observations=2)
    rounds = [sample(budget),sample(budget)]
    if rounds[0] != rounds[1]:
        raise MaterialError("prepared filesystem changed between complete root observations")
    # Helper staging is instrumentation, not workload content. Its implementation
    # is the versioned observation protocol; root exclusions are exact.
    sha256 = canonical_digest({"scope":"single-owned-container-root-mounts-v2", "definition":definition,
        "runtime_exclusions":[*RUNTIME_ROOTS, *RUNTIME_FILES], "instrumentation_root":RUNTIME,
        "observation":rounds[0]}, "asb-prepared-workload-material-v1")
    return {"schema_version":1, "scope":"prepared-workload-material-v1", "sha256":sha256}
