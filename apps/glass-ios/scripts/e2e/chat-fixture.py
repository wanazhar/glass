#!/usr/bin/env python3
"""Local chat rendering fixture for ScreenshotTourUITests.

Run this server and set TEST_RUNNER_GLASS_CHAT_FIXTURE=1 for xcodebuild test.
"""

import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit


REASONING = (
    "I’ll compare the two build paths, starting with the Swift toolchain and the matching SDK. "
    "The simulator and device builds use different destinations, so I’ll keep their results separate. "
    "Next I’ll check that the app and its widget are both in the release payload. "
    "For the table, I’ll keep the capability names short and use the status column for the details. "
    "Long descriptions should wrap without changing the width of their column. "
    "Finally I’ll verify that the chat remains readable in light and dark, including while scrolling. "
    "The final check is whether the entire reasoning segment can be read without opening a second screen."
)
TABLE = """The build has four capabilities to review:

| Capability | Status | Verification | Platform |
| --- | --- | --- | --- |
| Compile Swift and link the app | The compiler and linker finish without needing a signing identity. | Build log | iPhone |
| Embed the widget extension | The extension remains in the app’s PlugIns folder. | Bundle inspection | iPhone |
| Check the application layout | The long descriptions wrap in their own column. | Simulator screenshot | iOS |
| Package the unsigned app | The app bundle is placed inside the Payload directory. | Archive inspection | arm64 |

The table scrolls sideways to show Verification and Platform."""

THREADS = [
    {"session": "reasoning", "title": "Review the Swift build", "model": "Swift workspace"},
    {"session": "table", "title": "Build capabilities", "model": "Swift workspace"},
]


class Handler(BaseHTTPRequestHandler):
    def reply(self, value):
        data = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        url = urlsplit(self.path)
        thread = parse_qs(url.query).get("id", [""])[0]
        if url.path == "/api/status":
            self.reply({"workspace": "/swift-workspace"})
        elif url.path == "/api/threads":
            self.reply({"threads": THREADS, "offset": 0, "total_threads": len(THREADS)})
        elif url.path == "/api/history":
            if thread == "table":
                records = [{"k": "user", "t": "What can we verify in the build?"}, {"k": "assistant", "t": TABLE}]
            else:
                records = [
                    {"k": "user", "t": "Review the build and package the app."},
                    {"k": "assistant", "t": (
                        "I’ll check the target, SDK, and app bundle first.\n\n"
                        "The device build uses the iPhoneOS SDK. Its app bundle contains the executable, "
                        "storyboards, resources, and the embedded widget extension.\n\n"
                        "The Simulator build uses the same project but links against the Simulator SDK. "
                        "It can run the app’s unit and interface tests on an iPhone without a device profile.\n\n"
                        "I’ll package the device build separately. An unsigned archive contains the Payload "
                        "directory and the app bundle, without a provisioning profile or signing identity.\n\n"
                        "The last step is to open a conversation and check that messages, controls, "
                        "and Markdown render correctly in both appearances."
                    )},
                    {"k": "user", "t": "Please also check the result in the Simulator."},
                    {"k": "reasoning", "t": REASONING[:26]},
                ]
            self.reply({"records": [{"index": i, "record": record} for i, record in enumerate(records)], "before": 0})
        elif url.path == "/api/events":
            self.events(thread)
        elif url.path == "/api/context":
            self.reply({"attachments": []})
        else:
            self.reply({})

    def do_POST(self):
        data = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.reply({"session": data.get("id", "")})

    def events(self, thread):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()

        def send(cursor, payload):
            self.wfile.write(f"id: {cursor}\ndata: {json.dumps(payload)}\n\n".encode())
            self.wfile.flush()

        try:
            active = thread == "reasoning"
            live = [{"t": "reasoning", "delta": REASONING[:39]}] if active else []
            send(1, {"reset": True, "cursor": 1, "active": active, "control": {"can_write": True}, "live": live})
            if active:
                for i, start in enumerate(range(39, len(REASONING), 13), start=2):
                    event = {"t": "reasoning", "delta": REASONING[start:start + 13]}
                    send(i, {"cursor": i, "events": [event]})
                    time.sleep(0.08)
            while True:
                self.wfile.write(b": keepalive\n\n")
                self.wfile.flush()
                time.sleep(2)
        except (BrokenPipeError, ConnectionResetError):
            pass


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 51998), Handler).serve_forever()
