"""A tiny stand-in for the OpenAI Responses API, for Codex integration tests only.

Codex is pointed at it through a custom model provider (see the Codex adapter tests). It answers:
- a prompt containing "run" with a shell tool call (`echo agentdeck-ok`),
- a prompt containing "write" with an apply_patch call that adds hello.txt,
- a prompt containing "list the bots" with a call to the agentdeck MCP server's list_bots tool,
  then a reply quoting what the tool answered; "delegate a hello" with a delegate_task call
  handing "Say hello" to the Greeter,
- a turn whose last input is a tool output with a short text reply,
- a relay prompt with an "implemented it" reply,
- anything else with plain text.
It also serves /v1/chat/completions (streamed, with function tool calls) for AgentDeck's own agent.
Every request body is appended to /tmp/mock_openai_requests.jsonl for inspection.
"""
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8788
LOG_PATH = "/tmp/mock_openai_requests.jsonl"

PATCH = "*** Begin Patch\n*** Add File: hello.txt\n+hello from agentdeck\n*** End Patch\n"


def tool_names(body):
    out = []
    for t in body.get("tools") or []:
        out.append(t.get("name") or (t.get("function") or {}).get("name") or t.get("type"))
    return out


def all_tool_names(tools):
    """Every (namespace, name) of the request's tools; Codex puts MCP tools in a namespace."""
    out = []
    for t in tools:
        if t.get("type") == "namespace":
            out += [(t.get("name"), x.get("name")) for x in t.get("tools") or []]
        elif t.get("name"):
            out.append((None, t["name"]))
    return out


def plan(body):
    if '"input_image"' in json.dumps(body.get("input", [])[-1:]):
        return [{"type": "message", "text": "I can see 1 image."}]
    items = body.get("input") or []
    last = items[-1] if items else {}
    if last.get("type") in ("function_call_output", "custom_tool_call_output"):
        out = last.get("output")
        text = out if isinstance(out, str) else json.dumps(out)
        if "list_bots" in json.dumps(items[-3:]) and "bots" in text:
            return [{"type": "message", "text": "The agentdeck tools answered: " + text[:2000]}]
        declined = any(w in text.lower() for w in ("declined", "rejected", "denied", "not approved", "aborted"))
        return [{"type": "message", "text": "The action was not approved, so I stopped." if declined else "Done. The command printed agentdeck-ok and the file is in place."}]
    text = ""
    for it in reversed(items):
        if it.get("role") == "user":
            for c in it.get("content") or []:
                text += " " + (c.get("text") or "")
            break
    text = text.lower()
    names = tool_names(body)
    for ask, tool, args in (("list the bots", "list_bots", {}), ("delegate a hello", "delegate_task", {"bot": "greeter", "task": "Say hello"})):
        if ask not in text:
            continue
        found = [ns for ns, n in all_tool_names(body.get("tools") or []) if n == tool and "agentdeck" in (ns or "")]
        if not found:
            return [{"type": "message", "text": f"There's no {tool} tool here."}]
        return [{"type": "function", "namespace": found[0], "name": tool, "arguments": json.dumps(args)}]
    if "asks for these fixes" in text:
        return [{"type": "message", "text": "Added a test for a zero discount in test_invoice.py. All tests pass."}]
    if "agentdeck relay" in text or "(round " in text or "builder in an agentdeck workflow" in text:
        return [{"type": "message", "text": "Implemented it: invoice.py now applies the discount before tax, and test_invoice.py has a case for it. All tests pass. Over to you for review."}]
    if "write" in text:
        if "apply_patch" in names:
            tool = next(t for t in body["tools"] if (t.get("name") == "apply_patch"))
            if tool.get("type") == "custom":
                return [{"type": "message", "text": "I'll create the file."}, {"type": "custom", "name": "apply_patch", "input": PATCH}]
            return [{"type": "message", "text": "I'll create the file."}, {"type": "function", "name": "apply_patch", "arguments": json.dumps({"input": PATCH})}]
        shell = next((n for n in names if n in ("shell", "shell_command", "exec_command", "local_shell")), "shell")
        return [{"type": "function", "name": shell, "arguments": json.dumps({"command": ["bash", "-lc", "printf 'hello from agentdeck\\n' > hello.txt"]} if shell == "shell" else {"command": "printf 'hello from agentdeck\\n' > hello.txt", "cmd": "printf 'hello from agentdeck\\n' > hello.txt"})}]
    if "run" in text:
        shell = next((n for n in names if n in ("shell", "shell_command", "exec_command", "local_shell")), "shell")
        args = {"command": ["bash", "-lc", "touch marker.txt && echo agentdeck-ok"]} if shell == "shell" else {"command": "touch marker.txt && echo agentdeck-ok", "cmd": "touch marker.txt && echo agentdeck-ok"}
        return [{"type": "message", "text": "I'll run a quick check."}, {"type": "function", "name": shell, "arguments": json.dumps(args)}]
    return [{"type": "message", "text": "Hello from the mock model."}]


def chat_plan(body):
    """Chat Completions (AgentDeck's own agent): same behaviour, as tool calls."""
    msgs = body.get("messages") or []
    last = msgs[-1] if msgs else {}
    if last.get("role") == "tool":
        text = str(last.get("content", "")).lower()
        denied = "denied" in text
        return {"text": "The action was not approved, so I stopped." if denied else "Done. The command printed agentdeck-ok and the file is in place."}
    text = str(last.get("content", "")).lower()
    names = {t.get("function", {}).get("name") for t in body.get("tools") or []}
    if "asks for these fixes" in text:
        return {"text": "Added a test for a zero discount in test_invoice.py. All tests pass."}
    if "agentdeck relay" in text or "(round " in text or "builder in an agentdeck workflow" in text:
        return {"text": "Implemented it: invoice.py now applies the discount before tax, and test_invoice.py has a case for it. All tests pass. Over to you for review."}
    if "write" in text and "write_file" in names:
        return {"text": "I'll create the file.", "call": ("write_file", {"path": "hello.txt", "content": "hello from agentdeck\n"})}
    if "run" in text and "run_command" in names:
        return {"text": "I'll run a quick check.", "call": ("run_command", {"command": "echo agentdeck-ok"})}
    if "read" in text and "read_file" in names:
        return {"text": "", "call": ("read_file", {"path": "hello.txt"})}
    return {"text": "Hello from the mock model."}


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _json(self, code, obj):
        data = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self._json(200, {"data": [{"id": "mock-model", "object": "model"}], "models": []})

    def do_POST(self):
        n = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(n) or b"{}"
        try:
            body = json.loads(raw)
        except Exception:
            body = {}
        with open(LOG_PATH, "a") as f:
            f.write(json.dumps({"path": self.path, "body": body})[:200000] + "\n")
        if self.path.rstrip("/").endswith("/chat/completions"):
            return self.chat(body)
        if not self.path.rstrip("/").endswith("/responses"):
            return self._json(200, {})
        out = plan(body)
        rid = f"resp_{int(time.time()*1e6)}"
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()

        def ev(name, data):
            data = {"type": name, **data}
            self.wfile.write(f"event: {name}\ndata: {json.dumps(data)}\n\n".encode())
            self.wfile.flush()

        ev("response.created", {"response": {"id": rid, "status": "in_progress"}})
        items = []
        for i, o in enumerate(out):
            if o["type"] == "message":
                mid = f"msg_{rid}_{i}"
                item = {"type": "message", "id": mid, "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": o["text"], "annotations": []}]}
                ev("response.output_item.added", {"output_index": i, "item": {**item, "content": [], "status": "in_progress"}})
                for w in o["text"].split(" "):
                    ev("response.output_text.delta", {"output_index": i, "item_id": mid, "content_index": 0, "delta": w + " "})
                ev("response.output_item.done", {"output_index": i, "item": item})
            elif o["type"] == "function":
                item = {"type": "function_call", "id": f"fc_{rid}_{i}", "call_id": f"call_{rid}_{i}", "name": o["name"], "arguments": o["arguments"], "status": "completed"}
                if o.get("namespace"):
                    item["namespace"] = o["namespace"]
                ev("response.output_item.added", {"output_index": i, "item": {**item, "arguments": ""}})
                ev("response.output_item.done", {"output_index": i, "item": item})
            else:
                item = {"type": "custom_tool_call", "id": f"ct_{rid}_{i}", "call_id": f"call_{rid}_{i}", "name": o["name"], "input": o["input"], "status": "completed"}
                ev("response.output_item.added", {"output_index": i, "item": {**item, "input": ""}})
                ev("response.output_item.done", {"output_index": i, "item": item})
            items.append(item)
        usage = {"input_tokens": 10, "input_tokens_details": {"cached_tokens": 0}, "output_tokens": 10, "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 20}
        ev("response.completed", {"response": {"id": rid, "status": "completed", "output": items, "usage": usage}})


    def chat(self, body):
        out = chat_plan(body)
        cid = f"chatcmpl_{int(time.time()*1e6)}"
        if not body.get("stream"):
            return self._json(200, {"id": cid, "object": "chat.completion", "choices": [{"index": 0, "message": {"role": "assistant", "content": out["text"]}, "finish_reason": "stop"}]})
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()

        def chunk(delta, finish=None):
            data = {"id": cid, "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
            self.wfile.write(f"data: {json.dumps(data)}\n\n".encode())
            self.wfile.flush()

        chunk({"role": "assistant"})
        for w in out["text"].split(" ") if out["text"] else []:
            chunk({"content": w + " "})
        if "call" in out:
            name, args = out["call"]
            raw = json.dumps(args)
            half = len(raw) // 2
            chunk({"tool_calls": [{"index": 0, "id": f"call_{cid}", "type": "function", "function": {"name": name, "arguments": raw[:half]}}]})
            chunk({"tool_calls": [{"index": 0, "function": {"arguments": raw[half:]}}]})
            chunk({}, "tool_calls")
        else:
            chunk({}, "stop")
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
