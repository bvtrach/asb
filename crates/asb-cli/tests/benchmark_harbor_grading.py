"""Provider-free retained original Harbor grading and immutable transport tests."""
import asyncio
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sys
import threading
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
from benchmark_retained import Fixture, retained, harbor
from benchmark_resources import client, Container, Image


class Paths:
    def __init__(self,root):
        self.root = root
        self.verifier_dir = root/"verifier"
        self.reward_text_path = self.verifier_dir/"reward.txt"
        self.reward_json_path = self.verifier_dir/"reward.json"
    def mkdir(self):self.verifier_dir.mkdir(parents=True)
    def chmod_dir(self):pass


class HarborGradingTests(unittest.TestCase):
    def test_retained_transport_subclass_uses_sdk_contract_without_start_or_compose_discovery(self):
        class Capabilities:
            def model_copy(self,update):return SimpleNamespace(**update)
        class Environment:
            @property
            def capabilities(self):return Capabilities()
        modules = {}
        for name,attributes in {
            "harbor.environments":{}, "harbor.environments.docker":{},
            "harbor.environments.base":{"ExecResult":SimpleNamespace},
            "harbor.environments.docker.docker":{"DockerEnvironment":Environment},
            "harbor.environments.docker.runtime":{"ContainerRuntime":SimpleNamespace},
        }.items():
            module = ModuleType(name)
            module.__dict__.update(attributes)
            modules[name] = module
        main = SimpleNamespace(id="a"*64,attrs={"Config":{"Labels":{"com.docker.compose.service":"main"}}})
        resumed = SimpleNamespace(main=main,containers=[main])
        # This local protocol fixture echoes argv; it is not a Docker or LLM call.
        command = [sys.executable,"-c","import json,sys; print(json.dumps(sys.argv[1:]))"]
        with patch.dict(sys.modules,modules):
            environment = harbor.retained_environment_class(command,resumed)()
            self.assertFalse(environment.capabilities.mounted)
            self.assertFalse(environment.capabilities.stream)
            async def check():
                result = await environment._run_docker_compose_command(["exec","main","true"])
                self.assertEqual(result.return_code,0)
                self.assertEqual(json.loads(result.stdout),["exec","-i","a"*64,"true"])
                with self.assertRaises(harbor.ExecutionError):await environment.start()
                with self.assertRaises(harbor.ExecutionError):await environment.stop()
                with self.assertRaises(harbor.ExecutionError):await environment._run_docker_compose_command(["up"])
            asyncio.run(check())
            noisy = [sys.executable,"-c","import sys; sys.stdout.write('x' * (2 * 1024**2))"]
            environment = harbor.retained_environment_class(noisy,resumed)()
            async def bounded():
                with self.assertRaisesRegex(harbor.ExecutionError,"exceeds 1 MiB"):
                    await environment._run_docker_compose_command(["exec","main","true"])
            asyncio.run(bounded())

    def fixture(self,resolved=False,completed=False):
        fixture = Fixture("terminal-bench-2.1",resolved)
        self.addCleanup(fixture.temporary.cleanup)
        fixture.preserve()
        identity = fixture.request["summary"]["task"]
        task = json.loads((fixture.root/"task.json").read_bytes())
        task["original_networks"] = {}
        task["baseline"]["definition_sha256"] = hashlib.sha256(b"[]").hexdigest()
        fixture.write("task.json",task)
        submission = json.loads((fixture.root/"submission.json").read_bytes())
        submission.update(container_id="c"*64,grading_state="captured-before-verifier")
        fixture.write("submission.json",submission)
        if not completed:
            (fixture.root/"evaluation.json").unlink()
            (fixture.root/"official/verifier/reward.txt").unlink()
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

    def grade(self,value,fake,catalog,resolved=False,failure=False,cancel=None):
        calls = []
        modules = {}
        for name in ["harbor","harbor.models","harbor.models.task","harbor.models.trial","harbor.verifier"]:
            module = ModuleType(name)
            module.__path__ = []
            modules[name] = module
        class Task:
            has_steps = False
            short_name = "fixture-task"
            def __init__(self,_path):
                self.config = SimpleNamespace(environment=SimpleNamespace(os="linux"),
                    verifier=SimpleNamespace(timeout_sec=5,user="verifier"))
        class Verifier:
            def __init__(self,task,paths,environment):
                self.paths,self.environment = paths,environment
            async def verify(self):
                calls.append("official-verifier")
                self.assert_running = fake.containers.values[value.identity["attempt_id"]].attrs["State"]["Running"]
                assert self.assert_running
                assert self.environment.default_user == "verifier"
                if failure:raise RuntimeError("private SDK fixture diagnostic")
                self.paths.reward_text_path.write_text("1" if resolved else "0")
                if cancel:cancel.set()
                return SimpleNamespace(rewards={"reward":int(resolved)})
        for name,attributes in {
            "harbor.models.task.task":{"Task":Task},
            "harbor.models.task.config":{"TaskOS":SimpleNamespace(LINUX="linux")},
            "harbor.models.trial.paths":{"TrialPaths":Paths},
            "harbor.verifier.verifier":{"Verifier":Verifier},
        }.items():
            module = ModuleType(name)
            module.__dict__.update(attributes)
            modules[name] = module
        def verified(path,workload):
            self.assertIsInstance(path,Path)
            self.assertEqual(workload,"terminal-bench-2.1")
            return catalog
        with patch.dict(sys.modules,modules), patch.object(harbor,"verify",side_effect=verified), \
             patch.object(harbor,"tree_manifest",return_value=[]), \
             patch("asb_terminal_catalog.runtime_identity",return_value={}), \
             patch.object(harbor,"DockerSession",return_value=contextlib.nullcontext(fake)), \
             patch.object(harbor,"docker_command",return_value=["docker-fixture"]), \
             patch.object(harbor,"retained_environment_class",return_value=lambda **_:SimpleNamespace()):
            harbor.grade_retained(value,"unused/catalog.json",cancel or threading.Event())
        return calls

    def test_false_and_true_grades_use_original_verifier_and_preserve_completed_outcomes(self):
        for resolved in [False,True]:
            with self.subTest(resolved=resolved):
                fixture,value,fake,main,catalog = self.fixture()
                original = (fixture.root/"submission.json").read_bytes()
                self.assertEqual(self.grade(value,fake,catalog,resolved=resolved),["official-verifier"])
                self.assertFalse(main.attrs["State"]["Running"])
                self.assertEqual((fixture.root/"submission.json").read_bytes(),original)
                self.assertFalse((fixture.root/"evaluation.json").exists())
                prefix = value.selected_evaluation()
                self.assertTrue(prefix.startswith("recovered-evaluation-"))
                self.assertTrue(value.object(prefix+"grading-cleanup.json"))
                self.assertEqual(value.inspected_grade()["workload_success"],resolved)
                before = {str(path.relative_to(fixture.root)):path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()}
                repeated = retained.RetainedOutcome(fixture.request,inspect=True)
                repeated.grading,repeated.owner_lock = True,value.owner_lock
                self.assertEqual(self.grade(repeated,fake,catalog,resolved=not resolved),[])
                self.assertEqual({str(path.relative_to(fixture.root)):path.read_bytes() for path in fixture.root.rglob("*") if path.is_file()},before)

    def test_legacy_completed_grades_never_restart_or_invoke_a_verifier(self):
        for resolved in [False,True]:
            fixture,value,fake,main,catalog = self.fixture(resolved,completed=True)
            self.assertEqual(self.grade(value,fake,catalog,resolved=not resolved),[])
            self.assertFalse(main.attrs["State"]["Running"])
            self.assertIsNone(value.selected_evaluation())
            self.assertEqual(value.inspected_grade()["workload_success"],resolved)

    def test_sdk_failure_preserves_state_and_cannot_repeat_side_effecting_tests(self):
        fixture,value,fake,main,catalog = self.fixture()
        with self.assertRaises(retained.RetainedError):
            self.grade(value,fake,catalog,failure=True)
        self.assertFalse(main.attrs["State"]["Running"])
        self.assertIsNone(value.selected_evaluation())
        directory = next(fixture.root.glob("recovered-evaluation-*"))
        self.assertTrue((directory/"evaluation-started.json").is_file())
        self.assertTrue((directory/"grading-cleanup.json").is_file())
        with self.assertRaisesRegex(retained.RetainedError,"already started"):
            self.grade(value,fake,catalog)

    def test_changed_catalog_capture_definition_and_started_verifier_refuse_before_restart(self):
        for mutation in ["dataset","sdk","definition","capture","started","raw-reward"]:
            fixture,value,fake,main,catalog = self.fixture()
            if mutation == "dataset":catalog["dataset"] = dict(catalog["dataset"],revision="x"*40)
            if mutation == "sdk":catalog["sdk"] = {"version":"different"}
            if mutation == "definition":value.task["baseline"]["definition_sha256"] = "a"*64
            if mutation == "capture":value.submission.pop("grading_state")
            if mutation == "started":fixture.write("evaluation-started.json",{})
            if mutation == "raw-reward":(fixture.root/"official/verifier/reward.txt").write_text("1")
            with self.subTest(mutation=mutation),self.assertRaises(retained.RetainedError):
                self.grade(value,fake,catalog)
            self.assertFalse(main.attrs["State"]["Running"])
            self.assertEqual(list(fixture.root.glob("recovered-evaluation-*")),[])

    def test_cancelled_first_grading_stops_resources_and_does_not_select_a_grade(self):
        fixture,value,fake,main,catalog = self.fixture()
        cancelled = threading.Event()
        with self.assertRaisesRegex(retained.RetainedError,"cancelled"):
            self.grade(value,fake,catalog,cancel=cancelled)
        self.assertFalse(main.attrs["State"]["Running"])
        self.assertIsNone(value.selected_evaluation())
        # The completed raw verifier outcome is preserved and can be validated
        # and selected later, without invoking its side effects a second time.
        self.assertEqual(self.grade(value,fake,catalog,resolved=True),[])
        self.assertFalse(value.inspected_grade()["workload_success"])

    def test_pre_cancelled_grading_does_not_start_resources_or_publish_an_evaluator(self):
        fixture,value,fake,main,catalog = self.fixture()
        cancelled = threading.Event()
        cancelled.set()
        with self.assertRaisesRegex(retained.RetainedError,"cancelled"):
            self.grade(value,fake,catalog,cancel=cancelled)
        self.assertEqual(main.starts,0)
        self.assertEqual(list(fixture.root.glob("recovered-evaluation-*")),[])

    def test_transport_uses_only_verified_immutable_ids_and_cannot_create_resources(self):
        owned = {"main":"a"*64,"database":"b"*64}
        self.assertEqual(harbor.retained_command(["exec","-T","-u","root","main","tar","-xf","-"],owned),
            ["exec","-i","-u","root","a"*64,"tar","-xf","-"])
        self.assertEqual(harbor.retained_command(["cp","/private/tests/.","main:/tests"],owned),
            ["cp","/private/tests/.","a"*64+":/tests"])
        self.assertEqual(harbor.retained_command(["cp","database:/logs/.","/private/grader"],owned),
            ["cp","b"*64+":/logs/.","/private/grader"])
        for args in [["up"],["down"],["build"],["ps"],["exec","foreign","sh"],
                     ["exec","--privileged","main","sh"],["cp","foreign:/logs","/private"],
                     ["cp","main:/logs","database:/logs"],["cp","relative","main:/logs"]]:
            with self.subTest(args=args),self.assertRaises(harbor.ExecutionError):
                harbor.retained_command(args,owned)


if __name__ == "__main__":
    unittest.main()
