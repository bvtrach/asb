"""Opt-in original Harbor environment/verifier qualification, no LLM calls."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_HARBOR_ROOT"), "needs explicit original dataset/SDK and Docker qualification")
class HarborQualification(unittest.TestCase):
    def test_actual_environment_and_original_verifier_grade_reference_and_incorrect_changes(self):
        from asb_docker import DockerSession
        from asb_guest import upload
        import asb_harbor
        from asb_harbor import HarborTask, ImageUnavailable
        root = Path(os.environ["ASB_TEST_HARBOR_ROOT"]).absolute()
        force_fallback = os.environ.get("ASB_TEST_HARBOR_FORCE_FALLBACK") == "1"
        original_class = asb_harbor.environment_class
        def faulted_class(*arguments):
            owned = original_class(*arguments)
            class MissingImage(owned):
                async def _run_docker_compose_command(self, argv, *args, **kwargs):
                    if argv[0] == "up" and self._use_prebuilt:
                        raise ImageUnavailable("qualification-only missing prebuilt image")
                    return await super()._run_docker_compose_command(argv,*args,**kwargs)
            return MissingImage if force_fallback else owned
        for resolved in [False, True]:
            with self.subTest(resolved=resolved), DockerSession() as client, patch.object(asb_harbor,"environment_class",faulted_class):
                artifacts = Path(tempfile.mkdtemp(prefix="harbor-qualification-", dir=root)) / "task"
                print(f"Harbor SDK qualification artifacts: {artifacts}; reference={resolved} (not an AI solution)", flush=True)
                with HarborTask(client, root / "workloads/terminal-bench-2.1/catalog.json", "cancel-async-tasks", artifacts) as task:
                    self.assertEqual(task.workspace.container.exec_run(["test", "!", "-e", "/tests/test.sh"]).exit_code, 0)
                    for path in [task.environment._mounts_compose_path, task.environment._resources_compose_path,
                                 task.environment._env_compose_path]:
                        if path is not None:
                            self.assertTrue(path.is_relative_to(task.artifacts / "sdk-tmp"),path)
                    if resolved:
                        reference = (task.definition / "solution/solve.sh").read_bytes()
                        upload(task.workspace.container, "/tmp", [("qualification-reference.sh",reference,0o700)])
                        task.checked(["bash","/tmp/qualification-reference.sh"])
                    else:
                        task.checked(["/bin/sh", "-c", "printf 'unrelated change\\n' > fixture.txt"])
                    result = task.evaluate()
                    if force_fallback:
                        self.assertEqual(task.invocation["environment_source"],"original-source-build-fallback")
                    self.assertEqual(result["outcome"], "resolved" if resolved else "unresolved", result)
                    self.assertEqual(result["rewards"], {"reward":1 if resolved else 0})
                    attempt = task.attempt
                self.assertEqual(client.containers.list(all=True,filters={"label":"asb.attempt="+attempt}), [])
                self.assertEqual(json.loads((artifacts / "task-cleanup.json").read_text())["status"], "removed")


if __name__ == "__main__":
    unittest.main()
