"""Provider-free source/catalog contracts using tiny fake official SDK modules."""
import io
import hashlib
import json
from pathlib import Path
import sys
import tarfile
import tempfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
import asb_terminal_catalog as subject


class CatalogTests(unittest.TestCase):
    def test_runtime_probe_checks_sdk_and_transport_dependencies_without_a_daemon(self):
        for workload,spec in subject.SPECS.items():
            with patch.dict(sys.modules,{"docker":ModuleType("docker")}), patch.object(subject.importlib.metadata,"version",
                    side_effect=lambda package: "7.1.0" if package == "docker" else spec["version"]):
                self.assertEqual(subject.runtime_identity(workload)["docker_version"],"7.1.0")
            with patch.object(subject.importlib.metadata,"version",return_value="wrong"):
                with self.assertRaises(subject.CatalogError):
                    subject.runtime_identity(workload)
            with patch.object(subject.importlib.metadata,"version",side_effect=lambda package: "wrong" if package == "docker" else spec["version"]):
                with self.assertRaisesRegex(subject.CatalogError,"Docker Python SDK"):
                    subject.runtime_identity(workload)

    def fixture(self, root, workload):
        source = root / "repository"
        if workload == "terminal-bench-1.0":
            ids = subject.read_json(Path(subject.__file__).with_name("terminal_bench_1_tasks.json"))
        else:
            ids = [f"task-{index}" for index in range(89)]
        for name in ids:
            path = source / "tasks" / name
            path.mkdir(parents=True)
            (path / "task.yaml").write_text("instruction: public task\n")
            (path / "task.toml").write_text("version='1.0'\n")
            (path / "instruction.md").write_text("public task")
            (path / "solution.sh").write_text("private-solution-sentinel")
            (path / "tests.py").write_text("private-verifier-sentinel")
        archive = root / "source.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            tar.add(source, arcname="upstream")
        previous = subject.SPECS[workload]["source_sha256"]
        subject.SPECS[workload]["source_sha256"] = hashlib.sha256(json.dumps(subject.tree_manifest(source), sort_keys=True).encode()).hexdigest()
        self.addCleanup(lambda: subject.SPECS[workload].update(source_sha256=previous))
        return source, archive, ids

    def sdk(self, workload):
        modules = {}
        if workload == "terminal-bench-1.0":
            leaf = "terminal_bench.handlers.trial_handler"
            task = SimpleNamespace(from_yaml=lambda _: SimpleNamespace(instruction="public task", difficulty="easy"))
        else:
            leaf = "harbor.models.task.task"
            task = lambda path: SimpleNamespace(instruction=(path / "instruction.md").read_text(), config=SimpleNamespace(metadata={"difficulty":"easy"}))
        for index in range(1, len(leaf.split(".")) + 1):
            name = ".".join(leaf.split(".")[:index])
            modules[name] = ModuleType(name)
        modules[leaf].Task = task
        return patch.dict(sys.modules, modules)

    def test_complete_populations_use_separate_sdks_and_hide_private_content(self):
        for workload, spec in subject.SPECS.items():
            with self.subTest(workload=workload), tempfile.TemporaryDirectory() as directory, self.sdk(workload), \
                    patch.object(subject.importlib.metadata, "version", return_value=spec["version"]):
                root = Path(directory)
                _, archive, ids = self.fixture(root, workload)
                destination = root / "installed"
                result = subject.install(workload, archive, destination)
                self.assertEqual(len(result["tasks"]), spec["count"])
                self.assertEqual({task["id"] for task in result["tasks"]}, set(ids))
                public = json.dumps(result)
                self.assertNotIn("private-solution-sentinel", public)
                self.assertNotIn("private-verifier-sentinel", public)
                self.assertEqual(subject.verify(destination / "catalog.json", workload), result)
                first = destination / "source/tasks" / ids[0] / "tests.py"
                first.write_text("damaged retained evidence")
                with self.assertRaises(subject.CatalogError):
                    subject.verify(destination / "catalog.json", workload)
                subject.install(workload, archive, destination)
                backups = list(root.glob("installed.asb-backup-*"))
                self.assertEqual(len(backups), 1)
                self.assertEqual((backups[0] / "source/tasks" / ids[0] / "tests.py").read_text(), "damaged retained evidence")

    def test_sdk_failure_preserves_previous_dataset_bytes(self):
        workload = "terminal-bench-1.0"
        with tempfile.TemporaryDirectory() as directory, self.sdk(workload):
            root = Path(directory)
            _, archive, _ = self.fixture(root, workload)
            with patch.object(subject.importlib.metadata, "version", return_value="0.2.18"):
                subject.install(workload, archive, root / "installed")
            before = (root / "installed/catalog.json").read_bytes()
            with patch.object(subject.importlib.metadata, "version", return_value="unsupported"):
                with self.assertRaises(subject.CatalogError):
                    subject.install(workload, archive, root / "installed")
            self.assertEqual((root / "installed/catalog.json").read_bytes(), before)

    def test_extraction_rejects_traversal_links_and_duplicate_paths(self):
        for kind in ["traversal", "link", "duplicate"]:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                archive = root / "unsafe.tar"
                with tarfile.open(archive, "w") as tar:
                    entry = tarfile.TarInfo("upstream/../../outside" if kind == "traversal" else "upstream/file")
                    if kind == "link":
                        entry.type, entry.linkname = tarfile.SYMTYPE, "/etc/passwd"
                    tar.addfile(entry, io.BytesIO(b""))
                    if kind == "duplicate":
                        tar.addfile(entry, io.BytesIO(b""))
                with self.assertRaises(subject.CatalogError):
                    subject.extract(archive, root / "destination")
                self.assertFalse((root / "outside").exists())

    def test_oversized_member_is_rejected_before_reading_its_body(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            member = tarfile.TarInfo("upstream/large")
            member.size = subject.MAX_FILE + 1
            archive = root / "oversized.tar"
            archive.write_bytes(member.tobuf() + b"\0" * 1024)
            with self.assertRaises(subject.CatalogError):
                subject.extract(archive, root / "destination")


if __name__ == "__main__":
    unittest.main()
