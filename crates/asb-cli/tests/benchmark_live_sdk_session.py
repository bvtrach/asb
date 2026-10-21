"""Explicit original SDK shell transport qualification; no LLM or grading."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
    "requires explicit frozen ASB, original SDK/catalog and owned Docker opt-in")
class SDKSessionQualification(unittest.TestCase):
    def test_foreground_child_preserves_sdk_parent_and_measures_child_not_bridge(self):
        from asb_docker import DockerSession
        from asb_guest import upload
        from asb_runtime import bundle
        from asb_terminal_bench_1 import TerminalBenchTask
        from asb_terminal_session import SDKSessionLauncher
        import asb_execution,asb_terminal_session
        sources = [Path(module.__file__) for module in [asb_execution,asb_terminal_session]]
        hashes = {path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
        project = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="sdk-session-transport-",dir=project))
        print(f"Retained SDK-session qualification: {artifacts}",flush=True)
        launcher,targets = None,[]
        try:
            with DockerSession() as client:
                with TerminalBenchTask(client,project/"workloads/terminal-bench-1.0/catalog.json",
                        "hello-world",artifacts/"task") as task:
                    success = False
                    try:
                        task.checked(["mkdir","-p","/opt/asb-runtime/bin","/opt/asb-runtime/lib"])
                        files,_ = bundle(binary,task.workspace.image_identity["architecture"])
                        upload(task.workspace.container,"/opt/asb-runtime",files,temporary_dir=artifacts)
                        session = task.terminal.create_session("asb-launch-fixture",as_configured_user=True)
                        def parent():
                            session.send_keys(["printf '%s|%s|%s|%s\\n' \"$$\" \"$(id -u)\" \"$PWD\" "
                                "\"$ASB_SDK_PARENT_FIXTURE\" > /tmp/asb-parent-observation","Enter"],
                                block=True,max_timeout_sec=10)
                            return task.checked(["cat","/tmp/asb-parent-observation"]).decode().strip().split("|")
                        session.send_keys(["export ASB_SDK_PARENT_FIXTURE=parent-marker","Enter"],block=True,max_timeout_sec=10)
                        before = parent()
                        launcher = SDKSessionLauncher(session,binary,artifacts,require_host_target=True)
                        def attach(target):
                            bridge = client.api.exec_inspect(target["exec_id"])
                            self.assertIs(bridge["Running"],True)
                            self.assertNotEqual(target["daemon_pid"],bridge["Pid"])
                            self.assertEqual(target["pid_scope"],"docker-daemon-host")
                            targets.append(dict(target,transport_daemon_pid=bridge["Pid"]))
                        with patch.object(session,"send_keys",wraps=session.send_keys) as keys:
                            result = task.workspace.execute(["/bin/sh","-c",
                                "printf '%s|%s|%s\\n' \"$(id -u)\" \"$PWD\" \"$ASB_SDK_PARENT_FIXTURE\"; "
                                "printf fixture-stderr >&2; exit 7"],
                                environment={"ASB_PROVIDER_FIXTURE":"fixture-sensitive-value"},
                                transport_factory=launcher,before_resume=attach,preparation_timeout=90)
                            self.assertEqual(result["exit_code"],7)
                            self.assertEqual(result["stdout"],("|".join(before[1:])+"\n").encode())
                            self.assertEqual(result["stderr"],b"fixture-stderr")
                            self.assertNotIn("fixture-sensitive-value",str(keys.call_args_list))
                        self.assertEqual(parent(),before,"nonzero client must return to the same SDK shell")
                        result = task.workspace.execute(["/bin/sh","-c","printf second-client"],
                            transport_factory=launcher,before_resume=attach,preparation_timeout=90)
                        self.assertEqual(result["exit_code"],0)
                        self.assertEqual(result["stdout"],b"second-client")
                        self.assertEqual(parent(),before)
                        self.assertTrue(session.is_session_alive())
                        self.assertEqual(hashes,{path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources})
                        self.assertEqual(binary_hash,hashlib.sha256(binary.read_bytes()).hexdigest())
                        success = True
                    finally:
                        if not success:task.interrupted = True
        finally:
            if launcher is not None:launcher.close()
        (artifacts/"qualification.json").write_text(json.dumps({"scope":"original-sdk-shell-fake-client-transport",
            "parent":before,"source_hashes":hashes,"binary_sha256":binary_hash,"targets":targets,
            "client_statuses":[7,0],"provider_called":False,"coding_agent_started":False,"grader_started":False},sort_keys=True)+"\n")


if __name__ == "__main__":unittest.main()
