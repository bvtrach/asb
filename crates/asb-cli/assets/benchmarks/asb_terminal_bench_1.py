"""Original Terminal-Bench environment/test/parser adapter, never a new grader.

The installed 0.2.18 SDK owns task parsing, Compose environment construction,
test-script injection, tmux execution and result parsing. ASB retains agent/
provider selection through the generic task controller and Rust adapters.
"""
import argparse
import contextlib
import functools
import hashlib
import importlib.metadata
import json
import logging
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import uuid

from asb_docker import DockerSession, docker_command
from asb_execution import ExecutionError, ManagedDockerWorkspace
from asb_resources import ResourceJournal
from asb_terminal_catalog import verify, tree_manifest


def quiet_sdk(method):
    @functools.wraps(method)
    def run(*args, **kwargs):
        with contextlib.redirect_stdout(sys.stderr):
            return method(*args, **kwargs)
    return run


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True) + "\n")
    path.chmod(0o600)


def write_compose_environment(path, environment):
    """Only SDK task interpolation variables cross the elevated CLI boundary."""
    values = "".join(key + "=" + json.dumps(str(value).replace("$", "$$")) + "\n"
                     for key, value in environment.items() if key.startswith("T_BENCH_"))
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        output.write(values)


def copy_task_inputs(source, target, definition):
    """Do not put reference solutions or verifier scripts in a build/bind input."""
    private = {"tests", "solution.sh", "solution.yaml", "task.yaml", "run-tests.sh"}
    def exclude(directory, names):
        return private.intersection(names) if Path(directory).resolve() == definition.resolve() else set()
    if source.is_dir():
        shutil.copytree(source, target, dirs_exist_ok=True, ignore=exclude)
    else:
        if source.parent.resolve() == definition.resolve() and source.name in private:
            raise ExecutionError("task input would expose reference solution or verifier data")
        shutil.copy2(source, target)


def restore_task_networks(client, container, original):
    """Restore exactly the verifier's networks after a scoped agent override."""
    container.reload()
    connected = set(container.attrs["NetworkSettings"]["Networks"])
    for name in set(original) - connected:
        client.networks.get(name).connect(container)
    for name in connected - set(original):
        client.networks.get(name).disconnect(container)


def verify_retained(retained):
    from asb_retained import require
    result = retained.object("evaluation.json")
    retained.grade(result)
    require(retained.task.get("sdk") == {"package":"terminal-bench","version":"0.2.18"},
            "retained original Terminal-Bench SDK differs")
    tests = result.get("test_results")
    require(isinstance(tests,dict) and tests and len(tests) <= 10000
            and result.get("failure_mode") == "none" and result.get("parse_failure") == "none"
            and result["resolved"] == all(status == "passed" for status in tests.values()),
            "retained original Terminal-Bench parsed tests disagree")
    pane = "official/" + retained.identity["task_id"] + "/" + retained.identity["attempt_id"] + "/panes/post-test.txt"
    require(bool(retained.bytes(pane)), "retained original verifier pane is missing")


class TerminalBenchTask:
    def __init__(self, client, catalog, task, artifacts):
        self.client, self.catalog_path, self.task_id = client, Path(catalog), task
        self.artifacts = Path(artifacts).absolute()
        self.attempt = "asb-" + uuid.uuid4().hex
        self.workspace, self.terminal, self.invocation = None, None, None
        self.cancelled = None
        self.journal = None

    @quiet_sdk
    def __enter__(self):
        os.environ.setdefault("LITELLM_LOCAL_MODEL_COST_MAP", "True")
        import docker
        import terminal_bench.terminal.terminal as terminal_module
        from terminal_bench.terminal.docker_compose_manager import DockerComposeManager
        from terminal_bench.handlers.trial_handler import TrialHandler
        from terminal_bench.harness.harness import Harness
        self.catalog = verify(self.catalog_path, "terminal-bench-1.0")
        selected = next((task for task in self.catalog["tasks"] if task["id"] == self.task_id), None)
        if selected is None:
            raise ExecutionError("selected original Terminal-Bench task is not installed")
        self.definition = self.catalog_path.parent / selected["definition"]
        self.artifacts.mkdir(mode=0o700, parents=True, exist_ok=False)
        self.journal = ResourceJournal(self.client, self.artifacts, self.attempt)
        self.handler = TrialHandler(self.attempt, self.definition, self.artifacts / "official")
        self.agent_timeout = self.handler.task.max_agent_timeout_sec
        # Only these SDK test/parser operations are used. No SDK default agent,
        # provider, model, global run lock or publication path is initialized.
        self.evaluator = Harness.__new__(Harness)
        self.evaluator._global_test_timeout_sec = None
        self.evaluator._global_timeout_multiplier = 1.0
        self.evaluator._logger = logging.getLogger("asb.original-terminal-bench")
        command, attempt, artifacts = docker_command(), self.attempt, self.artifacts
        definition = self.definition
        journal = self.journal

        class ScopedCompose(DockerComposeManager):
            def get_docker_compose_command(manager, arguments):
                path = getattr(manager, "asb_config", manager._docker_compose_path)
                environment = artifacts / "compose.env"
                if not environment.exists():
                    write_compose_environment(environment, manager.env)
                # sudo intentionally does not preserve arbitrary host env. Give
                # Compose only the SDK's declared task variables through its
                # own private env-file, never provider credentials or a shell.
                return command + ["compose", "--env-file", str(environment), "-p", attempt, "-f", str(path), *arguments]

            def _run_docker_compose_command(manager, arguments):
                try:
                    return subprocess.run(manager.get_docker_compose_command(arguments), env=manager.env,
                        stdin=subprocess.DEVNULL, capture_output=True, text=True, check=True,
                        timeout=900 if arguments[0] == "build" else 120)
                except subprocess.CalledProcessError as error:
                    log = artifacts / ("compose-" + arguments[0] + ".log")
                    log.write_text((error.stdout or "") + (error.stderr or ""))
                    log.chmod(0o600)
                    raise ExecutionError("official task Compose " + arguments[0] + " failed; inspect private Compose artifacts") from error

            def start(manager):
                expanded = manager._run_docker_compose_command(["config", "--format", "json"])
                config = json.loads(expanded.stdout)
                services = config.get("services", {})
                if not services or len(services) > 16:
                    raise ExecutionError("official task declares an invalid number of services")
                for name, service in services.items():
                    if (service.get("privileged") or service.get("network_mode") == "host"
                            or service.get("pid") == "host" or service.get("ipc") == "host" or service.get("devices")):
                        raise ExecutionError("this task needs a separately isolated host for privileged/device/host-namespace access")
                    service["container_name"] = attempt if name == "client" else attempt + "-" + name
                    service["ports"] = []  # Internal service ports stay unchanged.
                    service.setdefault("labels", {}).update({"asb.owner": "asb", "asb.attempt": attempt})
                    service.setdefault("mem_limit", "4g")
                    service.setdefault("cpus", 2)
                    service.setdefault("pids_limit", 512)
                    build = service.get("build")
                    if isinstance(build, dict) and "context" in build:
                        context = Path(build["context"]).resolve()
                        if not context.is_relative_to(definition.resolve()):
                            raise ExecutionError("task needs an isolated mapping for a shared build context")
                        if build.get("privileged") or build.get("network") == "host" or build.get("entitlements") or build.get("ssh"):
                            raise ExecutionError("task build requires separately isolated host capabilities")
                        target = artifacts / "build-inputs" / hashlib.sha256(name.encode()).hexdigest()
                        target.parent.mkdir(parents=True, exist_ok=True)
                        copy_task_inputs(context, target, definition)
                        build["context"] = str(target)
                    for volume in service.get("volumes", []):
                        if volume.get("type") != "bind":
                            continue
                        source = Path(volume["source"]).resolve()
                        if source.is_relative_to(artifacts):
                            continue
                        if not source.is_relative_to(definition.resolve()):
                            raise ExecutionError("task bind mount would expose data outside the task definition")
                        target = artifacts / "inputs" / hashlib.sha256(str(source).encode()).hexdigest()
                        target.parent.mkdir(parents=True, exist_ok=True)
                        copy_task_inputs(source, target, definition)
                        volume["source"] = str(target)
                for name, network in config.get("networks", {}).items():
                    if network.get("external"):
                        raise ExecutionError("task requires an external network; an isolated task-local mapping is needed")
                    network["name"] = attempt + "-" + name
                    network.setdefault("labels", {}).update({"asb.owner": "asb", "asb.attempt": attempt})
                for name, volume in config.get("volumes", {}).items():
                    if volume.get("external"):
                        raise ExecutionError("task requires an external volume; an isolated task-local mapping is needed")
                    volume["name"] = attempt + "-" + name
                    volume.setdefault("labels", {}).update({"asb.owner": "asb", "asb.attempt": attempt})
                manager.asb_config = artifacts / "compose.json"
                journal.compose(config)
                write_json(manager.asb_config, config)
                return super().start()

        self.original_client, self.original_manager = docker.from_env, terminal_module.DockerComposeManager
        docker.from_env = lambda **_kwargs: self.client
        terminal_module.DockerComposeManager = ScopedCompose
        try:
            paths = self.handler.trial_paths
            image = "asb-terminal:" + self.catalog["source_sha256"][:24] + "-" + self.task_id
            self.terminal = terminal_module.Terminal(client_container_name=self.attempt, client_image_name=image,
                docker_image_name_prefix="asb-terminal-" + self.catalog["source_sha256"][:24],
                docker_compose_path=self.handler.task_paths.docker_compose_path, no_rebuild=False, cleanup=False,
                sessions_logs_path=paths.sessions_path, agent_logs_path=paths.agent_logging_dir,
                commands_path=paths.commands_path, disable_recording=self.handler.task.disable_asciinema)
            self.terminal.start()
            container = self.terminal.container
            self.original_networks = list(container.attrs["NetworkSettings"]["Networks"])
            self.workspace = ManagedDockerWorkspace.attach(self.client, container, self.attempt,
                container.attrs["Config"].get("WorkingDir") or "/")
            self.agent_session = self.terminal.create_session("agent", as_configured_user=True)
            definition_sha = hashlib.sha256(json.dumps(tree_manifest(self.definition), sort_keys=True).encode()).hexdigest()
            self.invocation = {"identity":{"workload":"terminal-bench-1.0", "task_id":self.task_id,
                "attempt_id":self.attempt, "dataset":self.catalog["dataset"]}, "prompt":self.handler.instruction,
                "workspace":self.workspace.workdir, "baseline":{"kind":"upstream-environment-definition",
                "source_revision":self.catalog["dataset"]["revision"], "definition_sha256":definition_sha},
                "sdk":self.catalog["sdk"], "image":self.workspace.image_identity, "resources":self.workspace.limits,
                "agent_timeout_seconds":self.agent_timeout}
            write_json(self.artifacts / "task.json", self.invocation)
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def checked(self, argv, output_limit=256 * 1024):
        result = self.workspace.execute(argv, timeout=60, output_limit=output_limit, cancelled=self.cancelled)
        if result["exit_code"] != 0 or result["timed_out"] or result["cancelled"] or result["output_truncated"]:
            raise ExecutionError("task environment operation failed; inspect task artifacts")
        return result["stdout"]

    def capture_submission(self):
        result = {"task":self.invocation["identity"], "kind":"in-place-environment",
                  "container_id":self.workspace.container.id, "image":self.workspace.image_identity}
        write_json(self.artifacts / "submission.json", result)
        return result

    @quiet_sdk
    def evaluate(self, timeout=1800):
        from terminal_bench.agents.failure_mode import FailureMode
        self.capture_submission()
        # A qualification-only agent network override must not change the
        # original verifier's service connectivity or dependency downloads.
        restore_task_networks(self.client, self.workspace.container, self.original_networks)
        session = self.agent_session if self.handler.task.run_tests_in_same_shell else self.terminal.create_session("tests", as_configured_user=False)
        failure = self.evaluator._run_tests(self.terminal, session, self.handler)
        pane = session.capture_pane(capture_entire=True)
        self.handler.trial_paths.post_test_pane_path.write_text(pane)
        parsed, parse_failure = self.evaluator._parse_results(self.handler, pane)
        valid = failure == FailureMode.NONE and parse_failure == FailureMode.NONE and bool(parsed)
        resolved = valid and self.evaluator._is_resolved(parsed)
        result = {"schema_version":1,"workload":"terminal-bench-1.0","task_id":self.task_id,
            "dataset":self.catalog["dataset"], "sdk":self.catalog["sdk"], "resolved":resolved,
            "outcome":"resolved" if resolved else "unresolved" if valid else "evaluation_error",
            "test_results":{name:status.value for name,status in (parsed or {}).items()},
            "failure_mode":failure.value, "parse_failure":parse_failure.value,
            "artifact":"evaluation.json"}
        write_json(self.artifacts / "evaluation.json", result)
        return result

    @quiet_sdk
    def __exit__(self, *_exception):
        import docker
        import terminal_bench.terminal.terminal as terminal_module
        status = "not_created"
        try:
            if self.terminal is not None:
                status = "cleanup_failed"
                for session in self.terminal._sessions.values():
                    try:
                        session.stop()
                    except Exception:
                        pass
                manager = self.terminal._compose_manager
                if getattr(manager, "asb_config", None) is not None:
                    manager._run_docker_compose_command(["down", "--timeout", "2", "--volumes", "--remove-orphans"])
                remaining = self.client.containers.list(all=True, filters={"label":"com.docker.compose.project=" + self.attempt})
                if remaining:
                    raise ExecutionError("official task containers could not be completely removed")
                status = "removed"
                if self.workspace:
                    self.workspace.container = None
        finally:
            if self.workspace:
                self.workspace.cleanup_status = status
            if hasattr(self, "original_client"):
                docker.from_env, terminal_module.DockerComposeManager = self.original_client, self.original_manager
            if self.artifacts.is_dir():
                write_json(self.artifacts / "task-cleanup.json", {"attempt_id":self.attempt,"status":status})
            if self.journal is not None:
                self.journal.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", required=True, type=Path)
    parser.add_argument("--task", required=True)
    parser.add_argument("--artifacts", required=True, type=Path)
    args = parser.parse_args()
    def cancelled(_signal, _frame):
        raise InterruptedError("ASB original Terminal-Bench session was cancelled")
    signal.signal(signal.SIGTERM, cancelled)
    from asb_task_server import serve
    with DockerSession() as client:
        return serve(TerminalBenchTask(client, args.catalog, args.task, args.artifacts), sys.stdin, sys.stdout)


if __name__ == "__main__":
    raise SystemExit(main())
