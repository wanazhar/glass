//! Modal native editor physics for the Glass Dev TUI.
//!
//! Motions, operators, textobjects, jump lists, hunks, ghosts, and prove-it
//! compile live here so the fullscreen editor is a runtime, not a notepad.
//! Syntax-aware objects come from the incremental tree-sitter cache; word
//! and pair objects still have a lexical fallback.

use crate::development::editor::{
    fold_regions, matching_bracket, text_position_at_offset, text_position_offset,
};
use crate::development::{TextPosition, TextSelection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorMode {
    Normal,
    #[default]
    Insert,
    Select,
    Agent,
}

impl EditorMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Select => "SELECT",
            Self::Agent => "AGENT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordForward,
    WordBackward,
    WordEnd,
    LineStart,
    LineEnd,
    FileStart,
    FileEnd,
    MatchPair,
    Find {
        needle: char,
        till: bool,
        reverse: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObject {
    Word {
        around: bool,
    },
    Pair {
        open: char,
        close: char,
        around: bool,
    },
    Function {
        around: bool,
    },
    Argument {
        around: bool,
    },
    Parameter {
        around: bool,
    },
    Field {
        around: bool,
    },
    String {
        around: bool,
    },
    Comment {
        around: bool,
    },
    Class {
        around: bool,
    },
}

pub fn textobject_from_key(character: char, around: bool) -> Option<TextObject> {
    Some(match character {
        'w' => TextObject::Word { around },
        'f' => TextObject::Function { around },
        'a' | ',' => TextObject::Argument { around },
        'p' => TextObject::Parameter { around },
        '.' => TextObject::Field { around },
        's' => TextObject::String { around },
        'c' | '/' => TextObject::Comment { around },
        't' => TextObject::Class { around },
        '(' | ')' | 'b' => TextObject::Pair {
            open: '(',
            close: ')',
            around,
        },
        '{' | '}' | 'B' => TextObject::Pair {
            open: '{',
            close: '}',
            around,
        },
        '[' | ']' => TextObject::Pair {
            open: '[',
            close: ']',
            around,
        },
        '"' => TextObject::Pair {
            open: '"',
            close: '"',
            around,
        },
        '\'' => TextObject::Pair {
            open: '\'',
            close: '\'',
            around,
        },
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jump {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostText {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkKind {
    Change,
    Add,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorHunk {
    pub start_line: u32,
    pub end_line: u32,
    pub kind: HunkKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GutterMark {
    Lsp,
    Git,
    Agent,
    Page,
    Proof,
}

impl GutterMark {
    pub fn glyph(self) -> char {
        match self {
            Self::Lsp => '!',
            Self::Git => '±',
            Self::Agent => '▸',
            Self::Page => '◎',
            Self::Proof => '✓',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProveIt {
    pub intent: String,
    pub verify: serde_json::Value,
}

#[derive(Debug, Default)]
pub struct EditorEngine {
    pub mode: EditorMode,
    pub pending_operator: Option<Operator>,
    pub pending_count: u32,
    pub pending_g: bool,
    pub pending_find: Option<char>,
    /// `f`/`t` wait for a character; `F`/`T` set reverse.
    pub pending_find_till: bool,
    pub pending_find_reverse: bool,
    /// `Some(false)` waits for an inner textobject, `Some(true)` for around.
    pub pending_around: Option<bool>,
    pub pending_mark: bool,
    pub yank: String,
    pub jumps: Vec<Jump>,
    pub jump_index: usize,
    pub ghost: Option<GhostText>,
    pub hunks: Vec<EditorHunk>,
    pub hunk_index: usize,
    pub overlay: Option<String>,
    pub symbol_query: String,
    pub symbols: Vec<(String, u32)>,
    pub symbol_selection: usize,
    pub marks: std::collections::HashMap<char, Jump>,
    pub agent_caret: Option<Jump>,
}

impl EditorEngine {
    pub fn enter_normal(&mut self) {
        self.mode = EditorMode::Normal;
        self.clear_pending();
    }

    pub fn enter_insert(&mut self) {
        self.mode = EditorMode::Insert;
        self.clear_pending();
    }

    pub fn enter_select(&mut self) {
        self.mode = EditorMode::Select;
        self.clear_pending();
    }

    pub fn clear_pending(&mut self) {
        self.pending_operator = None;
        self.pending_count = 0;
        self.pending_g = false;
        self.pending_find = None;
        self.pending_find_till = false;
        self.pending_find_reverse = false;
        self.pending_around = None;
        self.pending_mark = false;
    }

    pub fn count(&self) -> u32 {
        self.pending_count.max(1)
    }

    pub fn push_digit(&mut self, digit: u32) {
        self.pending_count = self.pending_count.saturating_mul(10).saturating_add(digit);
    }

    pub fn record_jump(&mut self, path: &str, line: u32, column: u32) {
        let jump = Jump {
            path: path.to_string(),
            line,
            column,
        };
        if self.jumps.last() == Some(&jump) {
            return;
        }
        if self.jump_index + 1 < self.jumps.len() {
            self.jumps.truncate(self.jump_index + 1);
        }
        self.jumps.push(jump);
        if self.jumps.len() > 64 {
            self.jumps.remove(0);
        }
        self.jump_index = self.jumps.len().saturating_sub(1);
    }

    pub fn jump_back(&mut self) -> Option<Jump> {
        if self.jump_index == 0 {
            return self.jumps.first().cloned();
        }
        self.jump_index -= 1;
        self.jumps.get(self.jump_index).cloned()
    }

    pub fn jump_forward(&mut self) -> Option<Jump> {
        if self.jumps.is_empty() {
            return None;
        }
        if self.jump_index + 1 < self.jumps.len() {
            self.jump_index += 1;
        }
        self.jumps.get(self.jump_index).cloned()
    }

    pub fn set_hunks(&mut self, hunks: Vec<EditorHunk>) {
        self.hunks = hunks;
        if self.hunk_index >= self.hunks.len() {
            self.hunk_index = self.hunks.len().saturating_sub(1);
        }
    }

    pub fn current_hunk(&self) -> Option<&EditorHunk> {
        self.hunks.get(self.hunk_index)
    }

    pub fn step_hunk(&mut self, delta: i32) -> Option<&EditorHunk> {
        if self.hunks.is_empty() {
            return None;
        }
        let next = (self.hunk_index as i32 + delta).rem_euclid(self.hunks.len() as i32) as usize;
        self.hunk_index = next;
        self.hunks.get(self.hunk_index)
    }
}

pub fn clamp_position(content: &str, position: TextPosition) -> TextPosition {
    let lines = line_count(content);
    let line = position.line.clamp(1, lines.max(1));
    let max_column = line_len(content, line).saturating_add(1).max(1);
    TextPosition {
        line,
        column: position.column.clamp(1, max_column),
    }
}

pub fn apply_motion(content: &str, position: TextPosition, motion: Motion) -> TextPosition {
    let position = clamp_position(content, position);
    match motion {
        Motion::Left => offset_column(content, position, -1),
        Motion::Right => offset_column(content, position, 1),
        Motion::Up => offset_line(content, position, -1),
        Motion::Down => offset_line(content, position, 1),
        Motion::WordForward => word_forward(content, position),
        Motion::WordBackward => word_backward(content, position),
        Motion::WordEnd => word_end(content, position),
        Motion::LineStart => TextPosition {
            line: position.line,
            column: 1,
        },
        Motion::LineEnd => TextPosition {
            line: position.line,
            column: line_len(content, position.line).saturating_add(1).max(1),
        },
        Motion::FileStart => TextPosition { line: 1, column: 1 },
        Motion::FileEnd => {
            let line = line_count(content).max(1);
            TextPosition {
                line,
                column: line_len(content, line).saturating_add(1).max(1),
            }
        }
        Motion::MatchPair => match_pair(content, position).unwrap_or(position),
        Motion::Find {
            needle,
            till,
            reverse,
        } => find_char(content, position, needle, till, reverse).unwrap_or(position),
    }
}

pub fn textobject_selection(
    content: &str,
    position: TextPosition,
    object: TextObject,
) -> Option<TextSelection> {
    let position = clamp_position(content, position);
    match object {
        TextObject::Word { around } => word_object(content, position, around),
        TextObject::Pair {
            open,
            close,
            around,
        } => pair_object(content, position, open, close, around),
        TextObject::Function { around } => function_object(content, position, around),
        TextObject::Argument { .. }
        | TextObject::Parameter { .. }
        | TextObject::Field { .. }
        | TextObject::String { .. }
        | TextObject::Comment { .. }
        | TextObject::Class { .. } => None,
    }
}

pub fn line_hunks(original: &str, proposed: &str) -> Vec<EditorHunk> {
    let before = original.split('\n').collect::<Vec<_>>();
    let after = proposed.split('\n').collect::<Vec<_>>();
    let mut hunks = Vec::new();
    let mut line = 1_u32;
    let max = before.len().max(after.len());
    let mut run_start = None;
    let mut run_kind = HunkKind::Change;
    for index in 0..max {
        let left = before.get(index).copied();
        let right = after.get(index).copied();
        let kind = match (left, right) {
            (Some(a), Some(b)) if a == b => None,
            (Some(_), Some(_)) => Some(HunkKind::Change),
            (None, Some(_)) => Some(HunkKind::Add),
            (Some(_), None) => Some(HunkKind::Delete),
            (None, None) => None,
        };
        if let Some(kind) = kind {
            if run_start.is_none() {
                run_start = Some(line);
                run_kind = kind;
            }
        } else if let Some(start) = run_start.take() {
            hunks.push(EditorHunk {
                start_line: start,
                end_line: line.saturating_sub(1).max(start),
                kind: run_kind,
            });
        }
        line = line.saturating_add(1);
    }
    if let Some(start) = run_start {
        hunks.push(EditorHunk {
            start_line: start,
            end_line: line.saturating_sub(1).max(start),
            kind: run_kind,
        });
    }
    hunks
}

pub fn compile_prove_it(text: &str) -> Option<ProveIt> {
    let lower = text.to_ascii_lowercase();
    if !(lower.contains("prove")
        || lower.contains("verify")
        || lower.contains("expect")
        || lower.contains("and prove"))
    {
        return None;
    }
    let mut clauses = Vec::new();
    if let Some(url) = text
        .split_whitespace()
        .find(|token| token.starts_with("http://") || token.starts_with("https://"))
    {
        clauses.push(serde_json::json!({"urlEquals": url}));
    }
    if let Some(rest) = capture_after(&lower, text, &["title ", "titled "]) {
        clauses.push(serde_json::json!({"titleContains": rest}));
    }
    if let Some(rest) = capture_after(&lower, text, &["text ", "contains "]) {
        clauses.push(serde_json::json!({"textContains": rest}));
    }
    if lower.contains("popup") {
        clauses.push(serde_json::json!({"popupOpened": true}));
    }
    if lower.contains("download") {
        clauses.push(serde_json::json!({"downloadStarted": true}));
    }
    if clauses.is_empty() {
        return None;
    }
    let verify = if clauses.len() == 1 {
        clauses.pop().expect("one clause")
    } else {
        serde_json::json!({ "all": clauses })
    };
    Some(ProveIt {
        intent: text.trim().to_string(),
        verify,
    })
}

pub fn evidence_card(
    url: &str,
    revision: u64,
    entity: Option<&str>,
    verify: &str,
    ok: bool,
) -> String {
    format!(
        "PROOF {}\n  url {}\n  revision {}\n  entity {}\n  verify {}",
        if ok { "✓" } else { "×" },
        url,
        revision,
        entity.unwrap_or("—"),
        verify
    )
}

pub fn review_object(
    github_summary: &str,
    github_review: &str,
    proposals: &[(String, String, String)],
    last_verify: Option<&str>,
    git_diff: Option<&str>,
    tasks: Option<&str>,
    checkpoint: Option<&str>,
) -> String {
    let mut lines = vec![
        "REVIEW".to_string(),
        github_summary.trim().to_string(),
        github_review.trim().to_string(),
        String::new(),
        "PROPOSALS".into(),
    ];
    if proposals.is_empty() {
        lines.push("  none pending".into());
    } else {
        for (id, path, state) in proposals {
            lines.push(format!("  {id} · {path} · {state}"));
        }
    }
    if let Some(verify) = last_verify {
        lines.push(String::new());
        lines.push(format!("LAST VERIFY\n{verify}"));
    }
    if let Some(diff) = git_diff.filter(|diff| !diff.trim().is_empty()) {
        lines.push(String::new());
        lines.push("DIFF".into());
        for line in diff.lines().take(40) {
            lines.push(format!("  {line}"));
        }
    }
    if let Some(tasks) = tasks.filter(|tasks| !tasks.trim().is_empty()) {
        lines.push(String::new());
        lines.push("CREW".into());
        for line in tasks.lines().take(16) {
            lines.push(format!("  {line}"));
        }
    }
    if let Some(checkpoint) = checkpoint.filter(|checkpoint| !checkpoint.is_empty()) {
        lines.push(String::new());
        lines.push(format!("CHECKPOINT {checkpoint}"));
    }
    lines.push(
        "\n:review accept [ID] · :review reject [ID] · :review ship TITLE · :review ask".into(),
    );
    lines.join("\n")
}

fn capture_after<'a>(lower: &str, original: &'a str, needles: &[&str]) -> Option<&'a str> {
    for needle in needles {
        if let Some(index) = lower.find(needle) {
            let rest = original[index + needle.len()..].trim();
            if !rest.is_empty() {
                let token = rest
                    .split(['.', '\n'])
                    .next()
                    .unwrap_or(rest)
                    .split(" and ")
                    .next()
                    .unwrap_or(rest)
                    .split_whitespace()
                    .next()
                    .unwrap_or(rest);
                return Some(token.trim());
            }
        }
    }
    None
}

fn line_count(content: &str) -> u32 {
    content.split('\n').count().max(1) as u32
}

fn line_len(content: &str, line: u32) -> u32 {
    content
        .split('\n')
        .nth(line.saturating_sub(1) as usize)
        .map(|value| value.chars().count() as u32)
        .unwrap_or(0)
}

fn offset_column(content: &str, position: TextPosition, delta: i32) -> TextPosition {
    let max = line_len(content, position.line).saturating_add(1).max(1) as i32;
    let column = (position.column as i32 + delta).clamp(1, max) as u32;
    TextPosition {
        line: position.line,
        column,
    }
}

fn offset_line(content: &str, position: TextPosition, delta: i32) -> TextPosition {
    let line = (position.line as i32 + delta).clamp(1, line_count(content) as i32) as u32;
    let max = line_len(content, line).saturating_add(1).max(1);
    TextPosition {
        line,
        column: position.column.min(max),
    }
}

fn is_word(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn word_forward(content: &str, position: TextPosition) -> TextPosition {
    let Some(mut offset) = text_position_offset(content, position) else {
        return position;
    };
    let bytes = content.as_bytes();
    while offset < content.len() && is_word(bytes[offset] as char) {
        offset += 1;
    }
    while offset < content.len() && !is_word(bytes[offset] as char) {
        offset += 1;
    }
    text_position_at_offset(content, offset.min(content.len())).unwrap_or(position)
}

fn word_backward(content: &str, position: TextPosition) -> TextPosition {
    let Some(offset) = text_position_offset(content, position) else {
        return position;
    };
    if offset == 0 {
        return TextPosition { line: 1, column: 1 };
    }
    let bytes = content.as_bytes();
    let mut index = offset.saturating_sub(1);
    while index > 0 && !is_word(bytes[index] as char) {
        index -= 1;
    }
    while index > 0 && is_word(bytes[index.saturating_sub(1)] as char) {
        index -= 1;
    }
    text_position_at_offset(content, index).unwrap_or(position)
}

fn word_end(content: &str, position: TextPosition) -> TextPosition {
    let Some(mut offset) = text_position_offset(content, position) else {
        return position;
    };
    let bytes = content.as_bytes();
    if offset < content.len() {
        offset += 1;
    }
    while offset < content.len() && !is_word(bytes[offset] as char) {
        offset += 1;
    }
    while offset + 1 < content.len() && is_word(bytes[offset + 1] as char) {
        offset += 1;
    }
    text_position_at_offset(content, offset.min(content.len())).unwrap_or(position)
}

fn find_char(
    content: &str,
    position: TextPosition,
    needle: char,
    till: bool,
    reverse: bool,
) -> Option<TextPosition> {
    let line = content
        .split('\n')
        .nth(position.line.saturating_sub(1) as usize)?;
    let chars = line.chars().collect::<Vec<_>>();
    let column = position.column.saturating_sub(1) as usize;
    if reverse {
        let start = column.min(chars.len());
        let found = chars[..start]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, character)| **character == needle)?;
        let index = if till {
            found.0.saturating_add(1)
        } else {
            found.0
        };
        Some(TextPosition {
            line: position.line,
            column: index as u32 + 1,
        })
    } else {
        let start = column.saturating_add(1).min(chars.len());
        let found = chars[start..]
            .iter()
            .enumerate()
            .find(|(_, character)| **character == needle)?;
        let index = start + found.0;
        let index = if till { index.saturating_sub(1) } else { index };
        Some(TextPosition {
            line: position.line,
            column: index as u32 + 1,
        })
    }
}

fn match_pair(content: &str, position: TextPosition) -> Option<TextPosition> {
    let offset = text_position_offset(content, position)?;
    let char_index = content[..offset].chars().count();
    let matched = matching_bracket(content, char_index)?;
    let byte_offset = content.char_indices().nth(matched)?.0;
    text_position_at_offset(content, byte_offset)
}

fn word_object(content: &str, position: TextPosition, around: bool) -> Option<TextSelection> {
    let offset = text_position_offset(content, position)?;
    let bytes = content.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut start = offset.min(bytes.len().saturating_sub(1));
    let mut end = start;
    while start > 0 && is_word(bytes[start] as char) && is_word(bytes[start - 1] as char) {
        start -= 1;
    }
    while end < bytes.len() && is_word(bytes[end] as char) {
        end += 1;
    }
    if around {
        while end < bytes.len() && bytes[end].is_ascii_whitespace() {
            end += 1;
        }
    }
    Some(TextSelection {
        anchor: text_position_at_offset(content, start)?,
        active: text_position_at_offset(content, end)?,
    })
}

fn pair_object(
    content: &str,
    position: TextPosition,
    open: char,
    close: char,
    around: bool,
) -> Option<TextSelection> {
    let offset = text_position_offset(content, position)?;
    let chars = content.chars().collect::<Vec<_>>();
    let mut index = content[..offset].chars().count();
    if index >= chars.len() {
        index = chars.len().saturating_sub(1);
    }
    let mut start = None;
    let mut depth = 0_i32;
    for scan in (0..=index).rev() {
        if chars[scan] == close {
            depth += 1;
        } else if chars[scan] == open {
            if depth == 0 {
                start = Some(scan);
                break;
            }
            depth -= 1;
        }
    }
    let start = start?;
    depth = 0;
    let mut end = None;
    for (scan, character) in chars.iter().enumerate().skip(start + 1) {
        if *character == open {
            depth += 1;
        } else if *character == close {
            if depth == 0 {
                end = Some(scan);
                break;
            }
            depth -= 1;
        }
    }
    let end = end?;
    let (anchor_idx, active_idx) = if around {
        (start, end + 1)
    } else {
        (start + 1, end)
    };
    let anchor = content.char_indices().nth(anchor_idx)?.0;
    let active = content
        .char_indices()
        .nth(active_idx)
        .map(|(offset, _)| offset)
        .unwrap_or(content.len());
    Some(TextSelection {
        anchor: text_position_at_offset(content, anchor)?,
        active: text_position_at_offset(content, active)?,
    })
}

fn function_object(content: &str, position: TextPosition, around: bool) -> Option<TextSelection> {
    let regions = fold_regions(content);
    let region = regions
        .into_iter()
        .rev()
        .find(|region| position.line >= region.start_line && position.line <= region.end_line)?;
    if around || region.end_line <= region.start_line + 1 {
        return Some(TextSelection {
            anchor: TextPosition {
                line: region.start_line,
                column: 1,
            },
            active: TextPosition {
                line: region.end_line,
                column: line_len(content, region.end_line).saturating_add(1).max(1),
            },
        });
    }
    Some(TextSelection {
        anchor: TextPosition {
            line: region.start_line + 1,
            column: 1,
        },
        active: TextPosition {
            line: region.end_line.saturating_sub(1),
            column: line_len(content, region.end_line.saturating_sub(1))
                .saturating_add(1)
                .max(1),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_and_pair_motions_are_structural() {
        let source = "fn main() {\n    hello_world();\n}\n";
        let start = TextPosition { line: 2, column: 5 };
        let end = apply_motion(source, start, Motion::WordEnd);
        assert_eq!(end.line, 2);
        assert!(end.column > start.column);
        let inside_call = TextPosition {
            line: 2,
            column: 16,
        };
        let inner = textobject_selection(
            source,
            inside_call,
            TextObject::Pair {
                open: '(',
                close: ')',
                around: false,
            },
        )
        .expect("paren object");
        assert!(inner.active.column >= inner.anchor.column);
        let _ = (
            GutterMark::Lsp,
            GutterMark::Git,
            GutterMark::Agent,
            GutterMark::Page,
            GutterMark::Proof,
        );
        let function = textobject_selection(source, start, TextObject::Function { around: true })
            .expect("function fold");
        assert_eq!(function.anchor.line, 1);
        assert_eq!(function.active.line, 3);
        assert!(textobject_selection(source, start, TextObject::Word { around: false }).is_some());
        assert!(textobject_selection(source, start, TextObject::Word { around: true }).is_some());
        assert!(
            textobject_selection(
                source,
                inside_call,
                TextObject::Pair {
                    open: '(',
                    close: ')',
                    around: true
                }
            )
            .is_some()
        );
        assert_eq!(
            textobject_from_key('f', false),
            Some(TextObject::Function { around: false })
        );
        let found = apply_motion(
            source,
            TextPosition { line: 2, column: 5 },
            Motion::Find {
                needle: 'w',
                till: false,
                reverse: false,
            },
        );
        assert_eq!(found.line, 2);
        assert!(found.column > 5);
        assert_eq!(GutterMark::Proof.glyph(), '✓');
    }

    #[test]
    fn hunks_group_changed_runs() {
        let hunks = line_hunks("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].start_line, 2);
        assert_eq!(hunks[0].kind, HunkKind::Change);
    }

    #[test]
    fn prove_it_compiles_causal_predicates() {
        let compiled =
            compile_prove_it("click save and prove title Dashboard and popup then download")
                .expect("predicate");
        let all = compiled.verify["all"]
            .as_array()
            .expect("composed predicate");
        assert!(
            all.iter()
                .any(|clause| clause["titleContains"] == "Dashboard")
        );
        assert!(all.iter().any(|clause| clause["popupOpened"] == true));
        assert!(all.iter().any(|clause| clause["downloadStarted"] == true));
        assert!(compile_prove_it("just chat").is_none());
    }

    #[test]
    fn jump_list_walks_back_and_forward() {
        let mut engine = EditorEngine {
            mode: EditorMode::Normal,
            ..EditorEngine::default()
        };
        engine.record_jump("a.rs", 1, 1);
        engine.record_jump("b.rs", 4, 2);
        let back = engine.jump_back().expect("back");
        assert_eq!(back.path, "a.rs");
        let forward = engine.jump_forward().expect("forward");
        assert_eq!(forward.path, "b.rs");
    }
}
