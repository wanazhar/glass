# Benchmarking

The focused capture investigation, reproducible commands, strategy assessment,
and measured results are in [capture-report.md](capture-report.md). The two Rust
drivers are:

    GLASS_CAPTURE_ITERATIONS=50 cargo run --release --example capture_benchmark
    GLASS_SCREENCAST_FRAMES=120 cargo run --release --example screencast_benchmark

The benchmarks report one cold startup measurement and warm-session client
operations against the local fixture. Startup ends when an `about:blank` page
is ready for automation; fixture navigation is excluded from startup and from
the operation timings.

    GLASS_BENCH_ITERATIONS=50 cargo run --release --example benchmark

For a Playwright comparison, install a pinned version outside this repository
and point both tools at the same Chrome binary:

    tmp_dir=$(mktemp -d)
    npm install --prefix "$tmp_dir" --no-save playwright@1.61.1
    NODE_PATH="$tmp_dir/node_modules" \
      CHROME_PATH=/usr/bin/chromium-browser \
      GLASS_BENCH_ITERATIONS=50 \
      node benchmarks/playwright.mjs

Compare operations with matching names using the same Chrome binary, fixture,
iteration count, machine, and warm/cold state. Report p50 and p95 rather than a
single average.
The Rust output distinguishes fresh context collection from cached observations;
the latter represents repeated agent turns without a page mutation. The
`observe_fresh` operation never captures an image, while
`observe_fresh_with_screenshot` measures the explicit visual opt-in.

The Rust `click_pair_fast` and `click_pair_human` operations each click the
fixture's Name field and Save button in sequence. Alternating targets ensures
that every measured human-mode operation includes pointer movement; one
unmeasured pair warms each pointer before sampling. Playwright's
`click_pair_default` is its normal locator-click baseline; Playwright has no
equivalent observation cache or built-in human-path mode, so those results are
useful within each tool rather than as identically named cross-tool operations.
Binary-size comparisons should use the stripped glass executable and clearly
state whether Playwright's Node runtime and browser downloads are included.
