#!/usr/bin/env python3
"""Coggy+Hyle Dashboard API Server
Serves the dashboard and proxies OpenRouter calls with logging.
"""
import http.server
import json
import os
import sys
import time
import threading
import urllib.request
import urllib.error
from datetime import datetime
from pathlib import Path

LOG_DIR = Path(__file__).parent.parent / "logs"
LOG_DIR.mkdir(exist_ok=True)
LOG_FILE = LOG_DIR / "openrouter.jsonl"

# In-memory state
state = {
    "logs": [],
    "traces": [],
    "atoms": [],
    "focus": [],
    "tikkun": [],
    "stats": {"total_calls": 0, "total_tokens": 0, "errors": 0}
}

OPENROUTER_KEY = os.environ.get("OPENROUTER_API_KEY", "")
FREE_MODELS = [
    "mistralai/mistral-7b-instruct:free",
    "meta-llama/llama-3.2-3b-instruct:free",
    "google/gemma-2-9b-it:free",
    "qwen/qwen-2-7b-instruct:free",
]

def log_call(entry):
    """Append to JSONL log and in-memory state"""
    state["logs"].insert(0, entry)
    state["logs"] = state["logs"][:200]  # keep last 200
    state["stats"]["total_calls"] += 1
    state["stats"]["total_tokens"] += entry.get("tokens_in", 0) + entry.get("tokens_out", 0)
    if not entry.get("ok"):
        state["stats"]["errors"] += 1

    with open(LOG_FILE, "a") as f:
        f.write(json.dumps(entry) + "\n")

def call_openrouter(prompt, model=None, max_tokens=512):
    """Call OpenRouter and log the interaction"""
    if not OPENROUTER_KEY:
        return {"error": "No OPENROUTER_API_KEY set", "response": ""}

    model = model or FREE_MODELS[state["stats"]["total_calls"] % len(FREE_MODELS)]
    start = time.time()
    tokens_in = len(prompt) // 4

    body = json.dumps({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": 0.7,
    }).encode()

    req = urllib.request.Request(
        "https://openrouter.ai/api/v1/chat/completions",
        data=body,
        headers={
            "Authorization": f"Bearer {OPENROUTER_KEY}",
            "Content-Type": "application/json",
            "HTTP-Referer": "https://hyle.lol",
            "X-Title": "Coggy+Hyle Dashboard",
        }
    )

    entry = {
        "time": datetime.now().strftime("%H:%M:%S"),
        "model": model,
        "prompt_preview": prompt[:100],
        "tokens_in": tokens_in,
    }

    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read())
            content = data.get("choices", [{}])[0].get("message", {}).get("content", "")
            usage = data.get("usage", {})
            tokens_out = usage.get("completion_tokens", len(content) // 4)

            entry.update({
                "ok": True,
                "tokens_out": tokens_out,
                "latency": int((time.time() - start) * 1000),
                "response_preview": content[:200],
                "error": None,
            })
            log_call(entry)
            return {"response": content, "model": model, "tokens": tokens_out}

    except urllib.error.HTTPError as e:
        error_body = e.read().decode() if e.fp else str(e)
        entry.update({
            "ok": False,
            "tokens_out": 0,
            "latency": int((time.time() - start) * 1000),
            "error": f"{e.code}: {error_body[:200]}",
        })
        log_call(entry)
        return {"error": entry["error"], "response": ""}

    except Exception as e:
        entry.update({
            "ok": False,
            "tokens_out": 0,
            "latency": int((time.time() - start) * 1000),
            "error": str(e)[:200],
        })
        log_call(entry)
        return {"error": str(e), "response": ""}

def load_logs():
    """Load existing logs from JSONL file"""
    if LOG_FILE.exists():
        with open(LOG_FILE) as f:
            for line in f:
                try:
                    entry = json.loads(line.strip())
                    state["logs"].append(entry)
                except:
                    pass
        state["logs"] = state["logs"][-200:]
        state["logs"].reverse()

class DashboardHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(Path(__file__).parent), **kwargs)

    def do_GET(self):
        if self.path == '/api/state':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Access-Control-Allow-Origin', '*')
            self.end_headers()
            self.wfile.write(json.dumps(state).encode())
        elif self.path == '/api/stats':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Access-Control-Allow-Origin', '*')
            self.end_headers()
            self.wfile.write(json.dumps(state["stats"]).encode())
        elif self.path == '/api/logs':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Access-Control-Allow-Origin', '*')
            self.end_headers()
            self.wfile.write(json.dumps(state["logs"]).encode())
        else:
            super().do_GET()

    def do_POST(self):
        if self.path == '/api/call':
            length = int(self.headers.get('Content-Length', 0))
            body = json.loads(self.rfile.read(length)) if length else {}
            result = call_openrouter(
                body.get("prompt", "hello"),
                body.get("model"),
                body.get("max_tokens", 512),
            )
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Access-Control-Allow-Origin', '*')
            self.end_headers()
            self.wfile.write(json.dumps(result).encode())

        elif self.path == '/api/trace':
            length = int(self.headers.get('Content-Length', 0))
            trace = json.loads(self.rfile.read(length)) if length else {}
            state["traces"].insert(0, trace)
            state["traces"] = state["traces"][:100]
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(b'{"ok":true}')

        elif self.path == '/api/atoms':
            length = int(self.headers.get('Content-Length', 0))
            data = json.loads(self.rfile.read(length)) if length else {}
            if "atoms" in data: state["atoms"] = data["atoms"]
            if "focus" in data: state["focus"] = data["focus"]
            if "tikkun" in data: state["tikkun"] = data["tikkun"]
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(b'{"ok":true}')
        else:
            self.send_response(404)
            self.end_headers()

    def do_OPTIONS(self):
        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        self.end_headers()

    def log_message(self, format, *args):
        pass  # silence logs

def boot_test():
    """Make a test call to OpenRouter on startup"""
    if OPENROUTER_KEY:
        print("[boot] Testing OpenRouter connection...")
        result = call_openrouter(
            "You are Coggy, a cognitive architecture. Say hello in exactly one sentence.",
            model=FREE_MODELS[0],
            max_tokens=64,
        )
        if result.get("response"):
            print(f"[boot] OpenRouter OK: {result['response'][:80]}")
        else:
            print(f"[boot] OpenRouter error: {result.get('error', 'unknown')}")
    else:
        print("[boot] No OPENROUTER_API_KEY — logging only, no live calls")

if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
    load_logs()
    print(f"[dashboard] Serving on http://0.0.0.0:{port}")
    print(f"[dashboard] Logs: {LOG_FILE}")
    print(f"[dashboard] API key: {'set' if OPENROUTER_KEY else 'NOT SET'}")

    # Boot test in background
    threading.Thread(target=boot_test, daemon=True).start()

    server = http.server.HTTPServer(('0.0.0.0', port), DashboardHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n[dashboard] Shutting down")
