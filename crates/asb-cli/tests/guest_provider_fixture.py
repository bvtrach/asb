"""Loopback-only, deterministic OpenAI-compatible tool-call fixture. No upstream."""
import http.server
import json
from pathlib import Path
import sys

root = None
calls = 0
requests = 0


def command():
    path = root / "command.txt"
    return path.read_text() if path.is_file() else "printf 'fixture\\n' > asb-real-agent.txt"


class Handler(http.server.BaseHTTPRequestHandler):
    def tool_command(self):
        return command()

    def log_message(self, *_args):
        pass

    def observe(self):
        global requests
        requests += 1
        # Count contacts without retaining credentials, prompts or bodies.
        (root / "requests.json").write_text(json.dumps({"requests": requests}))

    def do_GET(self):
        self.observe()
        if self.path != "/v1/models":
            self.send_error(404)
            return
        body = json.dumps({"data": [{"id": "fixture-model", "supported_parameters": ["tools"]}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        self.observe()
        global calls
        if self.path not in ["/v1/chat/completions", "/v1/responses"] or self.headers.get("Authorization") != "Bearer fixture-token":
            self.send_error(400)
            return
        length = int(self.headers.get("Content-Length", "0"))
        if not 0 < length <= 2 * 1024 * 1024:
            self.send_error(413)
            return
        body = json.loads(self.rfile.read(length))
        calls += 1
        (root / "calls.json").write_text(json.dumps({"calls": calls}))
        usage = {"prompt_tokens": 17, "completion_tokens": 5, "total_tokens": 22,
                 "prompt_tokens_details": {"cached_tokens": 0}}
        if self.path == "/v1/responses":
            self.respond_responses(body)
            return
        tool_result = any(message.get("role") == "tool" for message in body.get("messages", []))
        delta = {"role": "assistant", "content": "Fixture completed."}
        finish = "stop"
        if not tool_result:
            function = next((tool["function"] for tool in body.get("tools", [])
                             if "command" in tool.get("function", {}).get("parameters", {}).get("properties", {})), None)
            if function is None:
                self.send_error(422, "fixture needs a declared shell tool")
                return
            delta = {"role": "assistant", "tool_calls": [{"index": 0, "id": "fixture-call", "type": "function",
                "function": {"name": function["name"], "arguments": json.dumps({"command": self.tool_command()})}}]}
            finish = "tool_calls"
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        for choices, tokens in [([{"index": 0, "delta": delta, "finish_reason": None}], None),
                                ([{"index": 0, "delta": {}, "finish_reason": finish}], usage)]:
            event = {"id": "fixture-response", "object": "chat.completion.chunk", "model": body.get("model"), "choices": choices}
            if tokens is not None:
                event["usage"] = tokens
            self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()

    def respond_responses(self, body):
        usage = {"input_tokens": 17, "output_tokens": 5, "total_tokens": 22,
                 "input_tokens_details": {"cached_tokens": 0}, "output_tokens_details": {"reasoning_tokens": 0}}
        tool_result = any(item.get("type") == "function_call_output" for item in body.get("input", []) if isinstance(item, dict))
        response = {"id": "resp_fixture", "object": "response", "created_at": 1,
                    "status": "in_progress", "model": body.get("model"), "output": [], "error": None, "incomplete_details": None}
        events = [{"type": "response.created", "response": dict(response)}]
        if not tool_result:
            function = next((tool for tool in body.get("tools", [])
                             if "command" in tool.get("parameters", {}).get("properties", {})), None)
            if function is None:
                self.send_error(422, "fixture needs a declared shell tool")
                return
            arguments = json.dumps({"command": self.tool_command()})
            item = {"id": "fc_fixture", "type": "function_call", "call_id": "fixture-call",
                    "name": function["name"], "arguments": ""}
            events.append({"type": "response.output_item.added", "output_index": 0, "item": dict(item)})
            events.append({"type": "response.function_call_arguments.delta", "item_id": "fc_fixture", "output_index": 0, "delta": arguments})
            item["arguments"] = arguments
            events.append({"type": "response.function_call_arguments.done", "item_id": "fc_fixture", "output_index": 0, "arguments": arguments})
        else:
            item = {"id": "msg_fixture", "type": "message", "status": "in_progress", "role": "assistant", "content": []}
            part = {"type": "output_text", "text": "Fixture completed.", "annotations": []}
            events.extend([
                {"type": "response.output_item.added", "output_index": 0, "item": dict(item)},
                {"type": "response.content_part.added", "item_id": "msg_fixture", "output_index": 0, "content_index": 0,
                 "part": {"type": "output_text", "text": "", "annotations": []}},
                {"type": "response.output_text.delta", "item_id": "msg_fixture", "output_index": 0, "content_index": 0, "delta": part["text"]},
                {"type": "response.output_text.done", "item_id": "msg_fixture", "output_index": 0, "content_index": 0, "text": part["text"]},
                {"type": "response.content_part.done", "item_id": "msg_fixture", "output_index": 0, "content_index": 0, "part": part},
            ])
            item.update(status="completed", content=[part])
        events.append({"type": "response.output_item.done", "output_index": 0, "item": item})
        response.update(status="completed", output=[item], usage=usage)
        events.append({"type": "response.completed", "response": response})
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        for index, event in enumerate(events):
            event["sequence_number"] = index
            self.wfile.write(("event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n").encode())
        self.wfile.flush()


def main():
    global root
    root = Path(sys.argv[1])
    server = http.server.ThreadingHTTPServer((sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1", 0), Handler)
    (root / "port").write_text(str(server.server_address[1]))
    server.serve_forever()


if __name__ == "__main__":
    main()
