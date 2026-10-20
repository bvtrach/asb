"""Opt-in Python/standalone client installation in an original task; no LLM calls."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


def minimal_installer_environment(task):
    # Scope PATH only for the installation commands. Do not remove or replace
    # anything in the original task's OS, repository or benchmark environment.
    task.checked(["/bin/sh","-c",'set -eu; mkdir -p /opt/asb-bootstrap/commands; '
        'for name in tar gzip sha256sum; do ln -s "$(command -v "$name")" "/opt/asb-bootstrap/commands/$name"; done'])
    environment = {"PATH":"/opt/asb-bootstrap/commands"}
    result = task.workspace.execute(["/bin/sh","-c","command -v curl"],environment=environment)
    if result["exit_code"] == 0:
        raise AssertionError("qualification installer unexpectedly has curl on PATH")
    return environment


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_TOOL_BOOTSTRAP_ROOT"),
                     "requires explicit public-tool installation and Docker qualification")
class ToolBootstrapQualification(unittest.TestCase):
    def test_standalone_client_installs_from_its_verified_staged_archive(self):
        from asb_docker import DockerSession
        from asb_swebench_task import SweBenchTask
        from asb_guest import prepare_agent
        root = Path(os.environ["ASB_TEST_TOOL_BOOTSTRAP_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        installed = subprocess.run([binary,"tool","install","goose"],cwd=root,capture_output=True,text=True,timeout=180)
        self.assertEqual(installed.returncode,0,installed.stderr)
        archive = root/".asb-tools/goose/cache/goose.tar.gz"
        self.assertTrue(archive.is_file(),"the installer discarded its verified source archive")
        artifacts = Path(tempfile.mkdtemp(prefix="standalone-bootstrap-",dir=root))/"task"
        print(f"Standalone bootstrap qualification: {artifacts}",flush=True)
        with DockerSession() as client:
            with SweBenchTask(client,root/"workloads/swe-bench-lite/catalog.json","astropy__astropy-12907",artifacts) as task:
                request = {"binary":str(binary),"project_root":str(root),"tools_root":str(root/".asb-tools"),
                    "bootstrap":{"runtime_tools":[],"assets":[{"source":str(archive),"target":"goose/cache/goose.tar.gz"}]},
                    "environment":minimal_installer_environment(task),
                    "request":{"config":{"agent":"goose"}}}
                prepare_agent(task,request,None)
                version = task.checked(["/opt/asb-project/.asb-tools/goose/bin/goose","--version"])
                self.assertIn(b"1.54.0",version)
                receipt = json.loads((artifacts/"bootstrap.json").read_text())
                self.assertEqual(receipt["runtime_tools"],[])
                self.assertEqual(len(receipt["assets"]),1)
                attempt = task.attempt
            self.assertEqual(client.containers.list(all=True,filters={"label":"asb.attempt="+attempt}),[])

    def test_actual_uv_and_python_client_install_in_original_swe_environment(self):
        from asb_docker import DockerSession
        from asb_swebench_task import SweBenchTask
        from asb_guest import prepare_agent
        root = Path(os.environ["ASB_TEST_TOOL_BOOTSTRAP_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        installed = subprocess.run([binary,"tool","install","uv"],cwd=root,capture_output=True,text=True,timeout=180)
        self.assertEqual(installed.returncode,0,installed.stderr)
        archives = list((root/".asb-tools/uv/cache").glob("uv-*-unknown-linux-musl.tar.gz"))
        self.assertEqual(len(archives),1)
        archive = archives[0]
        artifacts = Path(tempfile.mkdtemp(prefix="python-bootstrap-",dir=root))/"task"
        print(f"Python bootstrap qualification: {artifacts}",flush=True)
        with DockerSession() as client:
            with SweBenchTask(client,root/"workloads/swe-bench-lite/catalog.json","astropy__astropy-12907",artifacts) as task:
                request = {"binary":str(binary),"project_root":str(root),"tools_root":str(root/".asb-tools"),
                    "bootstrap":{"runtime_tools":["uv"],"assets":[{"source":str(archive),"target":"uv/cache/"+archive.name}]},
                    "environment":minimal_installer_environment(task),
                    "request":{"config":{"agent":"mini-swe"}}}
                prepare_agent(task,request,None)
                uv = task.checked(["/opt/asb-project/.asb-tools/uv/bin/uv","--version"])
                self.assertIn(b"uv 0.9.9",uv)
                version = task.checked(["/opt/asb-project/.asb/python/venv/bin/python","--version"])
                self.assertIn(b"Python 3.12.",version)
                help_text = task.checked(["/opt/asb-project/.asb-tools/mini-swe/venv/bin/mini","--help"])
                self.assertIn(b"--model",help_text)
                receipt = json.loads((artifacts/"bootstrap.json").read_text())
                self.assertEqual(receipt["runtime_tools"],["uv"])
                self.assertEqual(len(receipt["assets"]),1)
                self.assertTrue((artifacts/"install-mini-swe.stdout").is_file())
                attempt = task.attempt
            self.assertEqual(client.containers.list(all=True,filters={"label":"asb.attempt="+attempt}),[])


if __name__ == "__main__":
    unittest.main()
