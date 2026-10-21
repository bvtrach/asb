"""Opt-in public retained original Harbor grading, no agent/provider calls."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
                     "requires explicit original Harbor catalog/SDK, ASB and Docker")
class RetainedHarborQualification(unittest.TestCase):
    def test_public_grade_uses_original_preserved_environment_and_keeps_true_and_false_results(self):
        from asb_docker import DockerSession
        from asb_harbor import HarborTask
        from asb_guest import upload
        from benchmark_live_interrupted import cleanup_fixture
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        qualification = Path(tempfile.mkdtemp(prefix="retained-harbor-grading-",dir=root))
        print(f"Retained original Harbor grading qualification: {qualification}",flush=True)
        receipts = []
        for resolved in [False,True]:
            with self.subTest(resolved=resolved),DockerSession() as client:
                preparation = uuid.uuid4().hex
                artifacts = root/".asb/attempts"/preparation
                reference = qualification/("resolved.json" if resolved else "unresolved.json")
                try:
                    with HarborTask(client,root/"workloads/terminal-bench-2.1/catalog.json","cancel-async-tasks",artifacts) as task:
                        if resolved:
                            upload(task.workspace.container,"/tmp",[("known-reference.sh",
                                (task.definition/"solution/solve.sh").read_bytes(),0o700)])
                            task.checked(["bash","/tmp/known-reference.sh"])
                        else:
                            task.checked(["/bin/sh","-c","printf 'retained partial work\\n' > asb-retained-partial.txt"])
                        task.capture_submission()
                        submission = (artifacts/"submission.json").read_bytes()
                        container = task.workspace.container
                        task.interrupted = True
                    identity = json.loads((artifacts/"task.json").read_bytes())["identity"]
                    reference.write_text(json.dumps({"schema_version":1,"preparation_id":preparation,
                        "workload":"terminal-bench-2.1","task_id":"cancel-async-tasks",
                        "dataset":identity["dataset"],"artifacts":str(artifacts)}))
                    reference.chmod(0o600)
                    environment = {"PATH":os.environ["PATH"],"HOME":str(qualification),"PYTHONDONTWRITEBYTECODE":"1"}
                    command = [binary,"workload","recover",reference,"--grade","--json"]
                    graded = subprocess.run(command,cwd=root,env=environment,capture_output=True,timeout=240)
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
                    selected = json.loads((artifacts/"selected-evaluation.json").read_bytes())
                    source = artifacts/selected["directory"]
                    archive = qualification/(reference.stem+"-lost-wrapper")
                    archive.mkdir(mode=0o700)
                    (artifacts/"selected-evaluation.json").rename(archive/"selection.json")
                    (source/"evaluation.json").rename(archive/"evaluation.json")
                    raw = {path:path.read_bytes() for path in source.rglob("*") if path.is_file()}
                    reconstruction_env = dict(environment,DOCKER_HOST="unix://"+str(qualification/"no-docker.sock"))
                    reconstructed = subprocess.run(command,cwd=root,env=reconstruction_env,capture_output=True,timeout=30)
                    reference.with_suffix(".reconstructed.stderr").write_bytes(reconstructed.stderr)
                    self.assertEqual(reconstructed.returncode,0,reconstructed.stderr.decode())
                    report = json.loads(reconstructed.stdout)
                    self.assertIs(report["workload_success"],resolved)
                    self.assertEqual(raw,{path:path.read_bytes() for path in source.rglob("*") if path.is_file()})
                    container.reload()
                    self.assertIs(container.attrs["State"]["Running"],False)
                    started_at = container.attrs["State"]["StartedAt"]
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
        (qualification/"qualification.json").write_text(json.dumps(receipts,indent=2))


if __name__ == "__main__":unittest.main()
