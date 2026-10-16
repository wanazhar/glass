#!/usr/bin/env python3
"""Release PTYs for syntax colors, native math, scroll and compact exit."""
import os
from pathlib import Path
import tempfile
from pty_vt import Terminal
import importlib.util
spec=importlib.util.spec_from_file_location('audit',Path(__file__).with_name('smoke-rendering-audit.py'))
audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)
SOURCE=r"""# RootReady

```python
def fib(n): # CommentReady
    return "StringReady" + 123
```

```javascript
function debounce(fn, delay) {
    return "JsReady";
}
```

```java
public class Greeter {
    private String name = "JavaReady";
}
```

```cpp
int main() { return 42; } // CppReady
```

```latex
\documentclass{article}
\begin{document}
Hello, world!

\[
E = mc^2
\]
\end{document}
```

Inline $E=mc^2$ and \(\alpha_i\).

$$
\frac{a}{b} \leq \infty
$$

```latex
\sqrt{x+1}
```
"""
SOURCE += r'''
> [!NOTE] Native output stays readable.
> [!WARNING] Check values before applying.

```python
value = """first
second line"""
```
```rust
let value = r##"raw
literal"##;
```
```graphql
query Example { user { name } }
```
```diff
--- before
+++ after
@@ -1 +1 @@
-old
+new
```
```log
INFO working
WARN attention
ERROR failed
```

$$
\begin{bmatrix}1 & 22 \\ 333 & 4\end{bmatrix}
$$

$$
\frac{x+1}{n}
$$

$$
\begin{aligned}a &= b \\ long &= c\end{aligned}
$$

Inline $\sqrt[3]{x} + \mathbb{R} + \hat{x}$; currency $5-$10.
'''
for size in [(80,30),(40,24),(20,8)]:
    audit.run('syntax-math',SOURCE,True,True,size,markdown=True)
    with tempfile.TemporaryDirectory(prefix='gc-exit-pty-') as tmp:
        root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        (cfg/'config.toml').write_text('provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        t=Terminal(root,env,size)
        try:
            t.idle();t.send('DraftReady');t.visible('DraftReady');t.send('\x04');t.collect(.15)
            assert t.p.poll() is None
            assert 'EXIT' in t.vt.text() or 'Quit' in t.vt.text(),t.vt.text()
            if size[1]>8:assert 'DraftReady' in t.vt.text(),t.vt.text()
            t.send('\x1b');t.collect(.2);t.visible('DraftReady')
            t.send('\x04');t.collect(.1);t.resize(30,12);assert 'Quit' in t.vt.text(),t.vt.text()
            t.send('\x1b');t.collect(.1);t.visible('DraftReady')
            t.send('\x04');t.collect(.1);t.send('\r');t.p.wait(timeout=5)
            assert t.p.returncode==0
            print('PASS compact exit, escape, draft, resize and confirm',size,flush=True)
        finally:
            if t.p.poll() is None:t.close()
            else:os.close(t.master)
print('PASS: release native code/math and exit PTYs')
