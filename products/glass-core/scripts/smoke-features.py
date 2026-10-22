#!/usr/bin/env python3
"""Offline CLI checks against a local streaming endpoint. Run after cargo build."""
import http.server
import codecs
import errno
import fcntl
import struct
import termios
import json
import os
import pty
import re
import select
import time
from pathlib import Path
import subprocess
import tempfile
import sys
import shlex
import signal
import threading
from core_binary import core_binary_path

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", str(core_binary_path('debug'))))


def process_exists(pid):
    """Check process cleanup on Linux and macOS without relying on /proc."""
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True

class Endpoint(http.server.BaseHTTPRequestHandler):
    calls = 0
    mode = "edit"

    def log_message(self, *_):
        pass

    def do_GET(self):
        payload = json.dumps({"data": [{"id": "mock", "name": "Mock model"}, {"id": "mock-2", "name": "Another model"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert body["stream"] is True
        type(self).calls += 1
        n = type(self).calls
        if self.mode == "loop" or n < 3:
            command = "echo loop" if self.mode == "loop" else ("cat ./file" if n == 1 else "echo 'agent edit' > ./file")
            delta = {"tool_calls": [{"index": 0, "id": f"call{n}", "function": {"name": "bash", "arguments": json.dumps({"command": command})}}]}
        else:
            delta = {"content": "Done."}
        frames = [
            {"choices": [{"delta": delta}]},
            {"choices": [], "usage": {"prompt_tokens": 1_000_000, "completion_tokens": 0}},
        ]
        payload = ("".join("data: " + json.dumps(x) + "\n\n" for x in frames) + "data: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


class Screen:
    """Small VT screen projection for prompt visibility regression checks."""
    def __init__(self, width=80, height=24):
        self.width, self.height = width, height
        self.cells = [[" "] * width for _ in range(height)]
        self.x = self.y = 0
        self.pending = ""
        self.saved = None
        self.saved_cursor = None
        self.invalid_positions = []
        self.margin_top, self.margin_bottom = 0, height - 1
        self.history = []

    def feed(self, text):
        self.pending += text
        while self.pending:
            if self.pending.startswith("\x1b]"):
                ends = [(self.pending.find("\x07"), 1), (self.pending.find("\x1b\\"), 2)]
                ends = [(end, length) for end, length in ends if end >= 0]
                if not ends: return
                end, length = min(ends)
                self.pending = self.pending[end + length:]
                continue
            if self.pending.startswith("\x1b["):
                sequence = re.match(r"\x1b\[([0-9;? ]*)([@-~])", self.pending)
                if not sequence:
                    return
                args, kind = sequence.groups()
                self.pending = self.pending[len(sequence.group()):]
                if args == "?1049" and kind == "h":
                    self.saved = (self.cells, self.x, self.y)
                    self.cells = [[" "] * self.width for _ in range(self.height)]
                    self.x = self.y = 0
                elif args == "?1049" and kind == "l" and self.saved:
                    self.cells, self.x, self.y = self.saved
                    self.saved = None
                elif kind == "r":
                    numbers=[int(n or 0) for n in args.split(";")] if args else []
                    self.margin_top=max(0,(numbers[0] if numbers else 1)-1)
                    self.margin_bottom=min(self.height-1,(numbers[1] if len(numbers)>1 else self.height)-1)
                    self.x=self.y=0
                elif kind == "s":
                    self.saved_cursor = (self.x, self.y)
                elif kind == "u" and self.saved_cursor is not None:
                    self.x, self.y = self.saved_cursor
                    self.x, self.y = min(self.width - 1, self.x), min(self.height - 1, self.y)
                elif kind in ("H", "f"):
                    numbers = [int(n or 1) for n in args.split(";")]
                    if numbers[0] > self.height or (len(numbers)>1 and numbers[1]>self.width): self.invalid_positions.append((numbers,self.width,self.height))
                    self.y = min(self.height - 1, max(0, numbers[0] - 1))
                    self.x = min(self.width - 1, max(0, (numbers[1] if len(numbers) > 1 else 1) - 1))
                elif kind == "J" and args == "2":
                    self.cells = [[" "] * self.width for _ in range(self.height)]
                elif kind == "K":
                    if args == "2": self.cells[self.y] = [" "] * self.width
                    else: self.cells[self.y][self.x:] = [" "] * (self.width - self.x)
                continue
            if self.pending == "\x1b":
                return
            char, self.pending = self.pending[0], self.pending[1:]
            if char == "\r": self.x = 0
            elif char == "\n": self.linefeed()
            elif char >= " ":
                if self.x == self.width:
                    self.linefeed()
                    self.x = 0
                self.cells[self.y][self.x] = char
                self.x += 1

    def linefeed(self):
        if self.y == self.margin_bottom:
            if self.margin_top == 0: self.history.append("".join(self.cells[self.margin_top]))
            del self.cells[self.margin_top]
            self.cells.insert(self.margin_bottom,[" "]*self.width)
        else: self.y=min(self.height-1,self.y+1)

    def text(self):
        return "\n".join("".join(row) for row in self.cells)


class Terminal:
    def __init__(self, workspace, env, args=(), size=(80,24)):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", size[1], size[0], 0, 0))
        self.process = subprocess.Popen([str(BINARY), *args], cwd=workspace, env=dict(env, TERM="xterm-256color"), stdin=slave, stdout=slave, stderr=slave)
        os.close(slave)
        self.received = ""
        self.screen = Screen(*size)
        self.decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")

    def read(self):
        try:
            data = os.read(self.master, 65536)
        except OSError as error:
            # EIO means the child closed the slave. During exit the slave
            # closes before the process is reaped, so poll() can still be
            # None here; treat it as end-of-stream either way.
            if error.errno == errno.EIO:
                return
            raise
        if not data:
            return
        text = self.decoder.decode(data)
        self.received += text
        self.screen.feed(text)

    def visible(self, text, timeout=2):
        deadline = time.monotonic() + timeout
        while text not in self.screen.text():
            assert time.monotonic() < deadline, (text, self.screen.text())
            if select.select([self.master], [], [], 0.05)[0]: self.read()

    def idle(self, timeout=10):
        deadline = time.monotonic() + timeout
        while not (self.screen.x == 2 and self.screen.y == max(0,self.screen.height-3) and "".join(self.screen.cells[max(0,self.screen.height-3)]).strip() == "❯"):
            assert time.monotonic() < deadline, self.screen.text()
            if select.select([self.master], [], [], 0.05)[0]: self.read()

    def wait(self, text, timeout=10):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            # The live painter uses cursor-addressed writes between chunks.
            # Strip CSI from only the newly received bytes for text matching;
            # keep protocol waits byte-exact and keep visible() as the actual
            # rendered-screen assertion.
            plain = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", self.received)
            plain = re.sub(r"\s+", " ", plain)
            if text in self.received or ("\x1b" not in text and text in plain):
                result, self.received = self.received, ""
                return result
            if select.select([self.master], [], [], 0.05)[0]:
                self.read()
        raise AssertionError((text, self.received))

    def send(self, command):
        # A new user action starts a fresh response observation window.
        self.received = ""
        os.write(self.master, (command + "\r").encode())

    def dismiss(self):
        self.received = ""
        os.write(self.master, b"\x1b")
        self.wait("\x1b[?25h")
        deadline = time.monotonic() + 5
        while self.screen.y != max(0,self.screen.height-3):
            assert time.monotonic() < deadline, self.screen.text()
            if select.select([self.master], [], [], 0.05)[0]: self.read()

    def resize(self, width, height):
        while select.select([self.master],[],[],0)[0]: self.read()
        self.received = ""
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH",height,width,0,0))
        old = self.screen
        def crop(cells): return [(row[:width]+[" "]*width)[:width] for row in (cells[:height]+[[" "]*width]*height)[:height]]
        old.cells=crop(old.cells)
        if old.saved:
            cells,x,y=old.saved; old.saved=(crop(cells),min(x,width-1),min(y,height-1))
        old.width,old.height=width,height
        old.x,old.y=min(old.x,width-1),min(old.y,height-1)
        os.kill(self.process.pid,signal.SIGWINCH)
        frame = self.wait("\x1b[?2026l" if old.saved else f"\x1b[{max(1,height-2)};1H")
        if not old.saved and "\x1b[?25h" not in frame: self.wait("\x1b[?25h")
        # A resize can queue a second repaint; finish it before the next key.
        while select.select([self.master],[],[],0.05)[0]: self.read()

    def close(self):
        # Cancel active provider/tool work first, then drain terminal output
        # while asking the app to exit so a full PTY cannot block shutdown.
        if self.process.poll() is not None:
            result = self.process.wait()
            os.close(self.master)
            assert result == 0, f"PTY app exited with status {result}"
            return
        # Leave any open modal first: quit keys are handled by the modal, not
        # the app, and a modal left open would turn a clean quit into a kill.
        # Modals can stack (list -> actions -> detail), so unwind a few.
        for _ in range(4):
            try:
                os.write(self.master, b"\x1b")
            except OSError:
                break
            deadline = time.monotonic() + 0.15
            while time.monotonic() < deadline:
                if select.select([self.master], [], [], 0.05)[0]: self.read()
        for key in (b"\x03", b"\x04"):
            try:
                os.write(self.master, key)
            except OSError:
                # The slave is already gone; fall through to reaping.
                break
            if key == b"\x03":
                deadline = time.monotonic() + 1
            else:
                deadline = time.monotonic() + 5
            while self.process.poll() is None and time.monotonic() < deadline:
                if select.select([self.master], [], [], 0.05)[0]: self.read()
        if self.process.poll() is None:
            self.process.terminate()
            deadline = time.monotonic() + 2
            while self.process.poll() is None and time.monotonic() < deadline:
                if select.select([self.master], [], [], 0.05)[0]: self.read()
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait()
        result = self.process.returncode
        os.close(self.master)
        assert result == 0, f"PTY app exited with status {result}"


def terminal_checks(workspace, root, env):
    Endpoint.mode = "edit"
    Endpoint.calls = 0
    (workspace / "file").write_text("interactive baseline\n")
    terminal = Terminal(workspace, env)
    try:
        terminal.wait("glass-core")
        terminal.send("edit the file")
        terminal.wait("session $3.00")
        assert (workspace / "file").read_text() == "agent edit\n"
        terminal.send("/diff")
        terminal.wait("Select a changed file")
        terminal.send("")
        terminal.wait("Select a diff hunk")
        terminal.send("")
        terminal.wait("+agent edit")
        os.write(terminal.master, b"\x1b"); terminal.wait("Select a diff hunk")
        os.write(terminal.master, b"\x1b"); terminal.wait("Select a changed file")
        terminal.dismiss()
        terminal.send("/undo")
        terminal.wait("pre-task edits preserved")
        assert (workspace / "file").read_text() == "interactive baseline\n"
        terminal.send("/checkpoint")
        terminal.wait("saved baseline")
        manual = max((root / "data/glass-core/checkpoints").glob("*/manual-*.json"), key=lambda p: p.stat().st_mtime)
        (workspace / "file").write_text("later user edit\n")
        terminal.send("/restore " + manual.stem)
        terminal.wait("recover with /restore")
        assert (workspace / "file").read_text() == "interactive baseline\n"
        terminal.send("/bg echo ready; sleep 30")
        terminal.wait("job 1 pid")
        terminal.send("/jobs")
        terminal.wait("1: running")
        terminal.dismiss()
        deadline = time.monotonic() + 3
        while not any("ready" in path.read_text() for path in (root / "data/glass-core/jobs").glob("*/*.log")):
            assert time.monotonic() < deadline, "background job produced no output"
            time.sleep(0.01)
        terminal.send("/job 1")
        terminal.wait("ready")
        terminal.dismiss()
        terminal.send("/job 1 stop")
        terminal.wait("stopped job 1")
        terminal.dismiss()
        terminal.send("/branch 1")
        output = terminal.wait("conversation forked")
        branch = re.search(r"branch ([a-z0-9]+(?:-[a-z0-9]+)+)", output).group(1)
        terminal.send("/budget 0.5")
        terminal.wait("limit $0.500")
        Endpoint.calls = 0
        terminal.send("edit again")
        terminal.wait("session cost limit reached")
        ledger = root / "data/glass-core" / (branch + ".cost.json")
        assert json.loads(ledger.read_text())["spent"] == 1.0
        assert (workspace / "file").read_text() == "interactive baseline\n"
        terminal.send("/clear")
        terminal.wait("new session")
        assert json.loads(ledger.read_text())["spent"] == 1.0
        terminal.send("/bg sleep 30")
        output = terminal.wait("job 1 pid")
        pid = int(re.search(r"job 1 pid (\d+)", output).group(1))
    finally:
        terminal.close()
    assert not process_exists(pid), "owned background process survived exit"
    resumed = Terminal(workspace, env, ("resume", manual.parent.name))
    try:
        resumed.wait("glass-core")
        resumed.send("/budget")
        resumed.wait("Spent $3.00")
        resumed.dismiss()
        resumed.send("/diff")
        resumed.wait("Select a changed file")
        resumed.send("")
        resumed.wait("Select a diff hunk")
        resumed.send("")
        resumed.wait("+agent edit")
    finally:
        resumed.close()


def input_palette_checks(workspace, root, env):
    calls = Endpoint.calls
    conf = root / "config/glass-core/config.toml"
    original_config = conf.read_text()
    extension = REPO / "extensions/hello.py"
    command = shlex.join([sys.executable, str(extension)])
    conf.write_text("extensions = " + json.dumps([command]) + "\n" + original_config)
    terminal = Terminal(workspace, env)
    try:
        terminal.visible("❯ ")
        # Observe each key independently; do not send a second key to trigger repaint.
        os.write(terminal.master, b"a")
        terminal.visible("❯ a")
        os.write(terminal.master, b"b")
        terminal.visible("❯ ab")
        os.write(terminal.master, b"c")
        terminal.visible("❯ abc")
        # Arrow, delete, and paste must keep the last visible character intact too.
        os.write(terminal.master, b"\x1b[D\x7f")
        terminal.visible("❯ ac")
        os.write(terminal.master, b"\x03")
        os.write(terminal.master, b"/")
        terminal.visible("Commands")
        terminal.visible("/help")
        os.write(terminal.master, b"bud")
        terminal.visible("/budget")
        assert "/skills" not in terminal.screen.text()
        os.write(terminal.master, b"\t")
        terminal.visible("❯ /budget ")
        os.write(terminal.master, b"\r")
        terminal.visible("Budget")
        terminal.visible("Spent $0.000")
        terminal.dismiss()
        # Provider and model are selected inside modals.
        os.write(terminal.master, b"/mod\r")
        terminal.visible("Provider and model")
        terminal.visible("Configured openai")
        terminal.dismiss()
        os.write(terminal.master, b"unfinished draft")
        terminal.visible("❯ unfinished draft")
        os.write(terminal.master, b"\x10")
        terminal.visible("Commands")
        terminal.dismiss()
        terminal.visible("❯ unfinished draft")
        # Filter to /btw directly; /keys is now an additional palette item.
        os.write(terminal.master, b"\x03/btw\r")
        terminal.visible("Side question")
        terminal.dismiss()
        # Unknown filters stay local and can be dismissed.
        os.write(terminal.master, b"/not-a-command")
        terminal.visible("No matching commands")
        os.write(terminal.master, b"\r\x1b")
        terminal.visible("❯ /not-a-command")
        os.write(terminal.master, b"\x03/hello")
        terminal.visible("Print a greeting from the extension")
        os.write(terminal.master, b"\r")
        terminal.wait("hello extension v1.0.0, args=''")
        assert Endpoint.calls == calls, "palette and typing must not request the model"
    finally:
        terminal.close()
        conf.write_text(original_config)


def namespace_checks(root, workspace):
    config = root / "namespace-config"
    data = root / "namespace-data"
    (config / "glass").mkdir(parents=True)
    (data / "glass").mkdir(parents=True)
    foreign_config = config / "glass/config.toml"
    foreign_session = data / "glass/other-project.jsonl"
    foreign_config.write_text("this is another project's config, not valid harness TOML")
    foreign_session.write_text('{"k":"user","t":"other Glass project"}\n')
    before = (foreign_config.read_bytes(), foreign_session.read_bytes())
    env = dict(os.environ, XDG_CONFIG_HOME=str(config), XDG_DATA_HOME=str(data))
    listed = subprocess.run([str(BINARY), "list"], cwd=workspace, env=env, text=True, capture_output=True, check=True)
    assert "no sessions" in listed.stdout and not (data / "glass-core").exists()
    refused = subprocess.run([str(BINARY), "run", "--json", "--budget", "nan", "test"], cwd=workspace, env=env, text=True, capture_output=True)
    assert refused.returncode == 1 and "budget must be positive and finite" in refused.stderr, refused.stderr
    assert not (config / "glass-core").exists()
    assert not (data / "glass-core/other-project.jsonl").exists()
    assert before == (foreign_config.read_bytes(), foreign_session.read_bytes())
    no_home = {k: v for k, v in env.items() if k not in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME")}
    subprocess.run([str(BINARY), "run", "--budget", "nan", "test"], cwd=workspace, env=no_home, text=True, capture_output=True)
    assert (workspace / ".glass-core/data").is_dir() and not (workspace / ".glass").exists()
    # The isolated fallback session is test state, not task source.
    import shutil
    shutil.rmtree(workspace / ".glass-core")


def main():
    with tempfile.TemporaryDirectory(prefix="glass-cli-smoke-") as scratch:
        root = Path(scratch)
        workspace = root / "repo"
        workspace.mkdir()
        subprocess.run(["git", "init", "-q", str(workspace)], check=True)
        (workspace / "file").write_text("user's dirty baseline\n")
        namespace_checks(root, workspace)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            conf = root / "config/glass-core"
            conf.mkdir(parents=True)
            (conf / "config.toml").write_text(
                f'provider = "openai"\nmodel = "mock"\napi_key_env = ""\nbase_url = "http://127.0.0.1:{server.server_port}/v1"\n'
                '[pricing]\ninput = 1.0\ncache_read = 0.0\ncache_write = 0.0\noutput = 0.0\n'
            )
            env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))

            def run(*args, stdin=""):
                return subprocess.run([str(BINARY), "run", "--json", *args], cwd=workspace, env=env, input=stdin, text=True, capture_output=True, timeout=30)

            Endpoint.calls = 0
            done = run("--budget", "5", "edit the file")
            result = json.loads(done.stdout)
            assert done.returncode == 0, done.stderr
            assert result["status"] == "finished" and result["text"] == "Done."
            assert result["cost_usd"] == 3.0 and Endpoint.calls == 3
            assert (workspace / "file").read_text() == "agent edit\n", ((workspace / "file").read_text(), (root / "data/glass-core" / (result["session"] + ".jsonl")).read_text())
            checkpoint = root / "data/glass-core/checkpoints" / result["session"] / "task-1.json"
            assert json.loads(checkpoint.read_text())["after"]

            Endpoint.calls = 0
            (workspace / "file").write_text("keep this edit\n")
            stopped = run("--budget", "0.5", "edit the file")
            result = json.loads(stopped.stdout)
            assert stopped.returncode == 3, stopped.stderr
            assert result["status"] == "error" and result["cost_usd"] == 1.0
            assert Endpoint.calls == 1 and (workspace / "file").read_text() == "keep this edit\n"

            Endpoint.calls = 0
            Endpoint.mode = "loop"
            exhausted = run(stdin="keep running")
            result = json.loads(exhausted.stdout)
            assert exhausted.returncode == 2, exhausted.stderr
            assert result["status"] == "incomplete" and Endpoint.calls == 24

            Endpoint.calls = 0
            invalid = run("--budget", "nan", "edit")
            assert invalid.returncode == 1 and Endpoint.calls == 0
            terminal_checks(workspace, root, env)
            input_palette_checks(workspace, root, env)
            print("PASS: first-character visibility and command palette; glass-core namespace isolation; headless edits, JSON, stdin, cost accounting, budget stop, round limit, invalid budget; terminal diff, undo, checkpoint restore, branching, budget isolation, background output/stop/exit cleanup")
        finally:
            server.shutdown()
            server.server_close()

if __name__ == "__main__":
    main()
