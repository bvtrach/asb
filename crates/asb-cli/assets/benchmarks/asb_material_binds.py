"""Private adapter-created bind declarations; metadata only, never file walks.

Failure leaves legacy preparation available but cannot authorize verified
material capture. Numeric kernel identities are private consistency witnesses,
not signatures or authenticated historical provenance.
"""
import json
import os
from pathlib import PurePosixPath
import stat

from asb_material import MaterialError


def absolute(value):
    if (not isinstance(value,str) or not value.startswith("/") or len(value.encode()) > 4096
            or str(PurePosixPath(value)) != value or ".." in value.split("/")
            or any(ord(char) < 32 for char in value) or value.startswith("//")):
        raise MaterialError("prepared bind declaration has an invalid absolute path")
    return value


def beneath(path, parent):
    return path == parent or path.startswith(parent.rstrip("/")+"/")


def witness(path, owned=None, create=False):
    """Pin a no-follow ancestor chain; create only SDK-declared owned directories."""
    absolute(path)
    descriptor = os.open("/",os.O_PATH | os.O_DIRECTORY | os.O_CLOEXEC)
    result, prefix = [], ""
    try:
        before = os.fstat(descriptor)
        result.append([before.st_dev,before.st_ino,stat.S_IFMT(before.st_mode)])
        for name in path.strip("/").split("/") if path != "/" else []:
            if not stat.S_ISDIR(os.fstat(descriptor).st_mode):
                raise MaterialError("prepared bind ancestor is not a directory")
            prefix += "/"+name
            try:
                child = os.open(name,os.O_PATH | os.O_NOFOLLOW | os.O_CLOEXEC,dir_fd=descriptor)
            except FileNotFoundError:
                if not create or owned is None or not beneath(prefix,owned):
                    raise
                os.mkdir(name,0o755,dir_fd=descriptor)
                child = os.open(name,os.O_PATH | os.O_NOFOLLOW | os.O_CLOEXEC,dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
            metadata = os.fstat(descriptor)
            if not (stat.S_ISDIR(metadata.st_mode) or stat.S_ISREG(metadata.st_mode)):
                raise MaterialError("prepared bind source has an unsupported or linked ancestor")
            result.append([metadata.st_dev,metadata.st_ino,stat.S_IFMT(metadata.st_mode)])
            if len(result) > 129:
                raise MaterialError("prepared bind ancestor depth exceeds bounds")
        return result
    finally:
        os.close(descriptor)


def declare(directory, configuration, record):
    """Called by the resource owner before Compose creates any service mounts."""
    identity = {key:record[key] for key in ["attempt","daemon","token"]}
    try:
        root = absolute(os.readlink(f"/proc/self/fd/{directory}"))
        declarations = []
        for service in configuration.get("services",{}).values():
            for item in service.get("volumes",[]):
                if item.get("type") != "bind":
                    continue
                source, target = absolute(item.get("source")),absolute(item.get("target"))
                if not beneath(source,root) or len(declarations) >= 32:
                    raise MaterialError("prepared bind is outside its owned attempt or exceeds bounds")
                create = item.get("bind",{}).get("create_host_path",False) is True
                declarations.append({"container":service["container_name"],"source":source,
                    "destination":target,"chain":witness(source,root,create)})
        value = {"schema_version":1, **identity,"available":True,"root":root,
            "root_chain":witness(root),"binds":sorted(declarations,key=lambda item:(item["container"],item["destination"]))}
        if len(json.dumps(value).encode()) > 1024**2:
            raise MaterialError("prepared bind declarations exceed metadata bounds")
        return value
    except (OSError,ValueError,KeyError,TypeError,MaterialError):
        return {"schema_version":1, **identity,"available":False}


def verify(journal, container):
    """Validate the exact durable creation declaration before using source aliases."""
    from asb_material_declarations import read
    value = read(journal,"material_binds","material-binds.json")
    if witness(value["root"]) != value["root_chain"]:
        raise MaterialError("prepared bind artifact root changed")
    result = {}
    for item in value["binds"]:
        if item["container"] != container.name.lstrip("/"):
            continue
        if (not beneath(item["source"],value["root"])
                or witness(item["source"]) != item["chain"] or item["destination"] in result):
            raise MaterialError("prepared bind source identity changed or is ambiguous")
        result[item["destination"]] = item
    return result
