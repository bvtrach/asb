"""Opt-in original Dockerfile build + official grading; no LLM provider.

The fixture forces only its own image lookup/pull to fail, leaving existing
published images untouched. It applies a known reference repair, not an AI
solution. Build CPU/memory/time limits are the production fallback limits.
"""
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_SWECATALOG_ROOT") and os.environ.get("ASB_TEST_SWE_SOURCE_ARCHIVE"),
                     "requires explicit official source-build/Docker qualification")
class OriginalSourceQualification(unittest.TestCase):
    def test_original_source_fallback_and_official_grading_preserve_shared_images(self):
        import docker.errors
        from asb_docker import DockerSession
        from asb_guest import upload
        import asb_swebench as benchmark
        import asb_swebench_sources as sources
        import asb_swebench_task as tasks
        root = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        archive = Path(os.environ["ASB_TEST_SWE_SOURCE_ARCHIVE"]).absolute()
        self.assertGreater(shutil.disk_usage(root).free, 20 * 1024**3)
        self.assertGreater(shutil.disk_usage("/").free, 20 * 1024**3)
        artifacts = Path(tempfile.mkdtemp(prefix="original-swe-source-", dir=root))
        print(f"Retained original source-build qualification: {artifacts}", flush=True)
        instance = "sympy__sympy-20590"
        catalog = root / "workloads/swe-bench-lite/catalog.json"
        _, row = benchmark.read_task(catalog, instance)
        with DockerSession() as client:
            try:
                original = client.images.get(row["image"]).id
            except docker.errors.ImageNotFound:
                original = None
            calls = []
            class Images:
                def get(self, name):
                    if name == row["image"]:
                        calls.append("get")
                        raise docker.errors.ImageNotFound("fixture-only unavailable image")
                    return client.images.get(name)
                def pull(self, name):
                    if name == row["image"]:
                        calls.append("pull")
                        raise docker.errors.ImageNotFound("fixture-only unavailable registry image")
                    return client.images.pull(name)
                def __getattr__(self, name):
                    return getattr(client.images, name)
            class Client:
                images = Images()
                def __getattr__(self, name):
                    return getattr(client, name)
            def build(*arguments, **options):
                return sources.build_image(*arguments, **options, archive=archive)
            with patch.object(tasks, "build_image", build):
                with tasks.SweBenchTask(Client(), catalog, instance, artifacts / "task") as task:
                    source = task.invocation["environment_source"]
                    self.assertEqual(source["status"], "built")
                    self.assertEqual(source["revision"], sources.SOURCE_REVISION)
                    self.assertEqual(calls, ["get", "pull"])
                    self.assertEqual([p.name for p in (artifacts / "task/source-build").iterdir()], ["Dockerfile"])
                    tag = source["tag"]
                    image = task.workspace.image_identity
                    upload(task.workspace.container, "/tmp", [("asb-reference.patch", row["patch"].encode(), 0o600)], temporary_dir=artifacts)
                    task.checked(["git", "apply", "/tmp/asb-reference.patch"])
                    result = task.evaluate(timeout=300)
                    self.assertTrue(result["resolved"], result)
                    print("swe-bench-lite original-source known repair: resolved", flush=True)
                    patch_path = artifacts / "known-repair.patch"
                    benchmark.atomic_write(patch_path, row["patch"].encode())
                    for suite in ["swe-bench", "swe-bench-verified"]:
                        result = benchmark.evaluate_submission(root / "workloads" / suite / "catalog.json", instance,
                            patch_path, artifacts / suite, timeout=300, image_identity=image)
                        self.assertTrue(result["resolved"], result)
                        print(f"{suite} original-source known repair: resolved", flush=True)
                    incorrect = artifacts / "incorrect.patch"
                    benchmark.atomic_write(incorrect, b"diff --git a/asb_wrong.txt b/asb_wrong.txt\nnew file mode 100644\n--- /dev/null\n+++ b/asb_wrong.txt\n@@ -0,0 +1 @@\n+deliberately incorrect\n")
                    result = benchmark.evaluate_submission(catalog, instance, incorrect, artifacts / "incorrect",
                        timeout=300, image_identity=image)
                    self.assertEqual(result["outcome"], "unresolved", result)
            with self.assertRaises(docker.errors.ImageNotFound):
                client.images.get(tag)
            if original is not None:
                self.assertEqual(client.images.get(row["image"]).id, original)
            cleanup = json.loads((artifacts / "task/source-image-cleanup.json").read_bytes())
            self.assertEqual(cleanup["status"], "removed")


if __name__ == "__main__":
    unittest.main()
