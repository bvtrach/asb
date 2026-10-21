"""Opt-in official harness qualification; no agent or live LLM provider.

Run with the installed official SDK's Python and ASB_TEST_SWECATALOG_ROOT set
to a project containing the three pinned workload catalogs. Docker access is
required. Test artifacts are intentionally retained under that project.
"""
import importlib.util
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0, str(SOURCE))
SPEC = importlib.util.spec_from_file_location("asb_swebench", SOURCE / "asb_swebench.py")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)
SUITES = ["swe-bench", "swe-bench-lite", "swe-bench-verified"]
INCORRECT_PATCH = """diff --git a/asb_incorrect_submission.txt b/asb_incorrect_submission.txt
new file mode 100644
--- /dev/null
+++ b/asb_incorrect_submission.txt
@@ -0,0 +1 @@
+ASB deliberately incorrect evaluation fixture.
"""


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT"), "needs explicit upstream Docker qualification")
class OfficialHarnessQualification(unittest.TestCase):
    def test_official_gold_and_incorrect_patch_for_each_original_swebench_population(self):
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        self.assertGreater(shutil.disk_usage(root).free, 20 * 1024**3, "reserve project disk capacity")
        self.assertGreater(shutil.disk_usage("/").free, 20 * 1024**3, "reserve Docker disk capacity")
        catalogs = {suite: root / "workloads" / suite / "catalog.json" for suite in SUITES}
        import json
        populations = {suite: json.loads(path.read_bytes()) for suite, path in catalogs.items()}
        common = set.intersection(*[{entry["id"] for entry in catalog["tasks"]} for catalog in populations.values()])
        self.assertTrue(common)
        instance = sorted(common)[0]
        artifacts = Path(tempfile.mkdtemp(prefix="official-swebench-", dir=root))
        print(f"Retained official evaluator qualification: {artifacts}", flush=True)
        for suite in SUITES:
            _, task = BENCHMARK.read_task(catalogs[suite], instance)
            for label, patch, expected in [("gold", task["patch"], True), ("incorrect", INCORRECT_PATCH, False)]:
                with self.subTest(suite=suite, submission=label):
                    patch_path = artifacts / f"{suite}-{label}.patch"
                    BENCHMARK.atomic_write(patch_path, patch.encode())
                    result = BENCHMARK.evaluate_submission(catalogs[suite], instance, patch_path,
                        artifacts / f"{suite}-{label}", timeout=300)
                    print(f"{suite}/{label}: {result['outcome']} ({result['reason']})", flush=True)
                    self.assertEqual(result["resolved"], expected, result)
                    self.assertEqual(result["outcome"], "resolved" if expected else "unresolved", result)
                    cleanup = json.loads((artifacts / f"{suite}-{label}" / "cleanup.json").read_bytes())
                    self.assertIn(cleanup["status"], ["absent", "removed"], cleanup)


if __name__ == "__main__":
    unittest.main()
