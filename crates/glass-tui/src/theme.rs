//! Coordinated semantic palettes; exact source text never depends on presentation.
use ratatui::style::Color;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU8, Ordering};

static ACTIVE: AtomicU8 = AtomicU8::new(0);
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
    ["◐", "◓", "◑", "◒", "◐", "◓", "◑", "◒"][tick % 8]
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
pub struct TerminalWriter<W>(pub W);
impl<W: Write> Write for TerminalWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if no_color()
            && let Ok(text) = std::str::from_utf8(bytes)
        {
            self.0.write_all(neutralize(text).as_bytes())?;
            return Ok(bytes.len());
        }
        self.0.write(bytes)
    }
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> io::Result<()> {
        self.0.write_all(neutralize(&format!("{args}")).as_bytes())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
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
    }
    #[test]
    fn active_frames_change_without_terminal_controls() {
        let frames: std::collections::BTreeSet<_> = (0..8).map(frame).collect();
        assert_eq!(frames.len(), 4);
        assert!(
            frames
                .iter()
                .all(|frame| crate::text::width(frame) == 1 && !frame.contains('\x1b'))
        );
    }
}
