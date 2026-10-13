#!/usr/bin/env python3
"""Minimal webhook sink for the OZ Monitor e2e test.

Appends every POST body as one JSON line to the output file so the test can
assert on received alerts.

Usage: python3 webhook_receiver.py [port] [outfile]
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

OUT = sys.argv[2] if len(sys.argv) > 2 else "alerts.jsonl"


class H(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
        with open(OUT, "a") as f:
            f.write(json.dumps({
                "path": self.path,
                "body": body.decode("utf-8", "replace"),
            }) + "\n")
        self.send_response(200)
        self.end_headers()

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8377
    HTTPServer(("127.0.0.1", port), H).serve_forever()
