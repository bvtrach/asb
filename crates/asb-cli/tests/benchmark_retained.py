"""Provider-free retained-artifact checks for the five original benchmarks."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0,str(SOURCE))
import asb_retained as retained
import asb_swebench as swe
import asb_terminal_bench_1 as terminal
import asb_harbor as harbor


class Fixture:
    def __init__(self,workload,resolved):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.sdk = ({"revision":swe.HARNESS_REVISION,"version":"5.0.2"} if workload.startswith("swe-") else
                    {"package":"harbor","version":"0.24.0"} if workload.endswith("2.1") else
                    {"package":"terminal-bench","version":"0.2.18"})
        identity = {"workload":workload,"task_id":"case-1","attempt_id":"asb-"+"a"*32,
                    "dataset":{"source":"fixture/"+workload,"revision":"pinned","split":"test"}}
        baseline = {"kind":"upstream-task-definition","base_commit":"b"*40}
        image = {"image_id":"sha256:"+"c"*64}
        self.request = {"reference":{**identity,"artifacts":str(self.root)},
                        "summary":{"task":identity,"workload_baseline":baseline,
                                   "workload_success":resolved,"workload_evaluation":"upstream-harness"}}
        self.write("task.json",{"identity":identity,"baseline":baseline,"image":image,"sdk":self.sdk})
        submission = {"task":identity,"image":image}
        self.write("task-cleanup.json",{"attempt_id":identity["attempt_id"],"status":"removed"})
        self.write("resources.json",{"attempt":identity["attempt_id"]})
        (self.root / "resources.lock").touch()
        result = {"schema_version":1,**{key:identity[key] for key in ["workload","task_id","dataset"]},
                  "resolved":resolved,"outcome":"resolved" if resolved else "unresolved","sdk":self.sdk}
        if workload.startswith("swe-"):
            self.module = swe
            body = b"diff --git a/task b/task\nfixture\n"
            (self.root / "candidate.patch").write_bytes(body)
            digest = hashlib.sha256(body).hexdigest()
            submission.update(patch_sha256=digest,patch_bytes=len(body),base_commit="b"*40)
            evaluator_image = {**image,"declared_image":image["image_id"]}
            result.update(patch_sha256=digest,image=evaluator_image,harness_revision=swe.HARNESS_REVISION,official_report="report.json",official_summary="summary.json")
            self.write("evaluation-fixture/dataset.json",[{"instance_id":"case-1","base_commit":"b"*40,"image":image["image_id"]}])
            self.write("evaluation-fixture/evaluation.json",result)
            self.write("evaluation-fixture/submission.jsonl",{"instance_id":"case-1","model_patch":body.decode()})
            self.write("evaluation-fixture/report.json",{"case-1":{"resolved":resolved,"patch_successfully_applied":True}})
            self.write("evaluation-fixture/summary.json",{"total_instances":1,"submitted_ids":["case-1"],"completed_ids":["case-1"],"resolved_ids":["case-1"] if resolved else []})
            self.write("evaluation-fixture/sdk-identity.json",self.sdk)
            self.write("evaluation-fixture/image-identity.json",evaluator_image)
        elif workload.endswith("1.0"):
            self.module = terminal
            result.update(test_results={"test_case":"passed" if resolved else "failed"},failure_mode="none",parse_failure="none")
            self.write("evaluation.json",result)
            path = self.root / "official/case-1" / identity["attempt_id"] / "panes/post-test.txt"
            path.parent.mkdir(parents=True)
            path.write_text("retained verifier output")
        else:
            self.module = harbor
            result["rewards"]={"reward":1 if resolved else 0}
            self.write("evaluation.json",result)
            path=self.root/"official/verifier/reward.txt"
            path.parent.mkdir(parents=True)
            path.write_text("1" if resolved else "0")
        self.write("submission.json",submission)

    def write(self,path,value):
        path=self.root/path
        path.parent.mkdir(parents=True,exist_ok=True)
        path.write_text(json.dumps(value))

    def verify(self):
        with patch.object(swe,"sdk_identity",return_value=self.sdk):
            self.module.verify_retained(retained.RetainedOutcome(self.request))


class RetainedTests(unittest.TestCase):
    def test_all_five_original_adapter_contracts_preserve_true_and_false_grades(self):
        for workload in ["swe-bench","swe-bench-lite","swe-bench-verified","terminal-bench-1.0","terminal-bench-2.1"]:
            for resolved in [False,True]:
                with self.subTest(workload=workload,resolved=resolved):
                    fixture=Fixture(workload,resolved)
                    try:
                        before={str(path.relative_to(fixture.root)):path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()}
                        fixture.verify()
                        after={str(path.relative_to(fixture.root)):path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()}
                        self.assertEqual(before,after,"validation must not rewrite any retained artifact")
                        fixture.request["summary"]["workload_success"]=not resolved
                        with self.assertRaises(retained.RetainedError):
                            fixture.verify()
                    finally:
                        fixture.temporary.cleanup()

    def test_missing_grade_cleanup_wrong_identity_and_changed_patch_are_not_completion(self):
        fixture=Fixture("swe-bench",True)
        self.addCleanup(fixture.temporary.cleanup)
        fixture.request["summary"]["workload_success"]=None
        with self.assertRaises(retained.RetainedError):fixture.verify()
        fixture.request["summary"]["workload_success"]=True
        fixture.write("task-cleanup.json",{"attempt_id":"other","status":"removed"})
        with self.assertRaises(retained.RetainedError):fixture.verify()
        fixture.write("task-cleanup.json",{"attempt_id":fixture.request["summary"]["task"]["attempt_id"],"status":"removed"})
        (fixture.root/"candidate.patch").write_text("changed candidate")
        with self.assertRaises(retained.RetainedError):fixture.verify()

    def test_symlink_escape_is_rejected_and_reward_precedence_matches_the_sdk(self):
        fixture=Fixture("terminal-bench-2.1",True)
        self.addCleanup(fixture.temporary.cleanup)
        with self.assertRaises(retained.RetainedError):retained.RetainedOutcome(fixture.request).child("../outside")
        fixture.write("official/verifier/reward.json",{"reward":1})
        fixture.verify()
        (fixture.root/"official/verifier/reward.json").unlink()
        path=fixture.root/"official/verifier/reward.txt"
        path.unlink()
        path.symlink_to(fixture.root/"submission.json")
        with self.assertRaises(retained.RetainedError):fixture.verify()

    def test_empty_swebench_submission_can_only_retain_an_unresolved_grade(self):
        fixture=Fixture("swe-bench-lite",False)
        self.addCleanup(fixture.temporary.cleanup)
        (fixture.root/"candidate.patch").write_bytes(b"")
        submission=json.loads((fixture.root/"submission.json").read_bytes())
        submission.update(patch_sha256=hashlib.sha256(b"").hexdigest(),patch_bytes=0)
        fixture.write("submission.json",submission)
        result=json.loads((fixture.root/"evaluation-fixture/evaluation.json").read_bytes())
        result.update(patch_sha256=submission["patch_sha256"],reason="empty_submission")
        fixture.write("evaluation-fixture/evaluation.json",result)
        fixture.verify()
        fixture.request["summary"]["workload_success"]=True
        with self.assertRaises(retained.RetainedError):fixture.verify()

    def test_cli_is_bounded_read_only_and_refuses_an_active_owner(self):
        import fcntl
        fixture=Fixture("terminal-bench-1.0",True)
        self.addCleanup(fixture.temporary.cleanup)
        (fixture.root/"asb_fixture_retained.py").write_text("def verify_retained(value): value.grade(value.object('evaluation.json'))\n")
        command=[sys.executable,str(SOURCE/"asb_retained.py"),"--module","asb_fixture_retained"]
        environment={"PYTHONPATH":str(fixture.root),"PYTHONDONTWRITEBYTECODE":"1"}
        def invoke(body):
            return subprocess.run(command,input=body,capture_output=True,env=environment,timeout=5)
        request=(json.dumps(fixture.request)+"\n").encode()
        result=invoke(request)
        self.assertEqual(result.returncode,0,result.stderr.decode())
        self.assertEqual(json.loads(result.stdout)["scope"],"retained-artifact-consistency-not-historical-attestation")
        with (fixture.root/"resources.lock").open("r+") as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            self.assertNotEqual(invoke(request).returncode,0)
        self.assertNotEqual(invoke(b"{"+b" "*(1024*1024)+b"\n").returncode,0)

    def test_owned_build_images_require_their_own_cleanup_evidence(self):
        for workload in ["swe-bench","swe-bench-lite","swe-bench-verified","terminal-bench-1.0","terminal-bench-2.1"]:
            fixture = Fixture(workload,True)
            try:
                attempt = fixture.request["summary"]["task"]["attempt_id"]
                tag = "asb-workload:" + attempt
                fixture.write("resources.json",{"attempt":attempt,"resources":[{"kind":"images","name":tag}]})
                with self.assertRaises(FileNotFoundError):fixture.verify()
                cleanup = {"schema_version":1,"attempt":attempt,"scope":"owned-build-tags-only",
                    "resources":[{"kind":"images","name":tag,"status":"removed","image_id":"sha256:"+"c"*64}]}
                fixture.write("image-cleanup.json",cleanup)
                fixture.verify()
                for value in ["preserved","unknown"]:
                    cleanup["resources"][0]["status"] = value
                    fixture.write("image-cleanup.json",cleanup)
                    with self.assertRaises(retained.RetainedError):fixture.verify()
                cleanup["resources"][0]["status"] = "removed"
                cleanup["resources"][0]["name"] = "another-tag"
                fixture.write("image-cleanup.json",cleanup)
                with self.assertRaises(retained.RetainedError):fixture.verify()
            finally:
                fixture.temporary.cleanup()


if __name__ == "__main__":unittest.main()
