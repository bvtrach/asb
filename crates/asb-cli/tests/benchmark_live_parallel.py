"""Opt-in mixed original SWE/Terminal task scheduling with real Pi, no LLM.

A two-request provider barrier proves both clients execute concurrently. The
provider supplies deterministic known repairs; this is not AI task solving.
"""
import http.server
import io
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
                     "requires explicit original catalogs, real client and Docker opt-in")
class ParallelOriginalTaskQualification(unittest.TestCase):
    def test_distinct_original_tasks_overlap_and_keep_separate_grades_and_collectors(self):
        from asb_docker import DockerSession
        import guest_provider_fixture as provider
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="parallel-original-tasks-", dir=root))
        print(f"Retained mixed original-task qualification: {artifacts}", flush=True)
        provider.root = artifacts
        fix = ("from pathlib import Path; p=Path('astropy/modeling/separable.py'); t=p.read_text(); "
               "old='cright[-right.shape[0]:, -right.shape[1]:] = 1'; "
               "assert t.count(old)==1; p.write_text(t.replace(old, 'cright[-right.shape[0]:, -right.shape[1]:] = right'))")
        (artifacts / "command.txt").write_text("if [ -f astropy/modeling/separable.py ]; then python3 -c "
            + shlex.quote(fix) + "; else printf 'Hello, world!\\n' > hello.txt; fi")
        barrier = threading.Barrier(2)
        lock = threading.Lock()
        arrivals = []
        class Handler(provider.Handler):
            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 2 * 1024**2:
                    self.send_error(413)
                    return
                payload = self.rfile.read(length)
                text = payload.decode("utf-8").lower()
                task = "swe" if "astropy" in text else "terminal" if "hello.txt" in text else "unknown"
                with lock:
                    ordinal = len(arrivals)
                    arrivals.append({"task":task, "started_ns":time.monotonic_ns()})
                if ordinal < 2:
                    try:
                        barrier.wait(timeout=90)
                    except threading.BrokenBarrierError:
                        self.send_error(504, "parallel-client barrier timed out")
                        return
                original = self.rfile
                try:
                    self.rfile = io.BytesIO(payload)
                    super().do_POST()
                finally:
                    self.rfile = original
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            base = f"http://127.0.0.1:{server.server_address[1]}/v1"
            network_name = "asb-fixture-" + uuid.uuid4().hex
            with DockerSession() as client:
                network = client.networks.create(network_name, internal=True, labels={"asb.owner":"asb"})
                try:
                    for name, suite, instance in [("swe", "swe-bench-lite", "astropy__astropy-12907"),
                                                  ("terminal", "terminal-bench-1.0", "hello-world")]:
                        (artifacts / (name + ".toml")).write_text(
                            f"agent='pi'\nprovider='openrouter'\nmodel='fixture-model'\nworkload='{suite}'\ntrial='{instance}'\n"
                            "[measurements]\nenabled=true\ndirectory='measurements'\ncollectors=['agent-memory','agent-workers']\n")
                    manifest = artifacts / "sweep.toml"
                    manifest.write_text("configs=['swe.toml','terminal.toml']\n")
                    environment = {"PATH":os.environ["PATH"], "HOME":str(artifacts),
                        "OPENROUTER_API_KEY":"fixture-token", "OPENROUTER_BASE_URL":base,
                        "OPENROUTER_MODELS_URL":base + "/models", "NO_PROXY":"*", "no_proxy":"*",
                        "ASB_BENCHMARK_NETWORK":network_name, "NPM_CONFIG_OFFLINE":"true",
                        "ASB_MEASURE_INTERVAL_MS":"50"}
                    result = subprocess.run([binary,"sweep",manifest,"--jobs","2","--max-active-attempts","2","--json"],
                        cwd=root, env=environment, capture_output=True, timeout=240)
                    (artifacts / "asb.stdout").write_bytes(result.stdout)
                    (artifacts / "asb.stderr").write_bytes(result.stderr)
                    self.assertEqual(result.returncode, 0, result.stderr.decode())
                    inputs = json.loads((artifacts / "sweep-results/.checkpoint/inputs.json").read_bytes())
                    self.assertEqual(inputs["status"], "stable", inputs["status"])
                    self.assertEqual(inputs["initial"], inputs["final"])
                    self.assertTrue(any(root["name"].startswith("runtime-source-") and
                        "libc.so" in root["path"] for root in inputs["roots"]))
                    records = [json.loads(line) for line in result.stdout.splitlines()]
                    self.assertEqual(len(records), 2)
                    self.assertGreaterEqual(len(arrivals), 4, "both real clients need a tool call and completion")
                    original_calls = len(arrivals)
                    resumed = subprocess.run([binary,"sweep",manifest,"--jobs","2","--max-active-attempts","2","--resume","--json"],
                        cwd=root, env=environment, capture_output=True, timeout=120)
                    (artifacts / "resume.stdout").write_bytes(resumed.stdout)
                    (artifacts / "resume.stderr").write_bytes(resumed.stderr)
                    self.assertEqual(resumed.returncode, 0, resumed.stderr.decode())
                    reused = [json.loads(line) for line in resumed.stdout.splitlines()]
                    self.assertEqual(len(reused), 2)
                    self.assertTrue(all(record["reused"] and record["workload_success"] for record in reused))
                    self.assertEqual({record["task"]["attempt_id"] for record in reused},
                        {record["task"]["attempt_id"] for record in records})
                    self.assertEqual(len(arrivals), original_calls, "continuation must not start either real client again")
                    self.assertFalse(barrier.broken)
                    self.assertEqual({entry["task"] for entry in arrivals[:2]}, {"swe","terminal"},
                        "the barrier must overlap distinct clients, not two auxiliary calls from one client")
                    for record in records:
                        self.assertTrue(record["process_success"], record)
                        self.assertTrue(record["workload_success"], record)
                        self.assertEqual(record["workload_evaluation"], "upstream-harness")
                        self.assertEqual(record["agent"], "pi")
                        self.assertEqual(record["provider"], "openrouter")
                    targets = []
                    for index, name in enumerate(["000-swe", "001-terminal"]):
                        directory = artifacts / "sweep-results" / name
                        self.assertFalse((directory / ".terminal-output.tmp").exists())
                        self.assertFalse((artifacts / "sweep-results" / f".terminal-output-{index:06}.tmp").exists())
                        receipt = json.loads((artifacts / "sweep-results/.checkpoint/completed" / f"{index:06}.json").read_bytes())
                        self.assertEqual(receipt["scope"], "outcome-evidence-only-not-complete-input-closure")
                        self.assertEqual(len(receipt["fingerprint"]["digest"]), 64)
                        self.assertEqual(len(receipt["roots"]), 2)
                        completions = list((directory / ".asb-completions").glob("*.json"))
                        self.assertEqual(len(completions), 1)
                        completion = json.loads(completions[0].read_bytes())
                        self.assertEqual(completion["replica"], {"index":1,"total":1})
                        replica = json.loads((artifacts / "sweep-results/.checkpoint/replicas" / f"{index:06}/001.json").read_bytes())
                        self.assertEqual(replica["replica"], {"index":1,"total":1})
                        self.assertEqual(replica["scope"], "completed-replica-declared-evidence")
                        self.assertTrue(completion["workload_cleanup_confirmed"])
                        self.assertTrue(completion["summary"]["workload_success"])
                        self.assertEqual(completion["summary"]["workload_evaluation"], "upstream-harness")
                        evidence = completion["workload_evidence"]
                        self.assertEqual(len(evidence), 1)
                        self.assertTrue(Path(evidence[0]).is_dir())
                        self.assertTrue(Path(evidence[0]).is_relative_to(root / ".asb/attempts"))
                        self.assertNotIn("stdout", completion["summary"])
                        self.assertNotIn("stderr", completion["summary"])
                        measured = directory / "measurements"
                        target = json.loads((measured / "execution-target.json").read_bytes())
                        self.assertEqual(target["scope"], "verified-local-isolated-process-tree")
                        self.assertEqual(target["cgroup_membership"], "preserved")
                        targets.append(target)
                        events = [entry["event"]["kind"] for entry in json.loads((measured / "lifecycle.json").read_bytes())]
                        self.assertEqual(completion["summary"]["task"], json.loads((measured / "run.json").read_bytes())["task"])
                        self.assertEqual([event for event in events if event.startswith("agent_")],
                            ["agent_spawn", "agent_execute_begin", "agent_started", "agent_execute_end", "agent_exited"])
                        for collector in ["agent-memory", "agent-workers"]:
                            data = json.loads((measured / (collector + ".json")).read_bytes())
                            self.assertEqual(data["status"], "observed", data)
                            self.assertEqual(data["source"], "execution-namespace-linux-proc-sampling")
                            self.assertGreater(data["samples"], 0)
                        identifier = re.search(r"[a-f0-9]{64}", target["cgroup"])
                        self.assertIsNotNone(identifier)
                        import docker.errors
                        with self.assertRaises(docker.errors.NotFound):
                            client.containers.get(identifier.group())
                    self.assertNotEqual(targets[0]["host_pid"], targets[1]["host_pid"])
                    self.assertNotEqual(targets[0]["cgroup"], targets[1]["cgroup"])
                    comparison = artifacts / "results/sweep-comparison"
                    report = json.loads((comparison / "comparison.json").read_bytes())
                    self.assertEqual({record["workload"] for record in report["benchmarks"]}, {"swe-bench-lite", "terminal-bench-1.0"})
                    self.assertEqual(report["evaluation_coverage"]["evaluated_attempts"], 2)
                    self.assertEqual(report["selection_plan"]["scheduling"], {"jobs":2,"max_active_attempts":2})
                    self.assertEqual(len(report["population_coverage"]), 2)
                    for population in report["population_coverage"].values():
                        self.assertEqual(population["selected_tasks"], 1)
                        self.assertIsNone(population["full_population_resolution_rate"])
                    html = (comparison / "report.html").read_text()
                    for graph in ["metric-agent-memory.peak_rss_mib.svg", "metric-agent-workers.spawned_worker_count.svg"]:
                        self.assertIn("<svg", (comparison / graph).read_text())
                        self.assertIn(graph, html)
                    (artifacts / "overlap.json").write_text(json.dumps({"barrier_participants":2,"arrivals":arrivals}))
                finally:
                    network.remove()
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
