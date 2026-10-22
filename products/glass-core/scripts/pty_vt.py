"""Accurate Unicode VT transport with explicit alternate/save-cursor handling. Requires pyte."""
import pyte, http.server, threading, time, json, tempfile, os, select, importlib.util, re, subprocess, fcntl, termios, struct, signal, codecs
from pathlib import Path
from wcwidth import wcwidth
from core_binary import core_binary_path

class VT:

    def __init__(self, w, h, alternate_history=False):
        self.w, self.h = (w, h)
        self.alternate_history = alternate_history
        self.primary = pyte.HistoryScreen(w, h, history=5000)
        self.alt = self._alternate(w, h)
        self.active = self.primary
        self.stream = pyte.Stream(self.active)
        self.pending = ''
        self.alternate = False

    def _alternate(self, w, h):
        if self.alternate_history:
            return pyte.HistoryScreen(w, h, history=5000)
        return pyte.Screen(w, h)

    def feed(self, text):
        self.pending += text
        while self.pending:
            match = re.search('\\x1b\\[(?:\\?1049[hl]|[su])', self.pending)
            if not match:
                incomplete = re.search('\\x1b\\[[?0-9;]*$', self.pending)
                n = incomplete.start() if incomplete else len(self.pending)
                self.stream.feed(self.pending[:n])
                self.pending = self.pending[n:]
                return
            self.stream.feed(self.pending[:match.start()])
            seq = match.group()
            self.pending = self.pending[match.end():]
            if seq == '\x1b[?1049h':
                self.primary.save_cursor()
                self.alt = self._alternate(self.w, self.h)
                self.active = self.alt
                self.alternate = True
                self.stream = pyte.Stream(self.active)
            elif seq == '\x1b[?1049l':
                self.active = self.primary
                self.primary.restore_cursor()
                self.alternate = False
                self.stream = pyte.Stream(self.active)
            elif seq == '\x1b[s':
                self.active.save_cursor()
            elif seq == '\x1b[u':
                self.active.restore_cursor()

    def text(self):
        # pyte leaves an empty continuation cell when a diff overwrites the
        # leading half of a wide glyph. Real terminals clear that second half.
        # Repair only orphan stubs; valid two-cell glyphs remain unchanged.
        for row in self.active.buffer.values():
            for x, cell in list(row.items()):
                previous = row[x - 1].data if x else " "
                if cell.data == "" and (not previous or wcwidth(previous[0]) != 2):
                    row[x] = cell._replace(data=" ")
        return '\n'.join(self.active.display)

    def resize(self, w, h):
        self.w, self.h = (w, h)
        if h < self.primary.lines:
            self.primary.history.top.extend((self.primary.buffer[row].copy() for row in range(self.primary.lines - h)))
        self.primary.resize(lines=h, columns=w)
        self.alt.resize(lines=h, columns=w)

    def history(self):
        if self.alternate_history and self.alternate and isinstance(self.active, pyte.HistoryScreen):
            rows = [''.join((row[x].data for x in sorted(row))) for row in self.active.history.top]
            return '\n'.join(rows + self.active.display)
        rows = [''.join((row[x].data for x in sorted(row))) for row in self.primary.history.top]
        return '\n'.join(rows + self.primary.display)

class Terminal:

    def __init__(self, root, env, size, args=()):
        import pty
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', size[1], size[0], 0, 0))
        self.p = subprocess.Popen([os.environ.get('GLASS_CORE_TEST_BINARY', str(core_binary_path('debug'))), *args], cwd=root, env=dict(env, TERM='xterm-256color'), stdin=slave, stdout=slave, stderr=slave)
        os.close(slave)
        self.vt = VT(*size)
        self.decoder = codecs.getincrementaldecoder('utf-8')()
        self.bytes = ''
        self.seen = ''

    def read(self):
        try:
            data = os.read(self.master, 65536)
        except OSError:
            # EIO: the child closed the slave during exit; end of stream.
            return
        text = self.decoder.decode(data)
        self.bytes += text
        self.seen += text
        self.vt.feed(text)

    def collect(self, seconds=0.3):
        self.bytes = ''
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            if select.select([self.master], [], [], 0.02)[0]:
                self.read()
        return self.bytes

    def wait(self, needle, seconds=8):
        until = time.monotonic() + seconds
        while needle not in self.bytes:
            assert time.monotonic() < until, (needle, self.vt.text(), self.bytes)
            if select.select([self.master], [], [], 0.02)[0]:
                self.read()
        return self.bytes

    def send(self, text):
        os.write(self.master, text.encode())

    def resize(self, w, h):
        self.vt.resize(w, h)
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack('HHHH', h, w, 0, 0))
        os.kill(self.p.pid, signal.SIGWINCH)
        self.collect(0.3)

    def close(self):
        self.send('\x03')
        self.collect(0.1)
        self.send('\x04')
        self.collect(0.1)
        self.send('\r')
        try:
            self.p.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.p.terminate()
            self.p.wait(timeout=2)
        os.close(self.master)

    def visible(self, text, seconds=5):
        until = time.monotonic() + seconds
        while text not in self.vt.text():
            assert time.monotonic() < until, (text, self.vt.text())
            if select.select([self.master], [], [], 0.02)[0]:
                self.read()

    def idle(self):
        self.visible('❯')
        self.collect(0.1)
        assert self.vt.alternate
