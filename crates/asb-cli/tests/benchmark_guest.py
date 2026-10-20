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

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
from asb_execution import ExecutionError
from asb_guest import execute_agent, portable_binary, stage_public_cache


def elf(interpreter=False):
    data = bytearray(120)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HH", data, 54, 56, 1)
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
    def test_portable_runtime_accepts_static_elf_but_rejects_dynamic_and_malformed_files(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "asb"
            binary.write_bytes(elf())
            self.assertEqual(portable_binary(binary), elf())
            for invalid in [elf(True), b"#!/bin/sh\n", elf()[:100]]:
                binary.write_bytes(invalid)
                with self.assertRaises(ExecutionError):
                    portable_binary(binary)

    def test_registry_cache_with_credentials_is_never_staged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / ".asb-cache/npm/_cacache/index-v5"
            cache.mkdir(parents=True)
            (cache / "entry").write_text('hash\t' + json.dumps({"metadata":{"reqHeaders":{"Authorization":"private-fixture"}}}))
            container = Container()
            stage_public_cache(container, root, root / "tools")
            self.assertEqual(container.files, [])
            (cache / "entry").write_text('hash\t' + json.dumps({"metadata":{"reqHeaders":{}}}))
            stage_public_cache(container, root, root / "tools")
            self.assertTrue(any(name == "npm/_cacache/index-v5/entry" for _, name, _ in container.files))

    def test_bootstrap_uses_exact_selected_tool_and_private_request_not_argv(self):
        for timed_out in [False, True]:
            with self.subTest(timed_out=timed_out), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / ".asb").mkdir()
                (root / ".asb/project.toml").write_text("tools_dir='tools'\n")
                binary = root / "asb"
                binary.write_bytes(elf())
                container = Container()
                calls = []

                def execute(argv, **options):
                    calls.append((argv, options))
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
                           "request":{"config":{"agent":"pi"},"connection":{"credential":"private-fixture"}}}
                result = execute_agent(task, request, lambda _: None, lambda *_: None, None)
                self.assertEqual([call[0][-1] for call in calls], ["node", "pi", "__asb-agent"])
                self.assertNotIn("private-fixture", repr([call[0] for call in calls]))
                private = calls[-1][1]["environment"]["ASB_GUEST_AGENT_REQUEST"]
                self.assertEqual(json.loads(private)["connection"]["credential"], "private-fixture")
                self.assertEqual(json.loads(private)["timeout_seconds"], 1234)
                self.assertEqual(calls[-1][1]["timeout"], 1234)
                self.assertEqual(calls[-1][1]["gate"], "application")
                self.assertIsNone(result["metadata"]["token_usage"].get("total_tokens"))
                self.assertEqual(result["metadata"]["exit_code"], 124 if timed_out else 0)


if __name__ == "__main__":
    unittest.main()
