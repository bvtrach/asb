"""Provider-free managed execution tests using a deterministic Docker API fixture."""
import importlib.util
from pathlib import Path
import sys
import threading
import time
from types import SimpleNamespace
import unittest

sys.dont_write_bytecode = True
SOURCE = Path(__file__).parents[1] / "assets/benchmarks/asb_execution.py"
SPEC = importlib.util.spec_from_file_location("asb_execution", SOURCE)
RUNTIME = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNTIME)


class Stream:
    def __init__(self, api):
        self.api, self.index = api, 0
        self.closed = False
        self._response = SimpleNamespace(closed=False)
        self._response.close = lambda: setattr(self._response, "closed", True)
    def __iter__(self):
        return self
    def __next__(self):
        api = self.api
        if self.index == 0:
            self.index += 1
            return api.ready, None
        if not api.container.resumed:
            raise AssertionError("read agent output before resume")
        if api.hang:
            if not api.container.killed.wait(2):
                raise AssertionError("deadline did not stop its owned container")
        position = self.index - 1
        if position < len(api.output) and not api.hang:
            self.index += 1
            return api.output[position]
        api.completed = True
        raise StopIteration
    def close(self):
        self.closed = True


class Container:
    def __init__(self):
        self.id = "owned-container"
        self.attrs = {}
        self.events = []
        self.resumed = False
        self.killed = threading.Event()
    def start(self):
        self.events.append("start")
    def wait(self, timeout):
        assert timeout == 125
        self.events.append("wait")
    def reload(self):
        pass
    def exec_run(self, argv):
        if "asb-resume" in argv:
            self.events.append("resume")
            self.resumed = True
        else:
            self.events.append("stopped")
        return SimpleNamespace(exit_code=0, output=b"")
    def kill(self):
        self.events.append("kill")
        self.killed.set()
    def remove(self, force):
        assert force is True
        self.events.append("remove")


class API:
    def __init__(self, container):
        self.container = container
        self.ready = b"__ASB_GATE__:42\n"
        self.output = [(b"stdout", None), (None, b"stderr")]
        self.hang = False
        self.status = 0
        self.pid = 9001
        self.calls = []
        self.completed = False
    def exec_create(self, container_id, **kwargs):
        assert container_id == self.container.id
        self.completed = False
        self.container.resumed = False
        self.container.killed.clear()
        self.calls.append(kwargs)
        return {"Id": "exec-1"}
    def exec_start(self, exec_id, stream, demux):
        assert exec_id == "exec-1" and stream and demux
        self.stream = Stream(self)
        return self.stream
    def exec_inspect(self, exec_id):
        assert exec_id == "exec-1"
        return {"Pid": self.pid, "Running": not self.completed,
                "ExitCode": (137 if self.hang else self.status) if self.completed else None}


class ExecutionTests(unittest.TestCase):
    def test_collector_attachment_has_its_own_budget_and_is_not_agent_elapsed_time(self):
        workspace, container, _, _ = self.workspace()
        with workspace:
            result = workspace.execute(["agent"], timeout=.02, preparation_timeout=.3,
                                       before_resume=lambda _: time.sleep(.06))
            self.assertGreaterEqual(result["preparation_seconds"], .06)
            self.assertLess(result["elapsed_seconds"], .05)
            self.assertFalse(result["timed_out"])
            self.assertNotIn("kill", container.events)

    def test_expired_or_cancelled_preparation_never_resumes_the_owned_gate(self):
        for cancel in (False, True):
            workspace, container, _, _ = self.workspace()
            cancelled = threading.Event()
            with workspace:
                def attach(_):
                    if cancel:
                        cancelled.set()
                    else:
                        time.sleep(.05)
                with self.assertRaises(RUNTIME.ExecutionError):
                    workspace.execute(["agent"], timeout=1, preparation_timeout=.02,
                                      before_resume=attach, cancelled=cancelled)
                self.assertFalse(container.resumed)
                self.assertIn("kill", container.events)

    def workspace(self):
        container = Container()
        api = API(container)
        creation = []
        def create(**kwargs):
            creation.append(kwargs)
            container.attrs = {"Config": {"Labels": kwargs["labels"]}}
            return container
        client = SimpleNamespace(api=api, containers=SimpleNamespace(create=create))
        image = SimpleNamespace(id="sha256:" + "f" * 64, attrs={"RepoDigests": [], "Os": "linux", "Architecture": "amd64"})
        workspace = RUNTIME.ManagedDockerWorkspace(client, image, "asb-" + "a" * 32, "/task")
        return workspace, container, api, creation

    def test_gate_stops_before_attachment_and_preserves_argv_env_and_output(self):
        workspace, container, api, creation = self.workspace()
        with workspace:
            def attach(target):
                self.assertFalse(container.resumed)
                self.assertEqual(container.events[-1], "stopped")
                self.assertEqual(target["daemon_pid"], 9001)
                self.assertEqual(target["namespace_pid"], 42)
                self.assertEqual(target["pid_scope"], "docker-daemon-host")
                container.events.append("attached")
            result = workspace.execute(["agent", "literal ; $value"], {"FIXTURE_KEY": "fixture-only"}, before_resume=attach)
            self.assertEqual(result["stdout"], b"stdout")
            self.assertEqual(result["stderr"], b"stderr")
            self.assertEqual(result["exit_code"], 0)
            self.assertFalse(result["timed_out"])
            self.assertEqual(api.calls[0]["cmd"][-2:], ["agent", "literal ; $value"])
            self.assertEqual(api.calls[0]["environment"], {"FIXTURE_KEY": "fixture-only"})
            self.assertLess(container.events.index("attached"), container.events.index("resume"))
            self.assertNotIn("volumes", creation[0])
            self.assertNotIn("network_mode", creation[0])
            self.assertNotIn("privileged", creation[0])
            self.assertGreater(creation[0]["pids_limit"], 0)
        self.assertEqual(container.events[-1], "remove")
        self.assertTrue(api.stream.closed)
        self.assertTrue(api.stream._response.closed)
        workspace.close()
        self.assertEqual(container.events.count("remove"), 1)

    def test_application_gate_preserves_self_contained_adapter_driver(self):
        workspace, _container, api, _ = self.workspace()
        with workspace:
            self.assertEqual(workspace.execute(["/opt/asb/asb", "__asb-agent"], gate="application")["exit_code"], 0)
            self.assertEqual(api.calls[0]["cmd"], ["/opt/asb/asb", "__asb-agent"])

    def test_nonzero_exit_preserves_workspace_and_output_is_bounded(self):
        workspace, container, api, _ = self.workspace()
        api.status = 7
        with workspace:
            result = workspace.execute(["agent"], output_limit=3)
            self.assertEqual(result["exit_code"], 7)
            self.assertEqual(result["stdout"], b"std")
            self.assertEqual(result["stderr"], b"std")
            self.assertTrue(result["output_truncated"])
            self.assertNotIn("kill", container.events)

    def test_deadline_stops_owned_tree_and_restarts_workspace_for_submission_capture(self):
        workspace, container, api, _ = self.workspace()
        api.hang = True
        with workspace:
            result = workspace.execute(["agent"], timeout=0.02)
            self.assertEqual(result["exit_code"], 137)
            self.assertTrue(result["timed_out"])
            self.assertEqual(container.events[-3:], ["kill", "wait", "start"])
            api.hang = False
            self.assertEqual(workspace.execute(["git", "diff"])["exit_code"], 0)

    def test_controller_disconnect_cancels_active_execution_without_waiting_for_deadline(self):
        workspace, container, api, _ = self.workspace()
        cancelled = threading.Event()
        api.hang = True
        trigger = threading.Timer(0.03, cancelled.set)
        with workspace:
            trigger.start()
            try:
                result = workspace.execute(["agent"], timeout=60, cancelled=cancelled)
            finally:
                trigger.join(timeout=1)
            self.assertTrue(result["cancelled"])
            self.assertFalse(result["timed_out"])
            self.assertLess(result["elapsed_seconds"], 2)
            self.assertEqual(container.events[-3:], ["kill", "wait", "start"])

    def test_callback_failure_cannot_resume_agent_or_leave_paused_process(self):
        workspace, container, api, _ = self.workspace()
        with workspace:
            def attach(_target):
                raise ValueError("synthetic collector failure")
            with self.assertRaisesRegex(ValueError, "synthetic collector"):
                workspace.execute(["agent"], before_resume=attach)
            self.assertNotIn("resume", container.events)
            self.assertEqual(container.events[-3:], ["kill", "wait", "start"])
            self.assertTrue(api.stream.closed)
            self.assertEqual(workspace.execute(["retry"])["exit_code"], 0)

    def test_invalid_gate_or_target_is_not_resumed(self):
        for field, value in [("ready", b"unrelated output\n"), ("pid", 0)]:
            workspace, container, api, _ = self.workspace()
            setattr(api, field, value)
            with workspace:
                with self.assertRaises(RUNTIME.ExecutionError):
                    workspace.execute(["agent"])
                self.assertNotIn("resume", container.events)
                self.assertIn("kill", container.events)

    def test_invalid_arguments_are_rejected_before_any_exec_is_created(self):
        workspace, _container, api, _ = self.workspace()
        with workspace:
            for argv, env in [([], {}), (["bad\x00"], {}), (["agent"], {"bad-key": "v"}), (["agent"], {None: "v"})]:
                with self.assertRaises(RUNTIME.ExecutionError):
                    workspace.execute(argv, env)
            self.assertEqual(api.calls, [])

    def test_cleanup_refuses_a_different_attempts_container(self):
        workspace, container, _api, _ = self.workspace()
        workspace.__enter__()
        container.attrs["Config"]["Labels"]["asb.attempt"] = "someone-else"
        with self.assertRaisesRegex(RUNTIME.ExecutionError, "outside this attempt"):
            workspace.close()
        self.assertNotIn("remove", container.events)

    def test_unconfirmed_agent_is_reset_only_after_observed_container_exit(self):
        workspace, container, _api, _ = self.workspace()
        with workspace:
            workspace.quiesce_unconfirmed_agent()
            self.assertEqual(container.events[-3:], ["kill", "wait", "start"])
            def failed_wait(timeout):
                raise RuntimeError("synthetic stop failure")
            container.wait = failed_wait
            with self.assertRaisesRegex(RUNTIME.ExecutionError, "confirm stopped"):
                workspace.quiesce_unconfirmed_agent()
            self.assertEqual(container.events[-1], "kill")

    def test_unconfirmed_agent_reset_refuses_other_owners_or_shared_pid_namespaces(self):
        for change in ["owner", "pid"]:
            workspace, container, _api, _ = self.workspace()
            workspace.__enter__()
            if change == "owner":
                container.attrs["Config"]["Labels"]["asb.attempt"] = "another-attempt"
            else:
                container.attrs["HostConfig"] = {"PidMode":"host"}
            with self.assertRaisesRegex(RUNTIME.ExecutionError, "owned PID namespace"):
                workspace.quiesce_unconfirmed_agent()
            self.assertNotIn("kill", container.events)

    def test_attach_refuses_shared_container_pid_namespaces(self):
        workspace, container, _api, creation = self.workspace()
        workspace.__enter__()
        container.image = workspace.image
        for mode in ["host", "container:some-other-container"]:
            container.attrs["HostConfig"] = {"PidMode":mode}
            with self.assertRaisesRegex(RUNTIME.ExecutionError, "private container PID namespace"):
                RUNTIME.ManagedDockerWorkspace.attach(workspace.client, container, workspace.attempt, "/task")
        workspace.close()


if __name__ == "__main__":
    unittest.main()
