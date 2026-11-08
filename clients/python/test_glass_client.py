import sys
import unittest

from glass_client import GlassClient, GlassError


class FramingTests(unittest.TestCase):
    def test_unterminated_response_is_rejected(self):
        script = (
            'import sys; sys.stdin.readline(); '
            'sys.stdout.write(\'{"jsonrpc":"2.0","id":1,"result":{}}\'); '
            'sys.stdout.flush()'
        )
        client = GlassClient(sys.executable, ["-c", script])
        try:
            with self.assertRaisesRegex(GlassError, "completing an MCP newline frame"):
                client._request("test", {})
        finally:
            client.close()


if __name__ == "__main__":
    unittest.main()
