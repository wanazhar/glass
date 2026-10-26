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
    let mut clean = String::with_capacity(text.len());
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c == '\u{1b}' || c == '\u{009b}' || c == '\u{009d}' {
            skip_terminal_sequence(&chars, &mut index);
            continue;
        }
        if matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}')
        {
            // Show direction overrides explicitly: filenames and tool output
            // must not silently reorder their displayed neighbors.
            clean.extend(c.escape_unicode());
        } else if !c.is_control() || matches!(c, '\n' | '\t') {
            clean.push(c);
        }
        index += 1;
    }
    clean
}

fn skip_terminal_sequence(chars: &[char], index: &mut usize) {
    let Some(first) = chars.get(*index).copied() else {
        return;
    };
    *index += 1;
    let introducer = if first == '\u{1b}' {
        let Some(next) = chars.get(*index).copied() else {
            return;
        };
        *index += 1;
        next
    } else {
        first
    };

    if introducer == '[' || first == '\u{009b}' {
        while let Some(c) = chars.get(*index).copied() {
            *index += 1;
            if ('@'..='~').contains(&c) {
                break;
            }
        }
    } else if matches!(introducer, ']' | 'P' | '^' | '_' | 'X') || first == '\u{009d}' {
        while let Some(c) = chars.get(*index).copied() {
            *index += 1;
            if matches!(c, '\u{0007}' | '\u{009c}') {
                break;
            }
            if c == '\u{1b}' && chars.get(*index) == Some(&'\\') {
                *index += 1;
                break;
            }
        }
    } else if first == '\u{1b}' && (' '..='/').contains(&introducer) {
        while let Some(c) = chars.get(*index).copied() {
            *index += 1;
            if ('0'..='~').contains(&c) {
                break;
            }
        }
    }
}

fn numbered_item(text: &str) -> Option<(&str, &str)> {
    let at = text.find(['.', ')'])?;
    let number = &text[..at];
    if number.is_empty() || number.len() > 9 || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((number, text[at + 1..].strip_prefix(' ')?))
}

fn inline(text: &str) -> Vec<Span<'static>> {
    inline_depth(text, 0)
}
fn inline_depth(text: &str, depth: usize) -> Vec<Span<'static>> {
    if depth >= 16 {
        return vec![Span::raw(text.to_owned())];
    }
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(['`', '*', '_', '~', '[', '!', '\\', '$']) {
        if start > 0 {
            spans.push(Span::raw(rest[..start].to_owned()));
        }
        let tail = &rest[start..];
        let math_delimiters = if tail.starts_with("\\(") {
            Some(("\\(", "\\)"))
        } else if tail.starts_with("\\[") {
            Some(("\\[", "\\]"))
        } else if tail.starts_with("$$") {
            Some(("$$", "$$"))
        } else if tail.starts_with('$') {
            Some(("$", "$"))
        } else {
            None
        };
        if let Some((open, close)) = math_delimiters
            && let Some(end) = tail[open.len()..].find(close)
        {
            let body = &tail[open.len()..open.len() + end];
            let body = if open == "$$" || open == "\\[" {
                body.trim()
            } else {
                body
            };
            if !body.starts_with(char::is_whitespace)
                && !body.ends_with(char::is_whitespace)
                && !(open == "$"
                    && tail[open.len() + end + close.len()..]
                        .starts_with(|c: char| c.is_ascii_digit()))
            {
                let consumed = open.len() + end + close.len();
                match crate::math::render(body) {
                    Ok(rendered) => spans.push(Span::styled(
                        rendered,
                        Style::new().fg(crate::theme::color(crate::theme::Tone::Secondary)),
                    )),
                    Err(_) => spans.push(Span::raw(tail[..consumed].to_owned())),
                }
                rest = &tail[consumed..];
                continue;
            }
        }
        if let Some(escaped) = tail.strip_prefix('\\')
            && let Some(c) = escaped.chars().next().filter(|c| c.is_ascii_punctuation())
        {
            spans.push(Span::raw(c.to_string()));
            rest = &escaped[c.len_utf8()..];
            continue;
        }
        let image = tail.starts_with("![");
        let link = if image { &tail[1..] } else { tail };
        if let Some(label) = link.strip_prefix('[')
            && let Some((label, destination)) = label.split_once("](")
            && !label.contains('[')
            && let Some(end) = destination.find(')')
        {
            let url = &destination[..end];
            if !url.is_empty() {
                if image {
                    spans.push(Span::styled(
                        "▧ ",
                        Style::new().fg(crate::theme::color(crate::theme::Tone::Accent)),
                    ));
                }
                spans.extend(inline_depth(label, depth + 1).into_iter().map(|mut span| {
                    span.style = span
                        .style
                        .fg(crate::theme::color(crate::theme::Tone::Accent));
                    span
                }));
                if url != label {
                    spans.push(Span::styled(
                        format!(" ({url})"),
                        Style::new().fg(crate::theme::color(crate::theme::Tone::Muted)),
                    ));
                }
                let consumed = usize::from(image) + 1 + label.len() + 2 + end + 1;
                rest = &tail[consumed..];
                continue;
            }
        }
        let previous = text[..text.len() - rest.len() + start].chars().next_back();
        let underscore_in_word =
            tail.starts_with('_') && previous.is_some_and(char::is_alphanumeric);
        let marker = if tail.starts_with('`') {
            let count = tail.chars().take_while(|c| *c == '`').count();
            &tail[..count]
        } else if tail.starts_with("***") {
            "***"
        } else if tail.starts_with("___") && !underscore_in_word {
            "___"
        } else if tail.starts_with("**") {
            "**"
        } else if tail.starts_with("__") && !underscore_in_word {
            "__"
        } else if tail.starts_with("~~") {
            "~~"
        } else if tail.starts_with('*') {
            "*"
        } else if tail.starts_with('_') && !underscore_in_word {
            "_"
        } else {
            ""
        };
        if !marker.is_empty()
            && let Some(end) = tail[marker.len()..].find(marker)
        {
            let body = &tail[marker.len()..marker.len() + end];
            let remaining = &tail[marker.len() * 2 + end..];
            if !body.is_empty()
                && (marker.starts_with('`')
                    || (!body.starts_with(char::is_whitespace)
                        && !body.ends_with(char::is_whitespace)))
                && (!marker.starts_with('_')
                    || remaining
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_alphanumeric()))
            {
                let style = match marker {
                    "***" | "___" => Style::new()
                        .fg(crate::theme::color(crate::theme::Tone::Secondary))
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                    "**" | "__" => Style::new()
                        .fg(crate::theme::color(crate::theme::Tone::Secondary))
                        .add_modifier(Modifier::BOLD),
                    "~~" => Style::new()
                        .fg(crate::theme::color(crate::theme::Tone::Muted))
                        .add_modifier(Modifier::CROSSED_OUT),
                    "*" | "_" => Style::new()
                        .fg(crate::theme::color(crate::theme::Tone::Secondary))
                        .add_modifier(Modifier::ITALIC),
                    _ => Style::new()
                        .fg(crate::theme::color(crate::theme::Tone::Success))
                        .bg(crate::theme::color(crate::theme::Tone::Surface)),
                };
                if marker.starts_with('`') {
                    spans.push(Span::styled(body.to_owned(), style));
                } else {
                    spans.extend(inline_depth(body, depth + 1).into_iter().map(|mut span| {
                        span.style = span.style.patch(style);
                        span
                    }));
                }
                rest = remaining;
                continue;
            }
        }
        let c = tail.chars().next().unwrap();
        spans.push(Span::raw(c.to_string()));
        rest = &tail[c.len_utf8()..];
    }
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_owned()));
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
            if c.is_whitespace()
                && !(c == ' '
                    && span.style.bg == Some(crate::theme::color(crate::theme::Tone::Surface)))
            {
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
            .map(|(c, _)| crate::text::char_width(*c))
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
            let cells = crate::text::char_width(*c);
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
    language: String,
    syntax: crate::syntax::State,
    fence: Option<(char, usize)>,
    continuation: Option<String>,
    previous_blank: bool,
}
impl Presentation {
    pub fn line(&mut self, raw: &str, width: usize, complete: bool) -> Vec<Line<'static>> {
        self.render(raw, width.max(1), complete, Cursor::default())
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
                self.language = label.to_owned();
                self.syntax = crate::syntax::State::default();
                let label = if label.eq_ignore_ascii_case("mermaid") {
                    if mermaid_enabled() {
                        "mermaid · source"
                    } else {
                        "mermaid · source (disabled)"
                    }
                } else {
                    label
                };
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
                        format!("╰{}", "─".repeat(width.saturating_sub(1))),
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
            let expanded = clean.replace('\t', "    ");
            let (highlighted, next) = crate::syntax::line(&expanded, &self.language, &self.syntax);
            if complete {
                self.syntax = next;
            }
            let chars: Vec<_> = highlighted
                .iter()
                .flat_map(|span| span.content.chars().map(move |c| (c, span.style)))
                .collect();
            let mut rows = Vec::new();
            let mut content = Vec::new();
            let mut used = 0;
            let prefix = crate::text::clip("│ ", width.saturating_sub(1));
            let limit = width.saturating_sub(crate::text::width(&prefix)).max(1);
            for (index, (c, style)) in chars.iter().enumerate().skip(cursor.character) {
                let cells = crate::text::char_width(*c);
                if used + cells > limit && used > 0 {
                    rows.push(Row {
                        line: code_line(std::mem::take(&mut content), &prefix),
                        end: Cursor {
                            word: 0,
                            character: index,
                        },
                    });
                    used = 0;
                }
                if cells <= limit {
                    content.push((*c, *style));
                    used += cells;
                }
            }
            rows.push(Row {
                line: code_line(content, &prefix),
                end: Cursor {
                    word: 0,
                    character: chars.len(),
                },
            });
            return rows;
        }
        // Structural rules use the same bounded reading column as all blocks.
        let rule = trimmed.chars().filter(|c| !c.is_whitespace());
        let rule_marker = rule.clone().next();
        if rule.clone().take(3).count() == 3
            && rule_marker.is_some_and(|c| matches!(c, '-' | '*' | '_'))
            && rule.clone().all(|c| Some(c) == rule_marker)
        {
            if !complete {
                return vec![];
            }
            return vec![Row {
                line: Line::from(Span::styled("─".repeat(width), border_style())),
                end: Cursor {
                    word: 1,
                    character: 0,
                },
            }];
        }
        if let Some(rest) = trimmed.strip_prefix("> [!")
            && let Some((kind, body)) = rest.split_once(']')
        {
            let tone = match kind {
                "NOTE" => Some(crate::theme::Tone::Secondary),
                "TIP" => Some(crate::theme::Tone::Success),
                "IMPORTANT" => Some(crate::theme::Tone::Accent),
                "WARNING" => Some(crate::theme::Tone::Warning),
                "CAUTION" => Some(crate::theme::Tone::Error),
                _ => None,
            };
            if let Some(tone) = tone {
                if !complete {
                    return Vec::new();
                }
                let title = kind[..1].to_owned() + &kind[1..].to_ascii_lowercase();
                let mut spans = vec![Span::styled(
                    title,
                    Style::new()
                        .fg(crate::theme::color(tone))
                        .add_modifier(Modifier::BOLD),
                )];
                if !body.trim().is_empty() {
                    spans.push(Span::raw(" · "));
                    spans.extend(inline(body.trim()));
                }
                return wrap_spans(spans, width, "│ ", "│ ", cursor);
            }
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
        } else if trimmed.starts_with('>') {
            let mut content = trimmed;
            let mut depth = 0;
            while let Some(after) = content.strip_prefix('>') {
                depth += 1;
                content = after.strip_prefix(' ').unwrap_or(after);
            }
            let prefix = format!("{indent}{}", "│ ".repeat(depth.min(8)));
            (content, prefix.clone(), prefix)
        } else if let Some((number, content)) = numbered_item(trimmed) {
            (
                content,
                format!("{indent}{number}. "),
                " ".repeat(indent.len() + number.len() + 2),
            )
        } else {
            (
                trimmed,
                self.continuation.clone().unwrap_or_else(|| indent.clone()),
                self.continuation.clone().unwrap_or(indent),
            )
        };
        let tone = [
            ("○ `T", crate::theme::Tone::Muted),
            ("◆ `T", crate::theme::Tone::Accent),
            ("✓ `T", crate::theme::Tone::Success),
            ("! `T", crate::theme::Tone::Warning),
            ("– `T", crate::theme::Tone::Muted),
        ]
        .into_iter()
        .find_map(|(prefix, tone)| trimmed.starts_with(prefix).then_some(tone));
        let mut rows = wrap_spans(inline(content), width, &first, &hanging, cursor);
        if let Some(tone) = tone {
            for row in &mut rows {
                for span in &mut row.line.spans {
                    span.style = span.style.fg(crate::theme::color(tone));
                }
            }
        }
        rows
    }
}

const DIAGRAM_BYTES: usize = 128 * 1024;
const DIAGRAM_LINES: usize = 256;
fn diagram_open(line: &str) -> Option<(char, usize)> {
    let line = line.trim();
    if line == "$$" {
        return Some(('$', 2));
    }
    if line == "\\[" {
        return Some(('[', 1));
    }
    let marker = line.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = line.chars().take_while(|c| *c == marker).count();
    (count >= 3
        && matches!(
            line[count..].trim().to_ascii_lowercase().as_str(),
            "mermaid" | "math" | "latex" | "tex"
        ))
    .then_some((marker, count))
}
fn diagram_close(line: &str, marker: char, count: usize) -> bool {
    let line = line.trim();
    if marker == '$' {
        return line == "$$";
    }
    if marker == '[' {
        return line == "\\]";
    }
    line.len() >= count && line.chars().all(|c| c == marker)
}

fn native_block_enabled(line: &str) -> bool {
    let label = line.trim().trim_start_matches(['`', '~']).trim();
    !label.eq_ignore_ascii_case("mermaid") || mermaid_enabled()
}
// A document fence is not a single equation. Preview only the inert document
// wrapper; leave unsupported preamble commands visible through source fallback.
fn latex_document(body: &str, width: usize) -> Option<Vec<Line<'static>>> {
    if body.len() > 16 * 1024 {
        return None;
    }
    let (preamble, rest) = body.split_once("\\begin{document}")?;
    let (content, trailing) = rest.rsplit_once("\\end{document}")?;
    if !trailing.trim().is_empty() || content.contains("\\begin{document}") {
        return None;
    }
    if !preamble.lines().all(|line| {
        let line = line.trim();
        line.is_empty()
            || line.starts_with('%')
            || (line.starts_with("\\documentclass{") && line.ends_with('}'))
    }) {
        return None;
    }
    let mut rows = vec![Line::from(Span::styled(
        crate::text::clip("LaTeX · document preview", width),
        Style::new()
            .fg(crate::theme::color(crate::theme::Tone::Accent))
            .add_modifier(Modifier::BOLD),
    ))];
    rows.extend(lines(content.trim(), width));
    Some(rows)
}
fn math_block(body: &str, source: &str, width: usize) -> Vec<Line<'static>> {
    let color = Style::new().fg(crate::theme::color(crate::theme::Tone::Secondary));
    match crate::math::display(body, width) {
        Ok(text) => {
            let mut rows = vec![Line::from(Span::styled(
                crate::text::clip("∑ Math", width),
                color.add_modifier(Modifier::BOLD),
            ))];
            for line in text {
                rows.extend(
                    if crate::text::width(&line) <= width {
                        vec![line]
                    } else {
                        crate::text::wrap_words(&line, width)
                    }
                    .into_iter()
                    .map(|line| Line::from(Span::styled(line, color))),
                );
            }
            rows
        }
        Err(reason) => {
            let mut rows = Vec::new();
            for line in source.lines() {
                rows.extend(
                    crate::text::wrap_words(&sanitize(line), width)
                        .into_iter()
                        .map(|line| Line::from(Span::styled(line, color))),
                );
            }
            rows.extend(
                crate::text::wrap_words(&format!("Math source · {reason}"), width)
                    .into_iter()
                    .map(|line| {
                        Line::from(Span::styled(
                            line,
                            Style::new().fg(crate::theme::color(crate::theme::Tone::Muted)),
                        ))
                    }),
            );
            rows
        }
    }
}

pub fn lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let width = width.clamp(1, 88);
    let mut presentation = Presentation::default();
    let mut table = None;
    let mut source = text.lines().peekable();
    let mut rows = Vec::new();
    while let Some(line) = source.next() {
        // Render only this diagram block; surrounding prose and later blocks remain visible.
        if presentation.fence.is_none()
            && native_block_enabled(line)
            && line.len() < DIAGRAM_BYTES
            && let Some((marker, count)) = diagram_open(line)
        {
            let mut next = source.clone();
            let mut block = format!("{line}\n");
            let mut closed = false;
            for body in next.by_ref().take(DIAGRAM_LINES - 1) {
                if block.len() + body.len() + 1 > DIAGRAM_BYTES {
                    break;
                }
                block.push_str(body);
                block.push('\n');
                if diagram_close(body, marker, count) {
                    closed = true;
                    break;
                }
            }
            if closed && let Some(diagram) = mermaid_diagram(&block, width) {
                rows.extend(diagram);
                table = None;
                presentation.previous_blank = false;
                source = next;
                continue;
            }
        }
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
            let separator = source.next().unwrap();
            let active = crate::table::Table::new(headers, separator);
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
    if matches!(text.lines().next()?.trim(), "$$" | "\\[") {
        let body = text
            .lines()
            .skip(1)
            .take(text.lines().count().saturating_sub(2))
            .collect::<Vec<_>>()
            .join("\n");
        return Some(math_block(&body, text, width));
    }
    for (language, body) in code_blocks(text) {
        if matches!(
            language.trim().to_ascii_lowercase().as_str(),
            "math" | "latex" | "tex"
        ) {
            if !language.trim().eq_ignore_ascii_case("math")
                && let Some(rows) = latex_document(&body, width)
            {
                return Some(rows);
            }
            return Some(math_block(&body, text, width));
        }
        if !language.trim().eq_ignore_ascii_case("mermaid") {
            continue;
        }
        let guard = MERMAID_RENDERER.lock().ok()?;
        let renderer = guard.as_ref()?;
        let mut rows = vec![Line::from(Span::styled(
            crate::text::clip("◇ Mermaid", width),
            Style::new()
                .fg(crate::theme::color(crate::theme::Tone::Accent))
                .add_modifier(Modifier::BOLD),
        ))];
        let drawn = match renderer(&body, width) {
            Ok(drawn) => drawn,
            Err(reason) => {
                let mut presentation = Presentation::default();
                let mut rows = Vec::new();
                for line in text.lines() {
                    rows.extend(presentation.line(line, width, true));
                }
                let note = format!("Diagram source · {}", sanitize(&reason));
                rows.extend(
                    crate::text::wrap_words(&note, width)
                        .into_iter()
                        .map(|line| {
                            Line::from(Span::styled(
                                line,
                                Style::new().fg(crate::theme::color(crate::theme::Tone::Muted)),
                            ))
                        }),
                );
                return Some(rows);
            }
        };
        for line in drawn {
            let heading = matches!(
                line.as_str(),
                "Nodes" | "Connections" | "Participants" | "Messages"
            ) || line.starts_with("Group · ")
                || line.starts_with('╭')
                || line.starts_with("Section ·")
                || line.starts_with("Bars ·")
                || line.starts_with("Line ·")
                || line.starts_with("Gantt ·")
                || line.starts_with("Sankey ·")
                || line.starts_with("Pie proportions");
            let tone = if heading {
                crate::theme::Tone::Accent
            } else if line.contains("──▶")
                || line.contains("┄┄▶")
                || line.contains("━━▶")
                || line.trim() == "▼"
                || line.trim() == "│"
                || line.contains('█')
                || line.contains('━')
                || line.contains('●')
            {
                crate::theme::Tone::Secondary
            } else {
                crate::theme::Tone::Body
            };
            let style = Style::new().fg(crate::theme::color(tone));
            rows.push(Line::from(Span::styled(
                line,
                if heading {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                },
            )));
        }
        rows.push(Line::default());
        return Some(rows);
    }
    None
}

static MERMAID_RENDERER: std::sync::Mutex<Option<MermaidRendererResultFn>> =
    std::sync::Mutex::new(None);
/// Host-provided mermaid renderer: parses the fence body and lays it out
/// under the given column, or `None` for anything it will not draw.
pub type MermaidRendererFn = Box<dyn Fn(&str, usize) -> Option<Vec<String>> + Send + Sync>;
/// Extended renderer with a user-readable reason when source must be shown.
pub type MermaidRendererResultFn =
    Box<dyn Fn(&str, usize) -> Result<Vec<String>, String> + Send + Sync>;

/// Host-side registration of the mermaid renderer implementation. The host
/// returns `None` for anything it will not draw (parse failure, overflow).
pub fn set_mermaid_renderer(f: MermaidRendererFn) {
    set_mermaid_renderer_detailed(Box::new(move |body, width| {
        f(body, width).ok_or_else(|| "unsupported diagram syntax or layout".into())
    }));
}
pub fn set_mermaid_renderer_detailed(f: MermaidRendererResultFn) {
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
    diagram: Option<PendingDiagram>,
}
struct PendingDiagram {
    source: String,
    marker: char,
    count: usize,
    lines: usize,
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
                    if let Some(diagram) = self.diagram.take() {
                        let margin = if width >= 24 {
                            2
                        } else if width >= 10 {
                            1
                        } else {
                            0
                        };
                        self.diagram_source(
                            io,
                            &diagram.source,
                            width.saturating_sub(margin * 2).clamp(1, 88),
                            margin,
                        );
                    }
                    // Bound unfinished lines and parsing work. This rollover may
                    // end a display row early; raw logs and copying are untouched.
                    let continuation = if self.pending.starts_with("- ")
                        || self.pending.starts_with("* ")
                        || self.pending.starts_with("+ ")
                    {
                        Some("  ".into())
                    } else if self.pending.starts_with("> ") {
                        Some("│ ".into())
                    } else if let Some((number, _)) = numbered_item(&self.pending) {
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
        if let Some(diagram) = self.diagram.take() {
            let margin = if width >= 24 {
                2
            } else if width >= 10 {
                1
            } else {
                0
            };
            let column = width.saturating_sub(margin * 2).clamp(1, 88);
            self.diagram_source(io, &diagram.source, column, margin);
        }
        self.table = None;
        self.presentation = Presentation::default();
    }
    fn diagram_source(
        &mut self,
        io: &mut impl std::io::Write,
        source: &str,
        column: usize,
        margin: usize,
    ) {
        for line in source.lines() {
            write_block(io, self.presentation.line(line, column, true), margin);
        }
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
        if let Some(mut diagram) = self.diagram.take() {
            if !complete {
                self.diagram = Some(diagram);
                return;
            }
            if diagram.lines >= DIAGRAM_LINES
                || diagram.source.len() + self.pending.len() + 1 > DIAGRAM_BYTES
            {
                self.diagram_source(io, &diagram.source, column, margin);
                // Continue this line through the ordinary code path after the cap.
            } else {
                diagram.source.push_str(&self.pending);
                diagram.source.push('\n');
                diagram.lines += 1;
                if diagram_close(&self.pending, diagram.marker, diagram.count) {
                    if let Some(rows) = mermaid_diagram(&diagram.source, column) {
                        write_block(io, rows, margin);
                    } else {
                        self.diagram_source(io, &diagram.source, column, margin);
                    }
                    self.presentation.previous_blank = false;
                } else {
                    self.diagram = Some(diagram);
                }
                return;
            }
        }
        if self.presentation.fence.is_none()
            && complete
            && native_block_enabled(&self.pending)
            && self.pending.len() < DIAGRAM_BYTES
            && let Some((marker, count)) = diagram_open(&self.pending)
        {
            if self.visible {
                let _ = write!(io, "\r\x1b[2K");
                self.visible = false;
            }
            if let Some(header) = self.table_header.take() {
                write_block(io, self.presentation.line(&header, column, true), margin);
            }
            self.table = None;
            self.diagram = Some(PendingDiagram {
                source: format!("{}\n", self.pending),
                marker,
                count,
                lines: 1,
            });
            return;
        }
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
                        let table = crate::table::Table::new(headers, &self.pending);
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
        let safe = if complete {
            self.pending.as_str()
        } else if self.presentation.fence.is_some() {
            crate::syntax::stable_prefix(&self.pending)
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
    let mut stable_end = 0;
    let trimmed = text.trim_start();
    let (mut remaining, mut offset) = if let Some(content) = trimmed.strip_prefix("* ") {
        (content, text.len() - trimmed.len() + 2)
    } else {
        (text, 0)
    };
    while let Some(start) = remaining.find(['`', '*', '_', '~', '[', '$', '\\']) {
        let tail = &remaining[start..];
        let delimiters = if tail.starts_with("\\(") {
            Some(("\\(", "\\)"))
        } else if tail.starts_with("\\[") {
            Some(("\\[", "\\]"))
        } else if tail.starts_with("$$") {
            Some(("$$", "$$"))
        } else if tail.starts_with('$') {
            Some(("$", "$"))
        } else {
            None
        };
        if let Some((open, close)) = delimiters {
            let Some(end) = tail[open.len()..].find(close) else {
                return &text[..offset + start];
            };
            let consumed = open.len() + end + close.len();
            offset += start + consumed;
            stable_end = offset;
            remaining = &tail[consumed..];
            continue;
        }
        if let Some(stripped) = tail.strip_prefix('\\') {
            let consumed = stripped.chars().next().map_or(1, |c| 1 + c.len_utf8());
            offset += start + consumed;
            remaining = &tail[consumed..];
            continue;
        }
        let previous = text[..offset + start].chars().next_back();
        if tail.starts_with('_') && previous.is_some_and(char::is_alphanumeric)
            || tail.starts_with('~') && !tail.starts_with("~~")
        {
            offset += start + 1;
            remaining = &tail[1..];
            continue;
        }
        if let Some(label) = tail.strip_prefix('[') {
            let Some(end) = label.find(']') else {
                return &text[..offset + start];
            };
            let after = &label[end + 1..];
            if after.is_empty() {
                return &text[..offset + start];
            }
            let consumed = if let Some(url) = after.strip_prefix('(') {
                let Some(close) = url.find(')') else {
                    return &text[..offset + start];
                };
                1 + end + 1 + 1 + close + 1
            } else {
                1 + end + 1
            };
            offset += start + consumed;
            stable_end = offset;
            remaining = &tail[consumed..];
            continue;
        }
        let marker = if tail.starts_with('`') {
            &tail[..tail.chars().take_while(|c| *c == '`').count()]
        } else if tail.starts_with("***") {
            "***"
        } else if tail.starts_with("___") {
            "___"
        } else if tail.starts_with("**") {
            "**"
        } else if tail.starts_with("__") {
            "__"
        } else if tail.starts_with("~~") {
            "~~"
        } else {
            &tail[..1]
        };
        if let Some(end) = tail[marker.len()..].find(marker) {
            let consumed = start + marker.len() * 2 + end;
            offset += consumed;
            stable_end = offset;
            remaining = &remaining[consumed..];
        } else {
            return &text[..offset + start];
        }
    }
    let whitespace = text
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(at, c)| at + c.len_utf8());
    &text[..whitespace.max(stable_end)]
}
fn code_line(body: Vec<(char, Style)>, prefix: &str) -> Line<'static> {
    let mut spans = vec![Span::styled(
        prefix.to_owned(),
        Style::new().fg(crate::theme::color(crate::theme::Tone::Muted)),
    )];
    for (c, style) in body {
        if spans.last().is_some_and(|span| span.style == style) {
            spans.last_mut().unwrap().content.to_mut().push(c);
        } else {
            spans.push(Span::styled(c.to_string(), style));
        }
    }
    Line::from(spans)
}
/// Emit styles chosen by our renderer; never serialize provider control bytes.
pub fn write_ansi(io: &mut impl std::io::Write, line: &Line<'_>) {
    use std::io::Write as _;
    // A terminal surface may save/restore attributes around each write. Keep
    // SGR and its text in one write so styles survive cursor restoration.
    let mut row =
        Vec::with_capacity(line.spans.iter().map(|s| s.content.len()).sum::<usize>() + 96);
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
    fn sanitizer_removes_complete_and_truncated_terminal_sequences() {
        assert_eq!(
            sanitize("before\u{1b}[32mgreen\u{1b}[0m after"),
            "beforegreen after"
        );
        assert_eq!(sanitize("\u{1b}]0;title\u{0007}safe"), "safe");
        assert_eq!(sanitize("tail\u{1b}[31"), "tail");
        assert_eq!(sanitize("\u{1b}(0x"), "x");
    }

    #[test]
    fn common_inline_markup_is_styled_without_hiding_destinations() {
        let spans = inline(
            "__bold__ _italic_ ~~old~~ ***both*** [Guide](https://example.test) ![Preview](image.png) snake_case ``a `tick` b``",
        );
        let text = spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        for marker in [
            "bold",
            "italic",
            "old",
            "both",
            "Guide",
            "https://example.test",
            "Preview",
            "image.png",
            "snake_case",
            "a `tick` b",
        ] {
            assert!(text.contains(marker), "{text}");
        }
        for (text, modifier) in [
            ("bold", Modifier::BOLD),
            ("italic", Modifier::ITALIC),
            ("old", Modifier::CROSSED_OUT),
            ("both", Modifier::BOLD | Modifier::ITALIC),
        ] {
            assert!(
                spans
                    .iter()
                    .any(|span| span.content == text && span.style.add_modifier.contains(modifier)),
                "{text}"
            );
        }
    }
    #[test]
    fn inline_code_preserves_spaces() {
        let rendered = lines("Use `a  b` here", 80)
            .iter()
            .map(ToString::to_string)
            .collect::<String>();
        assert!(rendered.contains("a  b"), "{rendered}");
    }
    #[test]
    fn blocks_remain_bounded_at_tiny_widths() {
        let source = "# Heading 界\n\n> Quoted **text**\n\n---\n\n- [x] Checked\n  1234. Nested list\n\n```rust\nlet 界 = 123456;\n```\n";
        for width in 1..=100 {
            for row in lines(source, width) {
                assert!(
                    crate::text::width(&row.to_string()) <= width,
                    "width {width}: {row}"
                );
            }
        }
    }
    #[test]
    fn styled_row_is_one_terminal_write() {
        struct Sink {
            writes: usize,
            bytes: Vec<u8>,
        }
        impl std::io::Write for Sink {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.writes += 1;
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut sink = Sink {
            writes: 0,
            bytes: Vec::new(),
        };
        let line = Line::from(vec![
            Span::styled("command", Style::new().fg(Color::Indexed(183))),
            Span::raw(" output"),
        ]);
        write_ansi(&mut sink, &line);
        assert_eq!(
            sink.writes, 1,
            "SGR and text must share the cursor save/restore scope"
        );
        let text = String::from_utf8(sink.bytes).unwrap();
        assert!(text.contains("command") && text.contains(" output"));
    }

    #[test]
    fn expanded_code_math_and_callouts_match_streaming() {
        let source = r####"> [!WARNING] Review before applying.
> Preserve this detail.

```rust
let text = r##"raw
still raw"##;
```
```python
def example():
    text = """triple
second line"""
    return text
```
```lua
local text = [[first
second]]
--[[ comment
still comment ]]
```
```diff
--- before
+++ after
@@ -1 +1 @@
-old
+new
```
$$
\begin{bmatrix}1 & 22 \\ 333 & 4\end{bmatrix}
$$

$$
\frac{x+1}{n}
$$

Costs $5-$10; math $x^2$.
"####;
        for width in [10usize, 20, 40, 90] {
            let margin = if width >= 24 { 2 } else { 1 };
            let expected = lines(source, width - margin * 2)
                .iter()
                .map(|r| r.to_string().trim_end().to_owned())
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>();
            let mut stream = Stream::default();
            let mut output = Vec::new();
            for c in source.chars() {
                stream.write(&mut output, &c.to_string(), width);
            }
            stream.flush(&mut output, width);
            let actual = project(&output)
                .iter()
                .map(|r| {
                    r.trim_end()
                        .strip_prefix(&" ".repeat(margin))
                        .unwrap_or(r.trim_end())
                        .to_owned()
                })
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{width}");
        }
        assert_eq!(
            inline("Costs $5-$10")
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "Costs $5-$10"
        );
    }
    #[test]
    fn unsupported_inline_math_keeps_delimiters_and_source() {
        for source in [r"\(\unknown_{x}\)", r"$\unknown_{x}$", r"$$\input{x}$$"] {
            assert_eq!(
                inline(source)
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>(),
                source
            );
        }
    }
    #[test]
    fn syntax_and_math_stream_match_replay() {
        let source = "```python\ndef fib(n): # readable\n    return \"text\" + 12\n```\nInline $E=mc^2$ and \\(\\alpha_i\\).\n\n$$\n\\frac{a}{b} \\leq \\infty\n$$\n\n```latex\n\\sqrt{x+1}\n```\nDone";
        for width in [20usize, 39, 65, 90] {
            let margin = if width >= 24 { 2 } else { 1 };
            let expected = lines(source, width - margin * 2)
                .iter()
                .map(|r| r.to_string().trim_end().to_owned())
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>();
            let mut stream = Stream::default();
            let mut output = Vec::new();
            for c in source.chars() {
                stream.write(&mut output, &c.to_string(), width);
            }
            stream.flush(&mut output, width);
            let actual = project(&output)
                .iter()
                .map(|r| {
                    r.trim_end()
                        .strip_prefix(&" ".repeat(margin))
                        .unwrap_or(r.trim_end())
                        .to_owned()
                })
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "width {width}");
            assert!(actual.iter().any(|r| r.contains("mc²")));
            assert!(actual.iter().any(|r| r.contains("a⁄b")));
        }
    }
    #[test]
    fn native_blocks_match_streaming_at_phone_and_desktop_widths() {
        let source = "# Results\n\n[Guide](https://example.test/path) ![Preview](image.png) __bold__ _italic_ ~~old~~ ***both*** snake_case ``a  `tick`  b`` \n\n> Quoted **detail**\n> > Nested quote\n\n1) First numbered item\n    Indented continuation\n\n---\n\n\n| Change | Status |\n| --- | --- |\n| `src/lib.rs` | **Done** |\n| 界 Unicode | ready to ship |\n\n- [x] Built\n  - Nested detail\n- [ ] Review\n\n```rust\nlet x = 1;\n```\nFinal text";
        for width in [10usize, 20, 27, 28, 39, 40, 41, 65, 90] {
            let margin = if width >= 24 {
                2
            } else if width >= 10 {
                1
            } else {
                0
            };
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
            if width >= 20 {
                assert!(actual.iter().any(|r| r.contains("◦ Nested")));
            }
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
    fn bidi_formatting_is_visible_without_removing_natural_unicode() {
        assert_eq!(
            sanitize("safe\u{202e}txt\u{2069}"),
            "safe\\u{202e}txt\\u{2069}"
        );
        let natural = "مرحبا 👩\u{200d}💻 café\n\t";
        assert_eq!(sanitize(natural), natural);
        for c in [
            '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
            '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
        ] {
            assert!(!sanitize(&c.to_string()).contains(c));
        }
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

    static DIAGRAM_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    const DIAGRAM: &str = "```mermaid\nflowchart TD\n A[Start] -->|go| B[End]\n```\n";

    #[test]
    fn mermaid_off_renders_the_fenced_source() {
        let _guard = DIAGRAM_TEST.lock().unwrap();
        // Global state from sibling tests leaks in: reset both gates.
        enable_mermaid(false);
        set_mermaid_renderer_detailed(Box::new(|_, _| Err("unsupported diagram".into())));
        let off = lines(DIAGRAM, 60);
        // When never registered, the block is ordinary code: byte-identical
        // to the disabled build. Assert on the plain joined text.
        let text: String = off
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("flowchart TD"), "{text}");
        assert!(!text.contains("─▶"), "{text}");
    }

    #[test]
    fn mermaid_enabled_renders_its_diagram() {
        let _guard = DIAGRAM_TEST.lock().unwrap();
        enable_mermaid(true);
        // A stand-in renderer: the real one lives in the glass binary and
        // cannot be reached from this lib test. Same contract — parse, lay
        // out, None on refusal.
        set_mermaid_renderer_detailed(Box::new(|_body, width| {
            if width >= 12 {
                Ok(vec!["[Start] ─ go ─▶ [End]".to_string()])
            } else {
                Err("viewport too narrow".into())
            }
        }));
        let on = lines(
            &format!("Before diagram\n\n{DIAGRAM}Between diagrams\n\n{DIAGRAM}After diagram"),
            60,
        );
        let text: String = on
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(text.matches("─ go ─▶").count(), 2, "{text}");
        for marker in ["Before diagram", "Between diagrams", "After diagram"] {
            assert!(text.contains(marker), "{text}");
        }
        let source = format!("Before diagram\n\n{DIAGRAM}After diagram");
        let mut stream = Stream::default();
        let mut output = Vec::new();
        for c in source.chars() {
            stream.write(&mut output, &c.to_string(), 64);
        }
        stream.flush(&mut output, 64);
        let text = project(&output).join("\n");
        assert!(
            text.contains("─ go ─▶") && !text.contains("flowchart TD"),
            "{text}"
        );
        assert!(text.contains("Before diagram") && text.contains("After diagram"));
        // Unsupported syntax gets an explicit source reason, in both presentation paths.
        set_mermaid_renderer_detailed(Box::new(|_, _| Err("unsupported diagram type: pie".into())));
        let source = "Before\n```mermaid\npie\n \"A\" : 1\n```\nAfter";
        let expected = lines(source, 60)
            .into_iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>();
        let mut stream = Stream::default();
        let mut output = Vec::new();
        for c in source.chars() {
            stream.write(&mut output, &c.to_string(), 64);
        }
        stream.flush(&mut output, 64);
        let actual = project(&output)
            .into_iter()
            .map(|r| {
                r.trim_end()
                    .strip_prefix("  ")
                    .unwrap_or(r.trim_end())
                    .to_owned()
            })
            .filter(|r| !r.is_empty())
            .collect::<Vec<_>>();
        let expected = expected
            .into_iter()
            .map(|r| r.trim_end().to_owned())
            .filter(|r| !r.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert!(actual.join(" ").contains("unsupported diagram type: pie"));
        // Incomplete and oversized blocks remain source; their buffers stay bounded.
        let mut stream = Stream::default();
        let mut output = Vec::new();
        stream.write(&mut output, "```mermaid\nflowchart TD\nA --> B", 64);
        stream.flush(&mut output, 64);
        assert!(project(&output).join(" ").contains("A --> B"));
        let mut stream = Stream::default();
        let mut output = Vec::new();
        stream.write(
            &mut output,
            &format!(
                "```mermaid\n{}\n```\nTAIL",
                "A --> B\n".repeat(DIAGRAM_LINES + 1)
            ),
            64,
        );
        stream.flush(&mut output, 64);
        assert!(stream.diagram.is_none());
        assert!(project(&output).join(" ").contains("TAIL"));
        enable_mermaid(false);
    }
}

#[cfg(test)]
mod checklist_tests {
    #[test]
    fn checklist_status_colors_are_shared_by_stream_and_replay() {
        for (mark, tone) in [
            ("○", crate::theme::Tone::Muted),
            ("◆", crate::theme::Tone::Accent),
            ("✓", crate::theme::Tone::Success),
            ("!", crate::theme::Tone::Warning),
        ] {
            let text = format!("{mark} `T1` Native task — status");
            let rows = super::lines(&text, 40);
            assert!(!rows.is_empty());
            assert!(
                rows.iter()
                    .flat_map(|row| row.spans.iter())
                    .filter(|span| !span.content.trim().is_empty())
                    .all(|span| span.style.fg == Some(crate::theme::color(tone)))
            );
        }
    }
}

#[cfg(test)]
mod latex_document_regression {
    #[test]
    fn document_wrapper_is_not_parsed_as_an_equation() {
        let source = "```latex\n\\documentclass{article}\n\\begin{document}\nHello, world!\n\n\\[\nE = mc^2\n\\]\n\\end{document}\n```";
        for width in [20, 40, 88] {
            let rows = super::lines(source, width);
            let text = rows
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("Hello, world!"), "{text}");
            assert!(text.contains("mc²"), "{text}");
            assert!(!text.contains("unsupported math"), "{text}");
            assert!(
                rows.iter()
                    .all(|row| crate::text::width(&row.to_string()) <= width)
            );
        }
        assert!(
            super::latex_document("\\usepackage{evil}\n\\begin{document}x\\end{document}", 40)
                .is_none()
        );
    }
}
