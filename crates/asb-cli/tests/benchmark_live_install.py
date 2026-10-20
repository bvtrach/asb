"""Opt-in public-dataset installer repair check; never invokes an LLM provider."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB"), "needs explicit public dataset qualification")
class OfficialInstallerQualification(unittest.TestCase):
    def test_changed_metadata_is_preserved_then_repaired_and_valid_subsets_survive(self):
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        directory = Path(tempfile.mkdtemp(prefix="official-installer-", dir=root))
        print(f"Retained installer qualification: {directory}", flush=True)
        def install():
            result = subprocess.run([str(binary), "workload", "install", "swe-bench-lite", "--dir", str(directory)],
                                    cwd=root, capture_output=True, text=True, timeout=180)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            return result.stdout
        self.assertIn("300 official test instances", install())
        catalog_path = directory / "swe-bench-lite/catalog.json"
        catalog = json.loads(catalog_path.read_bytes())
        self.assertEqual(len(catalog["tasks"]), 300)
        first = catalog["tasks"][0]
        metadata_path = catalog_path.parent / first["metadata"]
        original = metadata_path.read_bytes()
        self.assertEqual(hashlib.sha256(original).hexdigest(), first["metadata_sha256"])
        metadata_path.write_bytes(b"deliberately damaged installer fixture")
        self.assertIn("Preserved the incomplete or changed dataset", install())
        self.assertEqual(metadata_path.read_bytes(), original)
        backups = list(directory.glob("swe-bench-lite.asb-backup-*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / first["metadata"]).read_bytes(), b"deliberately damaged installer fixture")
        selection = directory / "swe-bench-lite.workload.toml"
        subset = f"# user-selected subset\nworkload='swe-bench-lite'\n[[trials]]\nid='{first['id']}'\n"
        selection.write_text(subset)
        self.assertNotIn("Preserved the incomplete", install())
        self.assertEqual(selection.read_text(), subset)


if __name__ == "__main__":
    unittest.main()
