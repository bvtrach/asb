"""SDK-return recovery for both original Terminal benchmarks, no live SDK/provider."""
import json
from pathlib import Path
import shutil
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
import benchmark_terminal_grading as original
import benchmark_harbor_grading as newer
from benchmark_retained import retained,terminal,harbor


class VerifierReconstructionTests(unittest.TestCase):
    def fixture(self,version,resolved=False,missing=True,root_source=False):
        creator = (original.TerminalGradingTests if version == "1.0" else newer.HarborGradingTests)()
        self.addCleanup(creator.doCleanups)
        fixture,value,fake,main,catalog = creator.fixture()
        creator.grade(value,fake,catalog,resolved=resolved)
        selected = value.selected_evaluation()
        source = fixture.root/selected
        archive = tempfile.TemporaryDirectory()
        self.addCleanup(archive.cleanup)
        (fixture.root/"selected-evaluation.json").rename(Path(archive.name)/"selection.json")
        if root_source:
            for path in source.iterdir():
                target = fixture.root/path.name
                if path.is_dir():shutil.copytree(path,target,dirs_exist_ok=True)
                else:shutil.copyfile(path,target)
            source.rename(Path(archive.name)/"original-grader")
            source = fixture.root
        (source/"evaluation.json").rename(Path(archive.name)/"wrapper.json")
        if not missing:(source/"evaluation.json").write_text('{"outcome":"evaluation_error"}')
        value = retained.RetainedOutcome(fixture.request,inspect=True)
        value.grading = True
        return fixture,value,main,source,terminal if version == "1.0" else harbor

    def reconstruct(self,value,module,cancelled=None):
        with patch.object(module,"DockerSession",side_effect=AssertionError("reconstruction started Docker")), \
             patch.object(module,"verify",side_effect=AssertionError("reconstruction prepared a new task")):
            module.grade_retained(value,"unused/catalog.json",cancelled or threading.Event())

    def test_all_original_sdk_returns_reconstruct_without_repeating_verifiers_or_starting_resources(self):
        for version in ["1.0","2.1"]:
            for resolved in [False,True]:
                for missing in [False,True]:
                    for root in [False,True]:
                        with self.subTest(version=version,resolved=resolved,missing=missing,root=root):
                            fixture,value,main,source,module = self.fixture(version,resolved,missing,root)
                            before = {path:path.read_bytes() for path in source.rglob("*") if path.is_file()}
                            starts = main.starts
                            self.reconstruct(value,module)
                            self.assertEqual(main.starts,starts)
                            self.assertFalse(main.attrs["State"]["Running"])
                            self.assertEqual(before,{path:path.read_bytes() for path in before})
                            self.assertEqual(value.inspected_grade()["workload_success"],resolved)
                            inspected = retained.RetainedOutcome(fixture.request,inspect=True)
                            module.inspect_retained(inspected)
                            self.assertEqual(inspected.inspected_grade()["workload_success"],resolved)
                            self.assertIsNone(inspected.inspected_grade()["elapsed_ms"])
                            repeated = retained.RetainedOutcome(fixture.request,inspect=True)
                            repeated.grading = True
                            self.reconstruct(repeated,module)
                            self.assertEqual(repeated.inspected_grade(),inspected.inspected_grade())

    def test_modified_raw_files_and_sdk_return_facts_cannot_mint_a_grade(self):
        for version in ["1.0","2.1"]:
            for mutation in ["raw","identity","facts","marker"]:
                fixture,value,main,source,module = self.fixture(version)
                observation = json.loads((source/"verifier-return.json").read_text())
                if mutation == "raw":
                    relative = next(name for name,fact in observation["files"].items()
                                    if fact is not None and name != "evaluation-started.json")
                    (source/relative).write_text("changed SDK output")
                if mutation == "identity":observation["task"]["task_id"] = "another-task"
                if mutation == "facts":observation["result"]["resolved"] = True
                if mutation == "marker":(source/"evaluation-started.json").write_text("{}")
                if mutation in ["identity","facts"]:(source/"verifier-return.json").write_text(json.dumps(observation))
                with self.subTest(version=version,mutation=mutation),self.assertRaises(retained.RetainedError):
                    self.reconstruct(value,module)
                self.assertFalse((fixture.root/"selected-evaluation.json").exists())
                self.assertEqual(main.starts,1)

    def test_changed_originals_after_reconstruction_and_missing_cleanup_are_rejected(self):
        for version in ["1.0","2.1"]:
            fixture,value,_main,source,module = self.fixture(version)
            self.reconstruct(value,module)
            (source/"verifier-return.json").write_text("{}")
            inspected = retained.RetainedOutcome(fixture.request,inspect=True)
            with self.subTest(version=version),self.assertRaises(retained.RetainedError):module.inspect_retained(inspected)
            fixture,value,main,source,module = self.fixture(version)
            (source/"grading-cleanup.json").unlink()
            with self.assertRaises(FileNotFoundError):self.reconstruct(value,module)
            self.assertEqual(main.starts,1)

    def test_missing_sdk_return_remains_unknown_and_cancellation_cannot_publish_a_grade(self):
        for version in ["1.0","2.1"]:
            fixture,value,main,source,module = self.fixture(version)
            (source/"verifier-return.json").unlink()
            with self.assertRaisesRegex(retained.RetainedError,"already started"):
                self.reconstruct(value,module)
            self.assertEqual(main.starts,1)
            fixture,value,main,_source,module = self.fixture(version)
            cancelled = threading.Event()
            cancelled.set()
            with self.assertRaisesRegex(retained.RetainedError,"cancelled"):
                self.reconstruct(value,module,cancelled)
            self.assertFalse((fixture.root/"selected-evaluation.json").exists())
            self.assertEqual(main.starts,1)

    def test_interrupted_metadata_publication_and_missing_selection_never_repeat_tests(self):
        for version in ["1.0","2.1"]:
            fixture,value,main,_source,module = self.fixture(version)
            with patch.object(retained,"publish_once",side_effect=OSError("fixture write failure")):
                with self.assertRaises(OSError):self.reconstruct(value,module)
            self.assertEqual(list(fixture.root.glob("recovered-evaluation-*/evaluation.json")),[])
            self.assertFalse((fixture.root/"selected-evaluation.json").exists())
            repeated = retained.RetainedOutcome(fixture.request,inspect=True)
            repeated.grading = True
            self.reconstruct(repeated,module)
            (fixture.root/"selected-evaluation.json").unlink()
            repeated = retained.RetainedOutcome(fixture.request,inspect=True)
            repeated.grading = True
            self.reconstruct(repeated,module)
            self.assertEqual(main.starts,1)
            self.assertFalse(repeated.inspected_grade()["workload_success"])


if __name__ == "__main__":unittest.main()
