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


def verify_cleanup(campaign):
    finished = json.loads((campaign/"finished.json").read_bytes())
    if finished.get("completed_batches") != 2 or finished.get("failed_batches") != 0 or finished.get("stopped") is not False:
        raise AssertionError("campaign did not finish both batches without failures")
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


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT") and os.environ.get("ASB_TEST_BENCHMARK_ASB"),
                     "requires explicit ASB, original SWE SDK/catalog and installed Pi assets")
class RepeatedOriginalQualification(unittest.TestCase):
    def test_public_repeat_prepares_isolated_original_tasks_before_real_client_execution(self):
        import guest_provider_fixture as provider
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).resolve()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).resolve()
        suite = os.environ.get("ASB_TEST_BENCHMARK_SUITE","swe-bench-lite")
        self.assertIn(suite,["swe-bench-lite","terminal-bench-1.0","terminal-bench-2.1"])
        instance = {"swe-bench-lite":"astropy__astropy-12907",
            "terminal-bench-1.0":"hello-world","terminal-bench-2.1":"cancel-async-tasks"}[suite]
        artifacts = Path(tempfile.mkdtemp(prefix="repeated-original-",dir=root))
        print(f"Retained repeated original-task qualification: {artifacts}",flush=True)
        provider.root = artifacts
        fix = ("from pathlib import Path; p=Path('astropy/modeling/separable.py'); t=p.read_text(); "
               "old='cright[-right.shape[0]:, -right.shape[1]:] = 1'; "
               "assert t.count(old)==1; p.write_text(t.replace(old, 'cright[-right.shape[0]:, -right.shape[1]:] = right'))")
        command = "python -c "+shlex.quote(fix)
        if suite == "terminal-bench-1.0":
            command = "printf 'Hello, world!\\n' > hello.txt"
        elif suite == "terminal-bench-2.1":
            solution = ("import asyncio\nasync def run_tasks(tasks, max_concurrent):\n"
                "    semaphore = asyncio.Semaphore(max_concurrent)\n"
                "    async def run(task):\n        async with semaphore:\n            await task()\n"
                "    async with asyncio.TaskGroup() as group:\n        for task in tasks:\n            group.create_task(run(task))\n")
            command = "python3 -c "+shlex.quote("from pathlib import Path; Path('run.py').write_text("+repr(solution)+")")
        (artifacts/"command.txt").write_text(command)
        barrier, lock, arrivals = threading.Barrier(2),threading.Lock(),[]
        class Handler(provider.Handler):
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
        try:
            config = artifacts/"benchmark.toml"
            config.write_text("agent='pi'\nprovider='openrouter'\nmodel='fixture-model'\n"
                f"workload='{suite}'\ntrial='{instance}'\nconcurrency=2\n"
                "workspace='unused-workspace'\nlog_file='unused.log'\n"
                "[measurements]\nenabled=false\ndirectory='unused-measurements'\ncollectors=[]\n")
            base = f"http://127.0.0.1:{server.server_address[1]}/v1"
            environment = {"PATH":os.environ["PATH"],"HOME":str(artifacts),
                "OPENROUTER_API_KEY":"fixture-token","OPENROUTER_BASE_URL":base,
                "OPENROUTER_MODELS_URL":base+"/models","NO_PROXY":"*","no_proxy":"*"}
            try:
                result = subprocess.run([binary,"run",config,"--repeat","2","--json"],
                    cwd=root,env=environment,capture_output=True,timeout=600)
            except subprocess.TimeoutExpired as error:
                (artifacts/"asb.stdout").write_bytes(error.stdout or b"")
                (artifacts/"asb.stderr").write_bytes(error.stderr or b"")
                raise
            (artifacts/"asb.stdout").write_bytes(result.stdout)
            (artifacts/"asb.stderr").write_bytes(result.stderr)
            self.assertEqual(result.returncode,0,result.stderr.decode())
            rows = [json.loads(line) for line in result.stdout.splitlines()]
            self.assertEqual(len(rows),5)
            self.assertGreaterEqual(len(arrivals),8,"each real client needs its tool call and completion")
            campaign = Path(rows[0]["path"]).parent
            verify_cleanup(campaign)
            plan = json.loads((campaign/"plan.json").read_bytes())
            self.assertEqual(len(plan["slots"]),4)
            identities = [json.loads((campaign/f"batch-{batch}/prepared.json").read_bytes())["compatibility"]
                          for batch in [1,2]]
            self.assertEqual(identities[0],identities[1])
            attempts = set()
            for slot,row in enumerate(rows[1:],1):
                self.assertEqual(row["execution"]["slot"],slot)
                self.assertTrue(row["process_success"],row)
                self.assertTrue(row["workload_success"],row)
                attempts.add(row["task"]["attempt_id"])
            self.assertEqual(len(attempts),4)
            for batch in [1,2]:
                finished = json.loads((campaign/f"batch-{batch}/finished.json").read_bytes())
                self.assertEqual(finished["completed_executions"],2)
                self.assertFalse(finished["stopped"])
                self.assertFalse(finished["failed"])
                self.assertEqual(len(list((campaign/f"batch-{batch}/outcomes").glob("*.json"))),2)
            self.assertFalse((artifacts/"unused-workspace").exists())
            (artifacts/"qualification.json").write_text(json.dumps({"campaign":str(campaign),
                "workload":suite,"task":instance,
                "executions":4,"distinct_original_attempts":4,"official_resolved":4,
                "loopback_inference_requests":len(arrivals),"live_llm_provider_called":False,
                "material_identity":identities[0]["material_sha256"]},sort_keys=True)+"\n")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main()
