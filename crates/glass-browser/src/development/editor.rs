use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextPosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextSelection {
    pub anchor: TextPosition,
    pub active: TextPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyntaxKind {
    Keyword,
    String,
    Comment,
    Number,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxSpan {
    pub start: usize,
    pub end: usize,
    pub kind: SyntaxKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FoldRegion {
    pub start_line: u32,
    pub end_line: u32,
}

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
