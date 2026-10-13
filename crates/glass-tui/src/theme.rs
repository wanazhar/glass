//! Coordinated semantic palettes; exact source text never depends on presentation.
use ratatui::style::Color;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU8, Ordering};

static ACTIVE: AtomicU8 = AtomicU8::new(0);
static ANIMATION: AtomicU8 = AtomicU8::new(0);
pub const ANIMATION_NAMES: [&str; 8] = ["Dot", "MiniDot", "Pulse", "Spark", "Bounce", "Orbit", "Grow", "Blink"];
pub const ACTIVITY_INTERVAL_MS: u64 = 100;
// Dot, MiniDot and Pulse frames follow Charm Bubbles (MIT); see THIRD_PARTY.md.
const DOT: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
const MINI_DOT: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const PULSE: [&str; 4] = ["█", "▓", "▒", "░"];
const SPARK: [&str; 4] = ["·", "+", "✦", "+"];
const BOUNCE: [&str; 6] = ["⠁", "⠂", "⠄", "⡀", "⠄", "⠂"];
const ORBIT: [&str; 4] = ["◐", "◓", "◑", "◒"];
const GROW: [&str; 8] = ["▁", "▂", "▄", "▆", "█", "▆", "▄", "▂"];
const BLINK: [&str; 4] = ["○", "◌", "●", "◌"];
pub fn select_animation(index: usize) {
    ANIMATION.store(
        index.min(ANIMATION_NAMES.len() - 1) as u8,
        Ordering::Relaxed,
    );
}
pub fn current_animation() -> usize {
    ANIMATION.load(Ordering::Relaxed) as usize
}
pub fn animation_frame(animation: usize, tick: usize) -> &'static str {
    let frames: &[&str] = match animation {
        1 => &MINI_DOT,
        2 => &PULSE,
        3 => &SPARK,
        4 => &BOUNCE,
        5 => &ORBIT,
        6 => &GROW,
        7 => &BLINK,
        _ => &DOT,
    };
    frames[tick % frames.len()]
}
pub fn activity_glyph_tick(glyph: char) -> Option<usize> {
    (0..10).find(|tick| frame(*tick).starts_with(glyph))
}
pub fn is_activity_glyph(glyph: char) -> bool {
    [&DOT[..], &MINI_DOT[..], &PULSE[..], &SPARK[..], &BOUNCE[..], &ORBIT[..], &GROW[..], &BLINK[..]]
        .iter()
        .flat_map(|frames| frames.iter())
        .any(|frame| frame.starts_with(glyph))
}
pub const NAMES: [&str; 5] = ["Aurora", "Amber", "Monochrome", "Orchid", "Ocean"];
pub fn select(index: usize) {
    ACTIVE.store(index.min(NAMES.len() - 1) as u8, Ordering::Relaxed);
}
pub fn current() -> usize {
    ACTIVE.load(Ordering::Relaxed) as usize
}
pub fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}
#[derive(Clone, Copy)]
pub enum Tone {
    Accent,
    Secondary,
    Success,
    Warning,
    Error,
    Body,
    Muted,
    Border,
    Surface,
}
fn index(tone: Tone) -> u8 {
    let tones = match current() {
        1 => [215, 180, 150, 221, 203, 253, 245, 240, 235],
        2 => [255, 250, 255, 255, 255, 252, 245, 240, 235],
        3 => [177, 111, 120, 222, 210, 253, 245, 60, 235],
        4 => [80, 117, 121, 215, 204, 252, 245, 24, 235],
        _ => [81, 183, 114, 222, 203, 252, 245, 60, 235],
    };
    tones[tone as usize]
}
pub fn color(tone: Tone) -> Color {
    if no_color() {
        Color::Reset
    } else {
        Color::Indexed(index(tone))
    }
}
pub fn ansi(tone: Tone) -> String {
    if no_color() {
        String::new()
    } else {
        format!("\x1b[38;5;{}m", index(tone))
    }
}
pub fn accent() -> String {
    ansi(Tone::Accent)
}
pub fn selected() -> String {
    if no_color() || current() == 2 {
        "\x1b[7m".into()
    } else {
        format!("\x1b[48;5;{}m{}", index(Tone::Surface), accent())
    }
}
pub fn frame(tick: usize) -> &'static str {
    animation_frame(current_animation(), tick)
}
pub fn activity_color(tick: usize) -> String {
    ansi(if tick % 4 < 2 {
        Tone::Accent
    } else {
        Tone::Secondary
    })
}

/// Remove only color SGR parameters; preserve cursor controls and neutral emphasis.
pub fn neutralize(text: &str) -> String {
    if !no_color() {
        return text.into();
    }
    strip_colors(text)
}
fn strip_colors(text: &str) -> String {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("\x1b[") {
        output.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let Some(end) = tail.find(|c: char| ('@'..='~').contains(&c)) else {
            output.push_str(&rest[start..]);
            return output;
        };
        if tail.as_bytes()[end] != b'm' {
            output.push_str(&rest[start..start + end + 3]);
        } else {
            // Crossterm suppresses both color values under NO_COLOR but
            // retains their separator. ESC[;m would reset emphasis as well.
            if &tail[..end] == ";" {
                rest = &rest[start + end + 3..];
                continue;
            }
            let codes: Vec<&str> = tail[..end].split(';').collect();
            let mut keep = Vec::new();
            let mut i = 0;
            while i < codes.len() {
                let code = codes[i].parse::<u16>().unwrap_or(0);
                if code == 38 || code == 48 {
                    i += match codes.get(i + 1).copied() {
                        Some("5") => 3,
                        Some("2") => 5,
                        _ => 1,
                    };
                    continue;
                }
                if !(30..=49).contains(&code) && !(90..=107).contains(&code) {
                    keep.push(codes[i]);
                }
                i += 1;
            }
            if !keep.is_empty() {
                output.push_str(&format!("\x1b[{}m", keep.join(";")));
            }
        }
        rest = &rest[start + end + 3..];
    }
    output.push_str(rest);
    output
}
/// Use on terminal writers only, never log files or headless source output.
pub struct TerminalWriter<W> {
    inner: W,
    pending: Vec<u8>,
    neutral: bool,
}
impl<W: Write> TerminalWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            neutral: no_color(),
        }
    }
}
impl<W: Write> Write for TerminalWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.neutral {
            return self.inner.write(bytes);
        }
        // Crossterm can split one CSI command across several writes. Buffer
        // only its bounded prefix, never transcript content or a whole frame.
        let mut out = Vec::with_capacity(bytes.len() + self.pending.len());
        for &byte in bytes {
            if self.pending.is_empty() {
                if byte == 0x1b {
                    self.pending.push(byte);
                } else {
                    out.push(byte);
                }
                continue;
            }
            self.pending.push(byte);
            if self.pending.len() == 2 && byte != b'[' {
                out.append(&mut self.pending);
            } else if self.pending.len() > 2 && (b'@'..=b'~').contains(&byte) {
                if let Ok(sequence) = std::str::from_utf8(&self.pending) {
                    out.extend_from_slice(strip_colors(sequence).as_bytes());
                } else {
                    out.extend_from_slice(&self.pending);
                }
                self.pending.clear();
            } else if self.pending.len() >= 64 {
                out.append(&mut self.pending);
            }
        }
        self.inner.write_all(&out)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.write_all(&self.pending)?;
        self.pending.clear();
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_palette_indices_remain_compatible() {
        assert_eq!(&NAMES[..3], &["Aurora", "Amber", "Monochrome"]);
        assert_eq!(NAMES.len(), 5);
    }
    #[test]
    fn neutral_colors_preserve_emphasis_and_cursor_commands() {
        assert_eq!(
            strip_colors("\x1b[38;5;81mblue\x1b[0m\x1b[3;2H\x1b[s\x1b[u"),
            "blue\x1b[0m\x1b[3;2H\x1b[s\x1b[u"
        );
        assert_eq!(
            strip_colors("\x1b[1;38;5;177;48;2;1;2;3mstrong\x1b[0m"),
            "\x1b[1mstrong\x1b[0m"
        );
        assert_eq!(strip_colors("\x1b[31merror\x1b[39m\x1b[49m"), "error");
        assert_eq!(strip_colors("\x1b[1m\x1b[;m❯ \x1b[0m"), "\x1b[1m❯ \x1b[0m");
    }
    #[test]
    fn neutral_writer_handles_fragmented_color_sequences() {
        let mut writer = TerminalWriter {
            inner: Vec::new(),
            pending: Vec::new(),
            neutral: true,
        };
        for fragment in [
            "\x1b[1m",
            "\x1b[",
            ";",
            "m",
            "❯ ",
            "\x1b[38;",
            "5;81m",
            "text",
            "\x1b[0m",
            "\x1b[3;2H",
        ] {
            writer.write_all(fragment.as_bytes()).unwrap();
        }
        writer.flush().unwrap();
        assert_eq!(
            String::from_utf8(writer.inner).unwrap(),
            "\x1b[1m❯ text\x1b[0m\x1b[3;2H"
        );
    }
    #[test]
    fn animation_frames_have_stable_single_cell_width() {
        for animation in 0..ANIMATION_NAMES.len() {
            for tick in 0..40 {
                let frame = animation_frame(animation, tick);
                assert_eq!(crate::text::width(frame), 1);
                assert!(is_activity_glyph(frame.chars().next().unwrap()));
                assert!(!frame.contains('\x1b'));
            }
        }
        assert_eq!(animation_frame(0, 0), "⣾");
        assert_eq!(animation_frame(0, 8), "⣾");
    }
    #[test]
    fn active_frames_change_without_terminal_controls() {
        let frames: std::collections::BTreeSet<_> = (0..8).map(frame).collect();
        assert!(frames.len() >= 3);
        assert!(
            frames
                .iter()
                .all(|frame| crate::text::width(frame) == 1 && !frame.contains('\x1b'))
        );
    }
}
