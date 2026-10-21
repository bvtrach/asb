"""Opt-in public CLI inspection of real retained original evaluator artifacts.

Copies completed fixture evidence under a new canonical reference; no past
ASB execution, process metrics or history attestation is claimed for that copy.
No Docker, provider, client or evaluator is started by this qualification.
"""
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import uuid

sys.dont_write_bytecode = True


def snapshot(root):
    values = {}
    for index, path in enumerate(root.rglob("*")):
        if index >= 4096:
            raise AssertionError("fixture tree is unexpectedly large")
        if path.is_symlink():
            values[str(path.relative_to(root))] = ("symlink",os.readlink(path))
        elif path.is_file():
            if path.stat().st_size > 16 * 1024**2:
                raise AssertionError("fixture file is unexpectedly large")
            values[str(path.relative_to(root))] = path.read_bytes()
    return values


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT")
                     and os.environ.get("ASB_TEST_GRADE_INSPECTION_SOURCES"),
                     "requires explicit ASB, project and completed original fixture sources")
class OriginalGradeInspection(unittest.TestCase):
    def test_public_inspection_checks_original_grades_without_mutating_evidence_or_sdks(self):
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        sources = json.loads(os.environ["ASB_TEST_GRADE_INSPECTION_SOURCES"])
        self.assertEqual(set(sources), {"swe-bench","swe-bench-lite","swe-bench-verified","terminal-bench-1.0","terminal-bench-2.1"})
        artifacts = Path(tempfile.mkdtemp(prefix="retained-grade-inspection-",dir=root))
        print(f"Retained real-grade inspection qualification: {artifacts}",flush=True)
        reports = {}
        for workload, definition in sources.items():
            with self.subTest(workload=workload):
                source = Path(definition["artifacts"]).absolute()
                self.assertTrue(source.resolve().is_relative_to(root.resolve()))
                self.assertIs(type(definition["resolved"]),bool)
                preparation = uuid.uuid4().hex
                destination = root / ".asb/attempts" / preparation
                descriptor = os.open(source/"resources.lock",os.O_RDWR|os.O_NOFOLLOW|os.O_NONBLOCK)
                try:
                    fcntl.flock(descriptor,fcntl.LOCK_EX|fcntl.LOCK_NB)
                    before = snapshot(source)
                    shutil.copytree(source,destination,symlinks=True)
                finally:
                    os.close(descriptor)
                task = json.loads((destination/"task.json").read_bytes())["identity"]
                self.assertEqual(task["workload"],workload)
                reference = {"schema_version":1,"preparation_id":preparation,"workload":workload,
                    "task_id":task["task_id"],"dataset":task["dataset"],"artifacts":str(destination)}
                path = artifacts / (workload+".reference.json")
                path.write_text(json.dumps(reference))
                path.chmod(0o600)
                tool = "swebench" if workload.startswith("swe-") else "terminal-bench" if workload.endswith("1.0") else "harbor"
                sdk = root / ".asb-tools" / tool
                helper_bytes = {item.name:item.read_bytes() for item in sdk.glob("*.py")}
                existing_scratch = set((root/".asb").glob("grade-inspection-*"))
                environment = {"PATH":os.environ["PATH"],"HOME":str(artifacts),"PYTHONDONTWRITEBYTECODE":"1"}
                result = subprocess.run([binary,"workload","recover",path,"--inspect-grade","--json"],
                    cwd=root,env=environment,capture_output=True,timeout=60)
                (artifacts/(workload+".stdout")).write_bytes(result.stdout)
                (artifacts/(workload+".stderr")).write_bytes(result.stderr)
                self.assertEqual(result.returncode,0,result.stderr.decode())
                report = json.loads(result.stdout)
                self.assertEqual(report["task"],task)
                self.assertEqual(report["workload_success"],definition["resolved"])
                self.assertEqual(report["scope"],"retained-official-grade-not-execution-recovery")
                for field in ["process_success","exit_code","elapsed_ms","token_usage","steps"]:
                    self.assertIsNone(report[field])
                self.assertEqual(snapshot(destination),before)
                self.assertEqual(snapshot(source),before)
                self.assertEqual({item.name:item.read_bytes() for item in sdk.glob("*.py")},helper_bytes)
                self.assertEqual(set((root/".asb").glob("grade-inspection-*")),existing_scratch)
                reports[workload] = report
        (artifacts/"qualification.json").write_text(json.dumps(reports,indent=2)+"\n")


if __name__ == "__main__":
    unittest.main()
