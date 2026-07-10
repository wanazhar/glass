# Benchmarking

The Rust benchmark reports one cold startup measurement and warm-session client
operations against the local fixture. The operation timings exclude startup
and page-load time.

    GLASS_BENCH_ITERATIONS=50 cargo run --release --example benchmark

For a Playwright comparison, install a pinned version outside this repository
and point both tools at the same Chrome binary:

    tmp_dir=$(mktemp -d)
    npm install --prefix "$tmp_dir" --no-save playwright@1.61.1
    NODE_PATH="$tmp_dir/node_modules" \
      CHROME_PATH=/usr/bin/chromium-browser \
      GLASS_BENCH_ITERATIONS=50 \
      node benchmarks/playwright.mjs

Compare the same operation names, Chrome binary, fixture, iteration count,
machine, and warm/cold state. Report p50 and p95 rather than a single average.
The Rust output distinguishes fresh context collection from cached observations;
the latter represents repeated agent turns without a page mutation.
Binary-size comparisons should use the stripped glass executable and clearly
state whether Playwright's Node runtime and browser downloads are included.
