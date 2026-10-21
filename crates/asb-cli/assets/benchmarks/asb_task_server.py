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
from enum import Enum
import time

from asb_execution import ExecutionError

MAX_REQUEST = 1024 * 1024


class Operation(str, Enum):
    EXECUTE = "execute"
    AGENT = "agent"
    RESUME = "resume"
    CAPTURE = "capture"
    EVALUATE = "evaluate"
    CLOSE = "close"
    CANCEL = "cancel"
    INITIAL_MATERIAL = "initial-material"


class Event(str, Enum):
    READY = "ready"
    PAUSED = "paused"
    OUTPUT = "output"
    EXECUTION_ENDED = "execution_ended"
    COMPLETED = "completed"
    SUBMISSION = "submission"
    EVALUATED = "evaluated"
    ERROR = "error"
    CLOSED = "closed"
    INITIAL_MATERIAL = "initial-material"
    PREPARATION_FAILED = "preparation_failed"


def serve(task, source, destination, idle_timeout=900):
    requests = queue.Queue(maxsize=64)
    peer_closed, stop_reader = threading.Event(), threading.Event()
    task.cancelled = peer_closed
    # Only an explicit close reaching the task loop authorizes destructive
    # normal teardown. EOF, failed publication, malformed input and signals
    # retain the attempt for adapter-owned submission/evaluation recovery.
    task.interrupted = True

    emission_lock = threading.Lock()
    def emit(event, **fields):
        with emission_lock:
            destination.write(json.dumps({"event": event.value, **fields}) + "\n")
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
                if request.get("op") == Operation.CANCEL:
                    peer_closed.set()
                    return
                requests.put(request, timeout=1)
        except (OSError, ValueError, queue.Full):
            peer_closed.set()

    def receive(timeout=None, cap_idle=True):
        # Polling this event also handles EOF while idle or paused. No global
        # filesystem leases or stale process-name assumptions are involved.
        timeout = idle_timeout if timeout is None else min(timeout, idle_timeout) if cap_idle else timeout
        deadline = time.monotonic()+timeout
        while time.monotonic() < deadline:
            if peer_closed.is_set():
                raise ExecutionError("ASB controller disconnected")
            try:
                return requests.get(timeout=max(0,min(0.05, deadline-time.monotonic())))
            except queue.Empty:
                pass
        raise ExecutionError("ASB controller did not respond before the protocol deadline")

    def attach(target, preparation_budget=None):
        emit(Event.PAUSED, target=target)
        response = receive(timeout=preparation_budget.remaining() if preparation_budget else 60,
                           cap_idle=preparation_budget is None)
        if response.get("op") != Operation.RESUME or response.get("exec_id") != target["exec_id"]:
            raise ExecutionError("execution was cancelled before collector attachment completed")

    def output(name, chunk):
        emit(Event.OUTPUT, stream=name, data=base64.b64encode(chunk).decode("ascii"))

    reader_thread = threading.Thread(target=reader, daemon=True)
    reader_thread.start()
    status = 0
    try:
        with task:
            emit(Event.READY, invocation=task.invocation)
            while True:
                request = receive()
                operation = request.get("op")
                if operation == Operation.CLOSE:
                    task.interrupted = False
                    break
                try:
                    try:
                        operation = Operation(operation)
                    except (ValueError, TypeError):
                        raise ExecutionError("unknown task protocol operation") from None
                    if operation in [Operation.EXECUTE, Operation.AGENT]:
                        if operation == Operation.AGENT:
                            from asb_guest import execute_agent, PreparationBudget
                            seconds = request.get("preparation_seconds")
                            budget = PreparationBudget(seconds) if seconds is not None else None
                            result = execute_agent(task, request, lambda target: attach(target,budget), output, peer_closed,
                                on_ended=lambda result: emit(Event.EXECUTION_ENDED, elapsed_seconds=result["elapsed_seconds"], exit_code=result["exit_code"]),
                                preparation_budget=budget, on_preparation_failure=lambda: emit(Event.PREPARATION_FAILED))
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
                        emit(Event.COMPLETED, result=result)
                    elif operation == Operation.CAPTURE:
                        emit(Event.SUBMISSION, submission=task.capture_submission())
                    elif operation == Operation.EVALUATE:
                        emit(Event.EVALUATED, result=task.evaluate(timeout=request.get("timeout", 1800)))
                    elif operation == Operation.INITIAL_MATERIAL:
                        capture = getattr(task, "initial_material", None)
                        if capture is None:
                            raise ExecutionError("workload adapter has not declared prepared material coverage")
                        emit(Event.INITIAL_MATERIAL, material=capture(binary=request["binary"]) if "binary" in request else capture())
                    else:
                        raise ExecutionError("unknown task protocol operation")
                except ExecutionError as error:
                    emit(Event.ERROR, error_class=type(error).__name__, reason=str(error))
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
            emit(Event.ERROR, **fields)
        except (BrokenPipeError, OSError):
            pass
    finally:
        stop_reader.set()
        try:
            workspace = getattr(task, "workspace", None)
            emit(Event.CLOSED, cleanup=getattr(task, "cleanup_status", getattr(workspace, "cleanup_status", "not_created")))
        except (BrokenPipeError, OSError):
            pass
    return status
