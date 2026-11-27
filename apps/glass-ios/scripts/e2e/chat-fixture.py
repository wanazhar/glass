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
    {"session": "stream-review", "title": "Summarize the build"},
    {"session": "reasoning", "title": "Review the Swift build", "model": "Swift workspace"},
    {"session": "table", "title": "Build capabilities", "model": "Swift workspace"},
    {"session": "long-reasoning", "title": "Long reasoning", "model": "Swift workspace"},
    {"session": "deep-reasoning", "title": "Reasoning after long history", "model": "Swift workspace"},
    {"session": "scrolled-reasoning", "title": "Earlier reasoning after long history", "model": "Swift workspace"},
    {"session": "paged-reasoning", "title": "Paged reasoning", "model": "Swift workspace"},
    {"session": "delegation", "title": "Review alert delivery", "model": "gpt-5.4", "running_process_verified": True},
]

AGENTS = [
    {"id": "delivery", "name": "Delivery", "status": "working", "activity": "**Editing alert_router.py**",
     "task": "Trace why urgent alerts sometimes arrive twice."},
    {"id": "tests", "name": "Tests", "status": "idle", "terminal_events": 1,
     "task": "Review delivery coverage.", "result": "All 18 delivery tests pass."},
    {"id": "replay", "name": "Replay", "status": "blocked", "terminal_events": 1,
     "task": "Reproduce the retry path offline.", "result": "The replay fixture is missing."},
]
AGENT_START = time.time() - 121
STREAM_REPLY = (
    "The device build and the simulator build both use the same Swift sources, but they target different platforms. "
    "Keep their outputs separate so an unsigned device archive never picks up a simulator binary.\n\n"
    "The Release bundle contains the app, its launch screen and the widget extension. The simulator runs provide "
    "a separate check that the layout and message rendering match in both appearances.\n\n"
    "For the final review, inspect the archive itself for a signing directory and a provisioning profile. "
    "Neither belongs in an unsigned package. Verify the build number from the bundle rather than from its filename."
)

DELEGATION = [
    {"k": "user", "t": "Review alert delivery in parallel. Have one agent trace the router, one check test coverage, and one replay the retry path."},
    {"k": "call", "id": "launch-review", "name": "run_subagents", "args": "{}"},
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
            if thread == "delegation":
                self.history(DELEGATION)
                return
            if thread == "stream-review":
                self.history([
                    {"k": "user", "t": "What should we capture before sharing the release?"},
                    {"k": "assistant", "t": "Capture the conversation and sidebar in light and dark. Keep each "
                     "simulator at its native size and check that message text stays legible when it scrolls "
                     "under the toolbar. Take an additional screenshot with the composer visible."},
                    {"k": "user", "t": "How should we label the captures?"},
                    {"k": "assistant", "t": "Name each capture by device, appearance, and the state it shows. "
                     "Record the build number alongside the images so they can be matched to the release archive."},
                    {"k": "user", "t": "Which builds are ready for release review?"},
                    {"k": "assistant", "t": REASONING + "\n\n" + TABLE + "\n\n"
                     "For a clean comparison, use the same source revision on both targets. Install the simulator "
                     "app on iPhone and iPad, and confirm light and dark appearance before collecting screenshots. "
                     "Review the app bundle from the device build separately, then record its version, build "
                     "number, and archive checksum so the release notes point to the exact artifact."},
                    {"k": "user", "t": "Summarize your verification plan."},
                ])
                return
            if thread == "paged-reasoning":
                if parse_qs(url.query).get("before") == ["1000"]:
                    self.reply({"records": [
                        {"index": 1, "record": {"k": "user", "t": "Read the whole reasoning."}},
                        {"index": 2, "record": {"k": "reasoning", "t": ("Earlier reasoning. " + REASONING + "\n\n") * 2}},
                    ], "before": 0})
                else:
                    self.reply({"records": [
                        {"index": 1000, "record": {"k": "reasoning", "t": "Already visible reasoning.\n\n" + REASONING}},
                        {"index": 2000, "record": {"k": "assistant", "t": "End of this conversation."}},
                    ], "before": 1000})
                return
            if thread == "table":
                records = [{"k": "user", "t": "What can we verify in the build?"}, {"k": "assistant", "t": TABLE}]
            elif thread == "long-reasoning":
                records = [{"k": "reasoning", "t": "Beginning of the long reasoning.\n\n" + (REASONING + "\n\n") * 6}]
            elif thread in {"deep-reasoning", "scrolled-reasoning"}:
                records = [
                    {"k": "user" if i % 2 == 0 else "assistant",
                     "t": f"History message {i}. " + (REASONING + "\n\n") * (i % 7)}
                    for i in range(60)
                ]
                records += [
                    {"k": "reasoning", "t": "Beginning of the earlier block.\n\n" + (REASONING + "\n\n") * 6},
                    {"k": "assistant", "t": "Separate the two reasoning segments.\n\n"
                     + (REASONING + "\n\n") * (4 if thread == "deep-reasoning" else 1)},
                    {"k": "reasoning", "t": "Beginning of the latest block.\n\n" + (REASONING + "\n\n") * 6},
                ]
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
        elif url.path == "/api/agents":
            self.reply([{"parent_call_id": "launch-review", "session": agent["id"], "model": "gpt-5.4",
                         "started_at": AGENT_START, "updated": AGENT_START + 101, **agent}
                        for agent in AGENTS] if thread == "delegation" else [])
        elif url.path == "/api/agent":
            id = parse_qs(url.query).get("agent", [""])[0]
            agent = next((agent for agent in AGENTS if agent["id"] == id), None)
            if agent is None:
                self.history([])
                return
            self.history([
                {"k": "user", "t": agent["task"]},
                {"k": "assistant", "t": agent.get("result", "The retry timer can enqueue the same alert twice. I found the two paths that can both claim the same delivery key.")},
            ])
        elif url.path == "/api/agent-events":
            self.agent_events()
        elif url.path == "/api/context":
            self.reply({"attachments": []})
        else:
            self.reply({})

    def do_POST(self):
        data = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if urlsplit(self.path).path in {"/api/agent/stop", "/api/agent/send"}:
            agent = next((agent for agent in AGENTS if agent["id"] == data.get("agent")), None)
            if agent:
                agent.update(status="parked" if self.path.endswith("/stop") else "working",
                             activity="Paused" if self.path.endswith("/stop") else "Retrying assignment")
        self.reply({"session": data.get("id", "")})

    def history(self, records):
        self.reply({"before": 0, "records": [{"index": i, "record": record} for i, record in enumerate(records)]})

    def agent_events(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        try:
            self.wfile.write(b'data: {"revision":1}\n\n')
            while True:
                self.wfile.flush()
                time.sleep(2)
                self.wfile.write(b": heartbeat\n\n")
        except (BrokenPipeError, ConnectionResetError):
            pass

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
            send(1, {"reset": True, "cursor": 1, "active": active or thread in {"long-reasoning", "deep-reasoning", "stream-review"},
                     "control": {"can_write": True}, "live": live})
            if active:
                for i, start in enumerate(range(39, len(REASONING), 13), start=2):
                    event = {"t": "reasoning", "delta": REASONING[start:start + 13]}
                    send(i, {"cursor": i, "events": [event]})
                    time.sleep(0.08)
            if thread == "stream-review":
                for i, start in enumerate(range(0, len(STREAM_REPLY), 19), start=2):
                    send(i, {"cursor": i, "events": [{"t": "text", "delta": STREAM_REPLY[start:start + 19]}]})
                    time.sleep(0.20 if i % 5 else 0.65)
                send(i + 1, {"cursor": i + 1, "events": [{"t": "done"}]})
            while True:
                self.wfile.write(b": keepalive\n\n")
                self.wfile.flush()
                time.sleep(2)
        except (BrokenPipeError, ConnectionResetError):
            pass


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 51998), Handler).serve_forever()
