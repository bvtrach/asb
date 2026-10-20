"""Loopback-only, deterministic OpenAI-compatible tool-call fixture. No upstream."""
import http.server
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
calls = 0


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        global calls
        if self.path != "/v1/chat/completions" or self.headers.get("Authorization") != "Bearer fixture-token":
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
                "function": {"name": function["name"], "arguments": json.dumps({"command": "printf 'fixture\\n' > asb-real-agent.txt"})}}]}
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


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
(root / "port").write_text(str(server.server_address[1]))
server.serve_forever()
