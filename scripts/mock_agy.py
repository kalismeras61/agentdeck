#!/usr/bin/env python3
"""A stand-in for Google's `agy` in headless stream-json mode, for AgentDeck's tests.

Follows https://antigravity.google/docs/cli/headless/: `--input-format stream-json
--output-format stream-json`, one `{"event":"user","message":{"content":...}}` per line on
stdin, `init` once, then `step_update` lines and one `result` per turn. Prompts:
  - "write" : edits hello.txt with a write_to_file tool step
  - "delete": the action needs approval, so it's skipped and reported in denied_actions
  - "fail"  : the turn ends with status ERROR
  - anything else: a short answer
`agy models --output-format json` lists two models.
"""
import json, os, sys, uuid

args = sys.argv[1:]
if args[:1] == ["models"]:
    print(json.dumps({"models": [{"slug": "gemini-3.8-flash-medium", "label": "Gemini 3.8 Flash"}, {"slug": "gemini-3.1-pro-high", "label": "Gemini 3.1 Pro"}]}))
    sys.exit(0)

def flag(name):
    return args[args.index(name) + 1] if name in args else None

if flag("--input-format") != "stream-json" or flag("--output-format") != "stream-json":
    print("error: this stand-in only speaks stream-json", file=sys.stderr)
    sys.exit(2)

conv = flag("--conversation") or str(uuid.uuid4())
mode = flag("--mode") or "default"
model = flag("--model") or "gemini-3.8-flash-medium"
turns = 0

def out(ev, **payload):
    line = {"event": ev, ev: payload}
    if ev == "init":
        line["conversation_id"] = conv
    sys.stdout.write(json.dumps(line) + "\n")
    sys.stdout.flush()

def step(i, ty, state, **extra):
    out("step_update", conversation_id=conv, step_index=i, state=state, step_type=ty, **extra)

out("init", cwd=os.getcwd(), tools=["view_file", "write_to_file", "run_command"], permission_mode="request-review", model=model)

for line in sys.stdin:
    if not line.strip():
        continue
    msg = json.loads(line)
    if msg.get("event") != "user":
        sys.exit(1)
    text = msg["message"]["content"]
    turns += 1
    step(0, "user_input", "DONE")
    denied, status, answer = [], "SUCCESS", ""
    if "fail" in text:
        status = "ERROR"
    elif "write" in text and mode != "plan":
        step(1, "agent_response", "ACTIVE", text_delta="I'll write the file.")
        step(1, "agent_response", "DONE")
        params = {"TargetFile": os.path.join(os.getcwd(), "hello.txt"), "CodeContent": "hello from antigravity\n"}
        step(2, "tool", "ACTIVE", tool_name="write_to_file", tool_info={"name": "write_to_file", "parameters": params})
        with open(params["TargetFile"], "w") as f:
            f.write(params["CodeContent"])
        step(2, "tool", "DONE", tool_name="write_to_file", duration_seconds=0.1, tool_info={"name": "write_to_file", "parameters": params, "output": "Created hello.txt"})
        answer = "Done: hello.txt is in place."
    elif "delete" in text:
        denied = [{"action": "command"}]
        print("notice: skipped run_command (rm -rf build): it needs approval. To allow it, add `command(rm -rf build)` to permissions.allow in ~/.gemini/antigravity-cli/settings.json", file=sys.stderr, flush=True)
        answer = "I couldn't delete the build folder: that needs your approval."
    else:
        answer = f"Hello from Antigravity ({model}, {mode} mode)."
    if answer:
        for part in (answer[: len(answer) // 2], answer[len(answer) // 2 :]):
            step(9, "agent_response", "ACTIVE", text_delta=part)
        step(9, "agent_response", "DONE", usage={"input_tokens": 10, "output_tokens": 5})
    result = {"conversation_id": conv, "status": status, "response": answer, "duration_seconds": 0.2, "num_turns": turns,
              "usage": {"input_tokens": 10 * turns, "output_tokens": 5 * turns, "total_tokens": 15 * turns}}
    if denied:
        result["denied_actions"] = denied
    if status == "ERROR":
        result["error"] = "model request failed: quota exhausted"
        print('AGY_ERROR: {"status":"RESOURCE_EXHAUSTED","message":"quota exhausted"}', file=sys.stderr)
    out("result", **result)
