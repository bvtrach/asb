"""Provider-free real-executable tests of the private guest TCP relay."""
import os
import select
import socket
import struct
import subprocess
import threading
import time
import unittest


class RelayTests(unittest.TestCase):
    def start(self, count=2):
        process = subprocess.Popen([os.environ["ASB_TEST_HOST_ASB"], "__asb-relay", str(count)],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, bufsize=0)
        self.addCleanup(self.cleanup, process)
        self.process = process
        kind, channel, payload = self.frame()
        self.assertEqual((kind, channel, len(payload)), (5, 0, 2 * count))
        return struct.unpack("!" + "H" * count, payload)

    @staticmethod
    def cleanup(process):
        if process.stdin and not process.stdin.closed:
            process.stdin.close()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=3)
        for stream in (process.stdout, process.stderr):
            if stream:
                stream.close()

    def exact(self, count):
        data, deadline = bytearray(), time.monotonic() + 5
        while len(data) < count:
            remaining = deadline - time.monotonic()
            self.assertGreater(remaining, 0, "relay frame timed out")
            ready, _, _ = select.select([self.process.stdout], [], [], remaining)
            self.assertTrue(ready, "relay did not publish a frame")
            chunk = os.read(self.process.stdout.fileno(), count - len(data))
            self.assertTrue(chunk, "relay unexpectedly closed stdout")
            data.extend(chunk)
        return bytes(data)

    def frame(self):
        kind, channel, size = struct.unpack("!BII", self.exact(9))
        self.assertLessEqual(size, 65536)
        return kind, channel, self.exact(size)

    def send(self, kind, channel, payload=b""):
        data = struct.pack("!BII", kind, channel, len(payload)) + payload
        while data:
            written = os.write(self.process.stdin.fileno(), data)
            data = data[written:]

    def connect(self, port, target):
        client = socket.create_connection(("127.0.0.1", port), timeout=5)
        self.addCleanup(client.close)
        kind, channel, payload = self.frame()
        self.assertEqual((kind, payload), (1, struct.pack("!I", target)))
        return client, channel

    def test_concurrent_targets_preserve_binary_bytes_and_independent_half_closes(self):
        ports = self.start()
        first, first_id = self.connect(ports[0], 0)
        second, second_id = self.connect(ports[1], 1)
        self.assertNotEqual(first_id, second_id)
        # Includes NUL/non-UTF8 bytes and HTTP-looking content: no HTTP parser,
        # token transformation or buffering-until-response is permitted here.
        payloads = {first_id: bytes(range(256)) * 128,
                    second_id: b"GET /stream HTTP/1.1\r\n\r\n\x00\xff"}
        first.sendall(payloads[first_id])
        second.sendall(payloads[second_id])
        observed = {first_id: bytearray(), second_id: bytearray()}
        while any(len(observed[key]) < len(value) for key, value in payloads.items()):
            kind, channel, data = self.frame()
            self.assertEqual(kind, 2)
            observed[channel].extend(data)
        self.assertEqual(observed, payloads)
        first.shutdown(socket.SHUT_WR)
        self.assertEqual(self.frame(), (3, first_id, b""))
        self.send(2, first_id, b"response after request EOF")
        self.send(3, first_id)
        received = bytearray()
        while chunk := first.recv(1024):
            received.extend(chunk)
        self.assertEqual(received, b"response after request EOF")
        self.send(2, second_id, b"still independent")
        self.assertEqual(second.recv(1024), b"still independent")
        self.send(4, first_id)
        self.send(4, second_id)
        # Closing a channel is idempotent; stale late data cannot reopen it.
        self.send(4, first_id)
        self.send(2, first_id, b"ignored after close")
        self.process.stdin.close()
        self.assertEqual(self.process.wait(timeout=3), 0)

    def test_transport_disconnect_closes_active_sockets_without_other_process_changes(self):
        ports = self.start(1)
        client, _ = self.connect(ports[0], 0)
        self.process.stdin.close()
        self.assertEqual(self.process.wait(timeout=3), 0)
        self.assertEqual(client.recv(1), b"")
        with self.assertRaises(OSError):
            socket.create_connection(("127.0.0.1", ports[0]), timeout=1)

    def test_streaming_many_frames_does_not_require_a_complete_response_buffer(self):
        port, = self.start(1)
        client, channel = self.connect(port, 0)
        payload = bytes(range(256)) * 4096
        received = bytearray()
        errors = []
        def receive():
            try:
                while chunk := client.recv(65536):
                    received.extend(chunk)
            except OSError as error:
                errors.append(error)
        reader = threading.Thread(target=receive, daemon=True)
        reader.start()
        for offset in range(0, len(payload), 65536):
            self.send(2, channel, payload[offset:offset + 65536])
        self.send(3, channel)
        reader.join(timeout=5)
        self.assertFalse(reader.is_alive())
        self.assertEqual(errors, [])
        self.assertEqual(received, payload)

    def test_channel_capacity_is_bounded_and_transport_eof_drains_all_connections(self):
        port, = self.start(1)
        clients = [self.connect(port, 0)[0] for _ in range(128)]
        overflow = socket.create_connection(("127.0.0.1", port), timeout=5)
        self.addCleanup(overflow.close)
        self.assertEqual(overflow.recv(1), b"")
        self.process.stdin.close()
        self.assertEqual(self.process.wait(timeout=5), 0)
        self.assertTrue(all(client.recv(1) == b"" for client in clients))

    def test_invalid_direction_and_oversized_frames_fail_without_payload_allocation(self):
        for kind, channel, length, body in [(1, 1, 0, b""), (2, 0, 0, b""),
                                             (3, 1, 1, b"x"), (2, 1, 65537, b"")]:
            with self.subTest(kind=kind, channel=channel, length=length):
                self.start(1)
                self.process.stdin.write(struct.pack("!BII", kind, channel, length) + body)
                self.process.stdin.flush()
                self.assertNotEqual(self.process.wait(timeout=3), 0)
                self.cleanup(self.process)

    def test_target_count_is_bounded(self):
        for count in ("0", "65", "-1", "not-a-number"):
            result = subprocess.run([os.environ["ASB_TEST_HOST_ASB"], "__asb-relay", count],
                                    capture_output=True, timeout=3)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    unittest.main()
