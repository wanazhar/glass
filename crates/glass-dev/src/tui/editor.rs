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
    Comment,
}

impl GutterMark {
    pub fn glyph(self) -> char {
        match self {
            Self::Lsp => '!',
            Self::Git => '±',
            Self::Agent => '▸',
            Self::Page => '◎',
            Self::Proof => '✓',
            Self::Comment => '#',
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
    pub pair_apply: Option<PairApply>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairApply {
    pub proposal_id: String,
    pub actor: String,
    pub path: String,
    pub original: String,
    pub proposed: String,
    pub revealed: usize,
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

    pub fn begin_pair_apply(
        &mut self,
        proposal_id: String,
        actor: String,
        path: String,
        original: String,
        proposed: String,
    ) {
        self.pair_apply = Some(PairApply {
            proposal_id,
            actor,
            path,
            original,
            proposed,
            revealed: 0,
        });
        self.mode = EditorMode::Agent;
    }

    pub fn stop_pair_apply(&mut self) {
        self.pair_apply = None;
        if self.mode == EditorMode::Agent {
            self.enter_normal();
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

/// Stream `proposed` over `original` by revealing `revealed` bytes of the replacement.
pub fn pair_apply_content(original: &str, proposed: &str, revealed: usize) -> String {
    let prefix = common_prefix_len(original, proposed);
    let suffix = common_suffix_len(original, proposed, prefix);
    let new_mid = &proposed[prefix..proposed.len() - suffix];
    let revealed = revealed.min(new_mid.len());
    let mut revealed = revealed;
    while revealed > 0 && !new_mid.is_char_boundary(revealed) {
        revealed -= 1;
    }
    let mut content = String::with_capacity(original.len() + new_mid.len());
    content.push_str(&original[..prefix]);
    content.push_str(&new_mid[..revealed]);
    content.push_str(&original[original.len() - suffix..]);
    content
}

pub fn pair_apply_caret(original: &str, proposed: &str, revealed: usize) -> usize {
    let prefix = common_prefix_len(original, proposed);
    let content = pair_apply_content(original, proposed, revealed);
    (prefix + revealed).min(content.len())
}

pub fn pair_apply_step(
    original: &str,
    proposed: &str,
    revealed: usize,
    step: usize,
) -> (String, usize, bool) {
    let prefix = common_prefix_len(original, proposed);
    let suffix = common_suffix_len(original, proposed, prefix);
    let new_mid_len = proposed.len() - prefix - suffix;
    let mut next = (revealed + step.max(1)).min(new_mid_len);
    while next < new_mid_len && !proposed.is_char_boundary(prefix + next) {
        next += 1;
    }
    let content = pair_apply_content(original, proposed, next);
    (content, next, next >= new_mid_len)
}

fn common_prefix_len(old: &str, new: &str) -> usize {
    let mut prefix = old
        .as_bytes()
        .iter()
        .zip(new.as_bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while prefix > 0 && (!old.is_char_boundary(prefix) || !new.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    prefix
}

fn common_suffix_len(old: &str, new: &str, prefix: usize) -> usize {
    let old_rest = &old.as_bytes()[prefix..];
    let new_rest = &new.as_bytes()[prefix..];
    let mut suffix = old_rest
        .iter()
        .rev()
        .zip(new_rest.iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    suffix = suffix.min(old.len() - prefix).min(new.len() - prefix);
    while suffix > 0
        && (!old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }
    suffix
}

/// Local fill-in-the-middle candidate from the current buffer.
pub fn local_fim(content: &str, offset: usize) -> Option<String> {
    let offset = offset.min(content.len());
    if !content.is_char_boundary(offset) {
        return None;
    }
    let prefix = &content[..offset];
    let token: String = prefix
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if token.len() >= 2 {
        let mut best: Option<&str> = None;
        for candidate in
            content.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        {
            if candidate.starts_with(&token) && candidate.len() > token.len() {
                let rest = &candidate[token.len()..];
                if best.is_none_or(|current| rest.len() > current.len()) {
                    best = Some(rest);
                }
            }
        }
        if let Some(rest) = best.filter(|rest| !rest.is_empty()) {
            return Some(rest.to_string());
        }
    }
    let line_start = prefix.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let line_prefix = content[line_start..offset].trim_start();
    if line_prefix.len() >= 4 {
        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(line_prefix) && trimmed.len() > line_prefix.len() {
                let rest = &trimmed[line_prefix.len()..];
                if !rest.is_empty() {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
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

pub struct ReviewEvidence<'a> {
    pub last_verify: Option<&'a str>,
    pub git_diff: Option<&'a str>,
    pub tasks: Option<&'a str>,
    pub checkpoint: Option<&'a str>,
    pub wake: Option<&'a str>,
}

pub fn review_object(
    github_summary: &str,
    github_review: &str,
    proposals: &[(String, String, String)],
    evidence: ReviewEvidence<'_>,
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
    if let Some(verify) = evidence.last_verify {
        lines.push(String::new());
        lines.push(format!("LAST VERIFY\n{verify}"));
    }
    if let Some(diff) = evidence.git_diff.filter(|diff| !diff.trim().is_empty()) {
        lines.push(String::new());
        lines.push("DIFF".into());
        for line in diff.lines().take(40) {
            lines.push(format!("  {line}"));
        }
    }
    if let Some(tasks) = evidence.tasks.filter(|tasks| !tasks.trim().is_empty()) {
        lines.push(String::new());
        lines.push("CREW".into());
        for line in tasks.lines().take(16) {
            lines.push(format!("  {line}"));
        }
    }
    if let Some(checkpoint) = evidence
        .checkpoint
        .filter(|checkpoint| !checkpoint.is_empty())
    {
        lines.push(String::new());
        lines.push(format!("CHECKPOINT {checkpoint}"));
    }
    if let Some(wake) = evidence.wake.filter(|wake| !wake.trim().is_empty()) {
        lines.push(String::new());
        lines.push(wake.trim_end().to_string());
    }
    lines.push(
        "\n:review accept [ID] · :review reject [ID] · :review ship TITLE · :review ask".into(),
    );
    lines.join("\n")
}

/// Infer a web route from a source path such as `app/settings/page.tsx`.
pub fn inferred_app_path(source_path: &str) -> Option<String> {
    let normalized = source_path.replace('\\', "/");
    let stem = strip_web_extension(&normalized)?;
    if let Some(rest) = strip_dir_prefix(stem, "src/app").or_else(|| strip_dir_prefix(stem, "app"))
    {
        return app_router_path(rest);
    }
    if let Some(rest) =
        strip_dir_prefix(stem, "src/pages").or_else(|| strip_dir_prefix(stem, "pages"))
    {
        return pages_router_path(rest);
    }
    strip_dir_prefix(stem, "src/routes").map(slash_path)
}

/// Join a detected origin with an inferred route.
pub fn join_app_url(base: &str, route: Option<&str>) -> String {
    let origin = base.trim().trim_end_matches('/');
    match route
        .map(str::trim)
        .filter(|route| !route.is_empty() && *route != "/")
    {
        None => origin.to_string(),
        Some(path) => {
            let path = if path.starts_with('/') {
                path
            } else {
                return origin.to_string();
            };
            format!("{origin}{path}")
        }
    }
}

fn strip_web_extension(path: &str) -> Option<&str> {
    for extension in [".tsx", ".ts", ".jsx", ".js", ".vue", ".svelte", ".html"] {
        if let Some(stem) = path.strip_suffix(extension) {
            return Some(stem);
        }
    }
    None
}

fn strip_dir_prefix<'a>(path: &'a str, directory: &str) -> Option<&'a str> {
    let prefix = format!("{directory}/");
    if let Some(rest) = path.strip_prefix(&prefix) {
        return Some(rest);
    }
    let needle = format!("/{prefix}");
    path.find(&needle)
        .map(|index| &path[index + directory.len() + 2..])
}

fn app_router_path(rest: &str) -> Option<String> {
    let rest = rest
        .trim_end_matches("/page")
        .trim_end_matches("/route")
        .trim_end_matches("/layout")
        .trim_end_matches("/default")
        .trim_end_matches("/loading")
        .trim_end_matches("/error");
    let segments = rest
        .split('/')
        .filter(|segment| {
            !segment.is_empty()
                && !segment.starts_with('(')
                && !segment.starts_with('@')
                && *segment != "page"
                && !(segment.starts_with('[') && segment.ends_with(']'))
        })
        .collect::<Vec<_>>();
    Some(if segments.is_empty() {
        "/".into()
    } else {
        format!("/{}", segments.join("/"))
    })
}

fn pages_router_path(rest: &str) -> Option<String> {
    if rest == "_app" || rest == "_document" || rest.starts_with("api/") {
        return None;
    }
    if rest == "index" {
        return Some("/".into());
    }
    if let Some(parent) = rest.strip_suffix("/index") {
        return Some(slash_path(parent));
    }
    Some(slash_path(rest))
}

fn slash_path(rest: &str) -> String {
    if rest.is_empty() {
        "/".into()
    } else {
        format!("/{}", rest.trim_matches('/'))
    }
}

/// Parse LSP `textDocument/inlayHint` results into one-based line suffixes.
pub fn parse_inlay_hints(value: &serde_json::Value) -> Vec<(u32, String)> {
    let items = value
        .as_array()
        .or_else(|| value.get("result").and_then(serde_json::Value::as_array))
        .cloned()
        .unwrap_or_default();
    let mut by_line = std::collections::BTreeMap::<u32, Vec<String>>::new();
    for item in items {
        let line = item
            .pointer("/position/line")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32
            + 1;
        let label = inlay_label(&item["label"]);
        if !label.is_empty() {
            by_line.entry(line).or_default().push(label);
        }
    }
    by_line
        .into_iter()
        .map(|(line, parts)| (line, parts.join(" ")))
        .collect()
}

fn inlay_label(value: &serde_json::Value) -> String {
    if let Some(text) = value.as_str() {
        return text.trim().to_string();
    }
    if let Some(parts) = value.as_array() {
        return parts
            .iter()
            .filter_map(|part| {
                part.as_str().map(str::to_string).or_else(|| {
                    part.get("value")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
            })
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}

const COMPOSER_MENTIONS: &[&str] = &[
    "@file",
    "@page",
    "@entity",
    "@workflow",
    "@workspace",
    "@diagnostic",
    "@selection",
    "@symbol",
    "@memory",
    "@browser",
];

/// Rewrite a bare `@file` / `@symbol` token to the focused buffer path.
pub fn expand_mentions(text: &str, file: Option<&str>) -> String {
    let Some(path) = file.filter(|path| !path.is_empty()) else {
        return text.to_string();
    };
    let mut out = String::with_capacity(text.len() + path.len());
    for token in text.split_inclusive(char::is_whitespace) {
        let core = token.trim_end_matches(|character: char| {
            character.is_whitespace() || matches!(character, ',' | '.' | '?' | '!' | ')' | ']')
        });
        let suffix = &token[core.len()..];
        if core == "@file" || core == "@symbol" {
            out.push_str("@file:");
            out.push_str(path);
            out.push_str(suffix);
        } else {
            out.push_str(token);
        }
    }
    out
}

/// Complete the `@mention` at `cursor`, cycling `@file:` through `files`.
pub fn complete_mention(text: &str, cursor: usize, files: &[String]) -> Option<(String, usize)> {
    let (start, token) = mention_token_at(text, cursor)?;
    let replacement = if token == "@file" || token.starts_with("@file:") {
        let current = token.strip_prefix("@file:").unwrap_or("");
        let path = if current.is_empty() {
            files.first().cloned()
        } else if let Some(index) = files.iter().position(|file| file == current) {
            files.get(index + 1).or_else(|| files.first()).cloned()
        } else {
            files
                .iter()
                .find(|file| file.starts_with(current) || file.contains(current))
                .cloned()
        }?;
        format!("@file:{path}")
    } else {
        let matches = COMPOSER_MENTIONS
            .iter()
            .copied()
            .filter(|mention| mention.starts_with(&token) && *mention != token)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => return None,
            [only] => (*only).to_string(),
            rest => {
                let prefix = longest_common_prefix(rest);
                if prefix.len() > token.len() {
                    prefix
                } else {
                    rest[0].to_string()
                }
            }
        }
    };
    let mut next = String::with_capacity(text.len() + replacement.len());
    next.push_str(&text[..start]);
    next.push_str(&replacement);
    next.push_str(&text[cursor.min(text.len())..]);
    let new_cursor = start + replacement.len();
    Some((next, new_cursor))
}

fn mention_token_at(text: &str, cursor: usize) -> Option<(usize, String)> {
    let cursor = cursor.min(text.len());
    if !text.is_char_boundary(cursor) {
        return None;
    }
    let before = &text[..cursor];
    let start = before
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(0);
    let token = &before[start..];
    token.starts_with('@').then(|| (start, token.to_string()))
}

fn longest_common_prefix(items: &[&str]) -> String {
    let Some(first) = items.first().copied() else {
        return String::new();
    };
    let mut prefix = first.to_string();
    for item in items.iter().skip(1) {
        prefix = prefix
            .chars()
            .zip(item.chars())
            .take_while(|(left, right)| left == right)
            .map(|(left, _)| left)
            .collect();
    }
    prefix
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
            GutterMark::Comment,
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
        assert_eq!(GutterMark::Comment.glyph(), '#');
    }

    #[test]
    fn parse_inlay_hints_groups_labels_by_one_based_line() {
        let hints = parse_inlay_hints(&serde_json::json!([
            {"position":{"line":0,"character":10},"label":": u32"},
            {"position":{"line":0,"character":18},"label":[{"value":"-> "},{"value":"bool"}]}
        ]));
        assert_eq!(hints, vec![(1, ": u32 -> bool".into())]);
    }

    #[test]
    fn expand_mentions_pins_the_focused_file() {
        assert_eq!(
            expand_mentions("inspect @file please", Some("src/main.rs")),
            "inspect @file:src/main.rs please"
        );
        assert_eq!(
            expand_mentions("inspect @file please", None),
            "inspect @file please"
        );
    }

    #[test]
    fn complete_mention_cycles_files_and_completes_prefixes() {
        let files = vec!["src/lib.rs".into(), "src/main.rs".into()];
        let (text, cursor) = complete_mention("look at @fi", 11, &files).expect("prefix");
        assert_eq!(text, "look at @file");
        assert_eq!(cursor, 13);
        let (text, _) = complete_mention("look at @file", 13, &files).expect("first file");
        assert_eq!(text, "look at @file:src/lib.rs");
        let (text, _) = complete_mention(&text, text.len(), &files).expect("next file");
        assert_eq!(text, "look at @file:src/main.rs");
    }

    #[test]
    fn review_object_includes_the_crew_wake() {
        let text = review_object(
            "repo",
            "no review",
            &[],
            ReviewEvidence {
                last_verify: None,
                git_diff: None,
                tasks: None,
                checkpoint: Some("before-crew"),
                wake: Some("WAKE crew-1\n  goal add toggle"),
            },
        );
        assert!(text.contains("CHECKPOINT before-crew"));
        assert!(text.contains("WAKE crew-1"));
        assert!(text.contains("goal add toggle"));
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

    #[test]
    fn pair_apply_reveals_the_replacement_then_matches_proposed() {
        let original = "fn test() {}\n";
        let proposed = "fn test(a: u32) {}\n";
        let mut revealed = 0;
        let mut content = original.to_string();
        let mut done = false;
        while !done {
            let step = pair_apply_step(original, proposed, revealed, 8);
            content = step.0;
            revealed = step.1;
            done = step.2;
        }
        assert_eq!(content, proposed);
        assert!(pair_apply_content(original, proposed, 3).len() >= original.len());
    }

    #[test]
    fn local_fim_completes_an_identifier_from_the_buffer() {
        let source = "fn hello_world() {}\nfn hello_";
        let ghost = local_fim(source, source.len()).expect("fim");
        assert_eq!(ghost, "world");
    }

    #[test]
    fn inferred_app_path_reads_app_and_pages_routes() {
        assert_eq!(
            inferred_app_path("app/settings/page.tsx").as_deref(),
            Some("/settings")
        );
        assert_eq!(
            inferred_app_path("src/app/(dashboard)/orders/[id]/page.tsx").as_deref(),
            Some("/orders")
        );
        assert_eq!(inferred_app_path("pages/index.tsx").as_deref(), Some("/"));
        assert_eq!(
            inferred_app_path("src/pages/account/billing.tsx").as_deref(),
            Some("/account/billing")
        );
        assert_eq!(inferred_app_path("src/main.rs"), None);
        assert_eq!(
            join_app_url("http://localhost:3000/", Some("/settings")),
            "http://localhost:3000/settings"
        );
        assert_eq!(
            join_app_url("http://127.0.0.1:5173", Some("/")),
            "http://127.0.0.1:5173"
        );
    }
}
