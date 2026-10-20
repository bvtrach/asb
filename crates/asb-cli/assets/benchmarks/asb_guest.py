"""Install and execute ASB-selected clients in an owned task environment.

No provider/agent registry lives here. Selection, package recipes, endpoint
configuration and output interpretation remain in the same Rust adapters used
by native execution. Host homes, venvs and project directories are not mounted.
"""
import io
import json
from pathlib import Path
import re
import tarfile
import tempfile

from asb_execution import ExecutionError
from asb_runtime import bundle


def upload(container, destination, files):
    with tempfile.TemporaryFile() as archive:
        with tarfile.open(fileobj=archive, mode="w") as tar:
            for name, content, mode in files:
                entry = tarfile.TarInfo(name)
                entry.size, entry.mode = len(content), mode
                tar.addfile(entry, io.BytesIO(content))
        archive.seek(0)
        if not container.put_archive(destination, archive):
            raise ExecutionError("could not stage the task's private runtime")


def stage_public_cache(container, project, tools):
    node = tools / "node/cache"
    if node.is_dir():
        for archive in sorted(node.glob("*.tar.*"), key=lambda path: path.stat().st_mtime, reverse=True)[:4]:
            if archive.is_file() and not archive.is_symlink() and archive.stat().st_size < 128 * 1024**2:
                upload(container, "/opt/asb-project/.asb-tools/node/cache", [(archive.name, archive.read_bytes(), 0o600)])
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
    with tempfile.TemporaryFile() as archive:
        with tarfile.open(fileobj=archive, mode="w") as tar:
            tar.add(cache, arcname="npm/_cacache", recursive=True)
        archive.seek(0)
        if not container.put_archive("/opt/asb-project/.asb-cache", archive):
            raise ExecutionError("could not stage public package cache")


def execute_agent(task, request, before_resume, on_output, cancelled):
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
                  "/opt/asb-project/.asb-tools/node/cache", "/opt/asb-project/.asb-cache"])
    upload(container, "/opt/asb-runtime", files)
    configuration = request.get("project_configuration", "tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n")
    if not isinstance(configuration, str) or len(configuration.encode()) > 64 * 1024:
        raise ExecutionError("guest project configuration exceeds its limit")
    upload(container, "/opt/asb-project/.asb", [("project.toml", configuration.encode(), 0o600)])
    stage_public_cache(container, project, tools)
    environment = request.get("environment", {})
    for selected in ["node", tool]:
        result = task.workspace.execute(["/bin/sh", "-c", 'cd /opt/asb-project && exec /opt/asb-runtime/asb tool install "$1"', "asb", selected],
                                        environment=environment, timeout=900, output_limit=256 * 1024, cancelled=cancelled)
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
    result = task.workspace.execute(["/opt/asb-runtime/asb", "__asb-agent"], environment=environment,
                                    timeout=request.get("timeout", getattr(task, "agent_timeout", 900)), output_limit=256 * 1024,
                                    gate="application", before_resume=before_resume,
                                    on_output=on_output, cancelled=cancelled)
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
