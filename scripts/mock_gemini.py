"""A tiny stand-in for the Gemini API, for integration tests only.

The real `gemini` CLI is pointed at it with GOOGLE_GEMINI_BASE_URL. It answers:
- a prompt containing "write" with a write_file call (hello.txt),
- a prompt containing "run" with a run_shell_command call (`echo agentdeck-ok`),
- a turn whose last part is a function response with a short text reply,
- a relay prompt with an "implemented it" reply,
- anything else with plain text.
Requests are appended to /tmp/mock_gemini_requests.jsonl.
"""
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8789


def last_user_parts(body):
    for c in reversed(body.get("contents") or []):
        if c.get("role") in (None, "user"):
            return c.get("parts") or []
    return []


def tool_names(body):
    names = set()
    for t in body.get("tools") or []:
        for f in t.get("functionDeclarations") or t.get("function_declarations") or []:
            names.add(f.get("name"))
    return names


def plan(body):
    parts = last_user_parts(body)
    responses = [p["functionResponse"] for p in parts if "functionResponse" in p]
    if responses:
        text = json.dumps(responses[-1].get("response", {})).lower()
        denied = any(w in text for w in ("cancel", "denied", "rejected", "declined", "not approved"))
        return [{"text": "The action was not approved, so I stopped." if denied else "Done. The command printed agentdeck-ok and the file is in place."}]
    text = " ".join(p.get("text", "") for p in parts).lower()
    names = tool_names(body)
    if "agentdeck relay" in text or "(round " in text:
        return [{"text": "Implemented it: invoice.py now applies the discount before tax, and test_invoice.py has a case for it. All tests pass. Over to you for review."}]
    if "write" in text and "write_file" in names:
        return [{"text": "I'll create the file."}, {"functionCall": {"name": "write_file", "args": {"file_path": "hello.txt", "content": "hello from agentdeck\n"}}}]
    if "run" in text and "run_shell_command" in names:
        return [{"text": "I'll run a quick check."}, {"functionCall": {"name": "run_shell_command", "args": {"command": "echo agentdeck-ok", "description": "Print a marker"}}}]
    if not names:
        # Helper calls (routing, next-speaker checks, titles): answer with tiny JSON-friendly text.
        return [{"text": "{\"next_speaker\": \"user\", \"reasoning\": \"done\"}"}]
    return [{"text": "Hello from the mock model."}]


def response(parts):
    return {
        "candidates": [{"content": {"role": "model", "parts": parts}, "finishReason": "STOP", "index": 0}],
        "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 10, "totalTokenCount": 20},
        "modelVersion": "mock-gemini",
    }


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
        self._json(200, {"models": []})

    def do_POST(self):
        n = int(self.headers.get("content-length") or 0)
        body = json.loads(self.rfile.read(n) or b"{}")
        with open("/tmp/mock_gemini_requests.jsonl", "a") as f:
            f.write(json.dumps({"path": self.path, "body": body}) + "\n")
        if ":countTokens" in self.path:
            return self._json(200, {"totalTokens": 10})
        if ":embedContent" in self.path or ":batchEmbedContents" in self.path:
            return self._json(200, {"embedding": {"values": [0.0] * 8}})
        # "pretend the account is blocked": the answer Google gives personal accounts now.
        if "pretend the account is blocked" in json.dumps(body):
            return self._json(403, {"error": {"code": 403, "status": "PERMISSION_DENIED", "message": "This client is no longer supported for Gemini Code Assist for individuals. To continue using Gemini, please migrate to the Antigravity suite of products: https://antigravity.google."}})
        parts = plan(body)
        if ":streamGenerateContent" in self.path:
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.end_headers()
            for i, p in enumerate(parts):
                chunk = response([p])
                if i < len(parts) - 1:
                    chunk["candidates"][0].pop("finishReason")
                self.wfile.write(f"data: {json.dumps(chunk)}\r\n\r\n".encode())
                self.wfile.flush()
            return
        return self._json(200, response(parts))


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
