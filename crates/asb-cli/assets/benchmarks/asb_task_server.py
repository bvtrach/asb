"""Bounded, private JSON-lines bridge between ASB and a workload-owned task.

Agent/provider choices stay in ASB. The bridge executes the exact command and
environment supplied over stdin; credentials are never process arguments.
EOF cancels active execution, so a departed worker does not leave its agent
running until the normal benchmark deadline.
"""
import base64
import json
import queue
import threading

from asb_execution import ExecutionError

MAX_REQUEST = 1024 * 1024


def serve(task, source, destination, idle_timeout=900):
    requests = queue.Queue(maxsize=64)
    peer_closed, stop_reader = threading.Event(), threading.Event()
    task.cancelled = peer_closed

    def emit(event, **fields):
        destination.write(json.dumps({"event": event, **fields}) + "\n")
        destination.flush()

    def reader():
        try:
            while not stop_reader.is_set():
                line = source.readline(MAX_REQUEST + 1)
                if not line:
                    peer_closed.set()
                    return
                if len(line.encode()) > MAX_REQUEST or not line.endswith("\n"):
                    peer_closed.set()
                    return
                request = json.loads(line)
                if not isinstance(request, dict):
                    raise ValueError("request must be an object")
                if request.get("op") == "cancel":
                    peer_closed.set()
                    return
                requests.put(request, timeout=1)
        except (OSError, ValueError, queue.Full):
            peer_closed.set()

    def receive(timeout=None):
        # Polling this event also handles EOF while idle or paused. No global
        # filesystem leases or stale process-name assumptions are involved.
        timeout = idle_timeout if timeout is None else min(timeout, idle_timeout)
        elapsed = 0.0
        while elapsed < timeout:
            if peer_closed.is_set():
                raise ExecutionError("ASB controller disconnected")
            try:
                return requests.get(timeout=min(0.05, timeout - elapsed))
            except queue.Empty:
                elapsed += min(0.05, timeout - elapsed)
        raise ExecutionError("ASB controller did not respond before the protocol deadline")

    def attach(target):
        emit("paused", target=target)
        response = receive(timeout=60)
        if response.get("op") != "resume" or response.get("exec_id") != target["exec_id"]:
            raise ExecutionError("execution was cancelled before collector attachment completed")

    def output(name, chunk):
        emit("output", stream=name, data=base64.b64encode(chunk).decode("ascii"))

    reader_thread = threading.Thread(target=reader, daemon=True)
    reader_thread.start()
    status = 0
    try:
        with task:
            emit("ready", invocation=task.invocation)
            while True:
                request = receive()
                operation = request.get("op")
                if operation == "close":
                    break
                try:
                    if operation in ["execute", "agent"]:
                        if operation == "agent":
                            from asb_guest import execute_agent
                            result = execute_agent(task, request, attach, output, peer_closed)
                        else:
                            result = task.workspace.execute(
                                request.get("argv"), request.get("environment"),
                                timeout=request.get("timeout", 900), output_limit=request.get("output_limit", 256 * 1024),
                                before_resume=attach, on_output=output, cancelled=peer_closed,
                                gate=request.get("gate", "shell"),
                            )
                        result = dict(result)
                        for stream in ["stdout", "stderr"]:
                            result[stream] = base64.b64encode(result[stream]).decode("ascii")
                        emit("completed", result=result)
                    elif operation == "capture":
                        emit("submission", submission=task.capture_submission())
                    elif operation == "evaluate":
                        emit("evaluated", result=task.evaluate(timeout=request.get("timeout", 1800)))
                    else:
                        raise ExecutionError("unknown task protocol operation")
                except ExecutionError as error:
                    emit("error", error_class=type(error).__name__, reason=str(error))
                    if peer_closed.is_set():
                        break
    except Exception as error:
        status = 1
        try:
            # SDK exceptions may contain HTTP bodies or environment values.
            # Known local errors are deliberately bounded and input-free.
            fields = {"error_class": type(error).__name__}
            if isinstance(error, ExecutionError):
                fields["reason"] = str(error)
            emit("error", **fields)
        except (BrokenPipeError, OSError):
            pass
    finally:
        stop_reader.set()
        try:
            workspace = getattr(task, "workspace", None)
            emit("closed", cleanup=getattr(workspace, "cleanup_status", "not_created"))
        except (BrokenPipeError, OSError):
            pass
    return status
