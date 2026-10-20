"""Install and execute ASB-selected clients in an owned task environment.

No provider/agent registry lives here. Selection, package recipes, endpoint
configuration and output interpretation remain in the same Rust adapters used
by native execution. Host homes, venvs and project directories are not mounted.
"""
import io
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import re
import tarfile
import tempfile

from asb_execution import ExecutionError
from asb_runtime import bundle
from asb_relay import HostRelay, ForwardingCancellation, forward_request


def upload(container, destination, files, temporary_dir=None):
    with tempfile.TemporaryFile(dir=temporary_dir) as archive:
        with tarfile.open(fileobj=archive, mode="w") as tar:
            for name, content, mode in files:
                entry = tarfile.TarInfo(name)
                entry.size, entry.mode = len(content), mode
                tar.addfile(entry, io.BytesIO(content))
        archive.seek(0)
        if not container.put_archive(destination, archive):
            raise ExecutionError("could not stage the task's private runtime")


def stage_public_cache(container, project, temporary_dir=None):
    cache = project / ".asb-cache/npm/_cacache"
    if not cache.is_dir():
        return
    paths = list(cache.rglob("*"))
    if (any(path.is_symlink() for path in paths)
            or len(paths) > 100000 or sum(path.stat().st_size for path in paths if path.is_file()) > 512 * 1024**2):
        return  # Do not copy unsafe/unbounded caches; normal install can download.
    for entry in paths:
        if entry.is_file() and "index-v5" in entry.parts:
            for line in entry.read_text().splitlines():
                if "\t" in line:
                    record = json.loads(line.split("\t", 1)[1])
                    metadata = record.get("metadata") or {}
                    if {key.lower() for key in metadata.get("reqHeaders", {})} & {"authorization", "proxy-authorization", "cookie"}:
                        return  # Never stage a credential-bearing registry cache.
    with tempfile.TemporaryFile(dir=temporary_dir) as archive:
        with tarfile.open(fileobj=archive, mode="w") as tar:
            tar.add(cache, arcname="npm/_cacache", recursive=True)
        archive.seek(0)
        if not container.put_archive("/opt/asb-project/.asb-cache", archive):
            raise ExecutionError("could not stage public package cache")


def stage_bootstrap(task, plan, tools):
    """Consume declared installer assets; never infer dependencies from names."""
    if (not isinstance(plan,dict) or not {"runtime_tools","assets"}.issubset(plan)
            or set(plan)-{"runtime_tools","assets","public_package_cache"}
            or type(plan.get("public_package_cache",False)) is not bool):
        raise ExecutionError("selected tool installation bootstrap plan is missing or invalid")
    runtimes,assets = plan["runtime_tools"],plan["assets"]
    if (not isinstance(runtimes,list) or len(runtimes) > 16
            or any(not isinstance(name,str) or not re.fullmatch(r"[a-z0-9][a-z0-9_.-]{0,79}",name) for name in runtimes)
            or len(set(runtimes)) != len(runtimes) or not isinstance(assets,list) or len(assets) > 64):
        raise ExecutionError("selected tool installation bootstrap plan exceeds its limits")
    staged,targets,total = [],set(),0
    for asset in assets:
        if not isinstance(asset,dict) or set(asset) != {"source","target"}:
            raise ExecutionError("invalid declared tool bootstrap asset")
        if not isinstance(asset["source"],str) or not isinstance(asset["target"],str):
            raise ExecutionError("invalid declared tool bootstrap path")
        source,target = Path(asset["source"]),Path(asset["target"])
        if (not source.is_absolute() or source.is_symlink() or not source.is_file()
                or not source.resolve().is_relative_to(tools.resolve())
                or target.is_absolute() or not target.parts or ".." in target.parts
                or any(not re.fullmatch(r"[A-Za-z0-9+_.-]{1,256}",part) for part in target.parts)
                or len(str(target)) > 1024 or str(target) in targets):
            raise ExecutionError("tool bootstrap asset escapes its project cache or guest destination")
        size = source.stat().st_size
        total += size
        if size > 256 * 1024**2 or total > 512 * 1024**2:
            raise ExecutionError("selected tool bootstrap assets exceed 512 MiB")
        targets.add(str(target))
        with source.open("rb") as incoming:
            content = incoming.read(256 * 1024**2 + 1)
        if len(content) != size:
            raise ExecutionError("tool bootstrap asset changed while it was being staged")
        staged.append((str(target),content,0o600))
    root = "/opt/asb-project/.asb-tools"
    for target in sorted({str(Path(name).parent) for name,_,_ in staged}):
        task.checked(["mkdir","-p",root+"/"+target])
    if staged:
        upload(task.workspace.container,root,staged,temporary_dir=task.artifacts)
    receipt = {"schema_version":1,"runtime_tools":runtimes,
               "assets":[{"target":name,"bytes":len(content),"sha256":hashlib.sha256(content).hexdigest()}
                         for name,content,_ in staged]}
    path = task.artifacts / "bootstrap.json"
    path.write_text(json.dumps(receipt,sort_keys=True)+"\n")
    path.chmod(0o600)
    return runtimes


def prepare_agent(task, request, cancelled, resources=None, relays=None):
    """Install the exact selected client independently of provider execution."""
    guest = request.get("request")
    if not isinstance(guest, dict):
        raise ExecutionError("private agent execution request is missing")
    tool = guest.get("config", {}).get("agent", "")
    if not re.fullmatch(r"[a-z0-9][a-z0-9_.-]{0,79}", tool):
        raise ExecutionError("selected agent tool identity is invalid")
    project = Path(request["project_root"]).absolute()
    tools = Path(request["tools_root"]).absolute()
    if not (project / ".asb/project.toml").is_file():
        raise ExecutionError("host benchmark project is not initialized")
    container = task.workspace.container
    files,receipt = bundle(request["binary"],getattr(task.workspace,"image_identity",{}).get("architecture"))
    runtime_receipt = task.artifacts / "runtime.json"
    runtime_receipt.write_text(json.dumps(receipt,sort_keys=True)+"\n")
    runtime_receipt.chmod(0o600)
    task.checked(["mkdir", "-p", "/opt/asb-runtime/bin", "/opt/asb-runtime/lib", "/opt/asb-project/.asb",
                  "/opt/asb-project/.asb-tools", "/opt/asb-project/.asb-cache"])
    upload(container, "/opt/asb-runtime", files,temporary_dir=task.artifacts)
    configuration = request.get("project_configuration", "tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n")
    if not isinstance(configuration, str) or len(configuration.encode()) > 64 * 1024:
        raise ExecutionError("guest project configuration exceeds its limit")
    if relays is None:
        relays = []
    def start_relay(targets):
        if resources is None:
            raise ExecutionError("local endpoint forwarding needs an owned execution scope")
        from asb_docker import docker_command
        relay = HostRelay(targets, [*docker_command(), "exec", "-i", container.id,
                                   "/opt/asb-runtime/asb", "__asb-relay"], cancelled)
        resources.callback(relay.close)
        relays.append(relay)
        return relay
    request = forward_request(dict(request, project_configuration=configuration), start_relay)
    configuration = request["project_configuration"]
    guest = request["request"]
    upload(container, "/opt/asb-project/.asb", [("project.toml", configuration.encode(), 0o600)],temporary_dir=task.artifacts)
    runtimes = stage_bootstrap(task,request.get("bootstrap"),tools)
    if request["bootstrap"].get("public_package_cache",False):
        stage_public_cache(container, project,temporary_dir=task.artifacts)
    environment = request.get("environment", {})
    for selected in [*runtimes, tool]:
        result = task.workspace.execute(["/bin/sh", "-c", 'cd /opt/asb-project && exec /opt/asb-runtime/asb tool install "$1"', "asb", selected],
                                        environment=environment, timeout=900, output_limit=256 * 1024,
                                        cancelled=ForwardingCancellation(cancelled, relays))
        for relay in relays:
            relay.check()
        for stream in ["stdout", "stderr"]:
            path = task.artifacts / f"install-{selected}.{stream}"
            path.write_bytes(result[stream])
            path.chmod(0o600)
        if result["exit_code"] != 0 or result["timed_out"] or result["cancelled"]:
            # The nested CLI puts command diagnostics in its project log.
            # Export a bounded tail before the owned environment is removed.
            detail = container.exec_run(["tail", "-c", "262144", "/opt/asb-project/.asb/install.log"])
            if detail.exit_code == 0:
                path = task.artifacts / f"install-{selected}.log"
                path.write_bytes(detail.output)
                path.chmod(0o600)
            raise ExecutionError("selected agent runtime installation failed; inspect private install artifacts")
    return guest,environment


def execute_agent(task, request, before_resume, on_output, cancelled):
    # The relay spans package installation AND agent execution and is closed
    # on success, cancellation and every preparation/execution exception.
    with ExitStack() as resources:
        return _execute_agent(task, request, before_resume, on_output, cancelled, resources, [])


def _execute_agent(task, request, before_resume, on_output, cancelled, resources, relays):
    guest,environment = prepare_agent(task,request,cancelled,resources,relays)
    container = task.workspace.container
    network = request.get("network")
    if network:
        # Change only the task's owned container, never host network policy.
        container.reload()
        container.client.networks.get(network).connect(container)
        for previous in list(container.attrs["NetworkSettings"]["Networks"]):
            if previous != network:
                container.client.networks.get(previous).disconnect(container)
    guest = dict(guest)
    guest["timeout_seconds"] = request.get("timeout", getattr(task, "agent_timeout", 900))
    guest.update(project_root="/opt/asb-project", runtime_dir="/opt/asb-private", run_dir="/opt/asb-private/attempt")
    environment = dict(environment, ASB_GUEST_AGENT_REQUEST=json.dumps(guest))
    def attach(target):
        if request.get("measure_target", False):
            # The task execution lock is already held while the gate is
            # stopped. This read-only control probe must not recursively start
            # another managed workspace execution.
            probe = container.exec_run(["/opt/asb-runtime/asb", "__asb-target", str(target["namespace_pid"])])
            if probe.exit_code != 0 or len(probe.output) > 65536:
                raise ExecutionError("cannot read isolated measurement identity")
            try:
                target = dict(target, process_identity=json.loads(probe.output))
            except ValueError:
                raise ExecutionError("isolated process did not return valid measurement identity") from None
        if before_resume is not None:
            before_resume(target)
    result = task.workspace.execute(["/opt/asb-runtime/asb", "__asb-agent"], environment=environment,
                                    timeout=request.get("timeout", getattr(task, "agent_timeout", 900)), output_limit=256 * 1024,
                                    gate="application", before_resume=attach,
                                    on_output=on_output, cancelled=ForwardingCancellation(cancelled, relays))
    for relay in relays:
        relay.check()
    observation = {key: result.get(key) for key in ["exit_code", "timed_out", "cancelled",
                   "output_truncated", "elapsed_seconds", "target"]}
    journal = task.artifacts / "execution.json"
    journal.write_text(json.dumps(observation))
    journal.chmod(0o600)
    try:
        metadata = task.checked(["cat", "/opt/asb-private/attempt/agent-result.json"], output_limit=1024 * 1024)
        result["metadata"] = json.loads(metadata)
    except (ExecutionError, ValueError):
        # Keep partial work eligible for evaluation after timeout/client death.
        # Unknown usage is never invented from text or an expected minimum.
        result["metadata"] = {"schema_version": 1, "exit_code": result["exit_code"],
                              "final_response": None, "token_usage": {}, "steps": None}
    return result
