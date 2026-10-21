"""Opt-in real Pi + original same-shell SDK/grader; synthetic provider only.

The provider supplies the known upstream repair. This tests integration, not AI
task solving, and never uses a cloud credential or a live inference service.
"""
import hashlib
import http.server
import json
import os
from pathlib import Path
import shlex
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))


@unittest.skipUnless(os.environ.get("ASB_TEST_BENCHMARK_ASB") and os.environ.get("ASB_TEST_SWECATALOG_ROOT"),
    "requires explicit frozen ASB, installed SDK/Pi caches and owned Docker opt-in")
class SDKInstalledAgentQualification(unittest.TestCase):
    def test_real_pi_uses_original_sdk_parent_and_retains_service_through_official_grading(self):
        from asb_docker import DockerSession
        from asb_guest import execute_agent,upload
        from asb_terminal_bench_1 import TerminalBenchTask
        import guest_provider_fixture as provider
        sources = sorted((Path(__file__).parents[1]/"assets/benchmarks").glob("asb_*.py"))
        hashes = {path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
        binary = Path(os.environ["ASB_TEST_BENCHMARK_ASB"]).absolute()
        binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
        project = Path(os.environ["ASB_TEST_SWECATALOG_ROOT"]).absolute()
        artifacts = Path(tempfile.mkdtemp(prefix="original-sdk-real-pi-",dir=project))
        print(f"Retained actual-client SDK qualification: {artifacts}",flush=True)
        provider.root,provider.calls,provider.requests = artifacts,0,0
        server = http.server.ThreadingHTTPServer(("127.0.0.1",0),provider.Handler)
        worker = threading.Thread(target=server.serve_forever,daemon=True)
        worker.start()
        parents,targets = [],[]
        try:
            with DockerSession() as client:
                # The ORIGINAL solution installs Debian/PyPI dependencies. Keep
                # ordinary bridge egress for real tools; only inference is
                # synthetic/local. Do not pre-solve or weaken the grader to
                # make a network-isolated fixture appear successful.
                network = client.networks.create("asb-sdk-fixture-"+artifacts.name,internal=False,
                    labels={"asb.owner":"asb"})
                try:
                    with TerminalBenchTask(client,project/"workloads/terminal-bench-1.0/catalog.json",
                            "jupyter-notebook-server",artifacts/"task") as task:
                        success = False
                        try:
                            self.assertTrue(task.handler.task.run_tests_in_same_shell)
                            self.assertIsNone(task.agent_session,"material preparation must not start SDK shell")
                            task.checked(["test","!","-e","/opt/asb-project"])
                            task.checked(["mkdir","-p","/opt/asb-fixture"])
                            upload(task.workspace.container,"/opt/asb-fixture",[("reference.sh",
                                (task.definition/"solution.sh").read_bytes(),0o700)],temporary_dir=artifacts)
                            def parent():
                                task.agent_session.send_keys(["printf '%s|%s|%s|%s\\n' \"$$\" \"$(id -u)\" \"$PWD\" "
                                    "\"$ASB_SDK_LIVE_CONTEXT\" > /tmp/asb-live-parent","Enter"],
                                    block=True,max_timeout_sec=10)
                                return task.checked(["cat","/tmp/asb-live-parent"]).decode().strip().split("|")
                            create_session = task.prepare_agent_session
                            def prepare_session():
                                create_session()
                                task.agent_session.send_keys(["export ASB_SDK_LIVE_CONTEXT=original-agent-parent","Enter"],
                                    block=True,max_timeout_sec=10)
                                parents.append(parent())
                                command = ("test \"$ASB_SDK_LIVE_CONTEXT\" = original-agent-parent && "
                                    "test \"$(id -u)\" = "+shlex.quote(parents[0][1])+" && "
                                    "test \"$PWD\" = "+shlex.quote(parents[0][2])+" && "
                                    "if /bin/bash /opt/asb-fixture/reference.sh > /tmp/asb-reference-install.log 2>&1; "
                                    "then printf 'Reference installation completed.\\n'; "
                                    "else tail -c 8192 /tmp/asb-reference-install.log >&2; exit 1; fi && "
                                    "printf actual-pi-tool > /tmp/asb-live-client-context")
                                (artifacts/"command.txt").write_text(command)
                            def attach(target):
                                bridge = client.api.exec_inspect(target["exec_id"])
                                self.assertIs(bridge["Running"],True)
                                self.assertNotEqual(target["daemon_pid"],bridge["Pid"])
                                self.assertEqual(target["pid_scope"],"docker-daemon-host")
                                targets.append(target)
                            architecture = {"amd64":"x64","arm64":"arm64"}[task.workspace.image_identity["architecture"]]
                            archive = project/".asb-tools/node/cache"/f"node-v22.20.0-linux-{architecture}.tar.gz"
                            self.assertTrue(archive.is_file(),"prefetch the pinned Node archive before this offline qualification")
                            request = {"binary":str(binary),"project_root":str(project),
                                "tools_root":str(project/".asb-tools"),"measure_target":True,"network":network.name,
                                "bootstrap":{"runtime_tools":["node"],"public_package_cache":True,
                                    "assets":[{"source":str(archive),"target":"node/cache/"+archive.name}]},
                                "environment":{"NPM_CONFIG_OFFLINE":"true","NO_PROXY":"*","no_proxy":"*"},
                                "request":{"process_lifetime":"through-evaluation","config":{
                                    "agent":"pi","provider":"openrouter","model":"fixture-model",
                                    "workload":"terminal-bench-1.0","workspace":task.workspace.workdir,"concurrency":1},
                                    "connection":{"interface":"open_ai_chat","authentication":"bearer",
                                        "base_url":f"http://127.0.0.1:{server.server_address[1]}/v1","credential":"fixture-token"},
                                    "prompt":task.invocation["prompt"]}}
                            with patch.object(task,"prepare_agent_session",side_effect=prepare_session):
                                result = execute_agent(task,request,attach,None,None)
                            for name in ["stdout","stderr"]:(artifacts/("driver."+name)).write_bytes(result[name])
                            self.assertEqual(result["exit_code"],0,result["stderr"].decode(errors="replace"))
                            self.assertTrue(result["workload_processes_retained"])
                            self.assertFalse(result["metadata"]["tree_quiesced"])
                            self.assertIsNone(result["metadata"]["execution_failure"])
                            self.assertEqual(len(targets),1)
                            self.assertEqual(provider.calls,2,"exactly one tool request plus its completion is expected")
                            self.assertFalse(result["output_truncated"])
                            events = [json.loads(line) for line in result["stdout"].splitlines() if line.strip()]
                            starts = [row for row in events if row.get("type") == "tool_execution_start"]
                            ends = [row for row in events if row.get("type") == "tool_execution_end"]
                            self.assertEqual(len(starts),1,"streaming progress is not another tool invocation")
                            self.assertEqual(len(ends),1)
                            self.assertEqual(starts[0]["toolCallId"],ends[0]["toolCallId"])
                            self.assertFalse(ends[0]["isError"])
                            package = json.loads(task.checked(["cat",
                                "/opt/asb-project/.asb-tools/pi/node_modules/@mariozechner/pi-coding-agent/package.json"]))
                            self.assertEqual(package["name"],"@mariozechner/pi-coding-agent")
                            self.assertEqual(package["version"],"0.73.1")
                            self.assertEqual(task.checked(["cat","/tmp/asb-live-client-context"]),b"actual-pi-tool")
                            self.assertEqual(parent(),parents[0])
                            self.assertTrue(task.workspace.held_executions[0][1].is_alive())
                            report = task.evaluate()
                            self.assertTrue(report["resolved"],report)
                            self.assertEqual(parent(),parents[0])
                            attempt = task.attempt
                            success = True
                        finally:
                            if not success:task.interrupted = True
                    self.assertEqual(task.cleanup_status,"removed")
                    self.assertEqual(client.containers.list(all=True,filters={"label":"asb.attempt="+attempt}),[])
                finally:
                    network.reload()
                    if not network.attrs.get("Containers"):
                        network.remove()
                    else:
                        (artifacts/"network-preserved.json").write_text(json.dumps({
                            "scope":"owned-failed-fixture","network":network.name,"status":"preserved"})+"\n")
        finally:
            server.shutdown();server.server_close();worker.join(timeout=5)
        self.assertFalse(worker.is_alive())
        self.assertEqual(hashes,{path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sources})
        self.assertEqual(binary_hash,hashlib.sha256(binary.read_bytes()).hexdigest())
        (artifacts/"qualification.json").write_text(json.dumps({"scope":"real-pi-original-sdk-same-shell-synthetic-provider",
            "binary_sha256":binary_hash,"source_hashes":hashes,"parent":parents[0],"targets":targets,
            "coding_agent":"pi","installed_package":{"name":package["name"],"version":package["version"]},
            "synthetic_provider_calls":provider.calls,"cloud_credentials_used":False,
            "tool_invocations":len(starts),"output_truncated":result["output_truncated"],
            "task_tool_network":"owned-bridge-with-package-egress",
            "official_resolved":True,"cleanup":"removed"},sort_keys=True)+"\n")


if __name__ == "__main__":unittest.main()
