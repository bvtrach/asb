"""Opt-in public grading of retained original Terminal-Bench 1.0 work."""
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
                     "requires explicit ASB, original Terminal-Bench catalog, SDK and Docker")
class OriginalTerminalGrading(unittest.TestCase):
    def test_public_grading_preserves_original_work_and_grades_without_an_agent(self):
        from asb_docker import DockerSession
        from asb_terminal_bench_1 import TerminalBenchTask
        from benchmark_live_interrupted import cleanup_fixture
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        qualification = Path(tempfile.mkdtemp(prefix="retained-terminal-grading-",dir=root))
        print(f"Retained original Terminal-Bench grading qualification: {qualification}",flush=True)
        receipts = []
        for resolved in [False,True]:
            with self.subTest(resolved=resolved), DockerSession() as client:
                preparation = uuid.uuid4().hex
                artifacts = root/".asb/attempts"/preparation
                reference = qualification/("resolved.json" if resolved else "unresolved.json")
                override = None
                try:
                    with TerminalBenchTask(client,root/"workloads/terminal-bench-1.0/catalog.json","hello-world",artifacts) as task:
                        command = "printf 'Hello, world!\\n' > hello.txt" if resolved else "printf 'retained partial work\\n' > asb-retained-partial.txt"
                        result = task.workspace.execute(["/bin/sh","-c",command])
                        self.assertEqual(result["exit_code"],0)
                        task.capture_submission()
                        submission = (artifacts/"submission.json").read_bytes()
                        container, workdir = task.workspace.container, task.workspace.workdir
                        task.interrupted = True
                    saved_networks = task.invocation["original_networks"]
                    if not resolved:
                        override = client.networks.create("asb-retained-grade-test-"+preparation,
                            labels={"asb.owner":"asb-qualification","asb.fixture":preparation})
                        override.connect(container)
                        for name in saved_networks:client.networks.get(name).disconnect(container)
                    identity = json.loads((artifacts/"task.json").read_bytes())["identity"]
                    reference.write_text(json.dumps({"schema_version":1,"preparation_id":preparation,"workload":"terminal-bench-1.0",
                        "task_id":"hello-world","dataset":identity["dataset"],"artifacts":str(artifacts)}))
                    reference.chmod(0o600)
                    environment = {"PATH":os.environ["PATH"],"HOME":str(qualification),"PYTHONDONTWRITEBYTECODE":"1"}
                    command = [binary,"workload","recover",reference,"--grade","--json"]
                    graded = subprocess.run(command,cwd=root,env=environment,capture_output=True,timeout=180)
                    reference.with_suffix(".stdout").write_bytes(graded.stdout)
                    reference.with_suffix(".stderr").write_bytes(graded.stderr)
                    self.assertEqual(graded.returncode,0,graded.stderr.decode())
                    report = json.loads(graded.stdout)
                    self.assertEqual(report["task"],identity)
                    self.assertIs(report["workload_success"],resolved)
                    self.assertEqual(report["recorded_cleanup"],"stopped-preserved")
                    for field in ["process_success","elapsed_ms","exit_code","token_usage","steps"]:
                        self.assertIsNone(report[field])
                    self.assertEqual((artifacts/"submission.json").read_bytes(),submission)
                    container.reload()
                    self.assertIs(container.attrs["State"]["Running"],False)
                    self.assertEqual(set(container.attrs["NetworkSettings"]["Networks"]),set(saved_networks))
                    for name,options in saved_networks.items():
                        self.assertTrue(set(options.get("aliases",[])) <= set(container.attrs["NetworkSettings"]["Networks"][name].get("Aliases") or []))
                    started_at = container.attrs["State"]["StartedAt"]
                    name = "hello.txt" if resolved else "asb-retained-partial.txt"
                    stream,_ = container.get_archive(workdir.rstrip("/")+"/"+name)
                    with tarfile.open(fileobj=io.BytesIO(b"".join(stream))) as archive:
                        self.assertEqual(archive.extractfile(archive.getmembers()[0]).read(),
                            b"Hello, world!\n" if resolved else b"retained partial work\n")
                    before = {path:path.read_bytes() for path in artifacts.rglob("*") if path.is_file()}
                    repeated = subprocess.run(command,cwd=root,env=environment,capture_output=True,timeout=30)
                    self.assertEqual(repeated.returncode,0,repeated.stderr.decode())
                    self.assertEqual(json.loads(repeated.stdout),report)
                    self.assertEqual(before,{path:path.read_bytes() for path in artifacts.rglob("*") if path.is_file()})
                    container.reload()
                    self.assertEqual(container.attrs["State"]["StartedAt"],started_at)
                    inspected = subprocess.run([binary,"workload","recover",reference,"--inspect-grade","--json"],
                        cwd=root,env=environment,capture_output=True,timeout=30)
                    self.assertEqual(inspected.returncode,0,inspected.stderr.decode())
                    self.assertEqual(json.loads(inspected.stdout),report)
                    receipts.append({"resolved":resolved,"artifacts":str(artifacts)})
                finally:
                    cleanup_fixture(client,artifacts)
                    if override is not None:override.remove()
        (qualification/"qualification.json").write_text(json.dumps(receipts,indent=2))


if __name__ == "__main__":unittest.main()
