"""Private task protocol tests; no Docker, agent or provider calls."""
import base64
import io
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parents[1] / "assets/benchmarks"))
from asb_task_server import serve


class Task:
    def __init__(self):
        self.invocation = {"prompt": "synthetic issue", "workspace": "/task"}
        self.workspace = SimpleNamespace(cleanup_status="pending", execute=self.execute)
        self.commands = []
    def __enter__(self):
        return self
    def __exit__(self, *args):
        self.workspace.cleanup_status = "removed"
    def execute(self, argv, environment, **kwargs):
        self.commands.append((argv, environment))
        kwargs["before_resume"]({"exec_id": "owned-exec", "daemon_pid": 9001})
        kwargs["on_output"]("stdout", b"live\x00stdout")
        return {"exit_code": 7, "stdout": b"live\x00stdout", "stderr": b"warning", "timed_out": False}
    def capture_submission(self):
        return {"patch_bytes": 123, "artifact": "candidate.patch"}
    def evaluate(self, timeout):
        assert timeout == 60
        return {"resolved": False, "outcome": "unresolved"}


class ProtocolTests(unittest.TestCase):
    def run_protocol(self, requests, task=None):
        task = task or Task()
        output = io.StringIO()
        # Keep stdin genuinely open like the Rust controller; EOF must always
        # cancel even if a graceful close request was previously queued.
        read_fd, write_fd = os.pipe()
        source = os.fdopen(read_fd, "r")
        try:
            os.write(write_fd, "".join(json.dumps(request) + "\n" for request in requests).encode())
            status = serve(task, source, output, idle_timeout=2)
        finally:
            os.close(write_fd)
            source.close()
        return status, [json.loads(line) for line in output.getvalue().splitlines()], task

    def test_selected_command_and_credentials_stay_in_private_request_not_events(self):
        status, events, task = self.run_protocol([
            {"op": "execute", "argv": ["selected-agent", "literal ; argument"], "environment": {"API_KEY": "fixture-key"}},
            {"op": "resume", "exec_id": "owned-exec"}, {"op": "capture"}, {"op": "evaluate", "timeout": 60}, {"op": "close"},
        ])
        self.assertEqual(status, 0)
        self.assertEqual([event["event"] for event in events], ["ready", "paused", "output", "completed", "submission", "evaluated", "closed"])
        self.assertEqual(task.commands, [(["selected-agent", "literal ; argument"], {"API_KEY": "fixture-key"})])
        self.assertNotIn("fixture-key", json.dumps(events))
        self.assertEqual(base64.b64decode(events[2]["data"]), b"live\x00stdout")
        self.assertEqual(base64.b64decode(events[3]["result"]["stderr"]), b"warning")
        self.assertEqual(events[3]["result"]["exit_code"], 7)
        self.assertEqual(events[-1]["cleanup"], "removed")

    def test_wrong_resume_identity_is_rejected_and_cleanup_remains_available(self):
        status, events, _ = self.run_protocol([
            {"op": "execute", "argv": ["selected-agent"]},
            {"op": "resume", "exec_id": "other-execution"}, {"op": "close"},
        ])
        self.assertEqual(status, 0)
        self.assertNotIn("completed", [event["event"] for event in events])
        self.assertEqual(events[2]["event"], "error")
        self.assertEqual(events[-1]["cleanup"], "removed")

    def test_eof_or_malformed_request_closes_task_without_executing_agent(self):
        for source in ["", "not-json\n", "[]\n", "x" * (1024 * 1024 + 1)]:
            task, output = Task(), io.StringIO()
            self.assertEqual(serve(task, io.StringIO(source), output), 1)
            self.assertEqual(task.commands, [])
            self.assertEqual(task.workspace.cleanup_status, "removed")
            self.assertEqual(json.loads(output.getvalue().splitlines()[-1])["event"], "closed")

    def test_unknown_operation_does_not_echo_arbitrary_input(self):
        status, events, _ = self.run_protocol([{"op": "sensitive-input-not-for-output"}, {"op": "close"}])
        self.assertEqual(status, 0)
        self.assertNotIn("sensitive-input-not-for-output", json.dumps(events))
        self.assertEqual(events[1]["reason"], "unknown task protocol operation")


if __name__ == "__main__":
    unittest.main()
