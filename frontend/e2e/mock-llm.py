"""Provider OpenAI-compatible tiruan untuk E2E lokal (P5). Tanpa dependensi; hanya 127.0.0.1.

Chat: bila tool ditawarkan dan belum ada hasil tool → minta get_business_hours; lalu jawab teks.
Embeddings: vektor tetap 4 dimensi.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
        if self.path.endswith("/embeddings"):
            out = {"data": [{"index": i, "embedding": [1.0, 0.0, 0.0, 0.0]} for i, _ in enumerate(body.get("input", []))]}
        else:
            msgs = body.get("messages", [])
            tool = [m for m in msgs if m.get("role") == "tool"]
            if body.get("tools") and not tool:
                msg = {"content": None, "tool_calls": [{"id": "c1", "type": "function",
                       "function": {"name": "get_business_hours", "arguments": "{}"}}]}
            else:
                msg = {"content": "Halo dari asisten uji. Data jam buka diterima." if tool else "Halo dari asisten uji."}
            out = {"model": body.get("model"), "choices": [{"message": msg}], "usage": {"prompt_tokens": 120, "completion_tokens": 12}}
        data = json.dumps(out).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


HTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 18080), H).serve_forever()
