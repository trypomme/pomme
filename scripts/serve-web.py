#!/usr/bin/env python3
"""Serve web/ on 127.0.0.1. The page needs no special headers (any static file
server does); this one sends .wasm as application/wasm and asks browsers not to
cache, so a rebuild shows at once.

Usage: scripts/serve-web.py [port]   (default 8787)
"""
import functools
import http.server
import os
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web")


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript"}

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8787
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), functools.partial(Handler, directory=ROOT))
    print(f"pomme: http://127.0.0.1:{port}/")
    server.serve_forever()
