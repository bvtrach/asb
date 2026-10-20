"""Opt-in official Terminal-Bench catalog repair checks; no agents/providers."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB"), "requires explicit public catalog qualification")
class TerminalInstallerQualification(unittest.TestCase):
    def test_original_populations_repair_changed_source_and_preserve_selection(self):
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        directory = Path(tempfile.mkdtemp(prefix="terminal-installer-", dir=root))
        print(f"Retained installer qualification: {directory}", flush=True)
        for workload, count in [("terminal-bench-1.0", 80), ("terminal-bench-2.1", 89)]:
            with self.subTest(workload=workload):
                def install():
                    result = subprocess.run([binary, "workload", "install", workload, "--dir", directory],
                                            cwd=root, capture_output=True, text=True, timeout=180)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn(f"{count} official tasks", result.stdout)
                install()
                catalog = json.loads((directory / workload / "catalog.json").read_bytes())
                self.assertEqual(len(catalog["tasks"]), count)
                first = catalog["tasks"][0]
                definition = directory / workload / first["definition"]
                path = definition / ("task.yaml" if workload.endswith("1.0") else "task.toml")
                relative = path.relative_to(directory / workload)
                original = path.read_bytes()
                path.write_bytes(b"changed source qualification fixture")
                install()
                self.assertEqual(path.read_bytes(), original)
                backups = list(directory.glob(workload + ".asb-backup-*"))
                self.assertEqual(len(backups), 1)
                self.assertEqual((backups[0] / relative).read_bytes(), b"changed source qualification fixture")
                selection = directory / (workload + ".workload.toml")
                selected = f"# Keep user selection\nworkload='{workload}'\n[[trials]]\nid='{first['id']}'\n"
                selection.write_text(selected)
                install()
                self.assertEqual(selection.read_text(), selected)


if __name__ == "__main__":
    unittest.main()
