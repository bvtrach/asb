"""Infrastructure shared by SDK harnesses, independent of a benchmark or grader.

SDK adapters retain task parsing, dataset validation and grading semantics.
These helpers own only protocol output, private metadata and Docker topology.
"""
import contextlib
import functools
import json
import os
import sys

from asb_execution import ExecutionError


def quiet_sdk(method):
    @functools.wraps(method)
    def run(*args, **kwargs):
        with contextlib.redirect_stdout(sys.stderr):
            return method(*args, **kwargs)
    return run


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True) + "\n")
    path.chmod(0o600)


def original_networks(container):
    """Persist only the SDK network options needed to reconnect this task."""
    container.reload()
    result = {}
    for name,endpoint in container.attrs["NetworkSettings"]["Networks"].items():
        ipam = endpoint.get("IPAMConfig") or {}
        options = {"aliases":endpoint.get("Aliases") or []}
        for source,target in [("IPv4Address","ipv4_address"),("IPv6Address","ipv6_address"),("LinkLocalIPs","link_local_ips")]:
            if ipam.get(source):options[target] = ipam[source]
        result[name] = options
    return network_options(result)


def network_options(original):
    if not isinstance(original,dict) or len(original) > 64:
        raise ExecutionError("original task network topology is missing or exceeds its bound")
    for name,options in original.items():
        if (not isinstance(name,str) or not 1 <= len(name) <= 255 or not isinstance(options,dict)
                or set(options) - {"aliases","ipv4_address","ipv6_address","link_local_ips"}):
            raise ExecutionError("original task network endpoint is malformed")
        for key,value in options.items():
            if key in ["aliases","link_local_ips"]:
                if not isinstance(value,list) or len(value) > 64 or any(not isinstance(item,str) or len(item) > 255 for item in value):
                    raise ExecutionError("original task network aliases or link-local addresses are malformed")
            elif not isinstance(value,str) or not 1 <= len(value) <= 128:
                raise ExecutionError("original task static network address is malformed")
    return original


def restore_task_networks(client, container, original):
    """Restore exactly the verifier's networks after a scoped agent override."""
    if isinstance(original,dict):network_options(original)
    container.reload()
    connected = set(container.attrs["NetworkSettings"]["Networks"])
    for name in set(original) - connected:
        options = original[name] if isinstance(original,dict) else {}
        client.networks.get(name).connect(container,**options)
    for name in connected - set(original):
        client.networks.get(name).disconnect(container)


def record_grading_start(artifacts, identity):
    """An absent result is never permission to repeat side-effecting tests."""
    path = artifacts/"evaluation-started.json"
    descriptor = os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
    with os.fdopen(descriptor,"w") as output:
        json.dump({"schema_version":1,"scope":"official-verifier-started-not-a-grade","task":identity},output)
        output.flush()
        os.fsync(output.fileno())
    descriptor = os.open(artifacts,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:os.fsync(descriptor)
    finally:os.close(descriptor)
