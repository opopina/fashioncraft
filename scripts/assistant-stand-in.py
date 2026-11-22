#!/usr/bin/env python3
"""A stand-in for Claude's Messages API and an OpenAI-compatible API, to see and test the
Assistant (Window > Assistant) without an API key or cost. It answers with a scripted turn (draft
a denim skirt, fabric, colour, simulate, screenshot, a closing message) and logs checks on what
FashionCraft sends.

    python3 scripts/assistant-stand-in.py 18765 /tmp/stand-in.log
    # preferences.json: "ai": {"provider": "anthropic", "base_url": "http://127.0.0.1:18765/v1"}
    ANTHROPIC_API_KEY=test-key FASHIONCRAFT_ASSISTANT_SEND="Draft a denim skirt" \
        cargo run --bin fashioncraft

FASHIONCRAFT_ASSISTANT_SEND sends a first message on its own in development builds only. With
"provider": "openai" and "model": "any", the /chat/completions answer colours the garment red.
Restart the stand-in for each run: it counts requests."""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
port, log = int(sys.argv[1]), sys.argv[2]
state = {"anthropic": 0, "openai": 0}
def note(*a):
    with open(log, "a") as f: print(*a, file=f, flush=True)
ANTHROPIC = [
  [{"type":"thinking","thinking":"plan the skirt","signature":"sig-1"},
   {"type":"text","text":"I'll look at the scene, then draft a **denim skirt** from the avatar."},
   {"type":"tool_use","id":"tu_1","name":"scene_summary","input":{}},
   {"type":"tool_use","id":"tu_2","name":"draft_garment","input":{"kind":"skirt","from_avatar":True}}],
  [{"type":"tool_use","id":"tu_3","name":"set_fabric","input":{"preset":"denim"}},
   {"type":"tool_use","id":"tu_4","name":"set_color","input":{"rgb":[40,60,120]}},
   {"type":"tool_use","id":"tu_5","name":"simulate","input":{"seconds":5}}],
  [{"type":"tool_use","id":"tu_6","name":"screenshot","input":{}}],
  [{"type":"text","text":"Done:\n- a denim skirt drafted from the avatar\n- simulated 5 s; it hangs from the waist to the knee."}],
]
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["content-length"])))
        if self.path.endswith("/messages"):
            i = state["anthropic"]; state["anthropic"] += 1
            msgs = body["messages"]
            checks = {
              "key": self.headers.get("x-api-key") == "test-key",
              "version": self.headers.get("anthropic-version") == "2026-10-21",
              "beta": self.headers.get("anthropic-beta") == "server-side-fallback-2026-07-01",
              "fallbacks": body.get("fallbacks") == "default",
              "effort": body.get("output_config") == {"effort": "medium"},
              "tools": len(body["tools"]) == 10,
              "alternates": all(m["role"] == ("user" if k % 2 == 0 else "assistant") for k, m in enumerate(msgs)),
            }
            if i > 0:
                checks["thinking_back"] = msgs[1]["content"][0] == ANTHROPIC[0][0]
                results = msgs[-1]["content"]
                checks["results"] = all(r["type"] == "tool_result" for r in results)
                note(f"anthropic {i} results:", [(r["tool_use_id"], r["is_error"], [c["type"] for c in r["content"]], (r["content"][0].get("text") or "")[:120]) for r in results])
            note(f"anthropic {i} checks:", checks)
            if i < len(ANTHROPIC):
                content = ANTHROPIC[i]
                stop = "tool_use" if any(b["type"] == "tool_use" for b in content) else "end_turn"
                self.reply(200, {"id": f"msg_{i}", "type": "message", "role": "assistant", "content": content, "stop_reason": stop, "usage": {"input_tokens": 1, "output_tokens": 1}})
            else:
                self.reply(529, {"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}})
        elif self.path.endswith("/chat/completions"):
            i = state["openai"]; state["openai"] += 1
            note(f"openai {i} auth={self.headers.get('authorization')} roles={[m['role'] for m in body['messages']]}")
            if i == 0:
                msg = {"role":"assistant","content":None,"tool_calls":[{"id":"c1","type":"function","function":{"name":"set_color","arguments":"{\"rgb\":[200,30,40]}"}}]}
                self.reply(200, {"choices":[{"message":msg,"finish_reason":"tool_calls"}]})
            else:
                self.reply(200, {"choices":[{"message":{"role":"assistant","content":"Coloured it red."},"finish_reason":"stop"}]})
        else:
            self.reply(404, {"error": {"message": "no such path"}})
    def reply(self, code, obj):
        data = json.dumps(obj).encode()
        self.send_response(code); self.send_header("content-type", "application/json"); self.send_header("content-length", str(len(data))); self.end_headers(); self.wfile.write(data)
HTTPServer(("127.0.0.1", port), H).serve_forever()
