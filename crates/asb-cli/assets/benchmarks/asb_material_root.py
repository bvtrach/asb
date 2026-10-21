"""Prepare one owned root sampler; compose whole-environment rounds above it.

Unsupported declarations fail; no definition-only fallback exists. Preparation
stages instrumentation before any environment sampling begins.
"""
import time

from asb_material import MaterialError, RUNTIME_FILES, RUNTIME_ROOTS, canonical_digest, change_inventory, image_digest, path_name, tar_digest
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
    from asb_guest import upload
    from asb_runtime import bundle
    container = task.workspace.container
    command = [*docker_command(), "exec", "-i", container.id]
    cancelled = task.cancelled

    def check():
        if time.monotonic() >= deadline or (cancelled is not None and cancelled.is_set()):
            raise MaterialError("prepared root observation was cancelled or timed out")

    def checked(arguments):
        return run([*command, *arguments], deadline=deadline, cancelled=cancelled)

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
    state = getattr(task, "_material_runtime", None)
    if state is None:
        # Both ancestors and destination are checked before the first write.
        # Nothing in the original workload may be overwritten or followed.
        checked(["/bin/sh", "-c", "test -d /opt && test ! -L /opt && test ! -e /opt/asb-runtime && test ! -L /opt/asb-runtime && mkdir -m 700 /opt/asb-runtime && mkdir -m 700 /opt/asb-runtime/bin /opt/asb-runtime/lib"])
        check(); files, receipt = bundle(binary, container.image.attrs.get("Architecture")); check()
        upload(container, RUNTIME, files, temporary_dir=task.artifacts)
        check()
        expected = {name:{"kind":"file", "mode":mode, "bytes":len(content),
            "sha256":receipt["files"][index]["sha256"]} for index,(name,content,mode) in enumerate(files)}
        expected.update({name:{"kind":"5","mode":0o700} for name in ["","bin","lib"]})
        state = task._material_runtime = {"container":container.id, "binary":str(binary),
            "expected":expected, "verified":None}
    elif state.get("container") != container.id or state.get("binary") != str(binary):
        raise MaterialError("prepared material helper ownership changed")

    def verify_runtime():
        checked(["/bin/sh", "-c", "test -d /opt && test ! -L /opt && test -d /opt/asb-runtime && test ! -L /opt/asb-runtime"])
        check()
        chunks, metadata = container.get_archive(RUNTIME, chunk_size=65536, encode_stream=False)
        try:
            if metadata.get("name") != "asb-runtime" or metadata.get("linkTarget"):
                raise MaterialError("prepared material helper root was substituted")
            digest = tar_digest(chunks,"asb-runtime", deadline=deadline, cancelled=cancelled,
                expected_root_kind=b"5", expected_entries=state["expected"],
                max_entries=128, max_bytes=384*1024**2, max_wire=385*1024**2)
        finally:
            chunks.close()
        check()
        if state["verified"] is not None and state["verified"] != digest:
            raise MaterialError("prepared material helper metadata changed")
        state["verified"] = digest
        checked(["/bin/sh", "-c", "test -d /opt && test ! -L /opt && test -d /opt/asb-runtime && test ! -L /opt/asb-runtime"])
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
