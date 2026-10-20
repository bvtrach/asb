"""Offline bootstrap contracts; no Docker daemon, installed client or provider."""
import io
import json
from pathlib import Path
import struct
import sys
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
from asb_execution import ExecutionError
from asb_guest import execute_agent, stage_public_cache, stage_bootstrap
from asb_runtime import elf as inspect_elf


def elf(interpreter=False):
    data = bytearray(120)
    data[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<H",data,16,2)
    struct.pack_into("<H",data,18,62)
    struct.pack_into("<I",data,20,1)
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HHH", data, 52, 64, 56, 1)
    struct.pack_into("<I", data, 64, 3 if interpreter else 1)
    return data


class Container:
    def __init__(self):
        self.files = []

    def put_archive(self, destination, source):
        if isinstance(source, bytes):
            source = io.BytesIO(source)
        with tarfile.open(fileobj=source, mode="r") as archive:
            self.files.extend((destination, item.name, archive.extractfile(item).read() if item.isfile() else None)
                              for item in archive.getmembers())
        return True


class GuestBootstrapTests(unittest.TestCase):
    def test_local_forwarding_precedes_install_and_is_closed_on_install_failure_or_success(self):
        for failure in (False, True):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / ".asb").mkdir()
                (root / ".asb/project.toml").write_text("tools_dir='tools'\n")
                binary = root / "asb"
                binary.write_bytes(elf())
                container = Container()
                container.id = "owned-container-fixture"
                container.exec_run = lambda _: SimpleNamespace(exit_code=1)
                calls, resources = [], []
                class Relay:
                    ports = (10001, 10002)
                    closed = False
                    def __init__(self, targets, command, cancelled):
                        self.targets = targets
                        resources.append(self)
                    def close(self):
                        self.closed = True
                    def check(self):
                        if self.closed:
                            raise ExecutionError("relay closed")
                def execute(argv, **options):
                    self.assertFalse(resources[0].closed)
                    self.assertEqual(options["environment"]["HTTP_PROXY"], "http://localhost:10002")
                    calls.append((argv, options))
                    return {"stdout":b"", "stderr":b"", "exit_code":1 if failure else 0,
                            "timed_out":False, "cancelled":False}
                def checked(argv, **_options):
                    return json.dumps({"schema_version":1,"exit_code":0,"final_response":"done",
                                       "token_usage":{},"steps":None}).encode() if argv[0] == "cat" else b""
                task = SimpleNamespace(workspace=SimpleNamespace(container=container, execute=execute),
                                       artifacts=root, checked=checked)
                request = {"binary":str(binary),"project_root":str(root),"tools_root":str(root/"tools"),
                    "project_configuration":"tools_dir='tools'\n[proxy]\nhttp='http://localhost:3128'\n",
                    "environment":{"HTTP_PROXY":"http://localhost:3128"},
                    "bootstrap":{"runtime_tools":[],"assets":[]},
                    "request":{"config":{"agent":"pi"},"connection":{"base_url":"http://localhost:8000/v1"}}}
                with patch("asb_guest.HostRelay", Relay), patch("asb_docker.docker_command", return_value=["docker-fixture"]):
                    if failure:
                        with self.assertRaises(ExecutionError):
                            execute_agent(task,request,None,None,None)
                    else:
                        execute_agent(task,request,None,None,None)
                self.assertTrue(resources[0].closed)
                self.assertEqual(resources[0].targets, [("localhost",8000),("localhost",3128)])
                project = next(data for destination,name,data in container.files
                               if destination == "/opt/asb-project/.asb" and name == "project.toml")
                self.assertIn(b"http://localhost:10002",project)
                if not failure:
                    guest = json.loads(calls[-1][1]["environment"]["ASB_GUEST_AGENT_REQUEST"])
                    self.assertEqual(guest["connection"]["base_url"],"http://localhost:10001/v1")

    def test_declared_assets_and_runtime_order_do_not_depend_on_agent_names(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root/"tools"
            source = tools/"example/cache/package.tar.gz"
            source.parent.mkdir(parents=True)
            source.write_bytes(b"public pinned distribution fixture")
            container = Container()
            calls = []
            task = SimpleNamespace(workspace=SimpleNamespace(container=container),artifacts=root,
                                   checked=lambda argv:calls.append(argv))
            plan = {"runtime_tools":["example-runtime"],"assets":[{"source":str(source),"target":"example/cache/package.tar.gz"}]}
            self.assertEqual(stage_bootstrap(task,plan,tools),["example-runtime"])
            self.assertEqual(container.files,[("/opt/asb-project/.asb-tools","example/cache/package.tar.gz",source.read_bytes())])
            receipt = json.loads((root/"bootstrap.json").read_text())
            self.assertNotIn(str(root),json.dumps(receipt))
            self.assertEqual(receipt["assets"][0]["bytes"],len(source.read_bytes()))

    def test_bootstrap_rejects_escape_symlinks_duplicate_targets_and_invalid_dependencies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root/"tools"
            tools.mkdir()
            source = tools/"package"
            source.write_bytes(b"fixture")
            link = tools/"link"
            link.symlink_to(source)
            external = root/"private"
            external.write_bytes(b"must-not-be-staged")
            assets = [{"source":str(source),"target":"../escape"},{"source":str(source),"target":"/escape"},
                      {"source":str(external),"target":"package"},{"source":str(link),"target":"package"}]
            for asset in assets:
                container = Container()
                task = SimpleNamespace(workspace=SimpleNamespace(container=container),artifacts=root,checked=lambda _:None)
                with self.subTest(asset=asset),self.assertRaises(ExecutionError):
                    stage_bootstrap(task,{"runtime_tools":[],"assets":[asset]},tools)
                self.assertEqual(container.files,[])
            valid = {"source":str(source),"target":"package"}
            for plan in [{"runtime_tools":[],"assets":[valid,valid]},
                         {"runtime_tools":["runtime","runtime"],"assets":[]},
                         {"runtime_tools":["../bad"],"assets":[]},
                         {"runtime_tools":[],"assets":[],"public_package_cache":"not-a-boolean"}, {}]:
                with self.assertRaises(ExecutionError):
                    stage_bootstrap(task,plan,tools)

    def test_runtime_parser_accepts_static_elf_and_rejects_malformed_files(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "asb"
            binary.write_bytes(elf())
            self.assertEqual(inspect_elf(binary),(elf(),62,None))
            for invalid in [elf(True), b"#!/bin/sh\n", elf()[:100]]:
                binary.write_bytes(invalid)
                with self.assertRaises(ExecutionError):
                    inspect_elf(binary)

    def test_registry_cache_with_credentials_is_never_staged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / ".asb-cache/npm/_cacache/index-v5"
            cache.mkdir(parents=True)
            (cache / "entry").write_text('hash\t' + json.dumps({"metadata":{"reqHeaders":{"Authorization":"private-fixture"}}}))
            container = Container()
            stage_public_cache(container, root)
            self.assertEqual(container.files, [])
            (cache / "entry").write_text('hash\t' + json.dumps({"metadata":{"reqHeaders":{}}}))
            stage_public_cache(container, root)
            self.assertTrue(any(name == "npm/_cacache/index-v5/entry" for _, name, _ in container.files))

    def test_bootstrap_uses_exact_selected_tool_and_private_request_not_argv(self):
        for timed_out, measured in [(False, False), (True, False), (False, True)]:
            with self.subTest(timed_out=timed_out, measured=measured), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / ".asb").mkdir()
                (root / ".asb/project.toml").write_text("tools_dir='tools'\n")
                binary = root / "asb"
                binary.write_bytes(elf())
                container = Container()
                calls = []
                probes, attached = [], []
                def probe(argv):
                    probes.append(argv)
                    return SimpleNamespace(exit_code=0, output=b'{"namespace_pid":42,"start_time_ticks":123,"boot_id":"fixture"}')
                container.exec_run = probe

                def execute(argv, **options):
                    calls.append((argv, options))
                    if argv[-1] == "__asb-agent":
                        options["before_resume"]({"namespace_pid":42})
                    return {"stdout":b"out", "stderr":b"err", "exit_code":124 if timed_out and argv[-1] == "__asb-agent" else 0,
                            "timed_out":timed_out and argv[-1] == "__asb-agent", "cancelled":False}

                def checked(argv, **_options):
                    if argv[0] == "cat":
                        if timed_out:
                            raise ExecutionError("no completed metadata")
                        return json.dumps({"schema_version":1,"exit_code":0,"final_response":"done", "token_usage":{},"steps":None}).encode()
                    return b""

                task = SimpleNamespace(workspace=SimpleNamespace(container=container, execute=execute),
                                       artifacts=root, checked=checked, agent_timeout=1234)
                request = {"binary":str(binary), "project_root":str(root), "tools_root":str(root / "tools"),
                           "measure_target":measured,
                           "bootstrap":{"runtime_tools":["node"],"assets":[]},
                           "request":{"config":{"agent":"pi"},"connection":{"credential":"private-fixture"}}}
                result = execute_agent(task, request, attached.append, lambda *_: None, None)
                self.assertEqual([call[0][-1] for call in calls], ["node", "pi", "__asb-agent"])
                self.assertNotIn("private-fixture", repr([call[0] for call in calls]))
                private = calls[-1][1]["environment"]["ASB_GUEST_AGENT_REQUEST"]
                self.assertEqual(json.loads(private)["connection"]["credential"], "private-fixture")
                self.assertEqual(json.loads(private)["timeout_seconds"], 1234)
                self.assertEqual(calls[-1][1]["timeout"], 1234)
                self.assertEqual(calls[-1][1]["gate"], "application")
                self.assertEqual(len(probes), int(measured))
                self.assertEqual(len(attached), 1)
                if measured:
                    self.assertEqual(probes[0], ["/opt/asb-runtime/asb", "__asb-target", "42"])
                    self.assertEqual(attached[0]["process_identity"]["start_time_ticks"], 123)
                self.assertIsNone(result["metadata"]["token_usage"].get("total_tokens"))
                self.assertEqual(result["metadata"]["exit_code"], 124 if timed_out else 0)


if __name__ == "__main__":
    unittest.main()
