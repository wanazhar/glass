import importlib.util
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'products' / 'glass-core' / 'scripts' / 'check-performance.py'
sys.path.insert(0, str(SCRIPT.parent))
spec = importlib.util.spec_from_file_location('check_performance', SCRIPT)
check_performance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_performance)
reader_frame_ready = check_performance.reader_frame_ready


class ReaderFrameReadyTests(unittest.TestCase):
    def test_accepts_marker_with_log_header(self):
        self.assertTrue(reader_frame_ready(b'Log \xe2\x80\x94 line 62029/6\nRFMK'))

    def test_accepts_recap_notice_with_log_header(self):
        self.assertTrue(reader_frame_ready(b'Log \xe2\x80\x94 line 62029/6\n0 extra tokens'))

    def test_accepts_ansi_log_header(self):
        self.assertTrue(reader_frame_ready(b'\x1b[1mLog \xe2\x80\x94 line 62029/6 \x1b[0m\nRFMK'))

    def test_rejects_live_prompt_recap_without_log_header(self):
        self.assertFalse(reader_frame_ready(b'glass-core ready...\n0 extra tokens'))

    def test_rejects_log_header_without_content(self):
        self.assertFalse(reader_frame_ready(b'Log \xe2\x80\x94 line 62029/6'))

    def test_rejects_unrelated_log_line(self):
        self.assertFalse(reader_frame_ready(b'Log \xe2\x80\x94 line not-a-number\nRFMK'))


if __name__ == '__main__':
    unittest.main()
