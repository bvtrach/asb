"""Opt-in declared-root census in an original owned task, without an LLM.

Requires ASB_TEST_SWECATALOG_ROOT, ASB_TEST_BENCHMARK_ASB and the installed
SWE-bench SDK interpreter. No agent, evaluator, image pruning or host mounts.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT") and os.environ.get("ASB_TEST_BENCHMARK_ASB"),
                     "requires explicit Docker, current ASB and installed original fixture")
class GuestCensusQualification(unittest.TestCase):
    def test_guest_census_preserves_cross_root_aliases_and_reads_original_repository(self):
        from asb_docker import DockerSession,docker_command
        from asb_guest import upload
        from asb_runtime import bundle
        from asb_swebench_task import SweBenchTask
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="guest-material-census-",dir=root))
        print(f"Retained guest census qualification: {artifacts}",flush=True)
        with DockerSession() as client:
            with SweBenchTask(client,root/"workloads/swe-bench-lite/catalog.json",
                              "astropy__astropy-12907",artifacts/"task") as task:
                # Refuse pre-existing input material; stage only this owned
                # fixture's private helper, never replace a task's runtime.
                task.checked(["test","!","-e","/opt/asb-runtime"])
                task.checked(["mkdir","-p","/opt/asb-runtime/bin","/opt/asb-runtime/lib",
                              "/tmp/asb-census-left","/tmp/asb-census-right"])
                files,receipt = bundle(os.environ["ASB_TEST_BENCHMARK_ASB"],
                                      task.workspace.image_identity["architecture"])
                upload(task.workspace.container,"/opt/asb-runtime",files,temporary_dir=artifacts)
                upload(task.workspace.container,"/tmp/asb-census-left",[("a",b"same bytes",0o644)],temporary_dir=artifacts)
                task.checked(["ln","/tmp/asb-census-left/a","/tmp/asb-census-right/b"])
                def observe(roots):
                    result = subprocess.run([*docker_command(),"exec","-i",task.workspace.container.id,
                        "/opt/asb-runtime/asb","__asb-material"],
                        input=json.dumps({"schema_version":1,"roots":roots}).encode(),
                        capture_output=True,timeout=90)
                    self.assertEqual(result.returncode,0,result.stderr.decode(errors="replace"))
                    self.assertLess(len(result.stdout),4096)
                    value = json.loads(result.stdout)
                    self.assertEqual(value["scope"],"declared-root-material-observation-v2")
                    self.assertTrue(value["coverage"]["visible_extended_attributes"])
                    self.assertFalse(value["coverage"]["complete_namespace"])
                    self.assertFalse(value["coverage"]["extended_attributes"])
                    return value
                roots = [{"name":"left","path":"/tmp/asb-census-left"},
                         {"name":"right","path":"/tmp/asb-census-right"}]
                linked = observe(roots)
                self.assertEqual(linked,observe(list(reversed(roots))))
                task.checked(["rm","--","/tmp/asb-census-right/b"])
                upload(task.workspace.container,"/tmp/asb-census-right",[("b",b"same bytes",0o644)],temporary_dir=artifacts)
                copied = observe(roots)
                self.assertNotEqual(linked["sha256"],copied["sha256"])
                task.checked(["python","-c","import os; os.setxattr('/tmp/asb-census-left/a', 'user.asb', b'fixture-secret\\0binary\\xff')"])
                attributed = observe(roots)
                self.assertNotEqual(copied["sha256"],attributed["sha256"])
                task.checked(["python","-c","import os; os.removexattr('/tmp/asb-census-left/a', 'user.asb')"])
                self.assertEqual(copied,observe(roots))
                repository = observe([{"name":"workspace","path":"/testbed"}])
                self.assertGreater(repository["entries"],1000)
                self.assertEqual(repository,observe([{"name":"workspace","path":"/testbed"}]))
            self.assertEqual(json.loads((artifacts/"task/task-cleanup.json").read_bytes())["status"],"removed")
        (artifacts/"qualification.json").write_text(json.dumps({
            "scope":"declared-roots-not-complete-environment","runtime":receipt,
            "linked":linked,"copied":copied,"attributed":attributed,"repository":repository,
            "agent_started":False,"provider_called":False,"evaluator_started":False,
        },sort_keys=True)+"\n")


if __name__ == "__main__":
    unittest.main()
