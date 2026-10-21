"""Provider-free original Terminal-Bench retained-environment grading."""
import contextlib
import enum
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
from benchmark_retained import Fixture, retained, terminal
from benchmark_resources import client, Container, Image


class Failure(enum.Enum):
    NONE = "none"


class Paths:
    def __init__(self, root, task, attempt):
        self.root = root/task/attempt
        self.post_test_pane_path = self.root/"panes/post-test.txt"
        self.commands_path = self.root/"commands.jsonl"
    def mkdir(self):self.post_test_pane_path.parent.mkdir(parents=True)


class TerminalGradingTests(unittest.TestCase):
    def test_network_snapshot_and_restore_keep_aliases_and_only_explicit_static_addresses(self):
        container = SimpleNamespace(attrs={"NetworkSettings":{"Networks":{"original":{
            "Aliases":["client","service"],"IPAddress":"172.30.1.99",
            "IPAMConfig":{"IPv4Address":"172.30.1.12"}}}}},reload=lambda:None)
        saved = terminal.original_networks(container)
        self.assertEqual(saved,{"original":{"aliases":["client","service"],"ipv4_address":"172.30.1.12"}})
        actions = []
        class Network:
            def __init__(self,name):self.name = name
            def connect(self,owned,**options):actions.append(("connect",self.name,options))
            def disconnect(self,owned):actions.append(("disconnect",self.name))
        container.attrs["NetworkSettings"]["Networks"] = {"override":{}}
        fake = SimpleNamespace(networks=SimpleNamespace(get=lambda name:Network(name)))
        terminal.restore_task_networks(fake,container,saved)
        self.assertEqual(actions,[("connect","original",saved["original"]),("disconnect","override")])

    def fixture(self, resolved=False, completed=False):
        fixture = Fixture("terminal-bench-1.0",resolved)
        self.addCleanup(fixture.temporary.cleanup)
        fixture.preserve()
        identity = fixture.request["summary"]["task"]
        identity["dataset"]["revision"] = "b"*40
        task = json.loads((fixture.root/"task.json").read_bytes())
        task["identity"] = identity
        task["original_networks"] = {}
        task["baseline"] = {"kind":"upstream-environment-definition","source_revision":"b"*40,
            "definition_sha256":hashlib.sha256(b"[]").hexdigest()}
        fixture.write("task.json",task)
        submission = json.loads((fixture.root/"submission.json").read_bytes())
        submission.update(task=identity,container_id="c"*64,grading_state="captured-before-verifier")
        fixture.write("submission.json",submission)
        pane = fixture.root/"official/case-1"/identity["attempt_id"]/"panes/post-test.txt"
        if completed:
            result = json.loads((fixture.root/"evaluation.json").read_bytes())
            result["dataset"] = identity["dataset"]
            fixture.write("evaluation.json",result)
        else:
            (fixture.root/"evaluation.json").unlink()
            pane.unlink()
        for path in fixture.root.iterdir():
            if path.is_file():path.chmod(0o600)
        value = retained.RetainedOutcome(fixture.request,inspect=True)
        value.grading = True
        descriptor = os.open(fixture.root/"resources.lock",os.O_RDWR)
        fcntl.flock(descriptor,fcntl.LOCK_EX|fcntl.LOCK_NB)
        self.addCleanup(os.close,descriptor)
        value.owner_lock = descriptor
        fake = client()
        labels = {"asb.owner":"asb","asb.attempt":identity["attempt_id"],"asb.recovery":"d"*32}
        main = Container(identity["attempt_id"],labels)
        main.attrs["Image"] = "sha256:"+"c"*64
        main.attrs["State"]["Running"] = False
        fake.containers.values[main.name] = main
        image_name = "asb-workload:"+main.name
        image = Image(image_name,labels)
        image.id = "sha256:"+"c"*64
        fake.images.values[image_name] = image
        catalog = {"dataset":identity["dataset"],"sdk":fixture.sdk,
                   "tasks":[{"id":"case-1","definition":"definitions/case-1"}]}
        return fixture,value,fake,main,catalog

    def sdk(self, resolved=False, same_shell=False, failure=False):
        calls = []
        class Handler:
            def __init__(self, attempt, definition):
                self.trial_name,self.task_id = attempt,definition.name
                self.task = SimpleNamespace(run_tests_in_same_shell=same_shell,max_test_timeout_sec=5)
        class Harness:
            def _run_tests(self, term, session, handler):
                calls.append("official-tests")
                if failure:raise RuntimeError("SDK fixture failure")
                return Failure.NONE
            def _parse_results(self, handler, pane):
                calls.append("official-parser")
                return {"test":SimpleNamespace(value="passed" if resolved else "failed")},Failure.NONE
            def _is_resolved(self, parsed):return all(item.value == "passed" for item in parsed.values())
        class Terminal:
            def create_session(self,name,*,as_configured_user):
                assert name.startswith("asb-grade-") and as_configured_user is False
                return SimpleNamespace(capture_pane=lambda **_:"SDK verifier fixture pane")
        modules = {}
        for name in ["terminal_bench","terminal_bench.handlers","terminal_bench.harness","terminal_bench.terminal","terminal_bench.agents"]:
            module = ModuleType(name)
            module.__path__ = []
            modules[name] = module
        for name,attributes in {
            "terminal_bench.handlers.trial_handler":{"TrialHandler":Handler,"TrialPaths":Paths},
            "terminal_bench.harness.harness":{"Harness":Harness},
            "terminal_bench.terminal.terminal":{"Terminal":Terminal},
            "terminal_bench.terminal.docker_compose_manager":{"DockerComposeManager":type("Compose",(),{})},
            "terminal_bench.agents.failure_mode":{"FailureMode":Failure},
        }.items():
            module = ModuleType(name)
            module.__dict__.update(attributes)
            modules[name] = module
        return modules,calls

    def grade(self,value,fake,catalog,**options):
        import threading
        modules,calls = self.sdk(**options)
        def verified(path,_workload):
            self.assertIsInstance(path,Path)
            return catalog
        with patch.dict(sys.modules,modules), patch.object(terminal,"verify",side_effect=verified), \
             patch.object(terminal,"tree_manifest",return_value=[]), \
             patch("asb_terminal_catalog.runtime_identity",return_value={}), \
             patch.object(terminal,"DockerSession",return_value=contextlib.nullcontext(fake)):
            terminal.grade_retained(value,"unused/catalog.json",threading.Event())
        return calls

    def test_true_false_grades_use_preserved_resources_and_the_original_sdk_methods(self):
        for resolved in [False,True]:
            fixture,value,fake,main,catalog = self.fixture(resolved)
            submission = (fixture.root/"submission.json").read_bytes()
            calls = self.grade(value,fake,catalog,resolved=resolved)
            self.assertEqual(calls,["official-tests","official-parser"])
            self.assertEqual(main.starts,1)
            self.assertFalse(main.attrs["State"]["Running"])
            self.assertEqual((fixture.root/"submission.json").read_bytes(),submission)
            inspected = retained.RetainedOutcome(fixture.request,inspect=True)
            terminal.inspect_retained(inspected)
            report = inspected.inspected_grade()
            self.assertIs(report["workload_success"],resolved)
            self.assertIsNone(report["process_success"])
            self.assertIsNone(report["elapsed_ms"])
            self.assertIn("selected-evaluation.json",report["evidence"])
            before = {path:path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()}
            repeated = retained.RetainedOutcome(fixture.request,inspect=True)
            repeated.grading = True
            self.assertEqual(self.grade(repeated,fake,catalog,resolved=not resolved),[])
            self.assertEqual(before,{path:path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()})
            self.assertEqual(main.starts,1)

    def test_completed_legacy_true_false_grades_never_restart_the_environment(self):
        for resolved in [False,True]:
            fixture,value,fake,main,catalog = self.fixture(resolved,completed=True)
            before = {path:path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()}
            self.assertEqual(self.grade(value,fake,catalog,resolved=not resolved),[])
            self.assertEqual(main.starts,0)
            self.assertEqual(before,{path:path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()})

    def test_lost_original_shell_refuses_before_starting_or_writing_any_grader(self):
        fixture,value,fake,main,catalog = self.fixture()
        with self.assertRaisesRegex(retained.RetainedError,"lost agent shell"):
            self.grade(value,fake,catalog,same_shell=True)
        self.assertEqual(main.starts,0)
        self.assertFalse(list(fixture.root.glob("recovered-evaluation-*")))

    def test_sdk_failure_preserves_resources_and_does_not_repeat_side_effecting_tests(self):
        fixture,value,fake,main,catalog = self.fixture()
        with self.assertRaisesRegex(RuntimeError,"SDK fixture failure"):
            self.grade(value,fake,catalog,failure=True)
        self.assertFalse(main.attrs["State"]["Running"])
        self.assertFalse((fixture.root/"selected-evaluation.json").exists())
        repeated = retained.RetainedOutcome(fixture.request,inspect=True)
        repeated.grading = True
        with self.assertRaisesRegex(retained.RetainedError,"verifier already started"):
            self.grade(repeated,fake,catalog)
        self.assertEqual(main.starts,1)

    def test_changed_catalog_and_capture_metadata_refuse_before_restart(self):
        for mutation in ["catalog","baseline","capture","network"]:
            fixture,value,fake,main,catalog = self.fixture()
            if mutation == "catalog":catalog = dict(catalog,dataset={"wrong":True})
            if mutation == "baseline":value.task["baseline"]["definition_sha256"] = "wrong"
            if mutation == "capture":value.submission.pop("grading_state")
            if mutation == "network":value.task.pop("original_networks")
            with self.assertRaises(retained.RetainedError):self.grade(value,fake,catalog)
            self.assertEqual(main.starts,0)


if __name__ == "__main__":unittest.main()
