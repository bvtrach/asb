"""Opt-in normal ASB run against an isolated synthetic provider and real grader."""
import json
import os
from pathlib import Path
import subprocess
import shlex
import sys
import tempfile
import time
import unittest
import uuid
import fcntl

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB"),
                     "requires explicit ASB builds and cached official dataset/tools/image")
class NormalRunQualification(unittest.TestCase):
    def test_selected_client_runs_and_is_officially_graded_through_normal_cli(self):
        from asb_docker import DockerSession
        from asb_execution import ManagedDockerWorkspace
        from asb_guest import upload
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        suite = os.environ.get("ASB_TEST_BENCHMARK_SUITE", "swe-bench-lite")
        self.assertIn(suite, ["swe-bench", "swe-bench-lite", "swe-bench-verified", "terminal-bench-1.0", "terminal-bench-2.1"])
        terminal = suite.startswith("terminal-bench-")
        instance = ("hello-world" if suite.endswith("1.0") else "cancel-async-tasks") if terminal else "astropy__astropy-12907"
        resolved = os.environ.get("ASB_TEST_BENCHMARK_RESOLVED") == "1"
        tape_mode = os.environ.get("ASB_TEST_BENCHMARK_TAPE_MODE")
        self.assertIn(tape_mode, [None, "record", "replay"])
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="normal-cli-guest-", dir=root))
        print(f"Retained normal-run qualification: {artifacts}", flush=True)
        print(f"Population: {suite}; deterministic known-good repair: {resolved} (not an AI solution)", flush=True)
        cold = os.environ.get("ASB_TEST_COLD_BOOTSTRAP") == "1"
        if cold:
            # Opt-in qualification owns this dedicated project. Preserve its
            # public caches before exercising actual primary-source downloads.
            with (root/".asb-tools/.asb-guest-bootstrap.lock").open("a") as lock:
                fcntl.flock(lock,fcntl.LOCK_EX)
                for cache in [root/".asb-tools/node",root/".asb-cache/npm"]:
                    if cache.exists():
                        self.assertFalse(cache.is_symlink())
                        backup = cache.with_name(cache.name+".before-cold-"+uuid.uuid4().hex)
                        cache.rename(backup)
                        print(f"Preserved public cache: {backup}",flush=True)
                        if cache.name == "npm":
                            def restore_package_cache(cache=cache,backup=backup):
                                with (root/".asb-tools/.asb-guest-bootstrap.lock").open("a") as lock:
                                    fcntl.flock(lock,fcntl.LOCK_EX)
                                    if not cache.exists():
                                        backup.rename(cache)
                                        print(f"Restored prior public package cache: {cache}",flush=True)
                            self.addCleanup(restore_package_cache)
        network_name = "asb-fixture-" + uuid.uuid4().hex
        with DockerSession() as client:
            network = client.networks.create(network_name, internal=True, labels={"asb.owner": "asb"})
            try:
                image = client.images.get("swebench/sweb.eval.x86_64.astropy_1776_astropy-12907:latest")
                with ManagedDockerWorkspace(client, image, "asb-" + uuid.uuid4().hex, "/tmp") as provider:
                    upload(provider.container, "/tmp", [("provider.py", (Path(__file__).parent / "guest_provider_fixture.py").read_bytes(), 0o600)])
                    if resolved:
                        # An explicit oracle fixture exercises the agent's real
                        # shell tool and normal grading; no gold/test dataset
                        # fields are passed to the task container or prompt.
                        code = ("from pathlib import Path; p=Path('astropy/modeling/separable.py'); t=p.read_text(); "
                                "old='cright[-right.shape[0]:, -right.shape[1]:] = 1'; "
                                "assert t.count(old)==1; p.write_text(t.replace(old, 'cright[-right.shape[0]:, -right.shape[1]:] = right'))")
                        if suite == "terminal-bench-1.0":
                            command = "printf 'Hello, world!\\n' > hello.txt"
                        elif suite == "terminal-bench-2.1":
                            # Deliberately deterministic qualification code,
                            # not an AI-generated solution or benchmark score.
                            solution = ("import asyncio\nasync def run_tasks(tasks, max_concurrent):\n"
                                "    semaphore = asyncio.Semaphore(max_concurrent)\n"
                                "    async def run(task):\n        async with semaphore:\n            await task()\n"
                                "    async with asyncio.TaskGroup() as group:\n        for task in tasks:\n            group.create_task(run(task))\n")
                            command = "python3 -c " + shlex.quote("from pathlib import Path; Path('run.py').write_text(" + repr(solution) + ")")
                        else:
                            command = "python3 -c " + shlex.quote(code)
                        upload(provider.container, "/tmp", [("command.txt", command.encode(), 0o600)])
                    network.connect(provider.container)
                    client.networks.get("bridge").disconnect(provider.container)
                    provider.container.exec_run(["python3", "/tmp/provider.py", "/tmp", "0.0.0.0"], detach=True)
                    for _ in range(100):
                        port = provider.container.exec_run(["cat", "/tmp/port"])
                        if port.exit_code == 0:
                            break
                        time.sleep(0.05)
                    self.assertEqual(port.exit_code, 0)
                    provider.container.reload()
                    address = provider.container.attrs["NetworkSettings"]["Networks"][network_name]["IPAddress"]
                    base = f"http://{address}:{int(port.output.strip())}/v1"
                    host_provider = os.environ.get("ASB_TEST_HOST_PROVIDER") == "1"
                    if host_provider:
                        host_root = artifacts / "host-provider"
                        host_root.mkdir()
                        if resolved:
                            (host_root / "command.txt").write_text(command)
                        process = subprocess.Popen([sys.executable,
                            str(Path(__file__).parent / "guest_provider_fixture.py"), str(host_root)],
                            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                        def stop_provider():
                            if process.poll() is None:
                                process.terminate()
                            try:
                                process.wait(timeout=3)
                            except subprocess.TimeoutExpired:
                                process.kill()
                                process.wait(timeout=3)
                        self.addCleanup(stop_provider)
                        for _ in range(100):
                            if (host_root / "port").is_file() or process.poll() is not None:
                                break
                            time.sleep(.05)
                        self.assertIsNone(process.poll())
                        base = f"http://127.0.0.1:{int((host_root / 'port').read_text())}/v1"
                    configuration = artifacts / "benchmark.toml"
                    configuration.write_text(f"agent='pi'\nprovider='openrouter'\nmodel='fixture-model'\nworkload='{suite}'\ntrial='{instance}'\n"
                                             + f"workspace='{artifacts}/workspace'\nlog_file='{artifacts}/run.log'\n")
                    environment = {"PATH": os.environ["PATH"], "HOME": str(artifacts),
                                   "OPENROUTER_API_KEY": "fixture-token", "OPENROUTER_BASE_URL": base,
                                   "OPENROUTER_MODELS_URL": base + "/models", "NO_PROXY": "*", "no_proxy": "*",
                                   "ASB_BENCHMARK_NETWORK": network_name, "NPM_CONFIG_OFFLINE": "true"}
                    if os.environ.get("ASB_TEST_GUEST_BINARY"):
                        environment["ASB_GUEST_BINARY"] = os.environ["ASB_TEST_GUEST_BINARY"]
                    if cold:
                        environment.pop("NPM_CONFIG_OFFLINE")
                    command_line = [binary, "run", configuration, "--json"]
                    if tape_mode:
                        command_line += ["--" + tape_mode, "--cassette", os.environ["ASB_TEST_BENCHMARK_CASSETTE"]]
                    result = subprocess.run(command_line, cwd=root,
                                            env=environment, capture_output=True, timeout=240)
                    (artifacts / "asb.stdout").write_bytes(result.stdout)
                    (artifacts / "asb.stderr").write_bytes(result.stderr)
                    self.assertEqual(result.returncode, 0 if resolved else 2, result.stderr.decode())
                    record = json.loads(result.stdout)
                    self.assertEqual(record["process_success"], True, record)
                    self.assertEqual(record["workload_success"], resolved, record)
                    self.assertEqual(record["workload_evaluation"], "upstream-harness")
                    self.assertEqual(record["workload_baseline"]["kind"], "upstream-environment-definition" if terminal else "upstream-task-definition")
                    self.assertIsNone(record["workload_baseline_passed"])
                    self.assertEqual(record["success_rate"], 1.0 if resolved else 0.0)
                    if tape_mode == "replay":
                        if host_provider:
                            self.assertFalse((host_root / "calls.json").exists(), "replay contacted the provider")
                            self.assertFalse((host_root / "requests.json").exists(), "replay contacted the provider catalog")
                        else:
                            self.assertNotEqual(provider.container.exec_run(["test", "-e", "/tmp/calls.json"]).exit_code, 0)
                            self.assertNotEqual(provider.container.exec_run(["test", "-e", "/tmp/requests.json"]).exit_code, 0)
                        calls = 2  # The deterministic recorded fixture has two calls.
                    else:
                        calls = json.loads((host_root / "calls.json").read_text() if host_provider else
                                           provider.container.exec_run(["cat", "/tmp/calls.json"]).output)["calls"]
                        self.assertGreaterEqual(calls, 2)
                    self.assertEqual(record["token_usage"]["total_tokens"], calls * 22)
                    identity = record["task"]
                    self.assertEqual(identity["task_id"], instance)
                    matching = [path.parent for path in (root / ".asb/attempts").glob("*/task.json")
                                if json.loads(path.read_text())["identity"] == identity]
                    self.assertEqual(len(matching), 1)
                    receipt = json.loads((matching[0]/"runtime.json").read_text())
                    bootstrap = json.loads((matching[0]/"bootstrap.json").read_text())
                    self.assertEqual(bootstrap["runtime_tools"],["node"])
                    self.assertEqual(len(bootstrap["assets"]),1)
                    if cold:
                        self.assertFalse((root/".asb-tools/node/runtime").exists(),
                                         "guest prefetch unexpectedly replaced or installed a host runtime")
                    if os.environ.get("ASB_TEST_EXPECT_RUNTIME_KIND"):
                        self.assertEqual(receipt["kind"],os.environ["ASB_TEST_EXPECT_RUNTIME_KIND"])
                    observation = json.loads((matching[0] / "execution.json").read_text())
                    self.assertLessEqual(abs(record["elapsed_ms"] - observation["elapsed_seconds"] * 1000), 1)
                    if terminal:
                        self.assertEqual(json.loads((matching[0] / "submission.json").read_text())["kind"], "in-place-environment")
                        grade = json.loads((matching[0] / "evaluation.json").read_text())
                        if suite == "terminal-bench-1.0":
                            self.assertTrue(grade["test_results"])
                        else:
                            self.assertEqual(grade["rewards"], {"reward":1 if resolved else 0})
                    else:
                        self.assertIn("cright" if resolved else "asb-real-agent.txt", (matching[0] / "candidate.patch").read_text())
                    self.assertEqual(json.loads((matching[0] / "task-cleanup.json").read_text())["status"], "removed")
            finally:
                network.remove()


if __name__ == "__main__":
    unittest.main()
