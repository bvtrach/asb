"""Opt-in public grading of retained original SWE patches, without an LLM."""
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
                     "requires explicit ASB, original catalogs, SDK and Docker opt-in")
class OriginalRetainedGrading(unittest.TestCase):
    def test_public_cli_grades_retained_frozen_patches_without_restarting_the_task(self):
        from asb_docker import DockerSession
        from asb_swebench_task import SweBenchTask
        from benchmark_live_interrupted import cleanup_fixture
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        qualification = Path(tempfile.mkdtemp(prefix="retained-original-grading-",dir=root))
        print(f"Retained original grading qualification: {qualification}",flush=True)
        cases = [("swe-bench",False),("swe-bench-lite",False),("swe-bench-verified",False),("swe-bench-lite",True)]
        receipts = []
        for workload,resolved in cases:
            with self.subTest(workload=workload,resolved=resolved), DockerSession() as client:
                preparation = uuid.uuid4().hex
                artifacts = root/".asb/attempts"/preparation
                reference = qualification/(workload+("-resolved" if resolved else "-unresolved")+".json")
                try:
                    with SweBenchTask(client,root/"workloads"/workload/"catalog.json","astropy__astropy-12907",artifacts) as task:
                        if resolved:
                            script = ("from pathlib import Path; p=Path('astropy/modeling/separable.py'); t=p.read_text(); "
                                "old='cright[-right.shape[0]:, -right.shape[1]:] = 1'; assert t.count(old)==1; "
                                "p.write_text(t.replace(old,'cright[-right.shape[0]:, -right.shape[1]:] = right'))")
                            result = task.workspace.execute(["python3","-c",script])
                        else:
                            result = task.workspace.execute(["/bin/sh","-c","printf 'retained partial work\\n' > asb-retained-partial.txt"])
                        self.assertEqual(result["exit_code"],0)
                        task.capture_submission()
                        candidate = (artifacts/"candidate.patch").read_bytes()
                        container = task.workspace.container
                        task.interrupted = True
                    container.reload()
                    self.assertIs(container.attrs["State"]["Running"],False)
                    task_identity = json.loads((artifacts/"task.json").read_bytes())["identity"]
                    reference.write_text(json.dumps({"schema_version":1,"preparation_id":preparation,"workload":workload,
                        "task_id":task_identity["task_id"],"dataset":task_identity["dataset"],"artifacts":str(artifacts)}))
                    reference.chmod(0o600)
                    environment = {"PATH":os.environ["PATH"],"HOME":str(qualification),"PYTHONDONTWRITEBYTECODE":"1"}
                    command = [binary,"workload","recover",reference,"--grade","--json"]
                    graded = subprocess.run(command,cwd=root,env=environment,capture_output=True,timeout=240)
                    reference.with_suffix(".stdout").write_bytes(graded.stdout)
                    reference.with_suffix(".stderr").write_bytes(graded.stderr)
                    self.assertEqual(graded.returncode,0,graded.stderr.decode())
                    report = json.loads(graded.stdout)
                    self.assertEqual(report["task"],task_identity)
                    self.assertIs(report["workload_success"],resolved)
                    self.assertEqual(report["recorded_cleanup"],"stopped-preserved")
                    for field in ["process_success","elapsed_ms","exit_code","token_usage","steps"]:
                        self.assertIsNone(report[field])
                    self.assertEqual((artifacts/"candidate.patch").read_bytes(),candidate)
                    # Fault-inject loss of only ASB's wrapper/selection after
                    # actual official completion. Archive them outside this
                    # attempt; every original SDK input/report stays untouched.
                    selection = json.loads((artifacts/"selected-evaluation.json").read_bytes())
                    original = artifacts/selection["directory"]
                    archived = qualification/(reference.stem+"-lost-wrapper")
                    archived.mkdir(mode=0o700)
                    (artifacts/"selected-evaluation.json").rename(archived/"selection.json")
                    (original/"evaluation.json").rename(archived/"evaluation.json")
                    raw = {path:path.read_bytes() for path in original.rglob("*") if path.is_file()}
                    # Reconstruction must succeed with no usable daemon. An
                    # accidental evaluator invocation cannot satisfy this test.
                    reconstruction_env = dict(environment,DOCKER_HOST="unix://"+str(qualification/"no-docker.sock"))
                    reconstructed = subprocess.run(command,cwd=root,env=reconstruction_env,capture_output=True,timeout=30)
                    reference.with_suffix(".reconstructed.stdout").write_bytes(reconstructed.stdout)
                    reference.with_suffix(".reconstructed.stderr").write_bytes(reconstructed.stderr)
                    self.assertEqual(reconstructed.returncode,0,reconstructed.stderr.decode())
                    report = json.loads(reconstructed.stdout)
                    self.assertIs(report["workload_success"],resolved)
                    self.assertEqual(raw,{path:path.read_bytes() for path in original.rglob("*") if path.is_file()})
                    derived = json.loads((artifacts/"selected-evaluation.json").read_bytes())
                    self.assertNotEqual(derived["directory"],selection["directory"])
                    self.assertEqual(json.loads((artifacts/derived["directory"]/"evaluation.json").read_bytes())
                                     ["reconstruction"]["source"],selection["directory"])
                    before = {path:path.read_bytes() for path in artifacts.rglob("*") if path.is_file()}
                    repeated = subprocess.run(command,cwd=root,env=environment,capture_output=True,timeout=30)
                    self.assertEqual(repeated.returncode,0,repeated.stderr.decode())
                    self.assertEqual(json.loads(repeated.stdout),report)
                    self.assertEqual({path:path.read_bytes() for path in before},before)
                    self.assertEqual(set(before),{path for path in artifacts.rglob("*") if path.is_file()})
                    inspected = subprocess.run([binary,"workload","recover",reference,"--inspect-grade","--json"],
                        cwd=root,env=environment,capture_output=True,timeout=30)
                    self.assertEqual(inspected.returncode,0,inspected.stderr.decode())
                    self.assertEqual(json.loads(inspected.stdout),report)
                    container.reload()
                    self.assertIs(container.attrs["State"]["Running"],False,"grading must not restart the retained task/client")
                    receipts.append({"workload":workload,"resolved":resolved,"artifacts":str(artifacts)})
                finally:
                    for directory in artifacts.glob("recovered-evaluation-*"):
                        cleanup_fixture(client,directory)
                    cleanup_fixture(client,artifacts)
        (qualification/"qualification.json").write_text(json.dumps(receipts,indent=2))


if __name__ == "__main__":unittest.main()
