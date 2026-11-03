"""A tiny stand-in for the Anthropic Messages API, for integration tests only.

The real `claude` CLI is pointed at it with ANTHROPIC_BASE_URL. It answers:
- a message with an image block with a reply naming its type,
- a prompt containing "run" with a Bash tool_use (`echo agentdeck-ok`),
- a relay prompt with a short plan, and the second relay round with a review ending in DONE,
- a workflow review: notes the first time, APPROVED after the fixes,
- a prompt containing "plan", when the ExitPlanMode tool is offered, with an ExitPlanMode tool_use,
- a prompt containing "write" with a Write tool_use (hello.txt),
- a prompt containing "ask me" with an AskUserQuestion tool_use, then a reply quoting the answer,
- a conversation that asks "ask the greeter" (a manager bot): ToolSearch for the agentdeck tools
  if they're deferred, mcp__agentdeck__delegate_task to the Greeter, mcp__agentdeck__wait_for_tasks
  on the task id it got back, then a reply quoting the Greeter's result,
- a turn whose last message carries a tool_result with a short text reply,
- anything else with a plain text reply.
Non-streaming helper calls (titles, token counts) get minimal valid answers.
"""
import json, os, re, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8787
LOG = []

def last_user_blocks(body):
    msgs = body.get("messages") or []
    for m in reversed(msgs):
        if m.get("role") == "user":
            c = m.get("content")
            return [{"type": "text", "text": c}] if isinstance(c, str) else (c or [])
    return []

PLAN_TEXT = "## Plan\n\n1. Read `invoice.py` and find where the discount is applied.\n2. Apply the discount before tax, not after.\n3. Add a test for a 10% discount on a taxed item.\n4. Run `python -m pytest`."

def last_tool_use_name(body):
    for m in reversed(body.get("messages") or []):
        if m.get("role") == "assistant" and isinstance(m.get("content"), list):
            uses = [b for b in m["content"] if b.get("type") == "tool_use"]
            return uses[-1]["name"] if uses else None
    return None

def ask_question():
    return {"type": "tool_use", "id": f"toolu_q{int(time.time()*1000)}", "name": "AskUserQuestion", "input": {"questions": [
        {"question": "Which state library should the app use?", "header": "State", "multiSelect": False,
         "options": [{"label": "Riverpod", "description": "Already used in two screens"}, {"label": "Bloc", "description": "More boilerplate"}]}]}}

def exit_plan():
    return [{"type": "text", "text": "I've looked at the code. Here's the plan."}, {"type": "tool_use", "id": f"toolu_p{int(time.time()*1000)}", "name": "ExitPlanMode", "input": {}}]

def plan_file(body):
    m = re.search(r"create your plan at (\S+?\.md)", json.dumps(body))
    return m.group(1) if m else None

def next_plan_step(body, tools):
    """Plan mode the way the CLI asks for it: write the plan file, then ExitPlanMode."""
    if "ExitPlanMode" in tools:
        return exit_plan()
    return [{"type": "tool_use", "id": f"toolu_s{int(time.time()*1000)}", "name": "ToolSearch", "input": {"query": "select:ExitPlanMode", "max_results": 1}}]

def all_user_text(body):
    out = []
    for m in body.get("messages") or []:
        if m.get("role") != "user":
            continue
        c = m.get("content")
        for b in ([{"type": "text", "text": c}] if isinstance(c, str) else (c or [])):
            if b.get("type") == "text":
                out.append(b.get("text", ""))
    return " ".join(out).lower()

def result_text(block):
    c = block.get("content")
    return c if isinstance(c, str) else " ".join(x.get("text", "") for x in (c or []) if isinstance(x, dict))

DELEGATE = "mcp__agentdeck__delegate_task"
WAIT = "mcp__agentdeck__wait_for_tasks"

def manager(body, blocks, tools):
    """A manager bot: hand the greeting to the Greeter, wait for it, pass its report on."""
    if "ask the greeter" not in all_user_text(body).split("</system-reminder>")[-1]:
        return None
    results = [b for b in blocks if b.get("type") == "tool_result"]
    last = last_tool_use_name(body)
    stamp = int(time.time() * 1000)
    if results and last == WAIT:
        got = json.loads(result_text(results[-1]) or "{}")
        tasks = got.get("tasks") or [{}]
        return [{"type": "text", "text": "The Greeter reported: " + str(tasks[0].get("result") or tasks[0].get("error") or "nothing")}]
    if results and last == DELEGATE:
        text = result_text(results[-1])
        try:
            tid = json.loads(text)["task_id"]
        except Exception:
            return [{"type": "text", "text": "Couldn't hand it over: " + text}]
        return [{"type": "text", "text": "Handed over. Waiting for the Greeter."}, {"type": "tool_use", "id": f"toolu_wt{stamp}", "name": WAIT, "input": {"task_ids": [tid], "timeout_seconds": 120}}]
    if DELEGATE in tools:
        return [{"type": "text", "text": "I'll ask the Greeter."}, {"type": "tool_use", "id": f"toolu_dt{stamp}", "name": DELEGATE, "input": {"bot": "greeter", "task": "Say hello"}}]
    if results and last == "ToolSearch":
        return [{"type": "text", "text": "The agentdeck tools aren't available, so I stopped."}]
    return [{"type": "tool_use", "id": f"toolu_ts{stamp}", "name": "ToolSearch", "input": {"query": f"select:{DELEGATE},{WAIT}", "max_results": 2}}]

def plan(body):
    blocks = last_user_blocks(body)
    m = manager(body, blocks, {t.get("name") for t in body.get("tools") or []})
    if m is not None:
        return m
    tools = {t.get("name") for t in body.get("tools") or []}
    if any(b.get("type") == "tool_result" for b in blocks) and last_tool_use_name(body) == "Write" and "/plans/" in json.dumps(body.get("messages", [])[-2:-1]):
        return next_plan_step(body, tools)
    if any(b.get("type") == "tool_result" for b in blocks) and last_tool_use_name(body) == "ToolSearch":
        return exit_plan()
    if any(b.get("type") == "tool_result" for b in blocks) and last_tool_use_name(body) == "ExitPlanMode":
        res = [b for b in blocks if b.get("type") == "tool_result"][-1]
        return [{"type": "text", "text": "OK, I'll keep planning." if res.get("is_error") else "Plan approved. Starting on step 1."}]
    if any(b.get("type") == "tool_result" for b in blocks) and last_tool_use_name(body) == "AskUserQuestion":
        res = [b for b in blocks if b.get("type") == "tool_result"][-1]
        c = res.get("content")
        got = c if isinstance(c, str) else " ".join(x.get("text", "") for x in (c or []) if isinstance(x, dict))
        return [{"type": "text", "text": ("Skipped: " if res.get("is_error") else "Got your answer: ") + got}]
    if any(b.get("type") == "tool_result" for b in blocks) and last_tool_use_name(body) == "ToolSearch" and "askuserquestion" in json.dumps(body.get("messages", [])[-2:-1]).lower():
        return [ask_question()]
    if any(b.get("type") == "tool_result" for b in blocks):
        res = [b for b in blocks if b.get("type") == "tool_result"][-1]
        denied = bool(res.get("is_error"))
        return [{"type": "text", "text": "The tool was not allowed, so I stopped." if denied else "Done. The command printed agentdeck-ok and the file is in place."}]
    images = [b for b in blocks if b.get("type") == "image"]
    if images:
        return [{"type": "text", "text": f"I can see {len(images)} image ({images[0]['source']['media_type']})."}]
    text = " ".join(b.get("text", "") for b in blocks if b.get("type") == "text").lower()
    if os.environ.get("MOCK_DUMP"):
        with open(os.environ["MOCK_DUMP"], "w") as f:
            json.dump(body, f)
    if os.environ.get("MOCK_DEBUG"):
        print("tools:", sorted(n for n in tools if n), "| text:", text[-200:], flush=True)
    if "made changes after your review" in text:
        return [{"type": "text", "text": "Checked the new test: it covers a zero discount and passes.\n\nAPPROVED"}]
    if "you're the reviewer in an agentdeck workflow" in text:
        return [{"type": "text", "text": "The discount is applied before tax now, but one case is missing:\n\n1. Add a test for a zero discount."}]
    if "agentdeck relay" in text or "(round " in text:
        if "(round 2" in text:
            return [{"type": "text", "text": "Reviewed Codex's change: the discount is applied before tax and the new test covers it.\n\nDONE"}]
        return [{"type": "text", "text": "Plan: apply the discount before tax in invoice.py and add a test for it. Codex, please implement that and tell me what you changed."}]
    if "plan" in text.split("</system-reminder>")[-1]:
        path = plan_file(body)
        if path:
            return [{"type": "text", "text": "I've read the code. Writing the plan down."}, {"type": "tool_use", "id": f"toolu_f{int(time.time()*1000)}", "name": "Write", "input": {"file_path": path, "content": PLAN_TEXT}}]
        return next_plan_step(body, tools)
    if "ask me" in text:
        if "AskUserQuestion" in tools:
            return [ask_question()]
        return [{"type": "tool_use", "id": f"toolu_s{int(time.time()*1000)}", "name": "ToolSearch", "input": {"query": "select:AskUserQuestion", "max_results": 1}}]
    if "slow" in text:
        return [{"type": "text", "text": "Thinking slowly… " * 40}], 0.25
    if "write" in text:
        return [{"type": "text", "text": "I'll create the file."}, {"type": "tool_use", "id": f"toolu_w{int(time.time()*1000)}", "name": "Write", "input": {"file_path": "hello.txt", "content": "hello from agentdeck\n"}}]
    if "run" in text:
        return [{"type": "text", "text": "I'll run a quick check."}, {"type": "tool_use", "id": f"toolu_b{int(time.time()*1000)}", "name": "Bash", "input": {"command": "echo agentdeck-ok", "description": "Print a marker"}}]
    return [{"type": "text", "text": "Hello from the mock model."}]

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _json(self, code, obj):
        data = json.dumps(obj).encode()
        self.send_response(code); self.send_header("content-type", "application/json"); self.send_header("content-length", str(len(data))); self.end_headers(); self.wfile.write(data)

    def do_GET(self):
        self._json(200, {"data": [], "has_more": False})

    def do_POST(self):
        n = int(self.headers.get("content-length") or 0)
        body = json.loads(self.rfile.read(n) or b"{}")
        LOG.append(self.path)
        if "count_tokens" in self.path:
            return self._json(200, {"input_tokens": 10})
        if not self.path.startswith("/v1/messages"):
            return self._json(200, {})
        p = plan(body)
        delay = 0.0
        if isinstance(p, tuple): p, delay = p
        model = body.get("model", "mock-model")
        usage = {"input_tokens": 10, "output_tokens": 10, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}
        stop = "tool_use" if any(b["type"] == "tool_use" for b in p) else "end_turn"
        msg_id = f"msg_{int(time.time()*1e6)}"
        if not body.get("stream"):
            return self._json(200, {"id": msg_id, "type": "message", "role": "assistant", "model": model, "content": p, "stop_reason": stop, "stop_sequence": None, "usage": usage})
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.send_header("cache-control", "no-cache"); self.end_headers()
        def ev(name, data):
            self.wfile.write(f"event: {name}\ndata: {json.dumps(data)}\n\n".encode()); self.wfile.flush()
        ev("message_start", {"type": "message_start", "message": {"id": msg_id, "type": "message", "role": "assistant", "model": model, "content": [], "stop_reason": None, "stop_sequence": None, "usage": usage}})
        for i, b in enumerate(p):
            if b["type"] == "text":
                ev("content_block_start", {"type": "content_block_start", "index": i, "content_block": {"type": "text", "text": ""}})
                words = b["text"].split(" ")
                for j, w in enumerate(words):
                    ev("content_block_delta", {"type": "content_block_delta", "index": i, "delta": {"type": "text_delta", "text": w + (" " if j < len(words) - 1 else "")}})
                    if delay: time.sleep(delay)
            else:
                ev("content_block_start", {"type": "content_block_start", "index": i, "content_block": {"type": "tool_use", "id": b["id"], "name": b["name"], "input": {}}})
                ev("content_block_delta", {"type": "content_block_delta", "index": i, "delta": {"type": "input_json_delta", "partial_json": json.dumps(b["input"])}})
            ev("content_block_stop", {"type": "content_block_stop", "index": i})
        ev("message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None}, "usage": {"output_tokens": 10}})
        ev("message_stop", {"type": "message_stop"})

if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
