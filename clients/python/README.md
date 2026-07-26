# Glass Python thin client

`glass_client.py` uses only the Python standard library. It starts an absolute
Glass binary, negotiates MCP, and provides `navigate`, `observe`, `click`, and
`wait` helpers for agent frameworks.

```python
from glass_client import GlassClient

glass = GlassClient(command="/absolute/path/to/glass")
try:
    glass.navigate("https://example.com")
    print(glass.observe())
finally:
    glass.close()
```

The client accepts newline-delimited and `Content-Length` MCP frames and caps
individual frames at 4 MiB.
