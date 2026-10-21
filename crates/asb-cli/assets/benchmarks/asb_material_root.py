"""Compose actual owned root observations with image and deletion inventory.

This extends the old work-directory reader, not the final multi-service/mount
composer. Unsupported declarations fail; no definition-only fallback exists.
"""
import time

from asb_material import MaterialError, RUNTIME_FILES, RUNTIME_ROOTS, canonical_digest, change_inventory, image_digest, path_name, tar_digest
from asb_material_exec import observe, run

RUNTIME = "/opt/asb-runtime"


def capture(task, definition, exclusions, binary, deadline):
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

    check(); container.reload(); check()
    if container.attrs.get("Mounts") != []:
        raise MaterialError("mounted root material still requires explicit topology coverage")
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
    hidden = sorted({*RUNTIME_ROOTS, *RUNTIME_FILES, RUNTIME,
        *(workdir.rstrip("/")+"/"+path_name(name) for name in exclusions)})

    def excluded(path):
        return any(path == name or path.startswith(name+"/") for name in hidden)

    def snapshot():
        check(); container.reload(); check()
        if container.attrs.get("Mounts") != []:
            raise MaterialError("prepared root mount coverage changed")
        image = container.image
        check(); image.reload(); check()
        anchor = image_digest(image.attrs, container.attrs.get("Config", {}))
        changes = [[path, kind] for path, kind in change_inventory(container.diff()) if not excluded(path)]
        check()
        return anchor, changes

    rounds = []
    for _ in range(2):
        verify_runtime()
        before = snapshot()
        roots = [{"name":"filesystem", "path":"/", "image_sha256":before[0],
            "content_paths":[path[1:] for path, kind in before[1] if kind != 2],
            "exclusions":[name[1:] for name in hidden]},
            {"name":"shared-memory", "path":"/dev/shm"}]
        result = observe([*command, RUNTIME+"/asb", "__asb-material"], roots,
            deadline=min(deadline, time.monotonic()+90), cancelled=cancelled)
        after = snapshot()
        verify_runtime()
        if before != after:
            raise MaterialError("prepared image or change inventory changed during root observation")
        rounds.append({"image_sha256":before[0], "changes":before[1], "files":result})
    if rounds[0] != rounds[1]:
        raise MaterialError("prepared filesystem changed between complete root observations")
    # Helper staging is instrumentation, not workload content. Its implementation
    # is the versioned observation protocol; root exclusions are exact.
    sha256 = canonical_digest({"scope":"single-owned-container-root-v1", "definition":definition,
        "runtime_exclusions":[*RUNTIME_ROOTS, *RUNTIME_FILES], "instrumentation_root":RUNTIME,
        "observation":rounds[0]}, "asb-prepared-workload-material-v1")
    return {"schema_version":1, "scope":"prepared-workload-material-v1", "sha256":sha256}
