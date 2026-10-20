"""Bounded same-architecture ASB runtime bundles for owned Linux task images.

Static ELF executables are copied directly. Dynamically linked executables
bring their own matching loader/libraries; no guest system library is replaced
and LD_LIBRARY_PATH is never inherited by benchmark clients.
"""
import hashlib
import os
from pathlib import Path
import re
import shlex
import struct
import subprocess

from asb_execution import ExecutionError

MAX_FILE = 256 * 1024**2
MAX_BUNDLE = 384 * 1024**2
ROOT = "/opt/asb-runtime"


def elf(path):
    path = Path(path)
    if not path.is_file() or path.stat().st_size > MAX_FILE:
        raise ExecutionError("ASB runtime executable is missing or exceeds 256 MiB")
    data = path.read_bytes()
    if data[:7] != b"\x7fELF\x02\x01\x01" or len(data) < 64:
        raise ExecutionError("ASB runtime must be a Linux little-endian ELF64 executable")
    if struct.unpack_from("<H",data,16)[0] not in [2,3] or struct.unpack_from("<I",data,20)[0] != 1:
        raise ExecutionError("ASB runtime has an invalid ELF executable type or version")
    machine = struct.unpack_from("<H",data,18)[0]
    if machine not in [62,183]:
        raise ExecutionError("ASB runtime supports Linux x86_64 and aarch64 task images")
    offset = struct.unpack_from("<Q",data,32)[0]
    size,count = struct.unpack_from("<HH",data,54)
    if (struct.unpack_from("<H",data,52)[0] != 64 or size < 56 or not 1 <= count <= 2048
            or offset < 64 or offset + size * count > len(data)):
        raise ExecutionError("ASB runtime has invalid ELF program headers")
    interpreters = []
    for index in range(count):
        header = offset + size * index
        if struct.unpack_from("<I",data,header)[0] == 3:
            start = struct.unpack_from("<Q",data,header+8)[0]
            length = struct.unpack_from("<Q",data,header+32)[0]
            if not 2 <= length <= 4096 or start + length > len(data):
                raise ExecutionError("ASB runtime has an invalid ELF interpreter")
            raw = data[start:start+length]
            if raw[-1:] != b"\0" or b"\0" in raw[:-1]:
                raise ExecutionError("ASB runtime has an invalid ELF interpreter")
            try:
                interpreters.append(raw[:-1].decode("ascii"))
            except UnicodeError as error:
                raise ExecutionError("ASB runtime has an invalid ELF interpreter") from error
    if len(interpreters) > 1 or (interpreters and not interpreters[0].startswith("/")):
        raise ExecutionError("ASB runtime has an invalid ELF interpreter")
    return data,machine,interpreters[0] if interpreters else None


def library_paths(text):
    """Parse loader --list output without executing shell commands or ldd."""
    result = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("linux-vdso") or line.startswith("linux-gate"):
            continue
        mapped = re.fullmatch(r"([A-Za-z0-9+_.-]+) => (/[^\n]+?) \(0x[0-9a-fA-F]+\)",line)
        direct = re.fullmatch(r"(/[^\n]+?) \(0x[0-9a-fA-F]+\)",line)
        if mapped:
            name,path = mapped.groups()
        elif direct:
            path = direct.group(1)
            name = Path(path).name
        else:
            raise ExecutionError("ASB loader could not resolve a complete runtime dependency list")
        if not re.fullmatch(r"[A-Za-z0-9+_.-]{1,128}",name) or name in [".",".."]:
            raise ExecutionError("ASB loader returned an invalid library name")
        if name in result and result[name] != path:
            raise ExecutionError("ASB loader returned conflicting library identities")
        result[name] = path
        if len(result) > 64:
            raise ExecutionError("ASB runtime exceeds its dependency limit")
    return result


def resolve_dependencies(interpreter,path):
    try:
        observed = subprocess.run([interpreter,"--list",str(Path(path).absolute())],
            env={"PATH":os.defpath,"LC_ALL":"C"},stdin=subprocess.DEVNULL,
            capture_output=True,text=True,timeout=15)
    except (OSError,subprocess.TimeoutExpired) as error:
        raise ExecutionError("cannot inspect the running ASB executable's dynamic libraries") from error
    if observed.returncode or len(observed.stdout.encode()) > 1024**2:
        raise ExecutionError("ASB loader could not resolve its runtime dependencies")
    return library_paths(observed.stdout)


def bundle(path, architecture=None):
    data,machine,interpreter = elf(path)
    expected = {62:"amd64",183:"arm64"}[machine]
    if architecture is not None and architecture != expected:
        raise ExecutionError("ASB runtime architecture does not match the selected task image")
    files = [("asb",data,0o755)]
    kind = "static"
    if interpreter:
        _,loader_machine,_ = elf(interpreter)
        if loader_machine != machine:
            raise ExecutionError("ASB loader architecture does not match its executable")
        dependencies = resolve_dependencies(interpreter,path)
        loader_name = Path(interpreter).name
        if not re.fullmatch(r"[A-Za-z0-9+_.-]{1,128}",loader_name):
            raise ExecutionError("ASB runtime loader has an invalid file name")
        dependencies[loader_name] = interpreter
        # Older glibc builds may load DNS/files NSS plugins lazily. Include
        # only these matching standard plugins, never host NSS configuration.
        if "libc.so.6" in dependencies:
            directory = Path(dependencies["libc.so.6"]).parent
            for name in ["libnss_dns.so.2","libnss_files.so.2"]:
                candidate = directory / name
                if candidate.is_file():
                    dependencies.setdefault(name,str(candidate))
                    for dependency,location in resolve_dependencies(interpreter,candidate).items():
                        if dependency in dependencies and Path(dependencies[dependency]).resolve() != Path(location).resolve():
                            raise ExecutionError("ASB NSS plugin requires conflicting runtime libraries")
                        dependencies[dependency] = location
        if len(dependencies) > 64:
            raise ExecutionError("ASB runtime exceeds its dependency limit")
        files = [("bin/asb",data,0o755)]
        for name,library in sorted(dependencies.items()):
            content,library_machine,_ = elf(library)
            if library_machine != machine:
                raise ExecutionError("ASB runtime contains an incompatible library architecture")
            files.append(("lib/"+name,content,0o755))
            if sum(len(content) for _,content,_ in files) > MAX_BUNDLE:
                raise ExecutionError("ASB runtime bundle exceeds 384 MiB")
        launcher = ("#!/bin/sh\nexport ASB_RUNTIME_EXECUTABLE="+shlex.quote(ROOT+"/asb")+"\n"
            "exec "+shlex.quote(ROOT+"/lib/"+loader_name)+" --library-path "+shlex.quote(ROOT+"/lib")
            +" "+shlex.quote(ROOT+"/bin/asb")+' "$@"\n')
        files.append(("asb",launcher.encode(),0o755))
        kind = "bundled-loader"
    receipt = {"schema_version":1,"kind":kind,"architecture":expected,
               "files":[{"name":name,"sha256":hashlib.sha256(content).hexdigest(),"bytes":len(content)}
                        for name,content,_ in files]}
    return files,receipt
