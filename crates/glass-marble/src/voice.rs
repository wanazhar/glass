//! Voice, kept entirely local.
//!
//! Marble never sends audio to a provider. The phone speaks and listens with
//! the on-device synthesizer and recognizer, and the host only shapes a reply
//! into something worth reading aloud: code, markup, and links are removed so a
//! voice does not recite asterisks. That shaping lives here so it is testable
//! and shared by both surfaces.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Longest spoken form the host returns; the client may trim further.
pub const SPOKEN_LIMIT: usize = 1600;

/// Voice preferences shared by the surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    /// Whether a surface should read replies aloud. Off by default.
    #[serde(default)]
    pub enabled: bool,
    /// Speaking rate, 0.3 to 0.7 is pleasant for most people.
    #[serde(default = "default_rate")]
    pub rate: f32,
    /// Optional preferred voice identifier chosen on the device.
    #[serde(default)]
    pub voice: String,
}

fn default_rate() -> f32 {
    0.5
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            rate: default_rate(),
            voice: String::new(),
        }
    }
}

impl VoiceSettings {
    /// Clamp anything a client sends into a usable range.
    pub fn normalize(mut self) -> Self {
        self.rate = self.rate.clamp(0.2, 0.8);
        self.voice = self.voice.trim().chars().take(120).collect();
        self
    }

    /// Product document.
    pub fn document(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "rate": self.rate,
            "voice": self.voice,
            "provider": "on-device",
            "note": "Speech recognition and synthesis stay on the device; the host never receives audio.",
            "limit": SPOKEN_LIMIT,
        })
    }
}

/// Shape one reply into a spoken form: no code, no markup, no URLs.
pub fn spoken_form(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if !in_fence {
                out.push_str(" Code omitted. ");
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let trimmed = trimmed.trim_start_matches(['#', '>', '-', '*', '+', ' ']);
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        out.push_str(&strip_inline(trimmed));
        out.push('\n');
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let collapsed = collapse_punctuation(&collapsed);
    if collapsed.chars().count() > limit {
        let mut short: String = collapsed.chars().take(limit).collect();
        short.push('…');
        short
    } else {
        collapsed
    }
}

/// Remove inline markup: backticks, emphasis, link targets, bare URLs.
fn strip_inline(line: &str) -> String {
    let prepared = replace_urls(line);
    let mut out = String::with_capacity(prepared.len());
    let mut chars = prepared.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '`' => {}
            '*' | '_' | '~' => {}
            '[' => {
                // Keep the label of [label](target).
                let mut label = String::new();
                for next in chars.by_ref() {
                    if next == ']' {
                        break;
                    }
                    label.push(next);
                }
                // Drop the parenthesised target if it follows.
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1;
                    for next in chars.by_ref() {
                        if next == '(' {
                            depth += 1;
                        } else if next == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                }
                out.push_str(&label);
            }
            other => out.push(other),
        }
    }
    out
}

/// A URL is not worth reciting character by character.
fn replace_urls(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        let next = match (rest.find("http://"), rest.find("https://")) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(index), None) | (None, Some(index)) => Some(index),
            (None, None) => None,
        };
        let Some(index) = next else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..index]);
        out.push_str("a link");
        let tail = &rest[index..];
        match tail.find(|character: char| character.is_whitespace() || character == ')') {
            Some(end) => rest = &tail[end..],
            None => break,
        }
    }
    out
}

fn collapse_punctuation(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for character in text.chars() {
        if character == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(character);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_and_markup_are_not_read_aloud() {
        let reply = "Here is the plan:\n\n1. **Read** the `notes.md` file\n2. See [the guide](https://example.com/x)\n\n```rust\nfn main() {}\n```\nDone.";
        let spoken = spoken_form(reply, SPOKEN_LIMIT);
        assert!(!spoken.contains("fn main"), "{spoken}");
        assert!(!spoken.contains("**"), "{spoken}");
        assert!(!spoken.contains('`'), "{spoken}");
        assert!(!spoken.contains("https://"), "{spoken}");
        assert!(spoken.contains("Read the notes.md file"), "{spoken}");
        assert!(spoken.contains("the guide"), "{spoken}");
        assert!(spoken.contains("Code omitted."), "{spoken}");
        assert!(spoken.contains("Done."), "{spoken}");
    }

    #[test]
    fn spoken_form_is_bounded_and_collapsed() {
        let spoken = spoken_form("word ".repeat(500).as_str(), 40);
        assert_eq!(spoken.chars().count(), 41, "{spoken}");
        assert!(spoken.ends_with('…'));
        assert_eq!(spoken_form("a   b\n\n c", 100), "a b c");
    }

    #[test]
    fn settings_clamp_and_document_the_local_provider() {
        let settings = VoiceSettings {
            enabled: true,
            rate: 3.0,
            voice: "  Samantha  ".into(),
        }
        .normalize();
        assert_eq!(settings.rate, 0.8);
        assert_eq!(settings.voice, "Samantha");
        assert_eq!(settings.document()["provider"], json!("on-device"));
        assert!(!VoiceSettings::default().enabled);
    }
}
