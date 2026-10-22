"""Opt-in original-SDK sweep qualification with installed Pi and fixture inference.

Shares the original repeated-task/material/grade checks. Adds actual comparison
and completed-only resume; does not claim interrupted SDK-worker recovery.
"""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest

import benchmark_live_repeated_original as original


def inventory(roots):
    """Bounded no-follow inventory of owned result/verifier evidence."""
    result = {}
    total = 0
    def observe(path):
        nonlocal total
        metadata = path.lstat()
        if len(result) >= 4096:
            raise AssertionError("qualification inventory file budget exceeded")
        if stat.S_ISLNK(metadata.st_mode):
            value = ("symlink",os.readlink(path))
        elif stat.S_ISDIR(metadata.st_mode):
            value = ("directory",stat.S_IMODE(metadata.st_mode))
        elif stat.S_ISREG(metadata.st_mode):
            total += metadata.st_size
            if total > 64*1024*1024:
                raise AssertionError("qualification inventory byte budget exceeded")
            with path.open("rb") as source:
                data = source.read(metadata.st_size+1)
            if len(data) != metadata.st_size:
                raise AssertionError("qualification evidence changed during inventory")
            value = ("file",stat.S_IMODE(metadata.st_mode),hashlib.sha256(data).hexdigest())
        else:
            raise AssertionError("unexpected special file in owned qualification evidence")
        result[str(path)] = value
        return stat.S_ISDIR(metadata.st_mode)
    def failed(error):
        raise error
    for root in roots:
        root = Path(root)
        if not observe(root):
            continue
        for directory, directories, files in os.walk(root,followlinks=False,onerror=failed):
            for name in sorted(directories+files):
                observe(Path(directory)/name)
    return result


class RepeatedOriginalSweepQualification(original.RepeatedOriginalQualification):
    def execution_command(self,binary,config):
        manifest = config.parent/"sweep.toml"
        manifest.write_text("configs=['benchmark.toml']\n")
        return [binary,"sweep",manifest,"--repeat","2","--json"]

    def execution_rows(self,result):
        emitted = [json.loads(line) for line in result.stdout.splitlines()]
        plans = [row for row in emitted if row.get("type") == "execution-plan"]
        summaries = [row for row in emitted if isinstance(row.get("task"),dict)]
        self.assertEqual(len(plans),1,"require one actual emitted campaign plan")
        self.assertEqual(len(summaries),4,"do not synthesize or discard executions")
        return plans+sorted(summaries,key=lambda row:row["execution"]["slot"])

    def qualification_sources(self):
        return super().qualification_sources()+[Path(__file__)]

    def after_execution(self,binary,config,root,environment,rows,arrivals,fault):
        self.assertFalse(fault,"this qualification covers completed-only SDK reuse")
        manifest = config.parent/"sweep.toml"
        results = config.parent/"sweep-results"
        index = json.loads((results/"plan.json").read_bytes())
        self.assertEqual(len(index["entries"]),1)
        checkpoint = results/".checkpoint"
        self.assertEqual(len(list((checkpoint/"campaign-completed/000000").glob("*.json"))),4)
        original_keys = {(row["execution"]["campaign"],row["execution"]["slot"]) for row in rows[1:]}
        original_attempts = {row["task"]["attempt_id"] for row in rows[1:]}
        artifacts = {path for row in rows[1:] for path in row.get("workload_artifacts",[])}
        before = inventory([results]+sorted(artifacts))
        calls = len(arrivals)
        try:
            resumed = subprocess.run([binary,"sweep",manifest,"--resume","--json"],
                cwd=root,env=environment,capture_output=True,timeout=180)
        except subprocess.TimeoutExpired as error:
            (config.parent/"resume.stdout").write_bytes(error.stdout or b"")
            (config.parent/"resume.stderr").write_bytes(error.stderr or b"")
            raise
        (config.parent/"resume.stdout").write_bytes(resumed.stdout)
        (config.parent/"resume.stderr").write_bytes(resumed.stderr)
        self.assertEqual(resumed.returncode,0,resumed.stderr.decode())
        self.assertEqual(len(arrivals),calls,"completed resume must issue no inference requests")
        self.assertEqual(inventory([results]+sorted(artifacts)),before,
            "resume must not alter plans, starts, attempts, verifiers, seals or receipts")
        reused = [json.loads(line) for line in resumed.stdout.splitlines()]
        reused = [row for row in reused if isinstance(row.get("task"),dict)]
        self.assertEqual(len(reused),4)
        self.assertTrue(all(row.get("reused") is True for row in reused))
        self.assertEqual({row["task"]["attempt_id"] for row in reused},original_attempts)
        self.assertEqual({(row["execution"]["campaign"],row["execution"]["slot"]) for row in reused},original_keys)
        report = json.loads((config.parent/"results/sweep-comparison/comparison.json").read_bytes())
        self.assertEqual(len(report["benchmarks"]),4)
        self.assertEqual({(row["execution"]["campaign"],row["execution"]["slot"]) for row in report["benchmarks"]},original_keys)
        self.assertEqual({row["task"]["attempt_id"] for row in report["benchmarks"]},original_attempts)
        self.assertTrue(all(row["metrics"]["success_rate"] == 1.0 for row in report["benchmarks"]))
        (config.parent/"sweep-qualification.json").write_text(json.dumps(dict(
            scope="original-sdk-completed-sweep-reuse",executions=4,reused_executions=4,
            additional_inference_requests=len(arrivals)-calls,unchanged_evidence_entries=len(before),
            original_execution_keys=sorted(original_keys),live_llm_provider_called=False),sort_keys=True)+"\n")


class InventoryTests(unittest.TestCase):
    def test_parser_requires_actual_emitted_plan_and_all_four_summaries(self):
        from types import SimpleNamespace
        case = RepeatedOriginalSweepQualification("test_public_repeat_prepares_isolated_original_tasks_before_real_client_execution")
        plan = dict(type="execution-plan",path="fixture-only-plan")
        summaries = [dict(task=dict(attempt_id=str(slot)),execution=dict(slot=slot)) for slot in [4,1,3,2]]
        def output(rows):
            return SimpleNamespace(stdout=b"\n".join(json.dumps(row).encode() for row in rows))
        selected = case.execution_rows(output([plan]+summaries))
        self.assertEqual(selected[0],plan)
        self.assertEqual([row["execution"]["slot"] for row in selected[1:]],[1,2,3,4])
        for rows in [summaries,[plan]+summaries[:3],[plan,plan]+summaries,[plan]+summaries+summaries[:1]]:
            with self.assertRaises(AssertionError):
                case.execution_rows(output(rows))

    def test_roots_are_observed_including_files_empty_directories_and_missing_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            file = root/"file"
            file.write_bytes(b"root evidence")
            self.assertEqual(inventory([file])[str(file)][0],"file")
            empty = root/"empty"
            empty.mkdir(mode=0o700)
            before = inventory([empty])
            self.assertEqual(before[str(empty)],("directory",0o700))
            empty.chmod(0o755)
            self.assertNotEqual(before,inventory([empty]))
            empty.rmdir()
            with self.assertRaises(FileNotFoundError):
                inventory([empty])

    def test_traversal_errors_are_not_silently_ignored(self):
        from unittest.mock import patch
        def inaccessible(*args,**kwargs):
            kwargs["onerror"](PermissionError("owned fixture inaccessible"))
        with tempfile.TemporaryDirectory() as temporary:
            with patch.object(os,"walk",side_effect=inaccessible):
                with self.assertRaises(PermissionError):
                    inventory([temporary])

    def test_contents_modes_and_symlinks_are_observed_without_following(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/"result").write_bytes(b"first")
            (root/"result").chmod(0o644)
            (root/"link").symlink_to("/unread-private-target")
            first = inventory([root])
            self.assertEqual(first[str(root/"link")],("symlink","/unread-private-target"))
            (root/"result").write_bytes(b"other")
            self.assertNotEqual(first,inventory([root]))
            second = inventory([root])
            (root/"result").chmod(0o600)
            self.assertNotEqual(second,inventory([root]))

    def test_inventory_rejects_special_files_and_excess_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            os.mkfifo(root/"fifo")
            with self.assertRaisesRegex(AssertionError,"special file"):
                inventory([root])
            (root/"fifo").unlink()
            with (root/"large").open("wb") as output:
                output.truncate(64*1024*1024+1)
            with self.assertRaisesRegex(AssertionError,"byte budget"):
                inventory([root])


def load_tests(loader,tests,pattern):
    # Do not inherit the separate costly verifier-fault qualification implicitly.
    return unittest.TestSuite([loader.loadTestsFromTestCase(InventoryTests),
        RepeatedOriginalSweepQualification("test_public_repeat_prepares_isolated_original_tasks_before_real_client_execution")])


if __name__ == "__main__":
    unittest.main()
