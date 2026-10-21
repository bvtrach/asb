"""Socketpair transport tests: no Docker, network or elevated access."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest
from types import SimpleNamespace

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks/asb_docker.py"
SPEC = importlib.util.spec_from_file_location("asb_docker", SOURCE)
RUNTIME = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNTIME)


class ConnectionTests(unittest.TestCase):
    def test_archive_policy_preserves_quiet_tty_stream_semantics(self):
        class SDK:
            def _stream_raw_result(self,response,chunk_size,decode):
                response.timeout = None  # Upstream's deliberate TTY behavior.
                return "upstream-stream"
            def _raise_for_status(self,response):
                if response.failed:raise RuntimeError("fixture HTTP failure")
        class Client(RUNTIME.ArchiveStreamPolicy,SDK):pass
        for path,archive in [("/v1.52/containers/fixture/archive?path=/",True),
                             ("/v1.52/containers/fixture/logs",False),
                             ("/v1.52/containers/fixture/attach?path=/archive",False),
                             ("/v1.52/images/get",False)]:
            response = SimpleNamespace(url="http+docker://localhost"+path,timeout=120,failed=False,
                closed=False,iter_content=lambda *_:iter([b"fixture"]))
            response.close = lambda:setattr(response,"closed",True)
            stream = Client()._stream_raw_result(response,512,False)
            if archive:
                self.assertEqual(response.timeout,120)
                self.assertEqual(list(stream),[b"fixture"])
                self.assertTrue(response.closed)
            else:
                self.assertEqual(stream,"upstream-stream")
                self.assertIsNone(response.timeout)
                self.assertFalse(response.closed)
        response.failed = True
        response.url = "http+docker://localhost/v1.52/containers/fixture/archive"
        with self.assertRaisesRegex(RuntimeError,"HTTP failure"):
            Client()._stream_raw_result(response)
        self.assertTrue(response.closed)

    def test_failed_stream_construction_and_client_close_release_owned_resources(self):
        for invalid in [False,True]:
            response = SimpleNamespace(closed=False)
            def content(*_):
                if invalid:raise RuntimeError("fixture construction failed")
                return 42  # Not an iterable.
            response.iter_content = content
            response.close = lambda:setattr(response,"closed",True)
            with self.assertRaises((TypeError,RuntimeError)):
                RUNTIME.ClosingResponseStream(response,512,False)
            self.assertTrue(response.closed)
        session = RUNTIME.DockerSession()
        def fail():raise RuntimeError("fixture client close failed")
        transport = SimpleNamespace(closed=False)
        transport.close = lambda:setattr(transport,"closed",True)
        session.client,session.connections = SimpleNamespace(close=fail),transport
        with self.assertRaisesRegex(RuntimeError,"client close failed"):
            session.__exit__(None,None,None)
        self.assertTrue(transport.closed)
        self.assertIsNone(session.client)
        self.assertIsNone(session.connections)
        session.__exit__(None,None,None)

    def test_raw_stream_releases_response_before_first_read_after_partial_and_on_error(self):
        class Response:
            def __init__(self, error=False):
                self.closed,self.reads,self.error = 0,0,error
            def iter_content(self, size, decode):
                self.reads += 1
                self.arguments = (size,decode)
                yield b"first"
                if self.error:raise TimeoutError("fixture read deadline")
                yield b"second"
            def close(self):self.closed += 1
        for mode in ["unread","partial","complete","error"]:
            response = Response(error=mode == "error")
            stream = RUNTIME.ClosingResponseStream(response,512,False)
            if mode == "partial":self.assertEqual(next(stream),b"first")
            if mode == "complete":self.assertEqual(list(stream),[b"first",b"second"])
            if mode == "error":
                self.assertEqual(next(stream),b"first")
                with self.assertRaises(TimeoutError):next(stream)
            stream.close()
            stream.close()
            self.assertEqual(response.closed,1)
            self.assertEqual(response.reads,0 if mode == "unread" else 1)
            self.assertEqual(list(stream),[])
            if mode != "unread":self.assertEqual(response.arguments,(512,False))

    def test_duplex_transport_retains_binary_payload_and_cleans_only_owned_children(self):
        command = [sys.executable, "-c", "import os; data=os.read(0, 1024); os.write(1, data[::-1])"]
        transport = RUNTIME.StdioConnections(command)
        connection = transport.connect(3)
        connection.sendall(b"HTTP/1.1\r\n\x00payload\xff")
        self.assertEqual(connection.recv(1024), b"HTTP/1.1\r\n\x00payload\xff"[::-1])
        connection.close()
        children = list(transport.children)
        transport.close()
        self.assertTrue(all(child.poll() is not None for child in children))
        transport.close()
        with self.assertRaises(RUNTIME.DockerAccessError):
            transport.connect(1)

    def test_closing_connections_terminates_their_live_cli_children_not_unrelated_processes(self):
        unrelated = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
        transport = RUNTIME.StdioConnections([sys.executable, "-c", "import time; time.sleep(60)"])
        connection = transport.connect(1)
        child = transport.children[0]
        try:
            self.assertIsNone(child.poll())
            transport.close()
            self.assertIsNotNone(child.poll())
            self.assertIsNone(unrelated.poll())
        finally:
            connection.close()
            unrelated.terminate()
            unrelated.wait(timeout=3)

    def test_failed_start_closes_socketpair_and_leaves_no_child(self):
        transport = RUNTIME.StdioConnections(["/does-not-exist/asb-test"])
        with self.assertRaises(FileNotFoundError):
            transport.connect(1)
        self.assertEqual(transport.children, [])
        transport.close()


if __name__ == "__main__":
    unittest.main()
