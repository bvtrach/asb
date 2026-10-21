"""Canonical guest-visible mount roots and private backing-alias relationships.

Based on Moby's mount.Mount/container.MountPoint contracts. Host sources and
numeric inode witnesses never enter the public material identity. Bind witnesses
must come from the adapter's pre-creation declaration and a proven local daemon.
"""
import copy
import json
import re
from pathlib import Path

from asb_material import MaterialError, canonical_digest, path_name
from asb_material_binds import absolute, beneath


def targets(attributes, runtime):
    mounts = attributes.get("Mounts")
    host = attributes.get("HostConfig",{})
    if not isinstance(mounts,list) or len(mounts) > 32 or not isinstance(host,dict):
        raise MaterialError("prepared mount topology exceeds bounds or is unavailable")
    mounts = [dict(item) if isinstance(item,dict) else {} for item in mounts]
    tmpfs = host.get("Tmpfs") or {}
    if not isinstance(tmpfs,dict) or len(tmpfs) > 32:
        raise MaterialError("prepared tmpfs topology exceeds bounds")
    for target, options in tmpfs.items():
        if not isinstance(options,str) or len(options) > 1024 or any(ord(char) < 32 for char in options):
            raise MaterialError("prepared tmpfs options are invalid")
        if not any(item.get("Destination") == target for item in mounts):
            mounts.append({"Type":"tmpfs","Source":"","Destination":target,
                "RW":"ro" not in str(options).split(","),"Mode":options,"Propagation":""})
    configured = host.get("Mounts") or []
    if not isinstance(configured,list) or len(configured) > 32:
        raise MaterialError("prepared mount options exceed bounds")
    for item in configured:
        if not isinstance(item,dict):
            raise MaterialError("prepared mount options are invalid")
        if item.get("Type") != "tmpfs":
            continue
        target = absolute(item.get("Target"))
        if target in tmpfs:
            raise MaterialError("prepared tmpfs has conflicting option declarations")
        if type(item.get("ReadOnly",False)) is not bool:
            raise MaterialError("prepared tmpfs access is invalid")
        if not any(point.get("Destination") == target for point in mounts):
            # Docker may retain tmpfs only in HostConfig, unlike persistent
            # MountPoints. The running guest census still opens this root and
            # observes its actual metadata/content; this is not a file fallback.
            mounts.append({"Type":"tmpfs","Source":"","Destination":target,
                "RW":not item.get("ReadOnly",False),"Mode":"","Propagation":""})
    seen = set()
    for item in mounts:
        target = absolute(item.get("Destination"))
        if (set(item)-{"Type","Name","Source","Destination","Driver","Mode","RW","Propagation"}
                or target in seen or len(mounts) > 32 or item.get("Type") not in ["bind","volume","tmpfs"]
                or beneath(runtime,target) or beneath(target,runtime)):
            raise MaterialError("prepared mount is duplicate, unsupported or shadows instrumentation")
        seen.add(target)
    return sorted(mounts,key=lambda item:item["Destination"])


def require_local_kernel(attributes, identity_bytes, proc=Path("/proc")):
    """Match the owned container's daemon PID/cgroup and guest kernel boot ID.

    No host source contents are read. Remote/VM Docker daemons cannot authorize
    local bind-path witnesses merely by using the same pathname strings.
    """
    pid = attributes.get("State",{}).get("Pid")
    identity = attributes.get("Id")
    try:
        guest = json.loads(identity_bytes)
    except (ValueError,UnicodeError):
        raise MaterialError("prepared bind daemon is not proven local") from None
    if (type(pid) is not int or pid <= 1 or attributes.get("State",{}).get("Running") is not True or not isinstance(identity,str)
            or not re.fullmatch(r"[0-9a-f]{64}",identity)
            or not isinstance(guest,dict) or set(guest) != {"boot_id","namespace_pid","start_time_ticks"}
            or not isinstance(guest["boot_id"],str) or not re.fullmatch(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}",guest["boot_id"])
            or type(guest["namespace_pid"]) is not int or guest["namespace_pid"] <= 1
            or type(guest["start_time_ticks"]) is not int or guest["start_time_ticks"] <= 0):
        raise MaterialError("prepared bind daemon is not proven local")
    def read(name):
        with (proc/str(pid)/name).open("rb") as source:
            value = source.read(65537)
        if len(value) > 65536:
            raise MaterialError("prepared bind kernel identity exceeds bounds")
        return value.decode("ascii")
    try:
        before = read("stat").rsplit(")",1)[1].split()[19]
        cgroup = read("cgroup")
        with (proc/"sys/kernel/random/boot_id").open("rb") as source:
            boot = source.read(65).decode("ascii").strip()
        namespace = next((line.split()[1:] for line in read("status").splitlines() if line.startswith("NSpid:")),[])
        if (boot != guest["boot_id"] or not namespace or namespace[0] != str(pid) or namespace[-1] != "1"
                or not any(part in [identity,"docker-"+identity+".scope"]
                    for line in cgroup.splitlines() for part in line.split(":",2)[-1].split("/"))
                or read("stat").rsplit(")",1)[1].split()[19] != before):
            raise MaterialError("prepared bind daemon is not proven local")
    except (OSError,ValueError,IndexError,UnicodeError):
        raise MaterialError("prepared bind daemon is not proven local") from None


def plan(attributes, binds, volumes, runtime):
    actual = targets(attributes,runtime)
    host = attributes.get("HostConfig",{})
    if host.get("IpcMode") not in [None,"","private"]:
        raise MaterialError("prepared shared or external IPC needs explicit topology coverage")
    if host.get("ShmSize") is not None and (type(host["ShmSize"]) is not int or not 0 <= host["ShmSize"] <= 2**63-1):
        raise MaterialError("prepared shared-memory size is invalid")
    configured = host.get("Mounts") or []
    legacy = host.get("Binds") or []
    if not isinstance(configured,list) or len(configured) > 32 or not isinstance(legacy,list) or len(legacy) > 32:
        raise MaterialError("prepared mount options exceed bounds")
    options = {}
    for item in configured:
        if (not isinstance(item,dict) or set(item)-{"Type","Source","Target","ReadOnly","Consistency",
                "BindOptions","VolumeOptions","TmpfsOptions"} or item.get("Target") in options):
            raise MaterialError("prepared mount has unsupported or duplicate options")
        options[absolute(item.get("Target"))] = item
    for item in legacy:
        if not isinstance(item,str) or len(item) > 8192 or len(item.split(":")) not in [2,3]:
            raise MaterialError("prepared legacy mount options are ambiguous")
        parts = item.split(":")
        target = absolute(parts[1])
        if target in options:
            raise MaterialError("prepared mount has conflicting option declarations")
        options[target] = {"Source":parts[0],"Target":target,"LegacyMode":parts[2] if len(parts)==3 else ""}
    declared_targets = {item["Destination"] for item in actual}
    if set(options)-declared_targets:
        raise MaterialError("prepared mount option target is not actually mounted")
    # Bound all private option strings/structure before retaining derived copies.
    canonical_digest({"mounts":actual,"options":options,"tmpfs":host.get("Tmpfs")},"asb-private-mount-options-v1")
    canonical, roots, sources = [], [], []
    for index,item in enumerate(actual):
        target, kind = item["Destination"],item["Type"]
        declared = options.get(target,{})
        if (type(item.get("RW")) is not bool or item.get("Propagation","") not in ["","private","rprivate"]
                or not isinstance(item.get("Mode",""),str) or len(item.get("Mode","")) > 1024
                or any(ord(char) < 32 for char in item.get("Mode",""))
                or ("Type" in declared and declared["Type"] != kind)
                or ("ReadOnly" in declared and (type(declared["ReadOnly"]) is not bool or declared["ReadOnly"] == item["RW"]))):
            raise MaterialError("prepared mount access or propagation is unsupported or contradictory")
        remaining = copy.deepcopy({key:value for key,value in declared.items() if key not in ["Source","Target","Type"]})
        bind_options = remaining.get("BindOptions") or {}
        if (not isinstance(bind_options,dict) or set(bind_options)-{"Propagation","NonRecursive","CreateMountpoint","ReadOnlyNonRecursive","ReadOnlyForceRecursive"}
                or bind_options.get("Propagation") not in [None,"","private","rprivate"]
                or any(type(value) is not bool for key,value in bind_options.items() if key != "Propagation")):
            raise MaterialError("prepared bind options require unsupported propagation or values")
        volume_options = remaining.get("VolumeOptions") or {}
        if not isinstance(volume_options,dict) or set(volume_options)-{"NoCopy","Labels","Subpath","DriverConfig"}:
            raise MaterialError("prepared volume options are unsupported")
        tmpfs_options = remaining.get("TmpfsOptions") or {}
        if (not isinstance(tmpfs_options,dict) or set(tmpfs_options)-{"SizeBytes","Mode","Options"}
                or any(type(tmpfs_options[key]) is not int or not 0 <= tmpfs_options[key] <= limit
                    for key,limit in [("SizeBytes",2**63-1),("Mode",0o7777)] if key in tmpfs_options)):
            raise MaterialError("prepared tmpfs options are unsupported or invalid")
        flags = tmpfs_options.get("Options",[])
        if (not isinstance(flags,list) or len(flags) > 64 or any(
                not isinstance(flag,list) or len(flag) not in [1,2]
                or any(not isinstance(value,str) or not value or len(value) > 1024
                    or any(ord(char) < 32 for char in value) for value in flag) for flag in flags)):
            raise MaterialError("prepared tmpfs option flags are invalid")
        subpath = volume_options.get("Subpath","")
        if not isinstance(subpath,str) or (subpath and path_name(subpath) != subpath):
            raise MaterialError("prepared volume subpath is invalid")
        if kind == "bind":
            witness = binds.get(target)
            if witness is None or item.get("Source") != witness["source"]:
                raise MaterialError("prepared bind differs from its owned creation declaration")
            if "Source" in declared and declared["Source"] != witness["source"]:
                raise MaterialError("prepared bind options identify another source")
            source = (kind,witness["source"],witness["chain"][-1][:2])
        elif kind == "volume":
            volume = volumes.get(item.get("Name"))
            if (volume is None or volume.get("Driver") != "local" or volume.get("Scope") != "local"
                    or volume.get("Options") not in [None,{}] or item.get("Driver","local") != "local"):
                raise MaterialError("prepared volume lacks supported owned local storage")
            if "Source" in declared and declared["Source"] != item["Name"]:
                raise MaterialError("prepared volume options identify another source")
            source = (kind,item["Name"],subpath)
            # Resource labels establish ownership separately, not runtime content.
            if "Labels" in volume_options:
                remaining = dict(remaining,VolumeOptions={key:value for key,value in volume_options.items() if key != "Labels"})
        else:
            if item.get("Source") not in [None,""] or declared.get("Source") not in [None,""]:
                raise MaterialError("prepared tmpfs unexpectedly identifies external storage")
            source = (kind,target,"")
        name = f"mount-{index+1:03}"
        nested = [other["Destination"][len(target.rstrip("/"))+1:] for other in actual
                  if other["Destination"] != target and beneath(other["Destination"],target)]
        roots.append({"name":name,"path":target,"exclusions":sorted(nested)})
        canonical.append({"name":name,"target":target,"kind":kind,"rw":item["RW"],
            "mode":item.get("Mode",""),"propagation":item.get("Propagation",""),
            "options":remaining,"tmpfs":(host.get("Tmpfs") or {}).get(target)})
        sources.append(source)
    storage = {}
    for index,source in enumerate(sources):
        key = (source[0],source[2][0] if source[0] == "bind" else source[1])
        canonical[index]["backing"] = storage.setdefault(key,canonical[index]["name"])
    aliases = []
    for left,a in enumerate(sources):
        for right,b in enumerate(sources[left+1:],left+1):
            relative, parent, child = None,left,right
            if a[0] == b[0] == "bind":
                if a[2] == b[2]:
                    relative = ""
                elif beneath(b[1],a[1]):
                    relative = b[1][len(a[1].rstrip("/"))+1:]
                elif beneath(a[1],b[1]):
                    relative, parent, child = a[1][len(b[1].rstrip("/"))+1:],right,left
            elif a[0] == b[0] == "volume" and a[1] == b[1]:
                if a[2] == b[2]:
                    relative = ""
                elif not a[2] or b[2].startswith(a[2]+"/"):
                    relative = b[2][len(a[2])+1:] if a[2] else b[2]
                elif not b[2] or a[2].startswith(b[2]+"/"):
                    relative, parent, child = (a[2][len(b[2])+1:] if b[2] else a[2]),right,left
            if relative is not None:
                aliases.append({"parent":canonical[parent]["name"],"child":canonical[child]["name"],"relative":relative})
    if "/dev/shm" not in declared_targets:
        roots.append({"name":"shared-memory","path":"/dev/shm", "exclusions":sorted(
            target[len("/dev/shm/"):] for target in declared_targets if target.startswith("/dev/shm/"))})
    return {"mounts":canonical,"aliases":aliases,"roots":roots,"shm_bytes":host.get("ShmSize"),
        "raw_sha256":canonical_digest({"actual":actual,"configured":configured,"legacy":legacy,
            "tmpfs":host.get("Tmpfs"),"ipc":host.get("IpcMode"),"shm":host.get("ShmSize")},"asb-private-mount-topology-v1")}
