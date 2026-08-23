use serde::{Deserialize, Serialize};

/// Zero-based line and character-column position in editor text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextPosition {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based character column within the line.
    pub column: u32,
}

/// A selection represented by its fixed anchor and active cursor positions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextSelection {
    /// Position where the selection began.
    pub anchor: TextPosition,
    /// Current cursor position; may be before or after `anchor`.
    pub active: TextPosition,
}

/// Coarse syntax categories emitted by [`syntax_spans`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyntaxKind {
    /// Recognized language keyword.
    Keyword,
    /// Reserved for string-token spans.
    String,
    /// A line comment.
    Comment,
    /// An ASCII decimal number token.
    Number,
}

/// Half-open byte range and category for one syntax span.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxSpan {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
    /// Category assigned to the span.
    pub kind: SyntaxKind,
}

/// One-based inclusive line range that can be folded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FoldRegion {
    /// Line containing the opening brace.
    pub start_line: u32,
    /// Line containing the matching closing brace.
    pub end_line: u32,
}

/// Find coarse keyword, comment, and number spans in source text.
///
/// `extension` is a bare lowercase suffix. Supported keyword tables are
/// `rs`, `js`, `jsx`, `ts`, `tsx`, `py`, and `go`; unknown suffixes produce
/// no keyword spans. Returned offsets are UTF-8 byte offsets and output is
/// capped at 4,096 spans. This low-level helper is independent of the
/// private syntect-based TUI renderer.
pub fn syntax_spans(content: &str, extension: &str) -> Vec<SyntaxSpan> {
    let keywords: &[&str] = match extension {
        "rs" => &[
            "fn", "let", "mut", "pub", "impl", "struct", "enum", "match", "use", "mod", "async",
            "await", "return",
        ],
        "js" | "jsx" | "ts" | "tsx" => &[
            "const",
            "let",
            "function",
            "class",
            "interface",
            "type",
            "import",
            "export",
            "return",
            "async",
            "await",
        ],
        "py" => &[
            "def", "class", "import", "from", "return", "async", "await", "if", "else",
        ],
        "go" => &[
            "func",
            "package",
            "import",
            "type",
            "struct",
            "interface",
            "return",
            "go",
            "defer",
        ],
        _ => &[],
    };
    let mut spans = Vec::new();
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            let start = offset + line.len().saturating_sub(trimmed.len());
            spans.push(SyntaxSpan {
                start,
                end: offset + line.trim_end_matches('\n').len(),
                kind: SyntaxKind::Comment,
            });
            offset += line.len();
            continue;
        }
        let mut token_start = None;
        for (index, character) in line.char_indices() {
            if character.is_ascii_alphanumeric() || character == '_' {
                token_start.get_or_insert(index);
            } else if let Some(start) = token_start.take() {
                let token = &line[start..index];
                if keywords.contains(&token) {
                    spans.push(SyntaxSpan {
                        start: offset + start,
                        end: offset + index,
                        kind: SyntaxKind::Keyword,
                    });
                } else if token.chars().all(|character| character.is_ascii_digit()) {
                    spans.push(SyntaxSpan {
                        start: offset + start,
                        end: offset + index,
                        kind: SyntaxKind::Number,
                    });
                }
            }
        }
        offset += line.len();
    }
    spans.truncate(4096);
    spans
}

/// Return the matching bracket's character index, if `character_index` points
/// to `(`, `[`, `{`, `)`, `]`, or `}` and a balanced partner exists.
pub fn matching_bracket(content: &str, character_index: usize) -> Option<usize> {
    let characters = content.chars().collect::<Vec<_>>();
    let bracket = *characters.get(character_index)?;
    let (open, close, direction) = match bracket {
        '(' => ('(', ')', 1_i32),
        '[' => ('[', ']', 1),
        '{' => ('{', '}', 1),
        ')' => ('(', ')', -1),
        ']' => ('[', ']', -1),
        '}' => ('{', '}', -1),
        _ => return None,
    };
    let mut depth = 0_i32;
    let mut index = character_index as i32;
    loop {
        index += direction;
        let current = *characters.get(index as usize)?;
        if current == open {
            depth += if direction > 0 { 1 } else { -1 };
        } else if current == close {
            depth += if direction > 0 { -1 } else { 1 };
        }
        if depth == -1 {
            return Some(index as usize);
        }
    }
}

/// Collect brace-delimited fold regions using one-based inclusive line numbers.
///
/// The scan is intentionally lexical and returns at most 1,024 regions.
pub fn fold_regions(content: &str) -> Vec<FoldRegion> {
    let mut stack = Vec::new();
    let mut regions = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        for character in line.chars() {
            if character == '{' {
                stack.push(line_index as u32 + 1);
            } else if character == '}'
                && let Some(start_line) = stack.pop()
                && line_index as u32 + 1 > start_line
            {
                regions.push(FoldRegion {
                    start_line,
                    end_line: line_index as u32 + 1,
                });
            }
        }
    }
    regions.truncate(1024);
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_intelligence_finds_keywords_brackets_and_folds() {
        let content = "pub fn main() {\n  let value = (1);\n}\n";
        assert!(
            syntax_spans(content, "rs")
                .iter()
                .any(|span| span.kind == SyntaxKind::Keyword)
        );
        let open = content
            .chars()
            .position(|character| character == '(')
            .unwrap();
        assert_eq!(
            content
                .chars()
                .nth(matching_bracket(content, open).unwrap()),
            Some(')')
        );
        assert_eq!(
            fold_regions(content),
            vec![FoldRegion {
                start_line: 1,
                end_line: 3
            }]
        );
    }
}
