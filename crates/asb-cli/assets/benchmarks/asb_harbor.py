"""Original Terminal-Bench 2.1 sessions using Harbor's environment and verifier.

No Harbor agent or model is instantiated. ASB's shared guest driver executes
the selected client, while the pinned SDK owns environment setup and grading.
"""
import argparse
import asyncio
import contextlib
import hashlib
import json
import logging
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import uuid

from asb_docker import DockerSession, docker_command
from asb_execution import ExecutionError, ManagedDockerWorkspace
from asb_resources import ResourceJournal, build_image_tag
from asb_terminal_catalog import verify, tree_manifest
from asb_terminal_bench_1 import quiet_sdk, restore_task_networks, original_networks, write_json


class ImageUnavailable(ExecutionError):
    pass


def start_environment(environment, public, wait, budget):
    """Use the official source only when its prebuilt image cannot be obtained."""
    started = time.monotonic()
    try:
        wait(environment.start(force_build=False), budget)
        return "prebuilt" if environment._use_prebuilt else "original-source-build"
    except ImageUnavailable:
        if not environment._use_prebuilt or not (public / "Dockerfile").is_file():
            raise
        wait(environment._run_docker_compose_command(
            ["down", "--timeout", "2", "--volumes", "--remove-orphans"]), 120)
        remaining = budget - (time.monotonic() - started)
        if remaining <= 0:
            raise ExecutionError("official task image preparation exhausted its build time budget")
        wait(environment.start(force_build=True), remaining)
        return "original-source-build-fallback"


def scoped_configuration(config, attempt, artifacts, environment):
    """Keep official service semantics without exposing host directories/ports."""
    services = config.get("services", {})
    if "main" not in services or len(services) > 16:
        raise ExecutionError("official Harbor task has an invalid service population")
    for name, service in services.items():
        if (service.get("privileged") or service.get("network_mode") == "host"
                or service.get("pid") == "host" or service.get("ipc") == "host" or service.get("devices")):
            raise ExecutionError("task requires a separately isolated host for privileged/device/host-namespace access")
        service["container_name"] = attempt if name == "main" else attempt + "-" + name
        service["ports"] = []
        service.setdefault("labels", {}).update({"asb.owner":"asb", "asb.attempt":attempt})
        service.setdefault("pids_limit", 512)
        for volume in service.get("volumes", []):
            if volume.get("type") == "bind":
                source = Path(volume["source"]).resolve()
                if not (source.is_relative_to(artifacts.resolve()) or source.is_relative_to(environment.resolve())):
                    raise ExecutionError("Harbor task bind mount would expose data outside this attempt")
        build = service.get("build")
        if isinstance(build, dict):
            if build.get("privileged") or build.get("network") == "host" or build.get("entitlements") or build.get("ssh"):
                raise ExecutionError("task build requires separately isolated host capabilities")
            if "context" in build and not Path(build["context"]).resolve().is_relative_to(environment.resolve()):
                raise ExecutionError("Harbor build context would expose private task or host data")
    for category in ["networks", "volumes"]:
        for name, resource in config.get(category, {}).items():
            if resource.get("external"):
                raise ExecutionError("Harbor task requires an isolated mapping for an external resource")
            resource["name"] = attempt + "-" + name
            resource.setdefault("labels", {}).update({"asb.owner":"asb", "asb.attempt":attempt})
    return config


def environment_class(command, attempt, artifacts, journal=None):
    from harbor.environments.base import ExecResult
    from harbor.environments.docker.docker import DockerEnvironment
    from harbor.environments.docker.runtime import ContainerRuntime

    class OwnedHarborEnvironment(DockerEnvironment):
        @property
        def _main_image_name(self):
            # Keep the SDK's OS validation/cache identity aligned with the
            # attempt-owned main build tag published by the Compose adapter.
            return build_image_tag(attempt,"main")

        @classmethod
        def runtime(cls):
            return ContainerRuntime(engine=tuple(command), compose=tuple(command + ["compose"]))

        async def _run_docker_compose_command(self, arguments, check=True, timeout_sec=None,
                                             stdin_data=None, on_output=None):
            # sudo drops arbitrary inherited env. Pass only SDK infrastructure
            # and explicitly declared task env through a private Compose file.
            environment = artifacts / ("harbor-env-" + uuid.uuid4().hex)
            descriptor = os.open(environment, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, "w") as output:
                output.write("".join(key + "=" + json.dumps(str(value).replace("$", "$$")) + "\n"
                    for key, value in self._compose_env_vars(include_os_env=False).items()))
            prefix = command + ["compose", "--env-file", str(environment), "-p", attempt,
                                "--project-directory", str(self.environment_dir)]
            for path in self._docker_compose_paths:
                prefix += ["-f", str(path)]
            def run():
                def invoke(argv, input_data=None):
                    return subprocess.run(argv, input=input_data, capture_output=True,
                        timeout=timeout_sec or self.task_env_config.build_timeout_sec or 600)
                expanded = invoke(prefix + ["config", "--format", "json"])
                if expanded.returncode:
                    return expanded
                if len(expanded.stdout) > 4 * 1024**2:
                    raise ExecutionError("expanded Harbor Compose definition exceeds 4 MiB")
                config = scoped_configuration(json.loads(expanded.stdout), attempt, artifacts, self.environment_dir)
                if journal is not None:
                    journal.compose(config)
                private = artifacts / "harbor-compose.json"
                write_json(private, config)
                return invoke(command + ["compose", "-p", attempt, "-f", str(private), *arguments], stdin_data)
            pending = asyncio.create_task(asyncio.to_thread(run))
            try:
                result = await asyncio.shield(pending)
            except asyncio.CancelledError:
                # Cancelling an asyncio waiter must not leave a Compose up or
                # build running in a detached worker during project teardown.
                await pending
                raise
            stdout, stderr = result.stdout.decode(errors="replace"), result.stderr.decode(errors="replace")
            if check and result.returncode:
                log = artifacts / ("harbor-compose-" + arguments[0] + ".log")
                log.write_text(stdout + stderr)
                log.chmod(0o600)
                if arguments[0] in ["up", "pull"] and any(reason in (stdout + stderr).lower() for reason in [
                        "pull access denied", "manifest unknown", "failed to resolve reference",
                        "failed to authorize", "no matching manifest", "toomanyrequests"]):
                    raise ImageUnavailable("official prebuilt task image is unavailable; original source build is needed")
                raise ExecutionError("official Harbor Compose " + arguments[0] + " failed; inspect private task artifacts")
            if on_output is not None:
                await on_output(stdout, "stdout")
                if stderr:
                    await on_output(stderr, "stderr")
            return ExecResult(stdout=stdout, stderr=stderr, return_code=result.returncode)

    return OwnedHarborEnvironment


def inspect_retained(retained):
    retained.select_grade(retained.object("evaluation.json"))
    verify_retained(retained)


def grade_retained(retained, _catalog_path, _cancelled):
    from asb_retained import require
    if retained.child("evaluation.json").exists():
        result = retained.object("evaluation.json")
        if result.get("outcome") in ["resolved","unresolved"]:
            inspect_retained(retained)
            return
    require(False,"retained Harbor environment grading is not implemented yet; preserved work was not changed")


def verify_retained(retained):
    from asb_retained import require
    retained.verify_environment_baseline()
    result = retained.object("evaluation.json")
    retained.grade(result)
    require(retained.task.get("sdk") == {"package":"harbor","version":"0.24.0"},
            "retained original Harbor SDK differs")
    rewards = result.get("rewards")
    require(isinstance(rewards,dict) and set(rewards) == {"reward"}
            and type(rewards["reward"]) in [int,float] and rewards["reward"] in [0,1]
            and result["resolved"] == (rewards["reward"] == 1), "retained original Harbor reward disagrees")
    encoded = retained.child("official/verifier/reward.json")
    # Match the pinned SDK: JSON takes precedence when both outputs exist.
    reward = retained.object("official/verifier/reward.json").get("reward") if encoded.is_file() else float(retained.bytes("official/verifier/reward.txt").decode().strip())
    require(type(reward) in [int,float] and reward == rewards["reward"], "retained reward file differs from its report")


class HarborTask:
    def __init__(self, client, catalog, task, artifacts):
        self.client, self.catalog_path, self.task_id = client, Path(catalog), task
        self.artifacts = Path(artifacts).absolute()
        self.attempt = "asb-" + uuid.uuid4().hex
        self.workspace, self.environment, self.invocation = None, None, None
        self.cancelled = None
        self.interrupted = False
        self.loop = asyncio.new_event_loop()
        self.journal = None

    def wait(self, operation, timeout):
        pending = self.loop.create_task(asyncio.wait_for(operation, timeout=timeout))
        try:
            return self.loop.run_until_complete(pending)
        except BaseException:
            if not pending.done():
                pending.cancel()
                # A synchronous termination signal must drain the SDK's
                # cancellation path before any Compose teardown starts.
                with contextlib.suppress(BaseException):
                    self.loop.run_until_complete(pending)
            raise

    @quiet_sdk
    def __enter__(self):
        os.environ.setdefault("LITELLM_LOCAL_MODEL_COST_MAP", "True")
        from harbor.models.task.task import Task
        from harbor.models.trial.paths import TrialPaths
        from harbor.models.task.config import TaskOS
        self.catalog = verify(self.catalog_path, "terminal-bench-2.1")
        selected = next((item for item in self.catalog["tasks"] if item["id"] == self.task_id), None)
        if selected is None:
            raise ExecutionError("selected original Terminal-Bench 2.1 task is not installed")
        self.definition = self.catalog_path.parent / selected["definition"]
        self.task = Task(self.definition)
        if self.task.has_steps or self.task.config.environment.os != TaskOS.LINUX:
            raise ExecutionError("this original dataset session expects its single-step Linux task definition")
        self.artifacts.mkdir(mode=0o700, parents=True, exist_ok=False)
        self.journal = ResourceJournal(self.client, self.artifacts, self.attempt)
        self.paths = TrialPaths(self.artifacts / "official")
        self.paths.mkdir()
        self.paths.chmod_dir()
        # Only environment inputs go into build contexts. Reference solution,
        # instruction and verifier directories stay outside this copied tree.
        public = self.artifacts / "environment"
        shutil.copytree(self.task.paths.environment_dir, public)
        mounts = [{"type":"bind", "source":str(path), "target":"/logs/" + name}
                  for name, path in [("agent",self.paths.agent_dir), ("verifier",self.paths.verifier_dir),
                                     ("artifacts",self.paths.artifacts_dir)]]
        owned = environment_class(docker_command(), self.attempt, self.artifacts, self.journal)
        self.previous_tempdir = tempfile.tempdir
        sdk_temp = self.artifacts / "sdk-tmp"
        sdk_temp.mkdir(mode=0o700)
        # This controller is a separate process. Do not put the SDK's Compose
        # overlays on a potentially small host /tmp filesystem.
        tempfile.tempdir = str(sdk_temp)
        try:
            self.environment = owned(environment_dir=public, environment_name=self.task.short_name,
                session_id=self.attempt, trial_paths=self.paths, task_env_config=self.task.config.environment,
                mounts=mounts, logger=logging.getLogger("asb.original-harbor"))
            self.environment_source = start_environment(self.environment,public,self.wait,
                self.task.config.environment.build_timeout_sec + 120)
            containers = self.client.containers.list(filters={"label":["asb.attempt=" + self.attempt,
                "com.docker.compose.service=main"]})
            if len(containers) != 1:
                raise ExecutionError("official Harbor environment did not create exactly one owned main service")
            container = containers[0]
            self.original_networks = original_networks(container)
            self.workspace = ManagedDockerWorkspace.attach(self.client, container, self.attempt,
                self.task.config.environment.workdir or container.attrs["Config"].get("WorkingDir") or "/")
            self.agent_timeout = self.task.config.agent.timeout_sec
            definition_sha = hashlib.sha256(json.dumps(tree_manifest(self.definition), sort_keys=True).encode()).hexdigest()
            self.invocation = {"identity":{"workload":"terminal-bench-2.1", "task_id":self.task_id,
                "attempt_id":self.attempt,"dataset":self.catalog["dataset"]}, "prompt":self.task.instruction,
                "workspace":self.workspace.workdir, "baseline":{"kind":"upstream-environment-definition",
                "source_revision":self.catalog["dataset"]["revision"], "definition_sha256":definition_sha},
                "sdk":self.catalog["sdk"], "image":self.workspace.image_identity,"resources":self.workspace.limits,
                "agent_timeout_seconds":self.agent_timeout,"environment_source":self.environment_source,
                "original_networks":self.original_networks}
            write_json(self.artifacts / "task.json", self.invocation)
            return self
        except BaseException:
            self.interrupted = True
            self.__exit__(None, None, None)
            raise

    def checked(self, argv, output_limit=256 * 1024):
        result = self.workspace.execute(argv, timeout=60, output_limit=output_limit, cancelled=self.cancelled)
        if result["exit_code"] != 0 or result["timed_out"] or result["cancelled"] or result["output_truncated"]:
            raise ExecutionError("task environment operation failed; inspect task artifacts")
        return result["stdout"]

    def capture_submission(self):
        result = {"task":self.invocation["identity"], "kind":"in-place-environment",
                  "container_id":self.workspace.container.id,"image":self.workspace.image_identity}
        write_json(self.artifacts / "submission.json", result)
        return result

    @quiet_sdk
    def evaluate(self, timeout=1800):
        from harbor.verifier.verifier import Verifier
        self.capture_submission()
        restore_task_networks(self.client, self.workspace.container, self.original_networks)
        # Reward files are outputs of this verifier invocation, not a cache or
        # something left by a client before the official test script ran.
        for path in [self.paths.reward_text_path, self.paths.reward_json_path]:
            if path.is_symlink() or path.is_file():
                path.unlink()
        self.environment.default_user = self.task.config.verifier.user
        try:
            value = self.wait(Verifier(self.task, self.paths, self.environment).verify(),
                              min(timeout, self.task.config.verifier.timeout_sec))
            rewards = value.rewards
            if (not rewards or set(rewards) != {"reward"}
                    or type(rewards["reward"]) not in [int, float] or rewards["reward"] not in [0, 1]):
                raise ExecutionError("original Terminal-Bench verifier returned an invalid binary reward")
            resolved = rewards["reward"] == 1
            result = {"resolved":resolved, "outcome":"resolved" if resolved else "unresolved", "rewards":rewards}
        except Exception as error:
            # SDK exceptions may include command output; retain only the class
            # on the protocol. Original verifier artifacts remain private.
            result = {"resolved":False, "outcome":"evaluation_error", "error_class":type(error).__name__}
        result.update(schema_version=1,workload="terminal-bench-2.1",task_id=self.task_id,
                      dataset=self.catalog["dataset"],sdk=self.catalog["sdk"],artifact="evaluation.json")
        write_json(self.artifacts / "evaluation.json", result)
        return result

    @quiet_sdk
    def __exit__(self, *_exception):
        self.interrupted = self.interrupted or bool(_exception and _exception[0] is not None)
        status = "not_created"
        try:
            if self.interrupted and self.journal is not None:
                status = "preservation_failed"
                self.journal.preserve(self.client)
                status = "stopped-preserved"
            elif self.environment is not None:
                status = "cleanup_failed"
                # Do not use SDK stop(delete=True): it removes shared local
                # build images. Remove only this unique Compose project.
                self.wait(self.environment.prepare_logs_for_host(), 60)
                self.wait(self.environment._run_docker_compose_command(
                    ["down", "--timeout", "2", "--volumes", "--remove-orphans"]), 120)
                if self.client.containers.list(all=True, filters={"label":"com.docker.compose.project=" + self.attempt}):
                    raise ExecutionError("official Harbor containers could not be completely removed")
                if self.journal is not None:
                    self.journal.cleanup_images(self.client)
                status = "removed"
                if self.workspace:
                    self.workspace.container = None
        finally:
            self.cleanup_status = status
            if hasattr(self,"previous_tempdir"):
                tempfile.tempdir = self.previous_tempdir
            if self.workspace:
                self.workspace.cleanup_status = status
            if self.artifacts.is_dir():
                write_json(self.artifacts / "task-cleanup.json", {"attempt_id":self.attempt,"status":status})
            self.loop.close()
            if self.journal is not None:
                self.journal.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", required=True, type=Path)
    parser.add_argument("--task", required=True)
    parser.add_argument("--artifacts", required=True, type=Path)
    args = parser.parse_args()
    terminating = False
    def cancelled(_signal, _frame):
        nonlocal terminating
        if not terminating:
            terminating = True
            raise InterruptedError("ASB Harbor session was cancelled")
    signal.signal(signal.SIGTERM, cancelled)
    from asb_task_server import serve
    with DockerSession() as client:
        return serve(HarborTask(client,args.catalog,args.task,args.artifacts),sys.stdin,sys.stdout)


if __name__ == "__main__":
    raise SystemExit(main())
