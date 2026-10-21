"""Original SDK shell + real guest driver with fixture clients; no LLM/grading."""
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
                        # Exercise the REAL registered-adapter driver, not just
                        # an arbitrary shell child. Its logical benchmark root
                        # differs deliberately from the SDK shell's cwd.
                        task.checked(["mkdir","-p","/opt/asb-project/.asb",
                            "/opt/asb-project/.asb-tools/pi/bin","/opt/asb-private","/tmp/asb-sdk-driver-context"])
                        upload(task.workspace.container,"/opt/asb-project/.asb",[("project.toml",
                            b"tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n",0o600)],temporary_dir=artifacts)
                        fixture = b'''#!/bin/sh
test -z "$ASB_GUEST_AGENT_REQUEST" || exit 90
test "$ASB_AGENT_API_KEY" = driver-private-fixture || exit 91
printf '%s|%s|%s\\n' "$(id -u)" "$PWD" "$ASB_SDK_PARENT_FIXTURE"
printf fixture-driver > performed.txt
printf '%s\\n' '{"type":"result","result":"driver-context-fixture"}'
exit 7
'''
                        upload(task.workspace.container,"/opt/asb-project/.asb-tools/pi/bin",
                            [("pi",fixture,0o755)],temporary_dir=artifacts)
                        session.send_keys(["cd /tmp/asb-sdk-driver-context","Enter"],block=True,max_timeout_sec=10)
                        driver_parent = parent()
                        guest = {"inherit_working_directory":True,"config":{
                            "agent":"pi","provider":"openrouter","model":"fixture-model","workload":"terminal-bench-1.0",
                            "workspace":task.workspace.workdir,"concurrency":1},
                            "connection":{"interface":"open_ai_chat","authentication":"bearer",
                                "base_url":"http://127.0.0.1:1/v1","credential":"driver-private-fixture"},
                            "prompt":"explicit fake client; no AI solving claim","project_root":"/opt/asb-project",
                            "runtime_dir":"/opt/asb-private","run_dir":"/opt/asb-private/attempt"}
                        with patch.object(session,"send_keys",wraps=session.send_keys) as keys:
                            result = task.workspace.execute(["/opt/asb-runtime/asb","__asb-agent"],
                                environment={"ASB_GUEST_AGENT_REQUEST":json.dumps(guest)},gate="application",
                                transport_factory=launcher,before_resume=attach,preparation_timeout=90)
                            self.assertNotIn("driver-private-fixture",str(keys.call_args_list))
                        self.assertEqual(result["exit_code"],7,result["stderr"].decode(errors="replace"))
                        self.assertIn(("|".join(driver_parent[1:])+"\n").encode(),result["stdout"])
                        self.assertEqual(task.checked(["cat","/tmp/asb-sdk-driver-context/performed.txt"]),b"fixture-driver")
                        self.assertEqual(parent(),driver_parent)
                        metadata = json.loads(task.checked(["cat","/opt/asb-private/attempt/agent-result.json"]))
                        self.assertEqual(metadata["exit_code"],7)
                        self.assertEqual(metadata["final_response"],"driver-context-fixture")
                        self.assertTrue(metadata["tree_quiesced"])
                        self.assertEqual(metadata["execution_workspace"],"/tmp/asb-sdk-driver-context")
                        self.assertTrue(session.is_session_alive())
                        self.assertEqual(hashes,{path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources})
                        self.assertEqual(binary_hash,hashlib.sha256(binary.read_bytes()).hexdigest())
                        success = True
                    finally:
                        if not success:task.interrupted = True
        finally:
            if launcher is not None:launcher.close()
        (artifacts/"qualification.json").write_text(json.dumps({"scope":"original-sdk-shell-guest-driver-fixture",
            "parent":before,"driver_parent":driver_parent,"source_hashes":hashes,"binary_sha256":binary_hash,"targets":targets,
            "client_statuses":[7,0,7],"provider_called":False,"coding_agent_started":False,
            "registered_adapter_driver_started":True,"grader_started":False},sort_keys=True)+"\n")


if __name__ == "__main__":unittest.main()
