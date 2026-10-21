"""Opt-in public repeated original benchmark execution with Pi and no LLM.

Uses installed original SDK/catalog/client assets and a deterministic loopback
provider. A known repair exercises grading, not AI task-solving capability.
"""
import http.server
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import threading
import time
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


def verify_cleanup(campaign,failed_batches=0):
    finished = json.loads((campaign/"finished.json").read_bytes())
    if finished.get("completed_batches") != 2 or finished.get("failed_batches") != failed_batches or finished.get("stopped") is not False:
        raise AssertionError("campaign did not finish both batches with the expected failure count")
    for batch in [1,2]:
        for replica in [1,2]:
            link = json.loads((campaign/f"batch-{batch}/outcomes/{replica}.json").read_bytes())
            relative = Path(link["path"])
            if relative.is_absolute() or ".." in relative.parts:
                raise AssertionError("outcome reference escapes campaign")
            payload = (campaign/relative).read_bytes()
            if len(payload) != link["bytes"] or hashlib.sha256(payload).hexdigest() != link["sha256"]:
                raise AssertionError("immutable outcome bytes changed")
            value = json.loads(payload)
            if value.get("workload_cleanup_confirmed") is not True or value.get("proxy_cleanup_confirmed") is not True:
                raise AssertionError("execution cleanup was not confirmed")
            if value.get("workload_resource_disposition") not in ["not-required","removed","not-created","stopped-preserved"] or value["workload_resource_disposition"] != value["summary"].get("workload_resource_disposition"):
                raise AssertionError("resource disposition differs between outcome and summary")


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT") and os.environ.get("ASB_TEST_BENCHMARK_ASB"),
                     "requires explicit ASB, original SWE SDK/catalog and installed Pi assets")
class RepeatedOriginalQualification(unittest.TestCase):
    def test_public_repeat_prepares_isolated_original_tasks_before_real_client_execution(self):
        self.qualify()

    @unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_FAULT") == "block-verifier-once",
                         "requires explicit owned-task verifier fault injection")
    def test_confirmed_preserved_grader_error_continues_new_batches_without_retry(self):
        self.qualify(fault=True)

    def qualify(self,fault=False):
        import guest_provider_fixture as provider
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).resolve()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).resolve()
        binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
        sources = sorted((Path(__file__).parents[1]/"assets/benchmarks").glob("asb_*.py"))
        source_hashes = {path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
        suite = os.environ.get("ASB_TEST_BENCHMARK_SUITE","swe-bench-lite")
        self.assertIn(suite,["swe-bench-lite","terminal-bench-1.0","terminal-bench-2.1"])
        if fault:
            self.assertEqual(suite,"terminal-bench-1.0","fault fixture targets the original Terminal1 test-directory injection")
        instance = {"swe-bench-lite":"astropy__astropy-12907",
            "terminal-bench-1.0":"hello-world","terminal-bench-2.1":"cancel-async-tasks"}[suite]
        selected_task = os.environ.get("ASB_TEST_BENCHMARK_TASK")
        if selected_task is not None:
            self.assertEqual(suite,"terminal-bench-1.0")
            self.assertIn(selected_task,["hello-world","jupyter-notebook-server"])
            instance = selected_task
        same_shell = instance == "jupyter-notebook-server"
        artifacts = Path(tempfile.mkdtemp(prefix="repeated-original-",dir=root))
        print(f"Retained repeated original-task qualification: {artifacts}",flush=True)
        provider.root = artifacts
        fix = ("from pathlib import Path; p=Path('astropy/modeling/separable.py'); t=p.read_text(); "
               "old='cright[-right.shape[0]:, -right.shape[1]:] = 1'; "
               "assert t.count(old)==1; p.write_text(t.replace(old, 'cright[-right.shape[0]:, -right.shape[1]:] = right'))")
        command = "python -c "+shlex.quote(fix)
        if suite == "terminal-bench-1.0":
            command = "printf 'Hello, world!\\n' > hello.txt"
            if same_shell:
                import yaml
                catalog_path = root/"workloads/terminal-bench-1.0/catalog.json"
                catalog = json.loads(catalog_path.read_bytes())
                selected = next(item for item in catalog["tasks"] if item["id"] == instance)
                definition = catalog_path.parent/selected["definition"]
                self.assertIs(yaml.safe_load((definition/"task.yaml").read_text())["run_tests_in_same_shell"],True)
                solution = (definition/"solution.sh").read_text()
                command = ("printf 'ASB_QUAL_CONTEXT=%s|%s|%s\\n' \"$(id -u)\" \"$PWD\" \"$HOME\"; "
                    "if /bin/bash -c "+shlex.quote(solution)+" > /tmp/asb-reference-install.log 2>&1; "
                    "then printf 'Reference installation completed.\\n'; "
                    "else tail -c 8192 /tmp/asb-reference-install.log >&2; exit 1; fi")
        elif suite == "terminal-bench-2.1":
            solution = ("import asyncio\nasync def run_tasks(tasks, max_concurrent):\n"
                "    semaphore = asyncio.Semaphore(max_concurrent)\n"
                "    async def run(task):\n        async with semaphore:\n            await task()\n"
                "    async with asyncio.TaskGroup() as group:\n        for task in tasks:\n            group.create_task(run(task))\n")
            command = "python3 -c "+shlex.quote("from pathlib import Path; Path('run.py').write_text("+repr(solution)+")")
        (artifacts/"command.txt").write_text(command)
        barrier, lock, arrivals = threading.Barrier(2),threading.Lock(),[]
        tool_commands = [0]
        injected_faults = []
        class Handler(provider.Handler):
            def tool_command(self):
                with lock:
                    ordinal = tool_commands[0]
                    tool_commands[0] += 1
                selected = super().tool_command()
                if fault and ordinal == 0:
                    with lock:
                        injected_faults.append(ordinal)
                    # Only this fresh owned task is modified. A regular file
                    # blocks verifier-directory injection deterministically,
                    # before any private tests execute; nothing is regraded.
                    selected += "\nprintf 'ASB explicit verifier fault\\n' > /tests"
                return selected

            def do_POST(self):
                with lock:
                    ordinal = len(arrivals)
                    arrivals.append(time.monotonic_ns())
                if ordinal < 2:
                    try:
                        barrier.wait(timeout=90)
                    except threading.BrokenBarrierError:
                        self.send_error(504,"parallel client fixture barrier expired")
                        return
                super().do_POST()
        server = http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
        thread = threading.Thread(target=server.serve_forever,daemon=True)
        thread.start()
        network_session,network = None,None
        try:
            if same_shell:
                from asb_docker import DockerSession
                network_session = DockerSession()
                docker_client = network_session.__enter__()
                network = docker_client.networks.create("asb-repeat-fixture-"+artifacts.name,
                    internal=False,labels={"asb.owner":"asb"})
            config = artifacts/"benchmark.toml"
            config.write_text("agent='pi'\nprovider='openrouter'\nmodel='fixture-model'\n"
                f"workload='{suite}'\ntrial='{instance}'\nconcurrency=2\n"
                "workspace='unused-workspace'\nlog_file='unused.log'\n"
                "[measurements]\nenabled=false\ndirectory='unused-measurements'\ncollectors=[]\n")
            base = f"http://127.0.0.1:{server.server_address[1]}/v1"
            environment = {"PATH":os.environ["PATH"],"HOME":str(artifacts),
                "OPENROUTER_API_KEY":"fixture-token","OPENROUTER_BASE_URL":base,
                "OPENROUTER_MODELS_URL":base+"/models","NO_PROXY":"*","no_proxy":"*"}
            if network is not None:environment["ASB_BENCHMARK_NETWORK"] = network.name
            try:
                result = subprocess.run([binary,"run",config,"--repeat","2","--json"],
                    cwd=root,env=environment,capture_output=True,timeout=600)
            except subprocess.TimeoutExpired as error:
                (artifacts/"asb.stdout").write_bytes(error.stdout or b"")
                (artifacts/"asb.stderr").write_bytes(error.stderr or b"")
                raise
            (artifacts/"asb.stdout").write_bytes(result.stdout)
            (artifacts/"asb.stderr").write_bytes(result.stderr)
            if fault:
                self.assertNotEqual(result.returncode,0,"expected grader error must remain an overall command failure")
            else:
                self.assertEqual(result.returncode,0,result.stderr.decode())
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            self.assertEqual(len(rows),5)
            self.assertGreaterEqual(len(arrivals),8,"each real client needs its tool call and completion")
            campaign = Path(rows[0]["path"]).parent
            failed_batches = sum(any(row["failure_kind"] is not None for row in rows[1:]
                if row["execution"]["batch"] == batch) for batch in [1,2])
            verify_cleanup(campaign,failed_batches if fault else 0)
            plan = json.loads((campaign/"plan.json").read_bytes())
            self.assertEqual(len(plan["slots"]),4)
            identities = [json.loads((campaign/f"batch-{batch}/prepared.json").read_bytes())["compatibility"]
                          for batch in [1,2]]
            self.assertEqual(identities[0],identities[1])
            attempts = set()
            for slot,row in enumerate(rows[1:],1):
                self.assertEqual(row["execution"]["slot"],slot)
                self.assertEqual(row["planning"]["repetition"],dict(index=(slot-1)//2+1,total=2))
                self.assertEqual(row["execution"]["admission"],plan["admission"])
                self.assertEqual(row["execution_plan_fingerprint"],rows[1]["execution_plan_fingerprint"])
                self.assertTrue(row["process_success"],row)
                if not fault:
                    self.assertTrue(row["workload_success"],row)
                attempts.add(row["task"]["attempt_id"])
                if same_shell:
                    directory = Path(row["workload_artifacts"][0])
                    execution = json.loads((directory/"execution.json").read_bytes())
                    self.assertEqual(execution["agent_tree_cleanup"],"delegated-owned-workload")
                    self.assertFalse(execution["output_truncated"])
                    self.assertFalse(execution["timed_out"])
                    self.assertFalse(execution["cancelled"])
                    log_path = Path(plan["slots"][slot-1]["log"])
                    if not log_path.is_absolute():log_path = campaign/log_path
                    logged_rows = [json.loads(line) for line in log_path.read_text().splitlines() if line.strip()]
                    matching = [value for value in logged_rows
                        if value.get("task",{}).get("attempt_id") == row["task"]["attempt_id"]
                        and value.get("execution",{}).get("slot") == slot]
                    self.assertEqual(len(matching),1,"the context observation must belong to this exact execution")
                    logged = matching[0]
                    self.assertIn("ASB_QUAL_CONTEXT=0|/app|/root",logged["stdout"])
                    self.assertTrue((directory/"official"/instance/row["task"]["attempt_id"]/"panes/post-test.txt").is_file())
            self.assertEqual(len(attempts),4)
            if fault:
                self.assertEqual(injected_faults,[0])
                preserved = [row for row in rows[1:] if row["workload_resource_disposition"] == "stopped-preserved"]
                self.assertTrue(any(row["execution"]["batch"] == 1 for row in preserved))
                for row in preserved:
                    self.assertIsNone(row["workload_success"])
                    self.assertIsNone(row["success_rate"])
                    self.assertEqual(row["failure_kind"],"grader_error")
                    self.assertTrue(row["workload_artifacts"])
                    directory = Path(row["workload_artifacts"][0])
                    self.assertEqual(json.loads((directory/"task-cleanup.json").read_bytes())["status"],"stopped-preserved")
                    started = json.loads((directory/"evaluation-started.json").read_bytes())
                    self.assertEqual(started["schema_version"],1)
                    self.assertEqual(started["scope"],"official-verifier-started-not-a-grade")
                    self.assertEqual(started["task"],row["task"])
                    self.assertFalse(list(directory.glob("recovered-evaluation-*")))
                    self.assertFalse((directory/"selected-evaluation.json").exists())
            for batch in [1,2]:
                finished = json.loads((campaign/f"batch-{batch}/finished.json").read_bytes())
                self.assertEqual(finished["completed_executions"],2)
                self.assertFalse(finished["stopped"])
                self.assertEqual(finished["failed"],fault and any(row["failure_kind"] is not None
                    for row in rows[1:] if row["execution"]["batch"] == batch))
                self.assertEqual(len(list((campaign/f"batch-{batch}/outcomes").glob("*.json"))),2)
            self.assertFalse((artifacts/"unused-workspace").exists())
            self.assertEqual(binary_hash,hashlib.sha256(binary.read_bytes()).hexdigest())
            self.assertEqual(source_hashes,{path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources})
            (artifacts/"qualification.json").write_text(json.dumps({"campaign":str(campaign),
                "binary_sha256":binary_hash,"source_hashes":source_hashes,
                "workload":suite,"task":instance,
                "original_same_shell":same_shell,
                "task_tool_network":"owned-bridge-with-package-egress" if same_shell else "original-task-network",
                "executions":4,"distinct_original_attempts":4,
                "official_resolved":sum(row["workload_success"] is True for row in rows[1:]),
                "expected_verifier_fault":fault,
                "loopback_inference_requests":len(arrivals),"live_llm_provider_called":False,
                "material_identity":identities[0]["material_sha256"]},sort_keys=True)+"\n")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
            try:
                if network is not None:
                    network.reload()
                    if not network.attrs.get("Containers"):network.remove()
                    else:(artifacts/"network-preserved.json").write_text(json.dumps(dict(
                        network=network.name,scope="owned-failed-qualification",status="preserved"))+"\n")
            finally:
                if network_session is not None:network_session.__exit__(None,None,None)


if __name__ == "__main__":
    unittest.main()
