//! Lightweight Markdown presentation; copying always uses the original text.
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub fn code_blocks(text: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut open: Option<(char, usize, String, String)> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let fence = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        let run = fence
            .map(|c| trimmed.chars().take_while(|v| *v == c).count())
            .unwrap_or(0);
        if let Some((marker, length, _, _)) = &open {
            if fence == Some(*marker) && run >= *length && trimmed[run..].trim().is_empty() {
                if let Some((_, _, language, body)) = open.take() {
                    result.push((language, body));
                }
                continue;
            }
        } else if run >= 3 {
            open = Some((
                fence.unwrap(),
                run,
                trimmed[run..].trim().into(),
                String::new(),
            ));
            continue;
        }
        if let Some((_, _, _, body)) = &mut open {
            body.push_str(line);
        }
    }
    if let Some((_, _, language, body)) = open {
        result.push((language, body));
    }
    result
}
/// Provider text is display data, never terminal instructions.
pub fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

fn inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(['`', '*']) {
        if start > 0 {
            spans.push(Span::raw(rest[..start].to_string()));
        }
        let tail = &rest[start..];
        let marker = if tail.starts_with("**") {
            "**"
        } else if tail.starts_with('`') {
            "`"
        } else {
            "*"
        };
        if let Some(end) = tail[marker.len()..].find(marker) {
            let body = &tail[marker.len()..marker.len() + end];
            let style = match marker {
                "`" => Style::new()
                    .fg(crate::theme::color(crate::theme::Tone::Success))
                    .bg(crate::theme::color(crate::theme::Tone::Surface)),
                "**" => Style::new()
                    .fg(crate::theme::color(crate::theme::Tone::Secondary))
                    .add_modifier(Modifier::BOLD),
                _ => Style::new()
                    .fg(crate::theme::color(crate::theme::Tone::Secondary))
                    .add_modifier(Modifier::ITALIC),
            };
            if marker == "`" {
                spans.push(Span::styled(body.to_string(), style));
            } else {
                spans.extend(inline(body).into_iter().map(|mut span| {
                    span.style = span.style.patch(style);
                    span
                }));
            }
            rest = &tail[marker.len() * 2 + end..];
        } else {
            spans.push(Span::raw(tail.to_string()));
            return spans;
        }
    }
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_string()));
    }
    spans
}

/// Wrap styled prose at words rather than independently wrapping each span.
/// A word may cross style boundaries; its cells still belong on the same row.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Cursor {
    word: usize,
    character: usize,
}
struct Row {
    line: Line<'static>,
    end: Cursor,
}

fn wrap_spans(
    spans: Vec<Span<'static>>,
    width: usize,
    first: &str,
    hanging: &str,
    cursor: Cursor,
) -> Vec<Row> {
    let width = width.max(1);
    let mut words: Vec<Vec<(char, Style)>> = Vec::new();
    let mut word = Vec::new();
    for span in spans {
        for c in span.content.chars() {
            if c.is_whitespace() {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            } else {
                word.push((c, span.style));
            }
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut rows = Vec::new();
    let first = crate::text::clip(first, width.saturating_sub(1));
    let hanging = crate::text::clip(hanging, width.saturating_sub(1));
    let prefix = if cursor == Cursor::default() {
        first
    } else {
        hanging.clone()
    };
    let mut used = crate::text::width(&prefix);
    let mut row = vec![Span::raw(prefix)];
    let mut has_word = false;
    for (index, word) in words.iter().enumerate().skip(cursor.word) {
        let skip = if index == cursor.word {
            cursor.character
        } else {
            0
        };
        let cells: usize = word
            .iter()
            .skip(skip)
            .map(|(c, _)| crate::text::width(&c.to_string()))
            .sum();
        if has_word && used + 1 + cells > width {
            rows.push(Row {
                line: Line::from(std::mem::take(&mut row))
                    .style(Style::new().fg(crate::theme::color(crate::theme::Tone::Body))),
                end: Cursor {
                    word: index,
                    character: 0,
                },
            });
            row.push(Span::raw(hanging.clone()));
            used = crate::text::width(&hanging);
            has_word = false;
        }
        if has_word {
            row.push(Span::raw(" "));
            used += 1;
        }
        for (character, (c, style)) in word.iter().enumerate().skip(skip) {
            let cells = crate::text::width(&c.to_string());
            if used + cells > width {
                rows.push(Row {
                    line: Line::from(std::mem::take(&mut row))
                        .style(Style::new().fg(crate::theme::color(crate::theme::Tone::Body))),
                    end: Cursor {
                        word: index,
                        character,
                    },
                });
                row.push(Span::raw(hanging.clone()));
                used = crate::text::width(&hanging);
            }
            if cells <= width.saturating_sub(crate::text::width(&hanging)) {
                if row.len() > 1 && row.last().is_some_and(|last| last.style == *style) {
                    row.last_mut().unwrap().content.to_mut().push(*c);
                } else {
                    row.push(Span::styled(c.to_string(), *style));
                }
                used += cells;
            }
        }
        has_word = true;
    }
    rows.push(Row {
        line: Line::from(row).style(Style::new().fg(crate::theme::color(crate::theme::Tone::Body))),
        end: Cursor {
            word: words.len(),
            character: 0,
        },
    });
    rows
}

#[derive(Default)]
pub struct Presentation {
    fence: Option<(char, usize)>,
    continuation: Option<String>,
}
impl Presentation {
    pub fn line(&mut self, raw: &str, width: usize, complete: bool) -> Vec<Line<'static>> {
        self.render(raw, width, complete, Cursor::default())
            .into_iter()
            .map(|row| row.line)
            .collect()
    }
    fn render(&mut self, raw: &str, width: usize, complete: bool, cursor: Cursor) -> Vec<Row> {
        let clean = sanitize(raw);
        let trimmed = clean.trim_start();
        let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        let count = marker
            .map(|m| trimmed.chars().take_while(|c| *c == m).count())
            .unwrap_or(0);
        if count >= 3 {
            if self.fence.is_none() {
                if !complete {
                    return vec![];
                }
                self.fence = Some((marker.unwrap(), count));
                let label = trimmed[count..].trim();
                return vec![Row {
                    line: Line::from(Span::styled(
                        crate::text::clip(
                            &format!("{} ─────", if label.is_empty() { "code" } else { label }),
                            width,
                        ),
                        Style::new().fg(crate::theme::color(crate::theme::Tone::Muted)),
                    )),
                    end: Cursor {
                        word: 1,
                        character: 0,
                    },
                }];
            } else if self
                .fence
                .is_some_and(|(m, n)| marker == Some(m) && count >= n)
                && trimmed[count..].trim().is_empty()
            {
                if !complete {
                    return vec![];
                }
                self.fence = None;
                return vec![Row {
                    line: Line::from(Span::styled(
                        "─".repeat(width.min(24)),
                        Style::new().fg(crate::theme::color(crate::theme::Tone::Muted)),
                    )),
                    end: Cursor {
                        word: 1,
                        character: 0,
                    },
                }];
            }
        }
        if self.fence.is_some() {
            let chars: Vec<char> = clean.replace('\t', "    ").chars().collect();
            let mut rows = Vec::new();
            let mut content = String::new();
            let mut used = 0;
            let limit = width.saturating_sub(2).max(1);
            for (index, c) in chars.iter().enumerate().skip(cursor.character) {
                let cells = crate::text::width(&c.to_string());
                if used + cells > limit && used > 0 {
                    rows.push(Row {
                        line: code_line(std::mem::take(&mut content)),
                        end: Cursor {
                            word: 0,
                            character: index,
                        },
                    });
                    used = 0;
                }
                if cells <= limit {
                    content.push(*c);
                    used += cells;
                }
            }
            rows.push(Row {
                line: code_line(content),
                end: Cursor {
                    word: 0,
                    character: chars.len(),
                },
            });
            return rows;
        }
        let heading = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&heading) && trimmed[heading..].starts_with(' ') {
            let content = trimmed[heading..].trim_start();
            return wrap_spans(
                inline(content)
                    .into_iter()
                    .map(|mut span| {
                        span.style = span.style.patch(
                            Style::new()
                                .fg(crate::theme::color(crate::theme::Tone::Accent))
                                .add_modifier(Modifier::BOLD),
                        );
                        span
                    })
                    .collect(),
                width,
                "",
                "",
                cursor,
            );
        }
        let (content, first, hanging) = if let Some(content) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            (content, "• ".to_string(), "  ".to_string())
        } else if let Some(content) = trimmed.strip_prefix("> ") {
            (content, "│ ".to_string(), "│ ".to_string())
        } else if let Some((number, content)) = trimmed.split_once(". ").filter(|(number, _)| {
            !number.is_empty() && number.len() <= 4 && number.chars().all(|c| c.is_ascii_digit())
        }) {
            (content, format!("{number}. "), " ".repeat(number.len() + 2))
        } else {
            (
                clean.as_str(),
                self.continuation.clone().unwrap_or_default(),
                self.continuation.clone().unwrap_or_default(),
            )
        };
        wrap_spans(inline(content), width, &first, &hanging, cursor)
    }
}

pub fn lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut presentation = Presentation::default();
    text.lines()
        .flat_map(|line| presentation.line(line, width.clamp(1, 88), true))
        .collect()
}

/// A line-oriented streaming view. Completed rows enter scrollback immediately;
/// only the last row is provisional and may be redrawn as its next word arrives.
/// Unclosed inline delimiters are held until they close (bounded to 4096 bytes).
#[derive(Default)]
pub struct Stream {
    presentation: Presentation,
    pending: String,
    committed: Cursor,
    visible: bool,
}
impl Stream {
    pub fn write(&mut self, io: &mut impl std::io::Write, text: &str, width: usize) {
        for c in sanitize(text).chars() {
            if c == '\n' {
                self.render(io, width, true);
                self.pending.clear();
                self.committed = Cursor::default();
                self.presentation.continuation = None;
            } else {
                self.pending.push(c);
                if (self.pending.len() >= 4096 && c.is_whitespace()) || self.pending.len() >= 8192 {
                    // Bound unfinished lines and parsing work. This rollover may
                    // end a display row early; raw logs and copying are untouched.
                    let continuation = if self.pending.starts_with("- ")
                        || self.pending.starts_with("* ")
                        || self.pending.starts_with("+ ")
                    {
                        Some("  ".into())
                    } else if self.pending.starts_with("> ") {
                        Some("│ ".into())
                    } else if let Some((number, _)) =
                        self.pending.split_once(". ").filter(|(number, _)| {
                            !number.is_empty()
                                && number.len() <= 4
                                && number.chars().all(|c| c.is_ascii_digit())
                        })
                    {
                        Some(" ".repeat(number.len() + 2))
                    } else {
                        self.presentation.continuation.clone()
                    };
                    self.render(io, width, true);
                    self.pending.clear();
                    self.committed = Cursor::default();
                    self.presentation.continuation = continuation;
                }
            }
        }
        if !self.pending.is_empty() {
            self.render(io, width, false);
        }
    }

    pub fn flush(&mut self, io: &mut impl std::io::Write, width: usize) {
        if !self.pending.is_empty() || self.visible {
            self.render(io, width, true);
            self.pending.clear();
            self.committed = Cursor::default();
        }
        self.presentation = Presentation::default();
    }
    fn render(&mut self, io: &mut impl std::io::Write, width: usize, complete: bool) {
        let margin = if width >= 24 {
            2
        } else if width >= 10 {
            1
        } else {
            0
        };
        let column = width.saturating_sub(margin * 2).clamp(1, 88);
        let safe = if complete || self.presentation.fence.is_some() {
            self.pending.as_str()
        } else {
            stable_inline(&self.pending)
        };
        let rows = self
            .presentation
            .render(safe, column, complete, self.committed);
        if self.visible {
            let _ = write!(io, "\r\x1b[2K");
            self.visible = false;
        }
        for (index, row) in rows.iter().enumerate() {
            let _ = write!(io, "{}", " ".repeat(margin));
            write_ansi(io, &row.line);
            if complete || index + 1 < rows.len() {
                let _ = write!(io, "\r\n");
                self.committed = row.end;
            } else {
                self.visible = true;
            }
        }
    }
}
fn stable_inline(text: &str) -> &str {
    if text.len() > 4096 {
        return text;
    }
    let trimmed = text.trim_start();
    let (mut remaining, mut offset) = if let Some(content) = trimmed.strip_prefix("* ") {
        (content, text.len() - trimmed.len() + 2)
    } else {
        (text, 0)
    };
    while let Some(start) = remaining.find(['`', '*']) {
        let tail = &remaining[start..];
        let marker = if tail.starts_with("**") {
            "**"
        } else if tail.starts_with('`') {
            "`"
        } else {
            "*"
        };
        if let Some(end) = tail[marker.len()..].find(marker) {
            let consumed = start + marker.len() * 2 + end;
            offset += consumed;
            remaining = &remaining[consumed..];
        } else {
            return &text[..offset + start];
        }
    }
    // The last unfinished word may move as more characters arrive. Holding
    // it preserves word boundaries while completed rows remain immutable.
    text.char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or("", |(index, c)| &text[..index + c.len_utf8()])
}
fn code_line(body: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "│ ",
            Style::new().fg(crate::theme::color(crate::theme::Tone::Muted)),
        ),
        Span::styled(
            body,
            Style::new().fg(crate::theme::color(crate::theme::Tone::Success)),
        ),
    ])
}
/// Emit styles chosen by our renderer; never serialize provider control bytes.
pub fn write_ansi(io: &mut impl std::io::Write, line: &Line<'_>) {
    let mut active = None;
    for span in &line.spans {
        if active != Some(span.style) {
            let _ = write!(
                io,
                "\x1b[0m{}",
                crate::theme::ansi(crate::theme::Tone::Body)
            );
            if span.style.add_modifier.contains(Modifier::BOLD) {
                let _ = write!(io, "\x1b[1m");
            }
            if span.style.add_modifier.contains(Modifier::ITALIC) {
                let _ = write!(io, "\x1b[3m");
            }
            if !crate::theme::no_color() {
                if let Some(Color::Indexed(n)) = span.style.fg {
                    let _ = write!(io, "\x1b[38;5;{n}m");
                }
                if let Some(Color::Indexed(n)) = span.style.bg {
                    let _ = write!(io, "\x1b[48;5;{n}m");
                }
            }
            active = Some(span.style);
        }
        let _ = write!(io, "{}", sanitize(&span.content));
    }
    let _ = write!(io, "\x1b[0m");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_copy_preserves_exact_newlines_and_markers() {
        assert_eq!(
            code_blocks("hello\n```rust\nlet x = `literal`;\n```\n"),
            vec![("rust".into(), "let x = `literal`;\n".into())]
        );
    }
    #[test]
    fn extracts_multiple_fence_styles() {
        assert_eq!(code_blocks("~~~py\nx\n~~~\n```\ny\n```\n").len(), 2);
    }
    fn project(output: &[u8]) -> Vec<String> {
        let text = String::from_utf8(output.to_vec()).unwrap();
        let mut rows = vec![String::new()];
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                let mut command = String::new();
                for c in chars.by_ref() {
                    command.push(c);
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
                if command == "2K" {
                    rows.last_mut().unwrap().clear();
                }
            } else if c == '\n' {
                rows.push(String::new());
            } else if c != '\r' {
                rows.last_mut().unwrap().push(c);
            }
        }
        rows
    }
    #[test]
    fn word_wrap_preserves_bold_and_list_hanging_indent() {
        let rows = lines(
            "- This **important phrase** belongs together with `code`.",
            22,
        );
        let text: Vec<String> = rows.iter().map(|row| row.to_string()).collect();
        assert_eq!(text[0], "• This important");
        assert!(text.iter().skip(1).all(|row| row.starts_with("  ")));
        assert!(text.iter().all(|row| crate::text::width(row) <= 22));
        assert!(
            rows.iter()
                .flat_map(|row| &row.spans)
                .filter(|span| span.content.contains("phrase"))
                .all(|span| span.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(!text.join(" ").contains("**"));
    }
    #[test]
    fn streamed_words_survive_growing_and_shrinking_widths() {
        for (first, second) in [(20, 80), (80, 20), (40, 24)] {
            let mut stream = Stream::default();
            let mut output = Vec::new();
            stream.write(&mut output, "one two three four five six seven", first);
            stream.write(&mut output, " eight nine ten eleven", second);
            stream.flush(&mut output, second);
            let words = project(&output)
                .join(" ")
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert_eq!(
                words,
                "one two three four five six seven eight nine ten eleven"
                    .split_whitespace()
                    .collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn stream_shows_long_prose_before_final_newline() {
        let mut stream = Stream::default();
        let mut output = Vec::new();
        stream.write(&mut output, "This paragraph is long enough to wrap over several rows without waiting for completion ", 24);
        let rows = project(&output);
        assert!(rows.len() >= 4);
        assert!(rows.iter().all(|row| crate::text::width(row) <= 22));
        stream.flush(&mut output, 24);
    }
    #[test]
    fn turn_boundary_resets_an_unclosed_fence() {
        let mut stream = Stream::default();
        let mut output = Vec::new();
        stream.write(&mut output, "```rust\nlet x=1;", 40);
        stream.flush(&mut output, 40);
        let boundary = output.len();
        stream.write(&mut output, "Hello next turn", 40);
        stream.flush(&mut output, 40);
        assert!(
            !String::from_utf8(output[boundary..].to_vec())
                .unwrap()
                .contains("│ ")
        );
    }
    #[test]
    fn no_newline_stream_memory_is_bounded_and_copy_text_unmodified() {
        let mut stream = Stream::default();
        let mut output = Vec::new();
        for _ in 0..2500 {
            stream.write(&mut output, "word ", 40);
            assert!(stream.pending.len() < 8192);
        }
        stream.flush(&mut output, 40);
        assert_eq!(
            project(&output)
                .join(" ")
                .split_whitespace()
                .filter(|word| *word == "word")
                .count(),
            2500
        );
    }
    #[test]
    fn unicode_and_provider_controls_cannot_escape_cell_bounds() {
        let source = "# Title\n\n- **界界** emoji 👋 café words more words\n```rust\n\x1b]52;c;evil\x07\t界界\n```";
        let rows = lines(source, 16);
        assert!(
            rows.iter()
                .all(|row| crate::text::width(&row.to_string()) <= 16)
        );
        assert!(rows.iter().all(|row| !row.to_string().contains('\x1b')));
    }
    #[test]
    fn star_list_streams_before_its_newline() {
        let mut stream = Stream::default();
        let mut output = Vec::new();
        stream.write(
            &mut output,
            "* Reading and editing many files in this project before its newline ",
            24,
        );
        let rows = project(&output);
        assert!(rows.len() > 2);
        assert!(rows[0].contains("• Reading"));
    }
}
