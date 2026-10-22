# Third-party notices

## Charm Bubbles activity frames

The Dot, MiniDot and Pulse frame sequences in `../../crates/glass-tui/src/theme.rs`
are adapted from [Charm Bubbles](https://github.com/charmbracelet/bubbles/blob/main/spinner/spinner.go).
GC supplies spacing and scheduling through its existing renderer.

```text
MIT License

Copyright (c) 2020-2026 Charmbracelet, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Optional web workspace

The separate web asset bundle uses Marked, DOMPurify, Highlight.js, KaTeX,
Mermaid, and PDF.js. Their dependency versions are pinned in `web/package-lock.json`.
The web build includes their upstream license texts in `THIRD_PARTY_NOTICES.txt`.
These libraries are not linked into the Rust executable.
