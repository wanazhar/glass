use super::syntax::SyntaxHighlighter;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use std::collections::BTreeMap;

const TEXT: Color = Color::Rgb(230, 237, 243);
const MUTED: Color = Color::Rgb(139, 148, 158);
const ACCENT: Color = Color::Rgb(88, 166, 255);
const ACCENT_BRIGHT: Color = Color::Rgb(121, 192, 255);
const SUCCESS: Color = Color::Rgb(126, 231, 135);
const WARNING: Color = Color::Rgb(210, 153, 34);
const ERROR: Color = Color::Rgb(255, 123, 114);
const PURPLE: Color = Color::Rgb(210, 168, 255);
const PANEL_INSET: Color = Color::Rgb(13, 17, 23);
const ACTIVE_BACKGROUND: Color = Color::Rgb(31, 50, 72);
const DIFF_ADD: Color = Color::Rgb(20, 64, 39);
const DIFF_REMOVE: Color = Color::Rgb(75, 32, 32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Go,
    Java,
    CLike,
    Shell,
    Json,
    Toml,
    Yaml,
    Markdown,
    Mermaid,
    Html,
    Css,
    Sql,
    Plain,
}

impl FileKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::CLike => "C/C++",
            Self::Shell => "Shell",
            Self::Json => "JSON",
            Self::Toml => "TOML",
            Self::Yaml => "YAML",
            Self::Markdown => "Markdown",
            Self::Mermaid => "Mermaid",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Sql => "SQL",
            Self::Plain => "Plain text",
        }
    }
}

pub(crate) fn classify(path: &str) -> FileKind {
    let lower = path.to_ascii_lowercase();
    if let Some(kind) = match lower.as_str() {
        "rust" | "rs" => Some(FileKind::Rust),
        "javascript" | "js" => Some(FileKind::JavaScript),
        "typescript" | "ts" => Some(FileKind::TypeScript),
        "python" | "py" => Some(FileKind::Python),
        "golang" | "go" => Some(FileKind::Go),
        "shell" | "bash" | "sh" => Some(FileKind::Shell),
        "json" => Some(FileKind::Json),
        "toml" => Some(FileKind::Toml),
        "yaml" | "yml" => Some(FileKind::Yaml),
        "markdown" | "md" => Some(FileKind::Markdown),
        "mermaid" | "mmd" => Some(FileKind::Mermaid),
        "html" => Some(FileKind::Html),
        "css" => Some(FileKind::Css),
        "sql" => Some(FileKind::Sql),
        _ => None,
    } {
        return kind;
    }
    let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if file_name == "dockerfile" || file_name.starts_with("dockerfile.") {
        return FileKind::Shell;
    }
    if file_name == "makefile" || file_name == "justfile" {
        return FileKind::Shell;
    }
    if file_name == "cargo.toml" || file_name == "pyproject.toml" {
        return FileKind::Toml;
    }
    if file_name == "readme" || file_name.starts_with("readme.") {
        return FileKind::Markdown;
    }
    let extension = lower.rsplit('.').next().unwrap_or_default();
    match extension {
        "rs" => FileKind::Rust,
        "js" | "jsx" | "mjs" | "cjs" => FileKind::JavaScript,
        "ts" | "tsx" | "mts" | "cts" => FileKind::TypeScript,
        "py" | "pyw" => FileKind::Python,
        "go" => FileKind::Go,
        "java" | "kt" | "kts" | "scala" => FileKind::Java,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" => FileKind::CLike,
        "sh" | "bash" | "zsh" | "fish" | "ps1" => FileKind::Shell,
        "json" | "jsonc" | "json5" => FileKind::Json,
        "toml" => FileKind::Toml,
        "yaml" | "yml" => FileKind::Yaml,
        "md" | "markdown" | "mdx" => FileKind::Markdown,
        "mmd" | "mermaid" => FileKind::Mermaid,
        "html" | "htm" | "xml" | "svg" => FileKind::Html,
        "css" | "scss" | "less" => FileKind::Css,
        "sql" => FileKind::Sql,
        _ => FileKind::Plain,
    }
}

pub(crate) fn render_editor(
    path: &str,
    content: &str,
    selection: Option<&crate::development::TextSelection>,
) -> Text<'static> {
    let selection = selection.filter(|selection| !selection.is_empty());
    if !content
        .lines()
        .any(|line| editor_header_path(line).is_some())
    {
        return render_source(path, content);
    }
    let fallback_kind = classify(path);
    if fallback_kind == FileKind::Mermaid
        && let Some(source) = editor_source_for_path(path, content)
        && let Some(diagram) = render_mermaid(&source)
    {
        let mut lines = Vec::with_capacity(diagram.len() + 1);
        if let Some(header) = content
            .lines()
            .find(|line| editor_header_path(line) == Some(path))
        {
            lines.push(Line::from(vec![
                Span::styled("▎ ", Style::default().fg(ACCENT)),
                Span::styled(
                    header.to_string(),
                    Style::default()
                        .fg(ACCENT_BRIGHT)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        lines.extend(diagram);
        return Text::from(lines);
    }
    let mut current_kind = fallback_kind;
    let mut highlighter = SyntaxHighlighter::for_path(path);
    let mut lines = Vec::new();
    let mut saw_editor_header = false;
    let mut source_line = 0;
    let mut markdown_fence = None;
    let mut markdown_highlighter = None;

    for line in content.lines() {
        if let Some(header_path) = editor_header_path(line) {
            current_kind = classify(header_path);
            highlighter = SyntaxHighlighter::for_path(header_path);
            markdown_fence = None;
            markdown_highlighter = None;
            saw_editor_header = true;
            lines.push(Line::from(vec![
                Span::styled("▎ ", Style::default().fg(ACCENT)),
                Span::styled(
                    line.to_string(),
                    Style::default()
                        .fg(ACCENT_BRIGHT)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }
        if let Some((gutter, source)) = line.split_once(" │ ") {
            let line_number = editor_line_number(gutter).unwrap_or(source_line + 1);
            source_line = line_number;
            let mut rendered = vec![Span::styled(
                format!("{gutter} │ "),
                Style::default().fg(if gutter.trim_start().starts_with('▶') {
                    SUCCESS
                } else {
                    MUTED
                }),
            )];
            let source_spans = editor_source_spans(
                current_kind,
                source,
                &mut markdown_fence,
                &mut markdown_highlighter,
                &mut highlighter,
            );
            rendered.extend(apply_selection_background(
                source_spans,
                selection_columns(selection, line_number, source),
            ));
            lines.push(Line::from(rendered));
        } else if line.trim().is_empty() {
            if saw_editor_header {
                source_line = source_line.saturating_add(1);
            }
            lines.push(Line::default());
        } else if saw_editor_header {
            source_line = source_line.saturating_add(1);
            let source_spans = editor_source_spans(
                current_kind,
                line,
                &mut markdown_fence,
                &mut markdown_highlighter,
                &mut highlighter,
            );
            lines.push(Line::from(apply_selection_background(
                source_spans,
                selection_columns(selection, source_line, line),
            )));
        } else {
            lines.push(Line::from(editor_source_spans(
                fallback_kind,
                line,
                &mut markdown_fence,
                &mut markdown_highlighter,
                &mut highlighter,
            )));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No file open · choose a file from the list",
            Style::default().fg(MUTED),
        )));
    }
    Text::from(lines)
}

pub(crate) fn render_editable_source(
    path: &str,
    content: &str,
    cursor_line: u32,
    cursor_column: u32,
    selection: Option<&crate::development::TextSelection>,
) -> Text<'static> {
    let selection = selection.filter(|selection| !selection.is_empty());
    let kind = classify(path);
    let mut highlighter = SyntaxHighlighter::for_path(path);
    let mut markdown_fence = None;
    let mut markdown_highlighter = None;
    let source_lines = content.split('\n').collect::<Vec<_>>();
    let gutter_width = source_lines.len().max(1).to_string().len().max(3);
    let mut lines = Vec::with_capacity(source_lines.len());

    for (index, source) in source_lines.into_iter().enumerate() {
        let line_number = index as u32 + 1;
        let active = line_number == cursor_line;
        let source_spans = editor_source_spans(
            kind,
            source,
            &mut markdown_fence,
            &mut markdown_highlighter,
            &mut highlighter,
        );
        let source_spans = apply_selection_background(
            source_spans,
            selection_columns(selection, line_number, source),
        );
        let source_spans = apply_active_line_background(source_spans, active);
        let source_spans = apply_cursor_style(source_spans, active, cursor_column);
        let gutter_style = if active {
            Style::default()
                .fg(ACCENT_BRIGHT)
                .bg(ACTIVE_BACKGROUND)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED)
        };
        let mut rendered = vec![Span::styled(
            format!(
                "{}{:>gutter_width$} │ ",
                if active { "▶" } else { " " },
                line_number,
                gutter_width = gutter_width
            ),
            gutter_style,
        )];
        rendered.extend(source_spans);
        lines.push(Line::from(rendered));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No file open",
            Style::default().fg(MUTED),
        )));
    }
    Text::from(lines)
}

const ACTIVE_LINE_BACKGROUND: Color = Color::Rgb(24, 34, 46);

fn apply_active_line_background(spans: Vec<Span<'static>>, active: bool) -> Vec<Span<'static>> {
    if !active {
        return spans;
    }
    spans
        .into_iter()
        .map(|span| {
            let style = if span.style.bg.is_none() {
                span.style.bg(ACTIVE_LINE_BACKGROUND)
            } else {
                span.style
            };
            Span::styled(span.content.to_string(), style)
        })
        .collect()
}

fn apply_cursor_style(
    spans: Vec<Span<'static>>,
    active: bool,
    cursor_column: u32,
) -> Vec<Span<'static>> {
    if !active {
        return spans;
    }
    let cursor_index = cursor_column.saturating_sub(1) as usize;
    let cursor_style = Style::default()
        .fg(Color::Black)
        .bg(ACCENT_BRIGHT)
        .add_modifier(Modifier::BOLD);
    let mut rendered = Vec::with_capacity(spans.len() + 1);
    let mut position = 0;
    for span in spans {
        let style = span.style;
        let mut chunk = String::new();
        let mut chunk_cursor = None;
        for character in span.content.chars() {
            let is_cursor = position == cursor_index;
            if chunk_cursor != Some(is_cursor) && !chunk.is_empty() {
                rendered.push(Span::styled(
                    std::mem::take(&mut chunk),
                    if chunk_cursor == Some(true) {
                        cursor_style
                    } else {
                        style
                    },
                ));
            }
            chunk_cursor = Some(is_cursor);
            chunk.push(character);
            position += 1;
        }
        if !chunk.is_empty() {
            rendered.push(Span::styled(
                chunk,
                if chunk_cursor == Some(true) {
                    cursor_style
                } else {
                    style
                },
            ));
        }
    }
    if cursor_index >= position {
        rendered.push(Span::styled(" ", cursor_style));
    }
    rendered
}

fn editor_line_number(gutter: &str) -> Option<u32> {
    gutter.trim_start_matches(['▶', ' ']).trim().parse().ok()
}

fn selection_columns(
    selection: Option<&crate::development::TextSelection>,
    line_number: u32,
    line: &str,
) -> Option<(usize, usize)> {
    let selection = selection?;
    let (start, end) = selection.ordered();
    if line_number < start.line || line_number > end.line {
        return None;
    }
    let character_count = line.chars().count();
    let start_column = if line_number == start.line {
        start.column.saturating_sub(1) as usize
    } else {
        0
    }
    .min(character_count);
    let end_column = if line_number == end.line {
        end.column.saturating_sub(1) as usize
    } else {
        character_count
    }
    .min(character_count);
    (start_column < end_column).then_some((start_column, end_column))
}

fn apply_selection_background(
    spans: Vec<Span<'static>>,
    range: Option<(usize, usize)>,
) -> Vec<Span<'static>> {
    let Some((selection_start, selection_end)) = range else {
        return spans;
    };
    let mut rendered = Vec::with_capacity(spans.len() + 2);
    let mut position = 0;
    for span in spans {
        let style = span.style;
        let mut chunk = String::new();
        let mut chunk_selected = None;
        for character in span.content.chars() {
            let selected = (selection_start..selection_end).contains(&position);
            if chunk_selected != Some(selected) && !chunk.is_empty() {
                let chunk_style = if chunk_selected == Some(true) {
                    style.bg(ACTIVE_BACKGROUND)
                } else {
                    style
                };
                rendered.push(Span::styled(std::mem::take(&mut chunk), chunk_style));
            }
            chunk_selected = Some(selected);
            chunk.push(character);
            position += 1;
        }
        if !chunk.is_empty() {
            let chunk_style = if chunk_selected == Some(true) {
                style.bg(ACTIVE_BACKGROUND)
            } else {
                style
            };
            rendered.push(Span::styled(chunk, chunk_style));
        }
    }
    rendered
}

fn editor_source_for_path(path: &str, content: &str) -> Option<String> {
    let mut current_path = None;
    let mut source = Vec::new();
    for line in content.lines() {
        if let Some(header_path) = editor_header_path(line) {
            current_path = Some(header_path);
            continue;
        }
        if current_path == Some(path)
            && let Some((_, line)) = line.split_once(" │ ")
        {
            source.push(line);
        }
    }
    (!source.is_empty()).then(|| source.join("\n"))
}

fn editor_source_spans(
    kind: FileKind,
    line: &str,
    markdown_fence: &mut Option<FileKind>,
    markdown_highlighter: &mut Option<SyntaxHighlighter>,
    highlighter: &mut SyntaxHighlighter,
) -> Vec<Span<'static>> {
    match kind {
        FileKind::Markdown => {
            render_markdown_line(line, markdown_fence, markdown_highlighter).spans
        }
        FileKind::Mermaid => render_mermaid_source_line(line).spans,
        _ => highlight_or_manual(highlighter, kind, line, None),
    }
}

fn highlight_or_manual(
    highlighter: &mut SyntaxHighlighter,
    kind: FileKind,
    line: &str,
    background: Option<Color>,
) -> Vec<Span<'static>> {
    highlighter
        .highlight(line, background)
        .unwrap_or_else(|| source_line_spans(kind, line, background))
}

pub(crate) fn render_source(path: &str, content: &str) -> Text<'static> {
    let kind = classify(path);
    if kind == FileKind::Mermaid
        && let Some(diagram) = render_mermaid(content)
    {
        return Text::from(diagram);
    }

    let mut lines = Vec::new();
    let mut highlighter = SyntaxHighlighter::for_path(path);
    let mut markdown_fence = None;
    let mut markdown_highlighter = None;
    for line in content.lines() {
        let rendered = match kind {
            FileKind::Markdown => {
                render_markdown_line(line, &mut markdown_fence, &mut markdown_highlighter)
            }
            FileKind::Mermaid => render_mermaid_source_line(line),
            _ => Line::from(highlight_or_manual(&mut highlighter, kind, line, None)),
        };
        lines.push(rendered);
    }
    if lines.is_empty() {
        lines.push(Line::default());
    }
    Text::from(lines)
}

pub(crate) fn render_diff(path: &str, content: &str) -> Text<'static> {
    let mut old_line = 0u32;
    let mut new_line = 0u32;
    let mut active_path = path.to_string();
    let mut highlighter = SyntaxHighlighter::for_path(path);
    let mut lines = Vec::new();
    for line in content.lines() {
        if let Some(header_path) = diff_file_path(line) {
            active_path = header_path;
            highlighter = SyntaxHighlighter::for_path(&active_path);
        }
        if line.starts_with("@@") {
            old_line = diff_hunk_start(line, '-').unwrap_or(0);
            new_line = diff_hunk_start(line, '+').unwrap_or(0);
            lines.push(Line::from(Span::styled(
                format!("        {line}"),
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .bg(ACTIVE_BACKGROUND)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        let (gutter, marker, body, background, marker_color) =
            if line.starts_with('+') && !line.starts_with("+++") {
                let number = format!("     {new_line:>4} ");
                new_line = new_line.saturating_add(1);
                (
                    number,
                    "+",
                    line.strip_prefix('+').unwrap_or_default(),
                    DIFF_ADD,
                    SUCCESS,
                )
            } else if line.starts_with('-') && !line.starts_with("---") {
                let number = format!("{old_line:>4}      ");
                old_line = old_line.saturating_add(1);
                (
                    number,
                    "-",
                    line.strip_prefix('-').unwrap_or_default(),
                    DIFF_REMOVE,
                    ERROR,
                )
            } else if line.starts_with(' ') {
                let number = format!("{old_line:>4} {new_line:>4} ");
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
                (
                    number,
                    " ",
                    line.strip_prefix(' ').unwrap_or_default(),
                    PANEL_INSET,
                    MUTED,
                )
            } else {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(if line.starts_with("diff ") {
                        ACCENT_BRIGHT
                    } else {
                        MUTED
                    }),
                )));
                continue;
            };

        let mut rendered = vec![
            Span::styled(gutter, Style::default().fg(MUTED).bg(background)),
            Span::styled(
                marker,
                Style::default()
                    .fg(marker_color)
                    .bg(background)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        rendered.extend(highlight_or_manual(
            &mut highlighter,
            classify(&active_path),
            body,
            Some(background),
        ));
        lines.push(Line::from(rendered));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No diff",
            Style::default().fg(MUTED),
        )));
    }
    Text::from(lines)
}

fn editor_header_path(line: &str) -> Option<&str> {
    let line = line
        .strip_prefix("● ")
        .or_else(|| line.strip_prefix("○ "))?;
    let end = line.find(" · cursor ")?;
    Some(&line[..end])
}

fn diff_file_path(line: &str) -> Option<String> {
    let raw = line
        .strip_prefix("+++ ")
        .or_else(|| line.strip_prefix("--- "))?;
    if raw == "/dev/null" {
        return None;
    }
    Some(
        raw.strip_prefix("a/")
            .or_else(|| raw.strip_prefix("b/"))
            .unwrap_or(raw)
            .to_string(),
    )
}

fn diff_hunk_start(line: &str, marker: char) -> Option<u32> {
    let offset = line.find(marker)? + 1;
    line[offset..].split([',', ' ', '@']).next()?.parse().ok()
}

fn source_line_spans(kind: FileKind, line: &str, background: Option<Color>) -> Vec<Span<'static>> {
    if kind == FileKind::Plain {
        return vec![span(line.to_string(), TEXT, background, Modifier::empty())];
    }
    let chars = line.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if let Some(end) = comment_end(kind, &chars, index) {
            spans.push(span(
                chars[index..end].iter().collect::<String>(),
                MUTED,
                background,
                Modifier::ITALIC,
            ));
            index = end;
            continue;
        }
        let character = chars[index];
        if matches!(character, '\'' | '"' | '`') {
            let quote = character;
            let mut end = index + 1;
            let mut escaped = false;
            while end < chars.len() {
                let current = chars[end];
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == quote {
                    end += 1;
                    break;
                }
                end += 1;
            }
            spans.push(span(
                chars[index..end].iter().collect::<String>(),
                SUCCESS,
                background,
                Modifier::empty(),
            ));
            index = end;
            continue;
        }
        if character.is_ascii_digit()
            && (index == 0 || !is_identifier_char(chars[index.saturating_sub(1)]))
        {
            let mut end = index + 1;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || matches!(chars[end], '.' | '_'))
            {
                end += 1;
            }
            spans.push(span(
                chars[index..end].iter().collect::<String>(),
                PURPLE,
                background,
                Modifier::empty(),
            ));
            index = end;
            continue;
        }
        if is_identifier_start(character) {
            let mut end = index + 1;
            while end < chars.len() && is_identifier_char(chars[end]) {
                end += 1;
            }
            let token = chars[index..end].iter().collect::<String>();
            let color = if is_keyword(kind, &token) {
                ACCENT_BRIGHT
            } else {
                TEXT
            };
            let modifier = if is_keyword(kind, &token) {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            spans.push(span(token, color, background, modifier));
            index = end;
            continue;
        }
        if is_punctuation(character) {
            spans.push(span(
                character.to_string(),
                ACCENT,
                background,
                Modifier::empty(),
            ));
            index += 1;
            continue;
        }

        let mut end = index + 1;
        while end < chars.len()
            && !is_identifier_start(chars[end])
            && !chars[end].is_ascii_digit()
            && !matches!(chars[end], '\'' | '"' | '`')
            && !is_punctuation(chars[end])
            && comment_end(kind, &chars, end).is_none()
        {
            end += 1;
        }
        spans.push(span(
            chars[index..end].iter().collect::<String>(),
            TEXT,
            background,
            Modifier::empty(),
        ));
        index = end;
    }
    if spans.is_empty() {
        spans.push(span(String::new(), TEXT, background, Modifier::empty()));
    }
    spans
}

fn span(
    text: String,
    foreground: Color,
    background: Option<Color>,
    modifier: Modifier,
) -> Span<'static> {
    let mut style = Style::default().fg(foreground).add_modifier(modifier);
    if let Some(background) = background {
        style = style.bg(background);
    }
    Span::styled(text, style)
}

fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn is_punctuation(character: char) -> bool {
    matches!(
        character,
        '{' | '}'
            | '['
            | ']'
            | '('
            | ')'
            | '<'
            | '>'
            | ':'
            | ';'
            | ','
            | '.'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '%'
            | '!'
            | '?'
            | '|'
            | '&'
            | '@'
    )
}

fn comment_end(kind: FileKind, chars: &[char], index: usize) -> Option<usize> {
    let slash_comment = matches!(
        kind,
        FileKind::Rust
            | FileKind::JavaScript
            | FileKind::TypeScript
            | FileKind::Go
            | FileKind::Java
            | FileKind::CLike
            | FileKind::Css
            | FileKind::Json
    ) && starts_with(chars, index, &['/', '/']);
    let hash_comment = matches!(
        kind,
        FileKind::Python | FileKind::Shell | FileKind::Toml | FileKind::Yaml
    ) && chars.get(index) == Some(&'#');
    let sql_comment = kind == FileKind::Sql && starts_with(chars, index, &['-', '-']);
    let block_comment = matches!(
        kind,
        FileKind::Rust
            | FileKind::JavaScript
            | FileKind::TypeScript
            | FileKind::Go
            | FileKind::Java
            | FileKind::CLike
            | FileKind::Css
    ) && starts_with(chars, index, &['/', '*']);
    if slash_comment || hash_comment || sql_comment || block_comment {
        Some(chars.len())
    } else {
        None
    }
}

fn starts_with(chars: &[char], index: usize, expected: &[char]) -> bool {
    chars.get(index..index.saturating_add(expected.len())) == Some(expected)
}

fn is_keyword(kind: FileKind, token: &str) -> bool {
    let keywords: &[&str] = match kind {
        FileKind::Rust => &[
            "as", "async", "await", "const", "crate", "else", "enum", "fn", "for", "if", "impl",
            "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
            "Self", "static", "struct", "trait", "type", "unsafe", "use", "where", "while", "dyn",
            "true", "false",
        ],
        FileKind::JavaScript | FileKind::TypeScript => &[
            "as",
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "else",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "from",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "let",
            "new",
            "null",
            "of",
            "return",
            "static",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "undefined",
            "var",
            "void",
            "while",
            "with",
            "yield",
            "interface",
            "type",
        ],
        FileKind::Python => &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
            "elif", "else", "False", "finally", "for", "from", "global", "if", "import", "in",
            "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True",
            "try", "while", "with", "yield",
        ],
        FileKind::Go => &[
            "break",
            "case",
            "chan",
            "const",
            "continue",
            "default",
            "defer",
            "else",
            "fallthrough",
            "for",
            "func",
            "go",
            "goto",
            "if",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "return",
            "select",
            "struct",
            "switch",
            "type",
            "var",
            "true",
            "false",
            "nil",
        ],
        FileKind::Java => &[
            "abstract",
            "boolean",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "do",
            "else",
            "enum",
            "extends",
            "final",
            "finally",
            "for",
            "if",
            "implements",
            "import",
            "instanceof",
            "interface",
            "native",
            "new",
            "null",
            "package",
            "private",
            "protected",
            "public",
            "return",
            "static",
            "super",
            "switch",
            "this",
            "throw",
            "throws",
            "transient",
            "try",
            "void",
            "volatile",
            "while",
            "true",
            "false",
        ],
        FileKind::CLike => &[
            "auto",
            "bool",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "do",
            "else",
            "enum",
            "extern",
            "false",
            "for",
            "if",
            "include",
            "inline",
            "namespace",
            "new",
            "nullptr",
            "private",
            "protected",
            "public",
            "return",
            "sizeof",
            "static",
            "struct",
            "switch",
            "template",
            "this",
            "throw",
            "true",
            "try",
            "typedef",
            "typename",
            "using",
            "virtual",
            "void",
            "volatile",
            "while",
        ],
        FileKind::Shell => &[
            "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if",
            "in", "local", "readonly", "select", "then", "time", "until", "while",
        ],
        FileKind::Json | FileKind::Toml | FileKind::Yaml => &["true", "false", "null"],
        FileKind::Sql => &[
            "alter",
            "and",
            "as",
            "begin",
            "by",
            "case",
            "create",
            "delete",
            "distinct",
            "drop",
            "else",
            "end",
            "from",
            "group",
            "having",
            "insert",
            "into",
            "join",
            "left",
            "limit",
            "not",
            "null",
            "on",
            "or",
            "order",
            "primary",
            "references",
            "select",
            "set",
            "table",
            "then",
            "union",
            "update",
            "values",
            "when",
            "where",
            "with",
        ],
        FileKind::Html
        | FileKind::Css
        | FileKind::Markdown
        | FileKind::Mermaid
        | FileKind::Plain => &[],
    };
    keywords
        .iter()
        .any(|keyword| keyword.eq_ignore_ascii_case(token))
}

fn render_markdown_line(
    line: &str,
    fence: &mut Option<FileKind>,
    fence_highlighter: &mut Option<SyntaxHighlighter>,
) -> Line<'static> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        if fence.is_some() {
            *fence = None;
            *fence_highlighter = None;
        } else {
            let language = trimmed
                .trim_start_matches(['`', '~'])
                .split_whitespace()
                .next()
                .unwrap_or_default();
            *fence = Some(classify(language));
            *fence_highlighter = Some(SyntaxHighlighter::for_token(language));
        }
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(WARNING).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(kind) = *fence {
        if let Some(highlighter) = fence_highlighter.as_mut() {
            return Line::from(highlight_or_manual(highlighter, kind, line, None));
        }
        return Line::from(source_line_spans(kind, line, None));
    }
    if trimmed.starts_with('#') {
        return Line::from(vec![
            Span::styled("▎ ", Style::default().fg(ACCENT)),
            Span::styled(
                line.to_string(),
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if trimmed.starts_with('>') {
        return Line::from(vec![
            Span::styled("> ", Style::default().fg(PURPLE)),
            Span::styled(
                trimmed.trim_start_matches('>').trim_start().to_string(),
                Style::default().fg(PURPLE).add_modifier(Modifier::ITALIC),
            ),
        ]);
    }
    if trimmed == "---" || trimmed == "***" || trimmed == "___" {
        return Line::from(Span::styled(line.to_string(), Style::default().fg(MUTED)));
    }
    let indent = &line[..line.len() - trimmed.len()];
    let marker_len = trimmed
        .find(|character: char| {
            !matches!(character, '-' | '*' | '+' | ' ' | '\t' | '0'..='9' | '.')
        })
        .unwrap_or(0);
    if marker_len > 0 {
        let marker = &trimmed[..marker_len];
        let body = &trimmed[marker_len..];
        let mut spans = vec![Span::styled(
            format!("{indent}{marker}"),
            Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
        )];
        spans.extend(markdown_inline_spans(body));
        return Line::from(spans);
    }
    Line::from(markdown_inline_spans(line))
}

fn markdown_inline_spans(line: &str) -> Vec<Span<'static>> {
    let chars = line.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '`'
            && let Some(end) = chars[index + 1..]
                .iter()
                .position(|character| *character == '`')
        {
            let end = index + end + 2;
            spans.push(span(
                chars[index..end].iter().collect::<String>(),
                WARNING,
                None,
                Modifier::empty(),
            ));
            index = end;
            continue;
        }
        if chars[index] == '['
            && let Some(close) = chars[index + 1..]
                .iter()
                .position(|character| *character == ']')
        {
            let close = index + close + 1;
            if chars.get(close + 1) == Some(&'(')
                && let Some(end) = chars[close + 2..]
                    .iter()
                    .position(|character| *character == ')')
            {
                let end = close + end + 3;
                spans.push(span(
                    chars[index..end].iter().collect::<String>(),
                    ACCENT,
                    None,
                    Modifier::UNDERLINED,
                ));
                index = end;
                continue;
            }
        }
        let mut end = index + 1;
        while end < chars.len() && chars[end] != '`' && chars[end] != '[' {
            end += 1;
        }
        spans.push(span(
            chars[index..end].iter().collect::<String>(),
            TEXT,
            None,
            Modifier::empty(),
        ));
        index = end;
    }
    if spans.is_empty() {
        spans.push(span(String::new(), TEXT, None, Modifier::empty()));
    }
    spans
}

fn render_mermaid_source_line(line: &str) -> Line<'static> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("%%") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ));
    }
    if trimmed.starts_with("flowchart")
        || trimmed.starts_with("graph")
        || trimmed.starts_with("sequenceDiagram")
        || trimmed.starts_with("classDiagram")
    {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(ACCENT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let mut spans = Vec::new();
    for token in line.split_inclusive(|character: char| character.is_whitespace()) {
        let color = if token.contains("-->") || token.contains("->") || token.contains("==") {
            SUCCESS
        } else if token.contains('[') || token.contains('{') || token.contains('(') {
            ACCENT
        } else {
            TEXT
        };
        spans.push(Span::styled(token.to_string(), Style::default().fg(color)));
    }
    Line::from(spans)
}

fn render_mermaid(content: &str) -> Option<Vec<Line<'static>>> {
    let mut nonempty = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let first = nonempty.next()?;
    if first.starts_with("sequenceDiagram") {
        let rows = content
            .lines()
            .filter_map(parse_sequence_edge)
            .map(|(from, to, label)| {
                Line::from(vec![
                    Span::styled(format!("{from:<16}"), Style::default().fg(ACCENT_BRIGHT)),
                    Span::styled(
                        " ──▶ ",
                        Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("{to:<16}"), Style::default().fg(ACCENT_BRIGHT)),
                    Span::styled(label, Style::default().fg(TEXT)),
                ])
            })
            .collect::<Vec<_>>();
        return (!rows.is_empty()).then_some(rows);
    }
    if !(first.starts_with("flowchart") || first.starts_with("graph")) {
        return None;
    }
    let direction = first.split_whitespace().nth(1).unwrap_or("TD");
    let mut nodes = BTreeMap::<String, String>::new();
    let mut edges = Vec::new();
    for line in content.lines().skip(1) {
        if let Some((from, to, label)) = parse_flow_edge(line) {
            let from_node = parse_mermaid_node(&from);
            let to_node = parse_mermaid_node(&to);
            nodes
                .entry(from_node.0.clone())
                .or_insert(from_node.1.clone());
            nodes.entry(to_node.0.clone()).or_insert(to_node.1.clone());
            edges.push((from_node.0, to_node.0, label));
        }
    }
    if edges.is_empty() {
        return None;
    }

    let mut rows = vec![Line::from(vec![
        Span::styled(
            "DIAGRAM ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(direction.to_string(), Style::default().fg(MUTED)),
    ])];
    for (from, to, label) in edges {
        let from_label = nodes.get(&from).map(String::as_str).unwrap_or(&from);
        let to_label = nodes.get(&to).map(String::as_str).unwrap_or(&to);
        let arrow = if direction.eq_ignore_ascii_case("LR") {
            " ──▶ "
        } else if direction.eq_ignore_ascii_case("RL") {
            " ◀── "
        } else if direction.eq_ignore_ascii_case("BT") {
            " ▲ "
        } else {
            " ▼ "
        };
        let mut line = vec![
            Span::styled(
                format!("[{}]", from_label),
                Style::default().fg(ACCENT_BRIGHT),
            ),
            Span::styled(
                arrow.to_string(),
                Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{}]", to_label),
                Style::default().fg(ACCENT_BRIGHT),
            ),
        ];
        if let Some(label) = label {
            line.push(Span::styled(
                format!("  {label}"),
                Style::default().fg(PURPLE),
            ));
        }
        rows.push(Line::from(line));
    }
    Some(rows)
}

fn parse_sequence_edge(line: &str) -> Option<(String, String, String)> {
    let arrows = ["-->>", "->>", "-->", "->", "-)"];
    let (index, arrow) = arrows
        .iter()
        .filter_map(|arrow| line.find(arrow).map(|index| (index, *arrow)))
        .min_by_key(|(index, _)| *index)?;
    let from = line[..index].trim();
    let rest = line[index + arrow.len()..].trim();
    let (to, label) = rest.split_once(':').unwrap_or((rest, ""));
    if from.is_empty() || to.trim().is_empty() {
        return None;
    }
    Some((
        from.to_string(),
        to.trim().to_string(),
        if label.is_empty() {
            String::new()
        } else {
            format!("  · {}", label.trim())
        },
    ))
}

fn parse_flow_edge(line: &str) -> Option<(String, String, Option<String>)> {
    let arrows = ["-.->", "==>", "-->", "---"];
    let (index, arrow) = arrows
        .iter()
        .filter_map(|arrow| line.find(arrow).map(|index| (index, *arrow)))
        .min_by_key(|(index, _)| *index)?;
    let from = line[..index].trim();
    let rest = line[index + arrow.len()..].trim();
    if from.is_empty() || rest.is_empty() {
        return None;
    }
    let (to, label) = if let Some(rest) = rest.strip_prefix('|') {
        let (label, rest) = rest.split_once('|')?;
        (rest.trim(), Some(label.trim().to_string()))
    } else {
        (rest.split_whitespace().next().unwrap_or_default(), None)
    };
    if to.is_empty() {
        None
    } else {
        Some((from.to_string(), to.to_string(), label))
    }
}

fn parse_mermaid_node(raw: &str) -> (String, String) {
    let raw = raw.trim().trim_end_matches(';');
    let mut id_end = 0;
    for (index, character) in raw.char_indices() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            id_end = index + character.len_utf8();
        } else {
            break;
        }
    }
    let id = if id_end == 0 { raw } else { &raw[..id_end] };
    let remainder = raw[id_end..].trim();
    let label = if remainder.len() >= 2 {
        let pairs = [('[', ']'), ('{', '}'), ('(', ')')];
        pairs
            .iter()
            .find(|(open, close)| remainder.starts_with(*open) && remainder.ends_with(*close))
            .map(|(open, close)| {
                remainder
                    .trim_start_matches(*open)
                    .trim_end_matches(*close)
                    .to_string()
            })
            .unwrap_or_else(|| id.to_string())
    } else {
        id.to_string()
    };
    (id.to_string(), label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols(text: Text<'static>) -> String {
        text.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn classifies_common_development_files() {
        assert_eq!(classify("src/lib.rs"), FileKind::Rust);
        assert_eq!(classify("web/app.tsx"), FileKind::TypeScript);
        assert_eq!(classify("README.md"), FileKind::Markdown);
        assert_eq!(classify("docs/architecture.mmd"), FileKind::Mermaid);
        assert_eq!(classify("config/unknown.data"), FileKind::Plain);
    }
    #[test]
    fn renders_path_selected_syntax_for_common_ecosystems() {
        let fixtures = [
            ("web/app.tsx", "const answer = 42;"),
            ("service.swift", "let answer = 42"),
            ("android/MainActivity.kt", "val answer = 42"),
            ("tools/build.dart", "final answer = 42;"),
            ("infra/Dockerfile", "FROM alpine\nRUN echo ok"),
        ];
        for (path, source) in fixtures {
            let rendered = render_source(path, source);
            assert_eq!(symbols(rendered.clone()), source, "{path} content changed");
            assert!(
                rendered.lines.iter().any(|line| line.spans.len() > 1),
                "{path} did not receive token-level styling"
            );
        }
    }
    #[test]
    fn unknown_format_keeps_plain_text_fallback() {
        let source = "opaque syntax { still visible: true }";
        let rendered = render_source("notes/example.unknown-glass-format", source);
        assert_eq!(symbols(rendered.clone()), source);
        assert_eq!(rendered.lines.len(), 1);
        assert_eq!(rendered.lines[0].spans.len(), 1);
    }

    #[test]
    fn markdown_renderer_preserves_content_and_marks_fenced_code() {
        let output = symbols(render_source(
            "README.md",
            "# Title\n\n- item with `code`\n\n```rust\nlet answer = 42;\n```",
        ));
        assert!(output.contains("# Title"));
        assert!(output.contains("- item with `code`"));
        assert!(output.contains("let answer = 42;"));
    }

    #[test]
    fn mermaid_flowchart_gets_a_terminal_representation() {
        let output = symbols(render_source(
            "docs/flow.mmd",
            "flowchart LR\n  start[Start] --> finish{Done}",
        ));
        assert!(output.contains("DIAGRAM LR"));
        assert!(output.contains("[Start]"));
        assert!(output.contains("[Done]"));
        assert!(output.contains("──▶"));
    }

    #[test]
    fn editor_renderer_keeps_gutters_and_highlights_source() {
        let output = render_editor(
            "src/main.rs",
            "○ src/main.rs · cursor 1:1 · actor local · 1 lines\n▶  1 │ fn main() { true }",
            None,
        );
        assert_eq!(output.lines.len(), 2);
        assert!(output.lines[1].spans[0].content.contains('1'));
        assert!(
            output.lines[1]
                .spans
                .iter()
                .any(|span| span.content == "fn")
        );
    }

    #[test]
    fn editor_renderer_marks_the_active_selection() {
        let selection = crate::development::TextSelection {
            anchor: crate::development::TextPosition { line: 1, column: 4 },
            active: crate::development::TextPosition { line: 1, column: 8 },
        };
        let output = render_editor(
            "src/main.rs",
            "○ src/main.rs · cursor 1:8 · actor local · 1 lines\n▶  1 │ fn main() { true }",
            Some(&selection),
        );
        assert!(
            output.lines[1]
                .spans
                .iter()
                .any(|span| span.style.bg == Some(ACTIVE_BACKGROUND))
        );
    }

    #[test]
    fn editable_renderer_marks_line_numbers_active_line_and_cursor() {
        let output = render_editable_source("src/main.rs", "fn main() {}\n", 1, 4, None);
        assert_eq!(symbols(output.clone()), "▶  1 │ fn main() {}\n   2 │ ");
        assert!(
            output.lines[0]
                .spans
                .iter()
                .any(|span| span.style.bg == Some(ACCENT_BRIGHT)),
            "cursor cell should be visibly highlighted"
        );
        assert_eq!(output.lines[0].spans[0].content, "▶  1 │ ");
    }

    #[test]
    fn formatted_mermaid_editor_projection_renders_the_diagram() {
        let output = symbols(render_editor(
            "docs/flow.mmd",
            "○ docs/flow.mmd · cursor 1:1 · actor local · 2 lines\n  1 │ flowchart LR\n  2 │ start[Start] --> finish{Done}",
            None,
        ));
        assert!(output.contains("DIAGRAM LR"));
        assert!(output.contains("[Start]"));
        assert!(output.contains("[Done]"));
    }

    #[test]
    fn diff_renderer_tracks_file_headers_and_line_numbers() {
        let output = symbols(render_diff(
            "src/lib.rs",
            "diff --git a/src/lib.rs b/src/lib.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}",
        ));
        assert!(output.contains("fn old() {}"));
        assert!(output.contains("fn new() {}"));
        assert!(output.contains("1"));
    }
}
