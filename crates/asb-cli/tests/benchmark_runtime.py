"""Offline runtime-bundle contracts; no Docker daemon or provider required."""
import os
import json
import hashlib
from pathlib import Path
import subprocess
import select
import signal
import shutil
import sys
import tempfile
import time
import unittest
from test_environment import coverage_env

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
import asb_runtime as subject
from asb_execution import ExecutionError


class RuntimeTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "linux" and os.environ.get("ASB_TEST_HOST_ASB"),
                         "needs the just-built Linux ASB executable")
    def test_kernel_reference_bundles_the_running_binary_after_path_replacement(self):
        original = Path(os.environ["ASB_TEST_HOST_ASB"]).absolute()
        digest = hashlib.sha256(original.read_bytes()).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "asb"
            shutil.copy2(original, executable)
            process = subprocess.Popen([executable, "__asb-gate", "/bin/true"],
                                       env=coverage_env({"PATH": os.defpath}), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 5
                while not any(line.startswith("State:") and "T" in line
                              for line in Path(f"/proc/{process.pid}/status").read_text().splitlines()):
                    self.assertLess(time.monotonic(), deadline, "gate never stopped")
                    self.assertIsNone(process.poll())
                    time.sleep(.005)
                executable.unlink()  # Only the test-created executable copy.
                files, receipt = subject.bundle(f"/proc/{process.pid}/exe")
                image = next(content for name, content, _ in files if name in ["asb", "bin/asb"] and content.startswith(b"\x7fELF"))
                self.assertEqual(hashlib.sha256(image).hexdigest(), digest)
                self.assertNotIn(str(directory), str(receipt))
                os.kill(process.pid, signal.SIGCONT)
                _, stderr = process.communicate(timeout=5)
                self.assertEqual(process.returncode, 0, stderr.decode())
            finally:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=5)

    def test_loader_dependency_parser_is_complete_and_rejects_unresolved_or_ambiguous_libraries(self):
        text = "linux-vdso.so.1 (0x123)\nlibc.so.6 => /lib/libc.so.6 (0x456)\n/lib64/ld-linux.so.2 (0x789)\n"
        self.assertEqual(subject.library_paths(text),{"libc.so.6":"/lib/libc.so.6","ld-linux.so.2":"/lib64/ld-linux.so.2"})
        for invalid in ["libc.so.6 => not found", "./bad => /lib/x (0x123)",text+"libc.so.6 => /other/libc.so.6 (0x124)"]:
            with self.subTest(invalid=invalid),self.assertRaises(ExecutionError):
                subject.library_paths(invalid)

    def test_captured_ubuntu26_main_mapping_is_not_a_library_identity(self):
        captured = (Path(__file__).parent/"fixtures/loader-ubuntu26-asb.txt").read_text()
        self.assertEqual(hashlib.sha256(captured.encode()).hexdigest(),
                         "2b5ef0db4ee89223c334d3eab68a7a1dbe8c6f503d3426844484153fce59847d")
        expected = {"libgcc_s.so.1":"/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
                    "libc.so.6":"/usr/lib/x86_64-linux-gnu/libc.so.6",
                    "ld-linux-x86-64.so.2":"/lib64/ld-linux-x86-64.so.2"}
        self.assertEqual(subject.library_paths(captured), expected)
        tail = "\n".join(captured.splitlines()[1:])+"\n"
        for address in ["0", "1", "ABCDEF", "0123456789abcdef"]:
            with self.subTest(address=address):
                self.assertEqual(subject.library_paths(f"\n\t (0x{address})\n"+tail),expected)

    def test_anonymous_mapping_never_hides_unknown_or_unresolved_dependencies(self):
        header = "\t (0x000072fb98000000)\n"
        library = "libc.so.6 => /lib/libc.so.6 (0x456)\n"
        invalid = [
            header, header+"linux-vdso.so.1 (0x123)\n",
            header+header+library, library+header,
            "linux-vdso.so.1 (0x123)\n"+header+library,
            "(0x)\n"+library, "(0x123456789abcdef01)\n"+library,
            "(0xNOTHEX)\n"+library, "(0x123) extra\n"+library,
            "missing => (0x123)\n"+library,
            header+"libmissing.so => not found\n",
            header+library+"libmissing.so => not found\n",
            header+library+"libc.so.6 => /other/libc.so.6 (0x457)\n",
            header+"unexpected loader text\n"+library,
            header+"".join(f"lib{i}.so => /lib/lib{i}.so (0x123)\n" for i in range(65)),
        ]
        for text in invalid:
            with self.subTest(text=text),self.assertRaises(ExecutionError):
                subject.library_paths(text)

    @unittest.skipUnless(os.environ.get("ASB_TEST_HOST_ASB"),"needs the just-built ASB executable")
    def test_real_asb_bundle_executes_without_injecting_library_paths_into_children(self):
        path = Path(os.environ["ASB_TEST_HOST_ASB"]).absolute()
        files,receipt = subject.bundle(path)
        self.assertIn(receipt["kind"],["static","bundled-loader"])
        self.assertNotIn(str(path),str(receipt))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name,content,mode in files:
                target = root/name
                target.parent.mkdir(parents=True,exist_ok=True)
                # Fixed deployment root is changed only in this local test
                # launcher; the ELF and libraries remain byte-for-byte equal.
                if name == "asb" and receipt["kind"] == "bundled-loader":
                    self.assertNotIn(b"LD_LIBRARY_PATH",content)
                    content = content.replace(subject.ROOT.encode(),str(root).encode())
                target.write_bytes(content)
                target.chmod(mode)
            result = subprocess.run([root/"asb","--version"],env=coverage_env({"PATH":os.defpath}),capture_output=True,text=True,timeout=10)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertTrue(result.stdout.startswith("asb "),result.stdout)
            result = subprocess.run([root/"asb","__asb-agent"],env=coverage_env({"PATH":os.defpath,"ASB_GUEST_AGENT_REQUEST":"invalid"}),
                                    capture_output=True,text=True,timeout=10)
            self.assertNotEqual(result.returncode,0)
            self.assertIn("invalid guest execution request",result.stderr)
            project = root/"project"
            for name in [".asb",".asb-tools/pi/bin","workspace"]:
                (project/name).mkdir(parents=True)
            (project/".asb/project.toml").write_text("tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n")
            client = project/".asb-tools/pi/bin/pi"
            client.write_text("#!/bin/sh\ntest -z \"$LD_LIBRARY_PATH\" || exit 90\n"
                "printf '%s\\n' '{\"type\":\"turn_end\",\"usage\":{\"input\":2,\"output\":3}}'\n"
                "printf '%s\\n' '{\"type\":\"result\",\"result\":\"offline fixture\"}'\n")
            client.chmod(0o755)
            request = {"config":{"agent":"pi","provider":"openrouter","model":"fixture","workload":"fixture",
                "workspace":str(project/"workspace")}, "connection":{"interface":"open_ai_chat","authentication":"bearer",
                "base_url":"http://127.0.0.1:1/v1","credential":"fixture"}, "prompt":"offline fixture",
                "project_root":str(project),"runtime_dir":str(project/"private"),"run_dir":str(project/"private/attempt")}
            process = subprocess.Popen([root/"asb","__asb-agent"],env=coverage_env({"PATH":os.defpath,"HOME":str(project),
                "ASB_GUEST_AGENT_REQUEST":json.dumps(request)}),stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                self.assertTrue(select.select([process.stdout],[],[],5)[0],"guest did not become ready")
                self.assertEqual(process.stdout.readline().strip(),f"__ASB_GATE__:{process.pid}".encode())
                deadline = time.monotonic()+5
                while not any(line.startswith("State:") and "T" in line for line in Path(f"/proc/{process.pid}/status").read_text().splitlines()):
                    self.assertLess(time.monotonic(),deadline)
                    time.sleep(0.005)
                os.kill(process.pid,signal.SIGCONT)
                _,stderr = process.communicate(timeout=5)
                self.assertEqual(process.returncode,0,stderr.decode())
                metadata = json.loads((project/"private/attempt/agent-result.json").read_text())
                self.assertEqual(metadata["token_usage"]["total_tokens"],5)
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=5)
        with self.assertRaisesRegex(ExecutionError,"architecture"):
            subject.bundle(path,"a-deliberately-incompatible-image")


if __name__ == "__main__":
    unittest.main()
