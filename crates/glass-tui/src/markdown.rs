//! Lightweight Markdown presentation; copying always uses the original text.
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Mermaid rendering gate (task six-wave-d-001). Off by default: a fenced
/// block stays a fenced block until the host enables diagram rendering
/// once at startup via [`enable_mermaid`].
static MERMAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn enable_mermaid(on: bool) {
    MERMAID.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn mermaid_enabled() -> bool {
    MERMAID.load(std::sync::atomic::Ordering::Relaxed)
}

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
    while let Some(start) = rest.find(['`', '*', '\\']) {
        if start > 0 {
            spans.push(Span::raw(rest[..start].to_string()));
        }
        let tail = &rest[start..];
        if let Some(escaped) = tail.strip_prefix('\\') {
            if let Some(c) = escaped.chars().next().filter(|c| c.is_ascii_punctuation()) {
                spans.push(Span::raw(c.to_string()));
                rest = &escaped[c.len_utf8()..];
                continue;
            }
            spans.push(Span::raw("\\"));
            rest = escaped;
            continue;
        }
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
    let prefix_style = Style::new().fg(crate::theme::color(if prefix.contains('✓') {
        crate::theme::Tone::Success
    } else if prefix.contains('│') {
        crate::theme::Tone::Muted
    } else {
        crate::theme::Tone::Accent
    }));
    let mut row = vec![Span::styled(prefix, prefix_style)];
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
    previous_blank: bool,
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
        if self.fence.is_none() && complete {
            if trimmed.is_empty() {
                if self.previous_blank {
                    return Vec::new();
                }
                self.previous_blank = true;
            } else {
                self.previous_blank = false;
            }
        }
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
                            &format!("╭─ {}", if label.is_empty() { "code" } else { label }),
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
                        format!("╰{}", "─".repeat(width.saturating_sub(1).min(30))),
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
                if heading == 1 { "◆ " } else { "▸ " },
                "  ",
                cursor,
            );
        }
        let indent = " ".repeat((clean.len() - trimmed.len()).min(width / 3));
        let (content, first, hanging) = if let Some(content) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            if let Some(content) = content
                .strip_prefix("[x] ")
                .or_else(|| content.strip_prefix("[X] "))
            {
                (content, format!("{indent}✓ "), format!("{indent}  "))
            } else if let Some(content) = content.strip_prefix("[ ] ") {
                (content, format!("{indent}□ "), format!("{indent}  "))
            } else {
                (
                    content,
                    format!("{indent}{} ", if indent.is_empty() { "•" } else { "◦" }),
                    format!("{indent}  "),
                )
            }
        } else if let Some(content) = trimmed.strip_prefix("> ") {
            (content, "│ ".to_string(), "│ ".to_string())
        } else if let Some((number, content)) = trimmed.split_once(". ").filter(|(number, _)| {
            !number.is_empty() && number.len() <= 4 && number.chars().all(|c| c.is_ascii_digit())
        }) {
            (
                content,
                format!("{indent}{number}. "),
                " ".repeat(indent.len() + number.len() + 2),
            )
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
    let width = width.clamp(1, 88);
    let mut presentation = Presentation::default();
    let mut table = None;
    let mut source = text.lines().peekable();
    let mut rows = Vec::new();
    // Mermaid blocks render as diagrams when the host enabled it; when the
    // parse or layout fails the block falls through and renders as
    // ordinary fenced code (never a halfway drawing).
    if mermaid_enabled()
        && let Some(rendered) = mermaid_diagram(text, width)
    {
        return rendered;
    }
    while let Some(line) = source.next() {
        if let Some(active) = table.as_mut() {
            if let Some(rendered) = crate::table::Table::row(active, line, width) {
                rows.extend(rendered);
                continue;
            }
            table = None;
        }
        if presentation.fence.is_none()
            && let Some(headers) = crate::table::cells(line)
            && source
                .peek()
                .is_some_and(|next| crate::table::separator(next, headers.len()))
        {
            source.next();
            let active = crate::table::Table::new(headers);
            rows.extend(active.header(width));
            table = Some(active);
            continue;
        }
        rows.extend(presentation.line(line, width, true));
    }
    rows
}

/// Render the first mermaid flowchart block in `text`, if any. Returns
/// `None` when there is no enabled, renderable mermaid block (including
/// unsupported kinds and overflow) so the caller keeps the fast path.
fn mermaid_diagram(text: &str, width: usize) -> Option<Vec<Line<'static>>> {
    // A lib crate cannot reach the host's parser: the host registers a
    // renderer at startup (`set_mermaid_renderer`). Unregistered = every
    // block stays code, byte-identical to the disabled build.
    let guard = MERMAID_RENDERER.lock().ok()?;
    let renderer = guard.as_ref()?;
    for (language, body) in code_blocks(text) {
        if language.trim() != "mermaid" {
            continue;
        }
        let muted = Style::new().fg(crate::theme::color(crate::theme::Tone::Muted));
        let mut rows = vec![Line::from(Span::styled(
            crate::text::clip("╭─ mermaid", width),
            muted,
        ))];
        let drawn = renderer(&body, width)?;
        for line in drawn {
            rows.push(Line::from(Span::styled(line, muted)));
        }
        rows.push(Line::from(Span::styled(
            format!("╰{}", "─".repeat(width.saturating_sub(1).min(30))),
            muted,
        )));
        return Some(rows);
    }
    None
}

static MERMAID_RENDERER: std::sync::Mutex<Option<MermaidRendererFn>> = std::sync::Mutex::new(None);
/// Host-provided mermaid renderer: parses the fence body and lays it out
/// under the given column, or `None` for anything it will not draw.
pub type MermaidRendererFn = Box<dyn Fn(&str, usize) -> Option<Vec<String>> + Send + Sync>;

/// Host-side registration of the mermaid renderer implementation. The host
/// returns `None` for anything it will not draw (parse failure, overflow).
pub fn set_mermaid_renderer(f: MermaidRendererFn) {
    *MERMAID_RENDERER.lock().unwrap() = Some(f);
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
    table_header: Option<String>,
    table: Option<crate::table::Table>,
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
        if let Some(header) = self.table_header.take() {
            let margin = if width >= 24 {
                2
            } else if width >= 10 {
                1
            } else {
                0
            };
            let column = width.saturating_sub(margin * 2).clamp(1, 88);
            write_block(io, self.presentation.line(&header, column, true), margin);
        }
        self.table = None;
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
        if self.presentation.fence.is_none() {
            if complete {
                if let Some(mut table) = self.table.take()
                    && let Some(rows) = table.row(&self.pending, column)
                {
                    write_block(io, rows, margin);
                    self.table = Some(table);
                    return;
                }
                if let Some(header) = self.table_header.take() {
                    let headers = crate::table::cells(&header).unwrap();
                    if crate::table::separator(&self.pending, headers.len()) {
                        let table = crate::table::Table::new(headers);
                        write_block(io, table.header(column), margin);
                        self.table = Some(table);
                        return;
                    }
                    write_block(io, self.presentation.line(&header, column, true), margin);
                }
                if crate::table::cells(&self.pending).is_some() {
                    self.table_header = Some(self.pending.clone());
                    return;
                }
            } else if self.table.is_some()
                || self.table_header.is_some()
                || self.pending.trim_start().starts_with('|')
            {
                return;
            }
        }
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
fn write_block(io: &mut impl std::io::Write, rows: Vec<Line<'static>>, margin: usize) {
    for row in rows {
        let _ = write!(io, "{}", " ".repeat(margin));
        write_ansi(io, &row);
        let _ = write!(io, "\r\n");
    }
}
pub(crate) fn border_style() -> Style {
    Style::new().fg(crate::theme::color(crate::theme::Tone::Border))
}
pub(crate) fn cell_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    wrap_spans(inline(&sanitize(text)), width, "", "", Cursor::default())
        .into_iter()
        .map(|r| r.line)
        .collect()
}
pub(crate) fn indented_cell_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    wrap_spans(
        inline(&sanitize(text)),
        width,
        "  ",
        "  ",
        Cursor::default(),
    )
    .into_iter()
    .map(|r| r.line)
    .collect()
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
    use std::io::Write as _;
    // A terminal surface may save/restore attributes around each write. Keep
    // SGR and its text in one write so styles survive cursor restoration.
    let mut row = Vec::with_capacity(line.spans.iter().map(|s| s.content.len()).sum::<usize>() + 96);
    let io_row = &mut row;
    let mut active = None;
    for span in &line.spans {
        let style = line.style.patch(span.style);
        if active != Some(style) {
            let _ = write!(
                io_row,
                "\x1b[0m{}",
                crate::theme::ansi(crate::theme::Tone::Body)
            );
            if style.add_modifier.contains(Modifier::BOLD) {
                let _ = write!(io_row, "\x1b[1m");
            }
            if style.add_modifier.contains(Modifier::ITALIC) {
                let _ = write!(io_row, "\x1b[3m");
            }
            if !crate::theme::no_color() {
                if let Some(Color::Indexed(n)) = style.fg {
                    let _ = write!(io_row, "\x1b[38;5;{n}m");
                }
                if let Some(Color::Indexed(n)) = style.bg {
                    let _ = write!(io_row, "\x1b[48;5;{n}m");
                }
            }
            active = Some(style);
        }
        let _ = write!(io_row, "{}", sanitize(&span.content));
    }
    let _ = write!(io_row, "\x1b[0m");
    let _ = io.write_all(&row);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn styled_row_is_one_terminal_write() {
        struct Sink { writes: usize, bytes: Vec<u8> }
        impl std::io::Write for Sink {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.writes += 1;
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let mut sink=Sink {writes:0, bytes:Vec::new()};
        let line=Line::from(vec![Span::styled("command",Style::new().fg(Color::Indexed(183))),Span::raw(" output")]);
        write_ansi(&mut sink,&line);
        assert_eq!(sink.writes,1,"SGR and text must share the cursor save/restore scope");
        let text=String::from_utf8(sink.bytes).unwrap();
        assert!(text.contains("command") && text.contains(" output"));
    }

    #[test]
    fn native_blocks_match_streaming_at_phone_and_desktop_widths() {
        let source = "# Results\n\n\n| Change | Status |\n| --- | --- |\n| `src/lib.rs` | **Done** |\n| 界 Unicode | ready to ship |\n\n- [x] Built\n  - Nested detail\n- [ ] Review\n\n```rust\nlet x = 1;\n```\nFinal text";
        for width in [20usize, 40, 90] {
            let margin = if width >= 24 { 2 } else { 1 };
            let expected = lines(source, width - margin * 2)
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>();
            let mut stream = Stream::default();
            let mut output = Vec::new();
            for c in source.chars() {
                stream.write(&mut output, &c.to_string(), width);
            }
            stream.flush(&mut output, width);
            let actual = project(&output)
                .into_iter()
                .map(|row| {
                    row.trim_end()
                        .strip_prefix(&" ".repeat(margin))
                        .unwrap_or(row.trim_end())
                        .to_string()
                })
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>();
            let expected = expected
                .into_iter()
                .map(|r| r.trim_end().to_string())
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "width {width}");
            assert!(
                actual
                    .iter()
                    .all(|r| crate::text::width(r) <= width - margin * 2)
            );
            assert!(actual.iter().any(|r| r.contains("✓ Built")));
            assert!(actual.iter().any(|r| r.contains("◦ Nested")));
        }
    }
    #[test]
    fn table_candidates_fences_and_escapes_do_not_drop_content() {
        for source in [
            "| plain | prose |",
            "| plain | prose |\nNot a table",
            "```text\n| one | two |\n| --- | --- |\n```",
            "| A | B |\n| --- | --- |\n| \\*literal\\* | x\\|y |",
        ] {
            let mut stream = Stream::default();
            let mut output = Vec::new();
            stream.write(&mut output, source, 80);
            stream.flush(&mut output, 80);
            let text = project(&output).join("\n");
            assert!(
                text.contains(if source.contains("literal") {
                    "*literal*"
                } else if source.contains("plain") {
                    "plain"
                } else {
                    "one"
                }),
                "{text}"
            );
            if source.contains("literal") {
                assert!(text.contains("x|y"));
            }
        }
    }
    #[test]
    fn extra_vertical_spacing_is_collapsed() {
        let rows = lines("First\n\n\n\nSecond", 40);
        assert_eq!(rows.len(), 3);
    }

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

    const DIAGRAM: &str = "```mermaid\nflowchart TD\n A[Start] -->|go| B[End]\n```\n";

    #[test]
    fn mermaid_off_renders_the_fenced_source() {
        // Global state from sibling tests leaks in: reset both gates.
        enable_mermaid(false);
        set_mermaid_renderer(Box::new(|_, _| None));
        let off = lines(DIAGRAM, 60);
        // When never registered, the block is ordinary code: byte-identical
        // to the disabled build. Assert on the plain joined text.
        let text: String = off.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
        assert!(text.contains("flowchart TD"), "{text}");
        assert!(!text.contains("─▶"), "{text}");
    }

    #[test]
    fn mermaid_enabled_renders_its_diagram() {
        enable_mermaid(true);
        // A stand-in renderer: the real one lives in the glass binary and
        // cannot be reached from this lib test. Same contract — parse, lay
        // out, None on refusal.
        set_mermaid_renderer(Box::new(|_body, width| {
            (width >= 12).then(|| vec!["[Start] ─ go ─▶ [End]".to_string()])
        }));
        let on = lines(DIAGRAM, 60);
        let text: String = on.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
        assert!(text.contains("─ go ─▶"), "{text}");
        // The header and footer frame remain, and the raw source does not.
        assert!(!text.contains("flowchart TD"), "{text}");
    }
}
