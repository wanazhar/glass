import sys
import concurrent.futures
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

    @unittest.skipIf(sys.platform == "win32", "select on pipes requires Unix")
    def test_concurrent_requests_receive_their_own_results(self):
        script = r'''
import json, os, select, sys
def read():
    data = bytearray()
    while not data.endswith(b'\n'):
        part = os.read(0, 1)
        if not part: return None
        data.extend(part)
    return json.loads(data)
def reply(request):
    print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":request["method"]}), flush=True)
first = read()
if select.select([0], [], [], 0.2)[0]:
    second = read()
    reply(second)
    reply(first)
else:
    reply(first)
    reply(read())
'''
        client = GlassClient(sys.executable, ["-c", script])
        try:
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
                first = executor.submit(client._request, "first", {})
                second = executor.submit(client._request, "second", {})
                self.assertEqual(first.result(timeout=5), "first")
                self.assertEqual(second.result(timeout=5), "second")
        finally:
            client.close()


if __name__ == "__main__":
    unittest.main()
