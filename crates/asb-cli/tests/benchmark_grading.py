"""Provider-free retained SWE submission grading and owner-loss contracts."""
import contextlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import MagicMock, patch

sys.dont_write_bytecode = True
from benchmark_retained import Fixture, retained, swe, SOURCE


class RetainedGradingTests(unittest.TestCase):
    def new(self, workload="swe-bench-lite", resolved=False, ungraded=True):
        fixture = Fixture(workload,resolved)
        self.addCleanup(fixture.temporary.cleanup)
        fixture.preserve()
        if ungraded:shutil.rmtree(fixture.root/"evaluation-fixture")
        else:(fixture.root/"evaluation-fixture").rename(fixture.root/("evaluation-"+"e"*32))
        value = retained.RetainedOutcome(fixture.request,inspect=True)
        value.grading = True
        return fixture,value

    def catalog(self, fixture):
        return ({"workload":fixture.request["reference"]["workload"],
                 "dataset":fixture.request["reference"]["dataset"]}, {"base_commit":"b"*40})

    def evaluator(self, fixture, resolved):
        oracle = Fixture(fixture.request["reference"]["workload"],resolved)
        self.addCleanup(oracle.temporary.cleanup)
        def evaluate(_catalog, _task, _patch, destination, _timeout, **_options):
            shutil.copytree(oracle.root/"evaluation-fixture",destination)
            return json.loads((destination/"evaluation.json").read_bytes())
        return evaluate

    def grade(self, fixture, value, resolved, cancelled=None):
        client = MagicMock()
        image = "sha256:"+"c"*64
        client.images.get.return_value.id = image
        with patch.object(swe,"sdk_identity",return_value=fixture.sdk), \
             patch.object(swe,"read_task",return_value=self.catalog(fixture)), \
             patch("asb_docker.DockerSession",return_value=contextlib.nullcontext(client)), \
             patch.object(swe,"evaluate_submission",side_effect=self.evaluator(fixture,resolved)) as evaluator:
            swe.grade_retained(value,"unused",cancelled or threading.Event())
        return evaluator

    def test_all_three_populations_grade_frozen_submissions_without_agent_execution(self):
        for workload in ["swe-bench","swe-bench-lite","swe-bench-verified"]:
            for resolved in [False,True]:
                with self.subTest(workload=workload,resolved=resolved):
                    fixture,value = self.new(workload,resolved)
                    failed = fixture.root/("evaluation-"+"d"*32)
                    failed.mkdir()
                    (failed/"evaluation.json").write_text(json.dumps({"outcome":"evaluation_timeout"}))
                    original = {path:path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()}
                    evaluator = self.grade(fixture,value,resolved)
                    evaluator.assert_called_once()
                    self.assertEqual({path:path.read_bytes() for path in original},original)
                    selected = json.loads((fixture.root/"selected-evaluation.json").read_bytes())
                    self.assertTrue(selected["directory"].startswith("recovered-evaluation-"))
                    with patch.object(swe,"sdk_identity",return_value=fixture.sdk):
                        report = fixture.inspect()
                    self.assertIs(report["workload_success"],resolved)
                    for field in ["process_success","elapsed_ms","token_usage","steps"]:
                        self.assertIsNone(report[field])
                    self.assertIn("selected-evaluation.json",report["evidence"])

    def test_completed_true_false_grades_are_never_rerun_or_replaced(self):
        for resolved in [False,True]:
            fixture,value = self.new(resolved=resolved,ungraded=False)
            before = {path:path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()}
            with patch.object(swe,"sdk_identity",return_value=fixture.sdk), \
                 patch.object(swe,"read_task",return_value=self.catalog(fixture)), \
                 patch.object(swe,"evaluate_submission") as evaluator, \
                 patch("asb_docker.DockerSession") as docker:
                swe.grade_retained(value,"unused",threading.Event())
                evaluator.assert_not_called()
                docker.assert_not_called()
            self.assertEqual({path:path.read_bytes() for path in before},before)

    def test_corrupt_completed_grade_cannot_authorize_another_grader(self):
        fixture,value = self.new(ungraded=False)
        path = fixture.root/("evaluation-"+"e"*32)/"report.json"
        path.write_text("{}")
        with patch.object(swe,"sdk_identity",return_value=fixture.sdk), \
             patch.object(swe,"read_task",return_value=self.catalog(fixture)), \
             patch.object(swe,"evaluate_submission") as evaluator:
            with self.assertRaises(retained.RetainedError):swe.grade_retained(value,"unused",threading.Event())
            evaluator.assert_not_called()
        self.assertFalse((fixture.root/"selected-evaluation.json").exists())

    def test_lost_wrapper_result_with_existing_official_grade_is_not_regraded(self):
        fixture,value = self.new(ungraded=False)
        path = fixture.root/("evaluation-"+"e"*32)/"evaluation.json"
        result = json.loads(path.read_text())
        result["outcome"] = "evaluation_error"
        path.write_text(json.dumps(result))
        with patch.object(swe,"sdk_identity",return_value=fixture.sdk), \
             patch.object(swe,"read_task",return_value=self.catalog(fixture)), \
             patch.object(swe,"evaluate_submission") as evaluator:
            with self.assertRaisesRegex(retained.RetainedError,"outcome reconstruction"):
                swe.grade_retained(value,"unused",threading.Event())
            evaluator.assert_not_called()
        self.assertFalse((fixture.root/"selected-evaluation.json").exists())

    def test_failed_recovered_grading_preserves_artifacts_without_selecting_a_grade(self):
        fixture,value = self.new()
        def failed(*args,**kwargs):
            directory = args[3]
            directory.mkdir()
            (directory/"evaluation.json").write_text('{"outcome":"evaluation_error","resolved":false}')
            return {"outcome":"evaluation_error","resolved":False}
        client = MagicMock()
        client.images.get.return_value.id = "sha256:"+"c"*64
        with patch.object(swe,"sdk_identity",return_value=fixture.sdk), \
             patch.object(swe,"read_task",return_value=self.catalog(fixture)), \
             patch("asb_docker.DockerSession",return_value=contextlib.nullcontext(client)), \
             patch.object(swe,"evaluate_submission",side_effect=failed):
            with self.assertRaises(retained.RetainedError):swe.grade_retained(value,"unused",threading.Event())
        self.assertFalse((fixture.root/"selected-evaluation.json").exists())
        self.assertEqual(len(list(fixture.root.glob("recovered-evaluation-*/evaluation.json"))),1)
        self.assertTrue((fixture.root/"candidate.patch").is_file())

    def test_active_old_grader_and_missing_ownership_journal_are_not_ignored(self):
        for journal in [False,True]:
            fixture,value = self.new()
            failed = fixture.root/("evaluation-"+"d"*32)
            failed.mkdir()
            if journal:(failed/"resources.json").write_text("{}")
            with patch.object(swe,"sdk_identity",return_value=fixture.sdk), \
                 patch.object(swe,"read_task",return_value=self.catalog(fixture)), \
                 patch("asb_docker.DockerSession",return_value=contextlib.nullcontext(MagicMock())), \
                 patch("asb_resources.quiesce",side_effect=RuntimeError("active owner")), \
                 patch.object(swe,"evaluate_submission") as evaluator:
                with self.assertRaises((retained.RetainedError,RuntimeError)):
                    swe.grade_retained(value,"unused",threading.Event())
                evaluator.assert_not_called()

    def test_cancelled_or_changed_catalog_patch_and_selection_never_start_evaluation(self):
        for mutation in ["cancelled","catalog","patch","selection","sdk"]:
            fixture,value = self.new()
            event = threading.Event()
            catalog = self.catalog(fixture)
            if mutation == "cancelled":event.set()
            if mutation == "catalog":catalog[0]["dataset"] = {"wrong":True}
            if mutation == "patch":(fixture.root/"candidate.patch").write_text("changed")
            if mutation == "selection":(fixture.root/"selected-evaluation.json").write_text("{}")
            if mutation == "sdk":fixture.sdk = {"wrong":True}
            with patch.object(swe,"sdk_identity",return_value=fixture.sdk), \
                 patch.object(swe,"read_task",return_value=catalog), \
                 patch.object(swe,"evaluate_submission") as evaluator, \
                 patch("asb_docker.DockerSession") as docker:
                with self.assertRaises(retained.RetainedError):swe.grade_retained(value,"unused",event)
                evaluator.assert_not_called()
                docker.assert_not_called()

    def test_helper_cancels_its_grading_callback_on_eof_or_sigterm(self):
        for termination in ["eof","term","complete"]:
            fixture,_value = self.new()
            with tempfile.TemporaryDirectory() as directory:
                module = Path(directory)/"asb_grading_fixture.py"
                module.write_text("from pathlib import Path\nfrom asb_retained import RetainedError\n"
                    "def grade_retained(value,catalog,cancelled):\n"
                    " Path(catalog).write_text('started')\n"
                    " if Path(catalog).name=='complete':return\n"
                    " assert cancelled.wait(5),'owner loss not observed'\n"
                    " Path(catalog).write_text('cancelled')\n"
                    " raise RetainedError('grading cancelled')\n"
                    "def inspect_retained(value):\n"
                    " value.select_grade({'resolved':False,'outcome':'unresolved'})\n")
                marker = Path(directory)/("complete" if termination=="complete" else "marker")
                environment = dict(os.environ,PYTHONPATH=directory+os.pathsep+str(SOURCE),PYTHONDONTWRITEBYTECODE="1")
                process = subprocess.Popen([sys.executable,str(SOURCE/"asb_retained.py"),"--module","asb_grading_fixture","--grade"],
                    env=environment,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                try:
                    process.stdin.write(json.dumps({"reference":fixture.request["reference"],"catalog":str(marker)}).encode()+b"\n")
                    process.stdin.flush()
                    deadline = time.monotonic()+5
                    while not marker.exists() and process.poll() is None and time.monotonic()<deadline:time.sleep(.01)
                    self.assertTrue(marker.exists())
                    if termination == "complete":
                        report = json.loads(process.stdout.readline())
                        self.assertIs(report["workload_success"],False)
                    if termination in ["eof","complete"]:
                        process.stdin.close()
                        process.stdin = None
                    else:process.terminate()
                    stdout,stderr = process.communicate(timeout=10)
                    if termination == "complete":
                        self.assertEqual(process.returncode,0,stderr.decode())
                        self.assertEqual(marker.read_text(),"started")
                    else:
                        self.assertEqual(process.returncode,1,stderr.decode())
                        self.assertEqual(marker.read_text(),"cancelled")
                        self.assertEqual(json.loads(stdout)["reason"],"grading cancelled")
                finally:
                    if process.poll() is None:process.kill()
                    process.wait(timeout=5)


if __name__ == "__main__":unittest.main()
