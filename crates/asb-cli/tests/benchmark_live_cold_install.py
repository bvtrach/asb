"""Opt-in fresh-project SDK/catalog/client installation from public sources.

This uses empty project/home/cache directories, not installed ASB tool copies.
Host prerequisites (uv, Git, Rust, Docker and OS packages) are not removed or
reinstalled. No agent or live LLM provider is contacted by this qualification.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest


@unittest.skipUnless(os.environ.get("ASB_TEST_COLD_INSTALL_ROOT") and os.environ.get("ASB_TEST_BENCHMARK_ASB"),
                     "requires explicit public-source download/install qualification")
class ColdProjectQualification(unittest.TestCase):
    def test_five_original_populations_and_their_sdk_tools_install_into_a_fresh_project(self):
        parent = Path(os.environ["ASB_TEST_COLD_INSTALL_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        root = Path(tempfile.mkdtemp(prefix="cold-original-benchmarks-",dir=parent))
        print(f"Retained cold-project qualification: {root}",flush=True)
        environment = {"PATH":os.environ["PATH"],"HOME":str(root / "home"),
            "XDG_CACHE_HOME":str(root / "host-cache"),"XDG_CONFIG_HOME":str(root / "host-config"),
            "UV_CACHE_DIR":str(root / "uv-cache"),"HF_HOME":str(root / "hf-cache"),
            "NPM_CONFIG_CACHE":str(root / "npm-cache"),"TMPDIR":str(root / "tmp"),
            "PYTHONDONTWRITEBYTECODE":"1"}
        for key in ["HTTP_PROXY","HTTPS_PROXY","ALL_PROXY","NO_PROXY","http_proxy","https_proxy","all_proxy","no_proxy"]:
            if key in os.environ:
                environment[key] = os.environ[key]
        (root / "home").mkdir(mode=0o700)
        (root / "tmp").mkdir(mode=0o700)
        logs = root / "qualification"
        logs.mkdir(mode=0o700)
        steps = []
        def command(label,arguments,timeout=1200):
            started = time.monotonic()
            with (logs / (label + ".stdout")).open("xb") as output, (logs / (label + ".stderr")).open("xb") as error:
                result = subprocess.run([binary,*arguments],cwd=root,env=environment,
                    stdin=subprocess.DEVNULL,stdout=output,stderr=error,timeout=timeout)
            steps.append({"step":label,"exit_code":result.returncode,"elapsed_seconds":time.monotonic()-started})
            (logs / "steps.json").write_text(json.dumps(steps,indent=2))
            self.assertEqual(result.returncode,0,
                f"{label} failed; inspect retained logs under {logs}")
            print(f"{label}: completed in {steps[-1]['elapsed_seconds']:.1f}s",flush=True)

        # EOF declines optional host changes: never use init --yes here.
        command("init",["init"],timeout=600)
        self.assertTrue((root / ".asb/project.toml").is_file())
        self.assertEqual(list((root / ".asb-tools").iterdir()),[],"fresh project unexpectedly contains installed SDKs")
        populations = [("swe-bench",2294,"swebench"),("swe-bench-lite",300,"swebench"),
            ("swe-bench-verified",500,"swebench"),("terminal-bench-1.0",80,"terminal-bench"),
            ("terminal-bench-2.1",89,"harbor")]
        catalog_receipts = []
        for workload,count,sdk in populations:
            command("install-" + workload,["workload","install",workload])
            path = root / "workloads" / workload / "catalog.json"
            catalog = json.loads(path.read_bytes())
            self.assertEqual(catalog["workload"],workload)
            self.assertEqual(len(catalog["tasks"]),count)
            self.assertTrue(catalog["dataset"]["revision"])
            self.assertEqual(len({task["id"] for task in catalog["tasks"]}),count)
            self.assertTrue((root / ".asb-tools" / sdk / "venv/bin/python").is_file())
            self.assertTrue((root / "workloads" / (workload + ".workload.toml")).is_file())
            catalog_receipts.append({"workload":workload,"population":count,"dataset":catalog["dataset"]})
        command("install-pi",["tool","install","pi"])
        pi = root / ".asb-tools/pi/node_modules/.bin/pi"
        self.assertTrue(pi.is_file())
        version = subprocess.run([pi,"--version"],cwd=root,env=environment,
            stdin=subprocess.DEVNULL,capture_output=True,timeout=30)
        self.assertEqual(version.returncode,0,"installed Pi version check failed")
        (logs / "pi-version.stdout").write_bytes(version.stdout)
        (logs / "pi-version.stderr").write_bytes(version.stderr)
        (logs / "catalogs.json").write_text(json.dumps(catalog_receipts,indent=2))
        (logs / "qualification.json").write_text(json.dumps({"scope":"cold-project-public-source-install-not-a-benchmark-score",
            "root":str(root),"host_prerequisites_reused":True,"docker_images_not_tested":True,
            "llm_provider_contacted":False,"populations":catalog_receipts,"steps":steps},indent=2))


if __name__ == "__main__":
    unittest.main()
