//! Browser-free benchmark for the terminal-native ANSI live renderer.

use std::time::Instant;

use glass_browser::terminal_graphics::{AnsiCanvas, FrameFit};

const DEFAULT_ITERATIONS: usize = 100;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations = std::env::var("GLASS_LIVE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ITERATIONS);
    let frames = [gradient_png(320, 180, 0)?, gradient_png(320, 180, 37)?];

    println!("profile,columns,rows,iterations,total_ms,mean_ms,changed_cells");
    for (name, width, height) in [("data", 40, 12), ("balanced", 80, 24), ("smooth", 120, 36)] {
        let mut canvas = AnsiCanvas::default();
        let mut changed_cells = 0_usize;
        let started = Instant::now();
        for iteration in 0..iterations {
            changed_cells = changed_cells.saturating_add(
                canvas
                    .update_png(
                        &frames[iteration % frames.len()],
                        width,
                        height,
                        FrameFit::Contain,
                    )?
                    .changed_cells,
            );
        }
        let elapsed = started.elapsed();
        println!(
            "{name},{width},{height},{iterations},{:.3},{:.3},{changed_cells}",
            elapsed.as_secs_f64() * 1_000.0,
            elapsed.as_secs_f64() * 1_000.0 / iterations as f64,
        );
    }
    Ok(())
}

fn gradient_png(width: u32, height: u32, phase: u8) -> Result<Vec<u8>, png::EncodingError> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[
                (x as u8).wrapping_add(phase),
                (y as u8).wrapping_add(phase),
                (x as u8).wrapping_add(y as u8).wrapping_add(phase),
            ]);
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixels)?;
    }
    Ok(bytes)
}
