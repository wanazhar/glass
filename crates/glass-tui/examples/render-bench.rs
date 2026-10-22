//! Reproducible native block throughput check: cargo run -p glass-tui --release --example render-bench
use glass_tui::markdown::{Stream, lines};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut source = String::from("# Results\n\n| File | Status |\n| --- | --- |\n");
    for i in 0..10_000 {
        source.push_str(&format!(
            "| src/module_{i}.rs | **checked** and ready to ship |\n"
        ));
    }
    for width in [20usize, 80] {
        let start = Instant::now();
        let rows = black_box(lines(&source, width)).len();
        let layout_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let mut stream = Stream::default();
        let mut sink = std::io::sink();
        for chunk in source.as_bytes().chunks(64) {
            stream.write(&mut sink, std::str::from_utf8(chunk).unwrap(), width);
        }
        stream.flush(&mut sink, width);
        let stream_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{{\"width\":{width},\"source_bytes\":{},\"table_rows\":10000,\"display_rows\":{rows},\"layout_ms\":{layout_ms:.3},\"stream_ms\":{stream_ms:.3}}}",
            source.len()
        );
        assert!(
            layout_ms < 2000.0 && stream_ms < 2000.0,
            "native block throughput exceeded 2s budget"
        );
    }
}
