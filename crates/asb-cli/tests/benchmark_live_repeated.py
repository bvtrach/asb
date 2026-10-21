"""Opt-in repeated native replay with actual delegated Perfetto, never an LLM.

Requires ASB_TEST_PERFETTO_PROJECT (already configured host capabilities) and
ASB_TEST_BENCHMARK_ASB. No host setup, global tracing reset or client install.
"""
import fcntl
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest


@unittest.skipUnless(os.environ.get("ASB_TEST_PERFETTO_PROJECT") and os.environ.get("ASB_TEST_BENCHMARK_ASB"),
                     "requires explicit ASB and already delegated Perfetto project")
class RepeatedCaptureQualification(unittest.TestCase):
    def test_repeated_captures_keep_project_authority_and_survive_a_failed_sibling(self):
        root = Path(os.environ["ASB_TEST_PERFETTO_PROJECT"]).resolve()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).resolve()
        settings = tomllib.loads((root / ".asb/project.toml").read_text())
        self.assertTrue(settings.get("perfetto_instances"), "configure the project explicitly before qualification")
        artifacts = Path(tempfile.mkdtemp(prefix="repeated-perfetto-", dir=root))
        print(f"Retained repeated Perfetto qualification: {artifacts}", flush=True)
        # This nested output project deliberately has foreign capabilities. It
        # owns no tracing resources and must never become the service authority.
        foreign = artifacts / "foreign-output"
        (foreign / ".asb").mkdir(parents=True)
        (foreign / ".asb/project.toml").write_text("perfetto_instances=['foreign-instance']\n")
        agent = artifacts / "fixture-agent"
        agent.write_text(r'''#!/usr/bin/env python3
import json, os, pathlib, sys, time, urllib.request
root = pathlib.Path(__file__).parent
workspace = pathlib.Path.cwd()
batch = int(workspace.parent.name.split('-')[-1])
replica = int(workspace.name.split('-')[-1])
start = time.monotonic_ns()
(root/f'barrier-{batch}-{replica}').write_text('ready')
deadline = time.monotonic()+15
while not all((root/f'barrier-{batch}-{index}').exists() for index in [1,2]):
    assert time.monotonic()<deadline, 'replicas did not overlap'
    time.sleep(.005)
body = {'model':'fixture','messages':[],'tools':[{'type':'function','function':{'name':'fixture','parameters':{'type':'object'}}}]}
url = os.environ['ASB_PROVIDER_BASE_URL'].rstrip('/')
if url.endswith('/v1'): url = url[:-3]
with urllib.request.urlopen(urllib.request.Request(url+'/v1/chat/completions',data=json.dumps(body).encode(),headers={'Content-Type':'application/json'}),timeout=5) as response:
    assert response.status == 200
time.sleep(.15 if batch == 1 and replica == 1 else 1)
path = workspace/'src/parser.py'
path.write_text("def parse_path(value: str) -> list[str]:\n    return [segment for segment in value.split('/') if segment]\n")
record = {'batch':batch,'replica':replica,'start':start,'end':time.monotonic_ns()}
(root/f'attempt-{batch}-{replica}.json').write_text(json.dumps(record))
sys.exit(7 if batch == 1 and replica == 1 else 0)
''')
        agent.chmod(0o700)
        body = {"model":"fixture", "messages":[], "tools":[{"type":"function", "function":{"name":"fixture", "parameters":{"type":"object"}}}]}
        cassette = artifacts / "input.jsonl"
        cassette.write_text(json.dumps({"request":{"method":"POST", "url":"/v1/chat/completions", "body":json.dumps(body)},
            "response":{"status":200, "headers":{"content-type":"application/json"}, "body":"{}"}})+"\n")
        config = artifacts / "benchmark.toml"
        config.write_text(f"agent={json.dumps(str(agent))}\nprovider='fixture'\nmodel='fixture'\nworkload='bug-fix'\nconcurrency=2\n"
            "workspace='unused-workspace'\nlog_file='unused.log'\n"
            "[measurements]\nenabled=true\ndirectory='unused-measurements'\ncollectors=['perfetto']\n")
        environment = {"PATH":os.environ["PATH"], "HOME":str(artifacts), "NO_PROXY":"*", "no_proxy":"*"}
        result = subprocess.run([binary, "run", config, "--repeat", "2", "--replay", "--cassette", cassette,
            "--measurements", foreign / "captures", "--json"], cwd=root, env=environment,
            capture_output=True, timeout=240)
        (artifacts / "asb.stdout").write_bytes(result.stdout)
        (artifacts / "asb.stderr").write_bytes(result.stderr)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("1 failed batch(es)", result.stderr.decode())
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(len(rows), 5, result.stdout.decode())
        plan_path = Path(rows[0]["path"])
        plan = json.loads(plan_path.read_bytes())
        self.assertEqual(len(plan["slots"]), 4)
        attempts = [json.loads((artifacts/f"attempt-{batch}-{replica}.json").read_bytes())
                    for batch in [1,2] for replica in [1,2]]
        for batch in [1,2]:
            selected = [item for item in attempts if item["batch"] == batch]
            self.assertLess(max(item["start"] for item in selected), min(item["end"] for item in selected))
        self.assertLess(max(item["end"] for item in attempts if item["batch"] == 1),
                        min(item["start"] for item in attempts if item["batch"] == 2))
        traces, used_instances = [], set()
        for slot in plan["slots"]:
            directory = Path(plan["measurement_root"]) / f"batch-{slot['batch']}"
            directory = directory / f"measurements-concurrency-{slot['replica']}"
            observation = json.loads((directory / "perfetto-tracepoints.json").read_bytes())
            self.assertTrue(observation["capture_complete"], observation)
            self.assertTrue(observation["marker_verified"], observation)
            self.assertIsNone(observation["scope_warning"], observation)
            self.assertIsNone(observation["capture_warning"], observation)
            instance = Path(observation["trace_marker"]).parent.name
            self.assertIn(instance, settings["perfetto_instances"])
            used_instances.add(instance)
            processed = json.loads((directory / "perfetto.json").read_bytes())
            self.assertGreater(processed["metrics"]["perfetto.scheduler_events"], 0)
            trace = directory / observation["trace"]
            self.assertGreater(trace.stat().st_size, 0)
            traces.append(str(trace))
        self.assertEqual(len(set(traces)), 4)
        self.assertFalse((foreign / ".asb/perfetto").exists())
        self.assertTrue(json.loads((plan_path.parent / "finished.json").read_bytes())["completed_batches"] == 2)
        for instance in used_instances:
            lease = root / ".asb/perfetto" / f"{instance}.lease"
            if lease.exists():
                # No lease deletion or service signals: only prove that this
                # finished process released its original persistent lock.
                with lease.open("rb") as source:
                    fcntl.flock(source, fcntl.LOCK_EX | fcntl.LOCK_NB)
        (artifacts / "qualification.json").write_text(json.dumps({"executions":4, "batches":2,
            "ordinary_failures":1, "traces":traces, "campaign":str(plan_path.parent),
            "provider_called":False, "real_agent_started":False}, sort_keys=True)+"\n")


if __name__ == "__main__":
    unittest.main()
