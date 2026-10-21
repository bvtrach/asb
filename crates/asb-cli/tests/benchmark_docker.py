"""Socketpair transport tests: no Docker, network or elevated access."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks/asb_docker.py"
SPEC = importlib.util.spec_from_file_location("asb_docker", SOURCE)
RUNTIME = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNTIME)


class ConnectionTests(unittest.TestCase):
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
