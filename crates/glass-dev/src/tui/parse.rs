//! Incremental tree-sitter parse cache for the native TUI editor.
//!
//! Each open buffer keeps a syntax tree. Edits are applied with [`InputEdit`]
//! derived from the common prefix and suffix, then `Parser::parse` reuses the
//! previous tree. Textobjects walk the cached tree and fall back to lexical
//! matching in [`super::editor`] when a language is missing or a node is not
//! found.

use super::editor::TextObject;
use crate::development::editor::{text_position_at_offset, text_position_offset};
use crate::development::{TextPosition, TextSelection};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

const MAX_CACHED_TREES: usize = 24;
const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;

/// Languages with a bundled tree-sitter grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageId {
    Rust,
    JavaScript,
    TypeScript,
    Tsx,
    Python,
    Go,
    Json,
    Html,
    Css,
    Bash,
}

impl LanguageId {
    fn language(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Html => tree_sitter_html::LANGUAGE.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
        }
    }
}

/// Cached syntax trees plus a reusable parser.
pub struct IncrementalSyntax {
    parser: Parser,
    language: Option<LanguageId>,
    trees: HashMap<String, BufferTree>,
    order: Vec<String>,
    /// True when the last successful parse did not reuse a previous tree.
    pub last_full_parse: bool,
    /// Edit applied before the last incremental parse, if any.
    pub last_edit: Option<InputEdit>,
    /// How many ranges [`Tree::changed_ranges`] reported after the last edit.
    pub last_changed_ranges: usize,
    /// True when the last `sync` reused an unchanged cached tree.
    pub last_cache_hit: bool,
}

struct BufferTree {
    language: LanguageId,
    source: String,
    tree: Tree,
}

impl Default for IncrementalSyntax {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalSyntax {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            language: None,
            trees: HashMap::new(),
            order: Vec::new(),
            last_full_parse: true,
            last_edit: None,
            last_changed_ranges: 0,
            last_cache_hit: false,
        }
    }

    /// Parse or incrementally update `path`, returning whether a tree is ready.
    pub fn sync(&mut self, path: &str, source: &str) -> bool {
        self.sync_tree(path, source).is_some()
    }

    /// Syntax-aware textobject covering `position`, or `None` to fall back.
    pub fn textobject(
        &mut self,
        path: &str,
        source: &str,
        position: TextPosition,
        object: TextObject,
    ) -> Option<TextSelection> {
        if matches!(object, TextObject::Word { .. }) {
            return None;
        }
        self.sync_tree(path, source)?;
        let offset = text_position_offset(source, position)?;
        let cached = self.trees.get(path)?;
        let (start, end) =
            object_byte_range(&cached.tree, source, cached.language, offset, object)?;
        Some(TextSelection {
            anchor: text_position_at_offset(source, start)?,
            active: text_position_at_offset(source, end)?,
        })
    }

    /// Every structural range of the same object as `position`.
    ///
    /// Arguments and parameters stay in the current list. Fields stay on the
    /// current declaration. Functions, strings, comments, and pairs are
    /// file-scoped. Words fall back to the lexical matcher.
    pub fn same_textobjects(
        &mut self,
        path: &str,
        source: &str,
        position: TextPosition,
        object: TextObject,
    ) -> Vec<TextSelection> {
        if matches!(object, TextObject::Word { .. }) {
            return Vec::new();
        }
        if self.sync_tree(path, source).is_none() {
            return Vec::new();
        }
        let Some(offset) = text_position_offset(source, position) else {
            return Vec::new();
        };
        let Some(cached) = self.trees.get(path) else {
            return Vec::new();
        };
        byte_ranges_to_selections(
            source,
            collect_same_object_ranges(&cached.tree, source, cached.language, offset, object),
        )
    }

    fn sync_tree(&mut self, path: &str, source: &str) -> Option<&Tree> {
        if source.len() > MAX_SOURCE_BYTES {
            self.drop_path(path);
            self.last_cache_hit = false;
            return None;
        }
        let language = language_id_from_path(path)?;
        if !self.ensure_language(language) {
            self.drop_path(path);
            self.last_cache_hit = false;
            return None;
        }

        let cached_unchanged = self
            .trees
            .get(path)
            .is_some_and(|cached| cached.language == language && cached.source == source);
        if cached_unchanged {
            self.last_cache_hit = true;
            self.touch(path);
            return self.trees.get(path).map(|cached| &cached.tree);
        }

        if let Some(mut cached) = self.trees.remove(path)
            && cached.language == language
        {
            let edit = compute_input_edit(&cached.source, source);
            cached.tree.edit(&edit);
            match self.parser.parse(source, Some(&cached.tree)) {
                Some(new_tree) => {
                    self.last_full_parse = false;
                    self.last_cache_hit = false;
                    self.last_edit = Some(edit);
                    self.last_changed_ranges = cached.tree.changed_ranges(&new_tree).count();
                    cached.tree = new_tree;
                    cached.source = source.to_string();
                    self.trees.insert(path.to_string(), cached);
                    self.touch(path);
                    self.evict();
                    return self.trees.get(path).map(|cached| &cached.tree);
                }
                None => {
                    tracing::debug!(path, "incremental parse timed out; retrying full parse");
                }
            }
        }

        let tree = self.parser.parse(source, None)?;
        self.last_full_parse = true;
        self.last_cache_hit = false;
        self.last_edit = None;
        self.last_changed_ranges = 0;
        self.trees.insert(
            path.to_string(),
            BufferTree {
                language,
                source: source.to_string(),
                tree,
            },
        );
        self.touch(path);
        self.evict();
        self.trees.get(path).map(|cached| &cached.tree)
    }

    fn ensure_language(&mut self, language: LanguageId) -> bool {
        if self.language == Some(language) {
            return true;
        }
        match self.parser.set_language(&language.language()) {
            Ok(()) => {
                self.language = Some(language);
                true
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    ?language,
                    "tree-sitter rejected grammar (ABI mismatch)"
                );
                false
            }
        }
    }

    fn touch(&mut self, path: &str) {
        self.order.retain(|entry| entry != path);
        self.order.push(path.to_string());
    }

    fn evict(&mut self) {
        while self.trees.len() > MAX_CACHED_TREES {
            let Some(oldest) = self.order.first().cloned() else {
                break;
            };
            self.order.remove(0);
            self.trees.remove(&oldest);
        }
    }

    fn drop_path(&mut self, path: &str) {
        self.trees.remove(path);
        self.order.retain(|entry| entry != path);
    }
}

/// Map a buffer path to a bundled grammar.
pub fn language_id_from_path(path: &str) -> Option<LanguageId> {
    let path = Path::new(path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match file_name.as_str() {
        "makefile" | "gnumakefile" | "justfile" | "dockerfile" | ".env" | ".bashrc"
        | ".bash_profile" | ".zshrc" => return Some(LanguageId::Bash),
        _ if file_name.starts_with("dockerfile.") => return Some(LanguageId::Bash),
        _ => {}
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "rs" => LanguageId::Rust,
        "js" | "jsx" | "mjs" | "cjs" => LanguageId::JavaScript,
        "ts" | "mts" | "cts" => LanguageId::TypeScript,
        "tsx" => LanguageId::Tsx,
        "py" | "pyi" => LanguageId::Python,
        "go" => LanguageId::Go,
        "json" | "jsonc" => LanguageId::Json,
        "html" | "htm" => LanguageId::Html,
        "css" => LanguageId::Css,
        "sh" | "bash" | "zsh" => LanguageId::Bash,
        _ => return None,
    })
}

/// Describe the replacement that turns `old` into `new` as a tree-sitter edit.
pub fn compute_input_edit(old: &str, new: &str) -> InputEdit {
    let prefix = common_prefix_len(old, new);
    let suffix = common_suffix_len(old, new, prefix);
    let start_byte = prefix;
    let old_end_byte = old.len() - suffix;
    let new_end_byte = new.len() - suffix;
    InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: byte_to_point(old, start_byte),
        old_end_position: byte_to_point(old, old_end_byte),
        new_end_position: byte_to_point(new, new_end_byte),
    }
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

fn byte_to_point(text: &str, byte: usize) -> Point {
    let byte = byte.min(text.len());
    let mut row = 0;
    let mut line_start = 0;
    for (index, value) in text.as_bytes()[..byte].iter().enumerate() {
        if *value == b'\n' {
            row += 1;
            line_start = index + 1;
        }
    }
    Point::new(row, byte - line_start)
}

fn collect_same_object_ranges(
    tree: &Tree,
    source: &str,
    language: LanguageId,
    offset: usize,
    object: TextObject,
) -> Vec<(usize, usize)> {
    let byte = cursor_byte(source, offset);
    let Some(node) = node_at(tree, byte) else {
        return Vec::new();
    };
    match object {
        TextObject::Word { .. } => Vec::new(),
        TextObject::Pair {
            open,
            close,
            around,
        } => collect_pair_ranges(tree.root_node(), open, close, around),
        TextObject::Function { around } => collect_kind_ranges(
            tree.root_node(),
            function_kinds(language),
            around,
            body_kinds(),
        ),
        TextObject::Class { around } => collect_kind_ranges(
            tree.root_node(),
            class_kinds(language),
            around,
            body_kinds(),
        ),
        TextObject::Argument { around } => list_item_ranges(node, argument_list_kinds(), around),
        TextObject::Parameter { around } => {
            list_item_ranges(node, parameter_container_kinds(), around)
        }
        TextObject::Field { around } => sibling_field_ranges(node, around),
        TextObject::String { around } => collect_predicate_ranges(
            tree.root_node(),
            source,
            around,
            |candidate| is_string_kind(candidate.kind()),
            string_range,
        ),
        TextObject::Comment { around } => collect_predicate_ranges(
            tree.root_node(),
            source,
            around,
            |candidate| candidate.kind().contains("comment"),
            comment_range,
        ),
    }
}

fn collect_kind_ranges(
    root: Node<'_>,
    kinds: &[&str],
    around: bool,
    inner_kinds: &[&str],
) -> Vec<(usize, usize)> {
    let mut nodes = Vec::new();
    collect_nodes(root, |node| kinds.contains(&node.kind()), &mut nodes);
    nodes
        .into_iter()
        .map(|matched| {
            if !around && let Some(body) = named_child_with_kinds(matched, inner_kinds) {
                (body.start_byte(), body.end_byte())
            } else {
                (matched.start_byte(), matched.end_byte())
            }
        })
        .collect()
}

fn collect_predicate_ranges(
    root: Node<'_>,
    source: &str,
    around: bool,
    pred: impl Fn(Node<'_>) -> bool + Copy,
    range: impl Fn(Node<'_>, &str, bool) -> Option<(usize, usize)>,
) -> Vec<(usize, usize)> {
    let mut nodes = Vec::new();
    collect_nodes(root, pred, &mut nodes);
    nodes
        .into_iter()
        .filter_map(|node| range(node, source, around))
        .collect()
}

fn collect_pair_ranges(
    root: Node<'_>,
    open: char,
    close: char,
    around: bool,
) -> Vec<(usize, usize)> {
    let mut nodes = Vec::new();
    collect_nodes(root, |_| true, &mut nodes);
    nodes
        .into_iter()
        .filter_map(|node| pair_range(node, open, close, around))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn list_item_ranges(node: Node<'_>, list_kinds: &[&str], around: bool) -> Vec<(usize, usize)> {
    let Some(list) = walk_up(node, |candidate| list_kinds.contains(&candidate.kind())) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    for index in 0..list.named_child_count() {
        let Some(child) = list.named_child(index as u32) else {
            continue;
        };
        let start = child.start_byte();
        let end = if around {
            trailing_separator(child, list.end_byte()).max(child.end_byte())
        } else {
            child.end_byte()
        };
        ranges.push((start, end));
    }
    ranges
}

fn sibling_field_ranges(node: Node<'_>, around: bool) -> Vec<(usize, usize)> {
    let Some(field) = walk_up(node, |candidate| field_kinds().contains(&candidate.kind())) else {
        return Vec::new();
    };
    let parent = field.parent().unwrap_or(field);
    let mut ranges = Vec::new();
    for index in 0..parent.named_child_count() {
        let Some(child) = parent.named_child(index as u32) else {
            continue;
        };
        if field_kinds().contains(&child.kind())
            && let Some(range) = field_range(child, around)
        {
            ranges.push(range);
        }
    }
    if ranges.is_empty() {
        field_range(node, around).into_iter().collect()
    } else {
        ranges
    }
}

fn collect_nodes<'a>(
    node: Node<'a>,
    pred: impl Fn(Node<'a>) -> bool + Copy,
    out: &mut Vec<Node<'a>>,
) {
    if pred(node) {
        out.push(node);
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index as u32) {
            collect_nodes(child, pred, out);
        }
    }
}

fn byte_ranges_to_selections(source: &str, ranges: Vec<(usize, usize)>) -> Vec<TextSelection> {
    let mut selections = Vec::new();
    let mut seen = BTreeSet::new();
    for (start, end) in ranges {
        if start >= end || !seen.insert((start, end)) {
            continue;
        }
        let Some(anchor) = text_position_at_offset(source, start) else {
            continue;
        };
        let Some(active) = text_position_at_offset(source, end) else {
            continue;
        };
        selections.push(TextSelection { anchor, active });
    }
    selections
}

fn object_byte_range(
    tree: &Tree,
    source: &str,
    language: LanguageId,
    offset: usize,
    object: TextObject,
) -> Option<(usize, usize)> {
    let byte = cursor_byte(source, offset);
    let node = node_at(tree, byte)?;
    match object {
        TextObject::Word { .. } => None,
        TextObject::Pair {
            open,
            close,
            around,
        } => pair_range(node, open, close, around),
        TextObject::Function { around } => {
            ancestor_range(node, function_kinds(language), around, body_kinds())
        }
        TextObject::Class { around } => {
            ancestor_range(node, class_kinds(language), around, body_kinds())
        }
        TextObject::Argument { around } => list_item_range(node, argument_list_kinds(), around),
        TextObject::Parameter { around } => list_item_range(node, parameter_list_kinds(), around),
        TextObject::Field { around } => field_range(node, around),
        TextObject::String { around } => string_range(node, source, around),
        TextObject::Comment { around } => comment_range(node, source, around),
    }
}

fn cursor_byte(source: &str, offset: usize) -> usize {
    if source.is_empty() {
        0
    } else if offset >= source.len() {
        source.len() - 1
    } else {
        offset
    }
}

fn node_at<'a>(tree: &'a Tree, byte: usize) -> Option<Node<'a>> {
    let root = tree.root_node();
    let end = byte.saturating_add(1).min(root.end_byte().max(byte));
    root.named_descendant_for_byte_range(byte, end)
        .or_else(|| root.descendant_for_byte_range(byte, end))
}

fn ancestor_range(
    node: Node<'_>,
    kinds: &[&str],
    around: bool,
    inner_kinds: &[&str],
) -> Option<(usize, usize)> {
    let matched = walk_up(node, |candidate| kinds.contains(&candidate.kind()))?;
    if around {
        return Some((matched.start_byte(), matched.end_byte()));
    }
    if let Some(body) = named_child_with_kinds(matched, inner_kinds) {
        return Some((body.start_byte(), body.end_byte()));
    }
    Some((matched.start_byte(), matched.end_byte()))
}

fn field_range(node: Node<'_>, around: bool) -> Option<(usize, usize)> {
    let inner = walk_up(node, |candidate| field_kinds().contains(&candidate.kind()))?;
    let mut matched = inner;
    let mut current = inner.parent();
    while let Some(parent) = current {
        if field_kinds().contains(&parent.kind()) {
            matched = parent;
            current = parent.parent();
        } else {
            break;
        }
    }
    if around {
        Some((matched.start_byte(), matched.end_byte()))
    } else if let Some(name) = named_child_with_kinds(matched, &["field_identifier", "identifier"])
    {
        Some((name.start_byte(), name.end_byte()))
    } else {
        Some((matched.start_byte(), matched.end_byte()))
    }
}

fn list_item_range(node: Node<'_>, list_kinds: &[&str], around: bool) -> Option<(usize, usize)> {
    let list = walk_up(node, |candidate| list_kinds.contains(&candidate.kind()))?;
    let byte = node.start_byte();
    let count = list.named_child_count();
    for index in 0..count {
        let child = list.named_child(index as u32)?;
        let start = child.start_byte();
        let end = child.end_byte();
        let next_start = list
            .named_child(index as u32 + 1)
            .map(|next| next.start_byte())
            .unwrap_or(list.end_byte());
        if byte >= start && byte < next_start.max(end) || (index + 1 == count && byte >= start) {
            if around {
                let trailing = trailing_separator(child, list.end_byte());
                return Some((start, trailing.max(end)));
            }
            return Some((start, end));
        }
    }
    None
}

fn trailing_separator(node: Node<'_>, limit: usize) -> usize {
    let mut end = node.end_byte();
    let mut sibling = node.next_sibling();
    while let Some(current) = sibling {
        if current.start_byte() >= limit {
            break;
        }
        if current.kind() == "," || current.kind() == ";" {
            end = current.end_byte();
            break;
        }
        if current.is_named() {
            break;
        }
        sibling = current.next_sibling();
    }
    end
}

fn pair_range(node: Node<'_>, open: char, close: char, around: bool) -> Option<(usize, usize)> {
    let open_kind = open.to_string();
    let close_kind = close.to_string();
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate.child_count() >= 2 {
            let first = candidate.child(0)?;
            let last = candidate.child(candidate.child_count().saturating_sub(1) as u32)?;
            if first.kind() == open_kind && last.kind() == close_kind {
                if around {
                    return Some((candidate.start_byte(), candidate.end_byte()));
                }
                return Some((first.end_byte(), last.start_byte()));
            }
        }
        current = candidate.parent();
    }
    None
}

fn string_range(node: Node<'_>, source: &str, around: bool) -> Option<(usize, usize)> {
    let matched = walk_up(node, |candidate| is_string_kind(candidate.kind()))?;
    if around {
        return Some((matched.start_byte(), matched.end_byte()));
    }
    if let Some(content) = named_child_with_kinds(
        matched,
        &[
            "string_content",
            "string_fragment",
            "escape_sequence",
            "raw_string_literal_content",
        ],
    ) {
        let start = content.start_byte();
        let last = (0..matched.named_child_count())
            .rev()
            .filter_map(|index| matched.named_child(index as u32))
            .find(|child| {
                matches!(
                    child.kind(),
                    "string_content"
                        | "string_fragment"
                        | "escape_sequence"
                        | "raw_string_literal_content"
                )
            })
            .unwrap_or(content);
        return Some((start, last.end_byte()));
    }
    inner_delimited(matched, source, &['"', '\'', '`'])
}

fn comment_range(node: Node<'_>, source: &str, around: bool) -> Option<(usize, usize)> {
    let matched = walk_up(node, |candidate| candidate.kind().contains("comment"))?;
    if around {
        return Some((matched.start_byte(), matched.end_byte()));
    }
    let text = matched.utf8_text(source.as_bytes()).ok()?;
    let start = matched.start_byte();
    let end = matched.end_byte();
    if let Some(rest) = text.strip_prefix("//") {
        let trim = rest.len() - rest.trim_start().len();
        return Some((start + 2 + trim, end));
    }
    if let Some(rest) = text.strip_prefix('#') {
        let trim = rest.len() - rest.trim_start().len();
        return Some((start + 1 + trim, end));
    }
    if text.starts_with("/*") && text.ends_with("*/") && text.len() >= 4 {
        return Some((start + 2, end - 2));
    }
    Some((start, end))
}

fn inner_delimited(node: Node<'_>, source: &str, delimiters: &[char]) -> Option<(usize, usize)> {
    let text = node.utf8_text(source.as_bytes()).ok()?;
    let mut chars = text.chars();
    let first = chars.next()?;
    let last = chars.next_back();
    if delimiters.contains(&first) && last == Some(first) && text.len() >= first.len_utf8() * 2 {
        return Some((
            node.start_byte() + first.len_utf8(),
            node.end_byte() - first.len_utf8(),
        ));
    }
    Some((node.start_byte(), node.end_byte()))
}

fn walk_up<'a>(node: Node<'a>, predicate: impl Fn(Node<'a>) -> bool) -> Option<Node<'a>> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if predicate(candidate) {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

fn named_child_with_kinds<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    for index in 0..node.named_child_count() {
        let child = node.named_child(index as u32)?;
        if kinds.contains(&child.kind()) {
            return Some(child);
        }
    }
    None
}

fn function_kinds(language: LanguageId) -> &'static [&'static str] {
    match language {
        LanguageId::Rust => &[
            "function_item",
            "function_signature_item",
            "closure_expression",
        ],
        LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::Tsx => &[
            "function_declaration",
            "function_expression",
            "arrow_function",
            "method_definition",
            "generator_function",
            "generator_function_declaration",
            "function",
        ],
        LanguageId::Python => &["function_definition", "lambda"],
        LanguageId::Go => &["function_declaration", "method_declaration", "func_literal"],
        LanguageId::Bash => &["function_definition"],
        LanguageId::Html | LanguageId::Css | LanguageId::Json => &[],
    }
}

fn class_kinds(language: LanguageId) -> &'static [&'static str] {
    match language {
        LanguageId::Rust => &[
            "struct_item",
            "enum_item",
            "impl_item",
            "trait_item",
            "mod_item",
            "union_item",
        ],
        LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::Tsx => &[
            "class_declaration",
            "class",
            "interface_declaration",
            "type_alias_declaration",
            "enum_declaration",
            "abstract_class_declaration",
        ],
        LanguageId::Python => &["class_definition"],
        LanguageId::Go => &[
            "type_declaration",
            "type_spec",
            "struct_type",
            "interface_type",
        ],
        LanguageId::Html => &["element"],
        LanguageId::Css => &["rule_set"],
        LanguageId::Json => &["object"],
        LanguageId::Bash => &[],
    }
}

fn body_kinds() -> &'static [&'static str] {
    &[
        "block",
        "statement_block",
        "compound_statement",
        "body",
        "class_body",
        "declaration_list",
        "field_declaration_list",
        "enum_variant_list",
        "suite",
        "interface_body",
        "enum_body",
    ]
}

fn argument_list_kinds() -> &'static [&'static str] {
    &[
        "arguments",
        "argument_list",
        "call_arguments",
        "tuple_expression",
    ]
}

fn parameter_list_kinds() -> &'static [&'static str] {
    &[
        "parameters",
        "parameter_list",
        "formal_parameters",
        "required_parameter",
    ]
}

fn parameter_container_kinds() -> &'static [&'static str] {
    &["parameters", "parameter_list", "formal_parameters"]
}

fn field_kinds() -> &'static [&'static str] {
    &[
        "field_declaration",
        "field_expression",
        "field_identifier",
        "struct_field",
        "member_expression",
        "attribute",
        "pair",
        "selector_expression",
        "shorthand_property_identifier",
        "property_identifier",
        "field",
    ]
}

fn is_string_kind(kind: &str) -> bool {
    matches!(
        kind,
        "string_literal"
            | "raw_string_literal"
            | "string"
            | "interpreted_string_literal"
            | "template_string"
            | "template_literal"
            | "string_value"
            | "quoted_string"
            | "double_quoted_string"
            | "single_quoted_string"
            | "concatenated_string"
            | "char_literal"
            | "quoted_attribute_value"
    ) || (kind.contains("string")
        && !kind.contains("type")
        && !kind.contains("ident")
        && !kind.contains("name")
        && kind != "string_content"
        && kind != "string_fragment")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_SOURCE: &str = r#"
/// docs
fn outer(a: i32, b: i32) -> i32 {
    let name = "glass";
    inner(a, b + 1);
    a
}

fn inner(x: i32, y: i32) -> i32 {
    x + y
}

struct Point {
    px: i32,
    py: i32,
}
"#;

    fn position_in(source: &str, needle: &str) -> TextPosition {
        let offset = source.find(needle).expect("needle");
        text_position_at_offset(source, offset).expect("position")
    }

    fn slice_of<'a>(source: &'a str, selection: &TextSelection) -> &'a str {
        let start = text_position_offset(source, selection.anchor).expect("start");
        let end = text_position_offset(source, selection.active).expect("end");
        &source[start..end]
    }

    #[test]
    fn language_id_follows_path_extension() {
        assert_eq!(language_id_from_path("src/main.rs"), Some(LanguageId::Rust));
        assert_eq!(language_id_from_path("web/app.tsx"), Some(LanguageId::Tsx));
        assert_eq!(
            language_id_from_path("web/app.ts"),
            Some(LanguageId::TypeScript)
        );
        assert_eq!(language_id_from_path("pkg/mod.go"), Some(LanguageId::Go));
        assert_eq!(language_id_from_path("README.md"), None);
    }

    #[test]
    fn input_edit_describes_middle_insertion() {
        let old = "fn test() {}";
        let new = "fn test(a: u32) {}";
        let edit = compute_input_edit(old, new);
        assert_eq!(edit.start_byte, 8);
        assert_eq!(edit.old_end_byte, 8);
        assert_eq!(edit.new_end_byte, 14);
        assert_eq!(edit.start_position, Point::new(0, 8));
        assert_eq!(edit.new_end_position, Point::new(0, 14));
    }

    #[test]
    fn input_edit_handles_multiline_replace_and_utf8() {
        let old = "fn café() {\n    a\n}\n";
        let new = "fn café() {\n    bé\n}\n";
        let edit = compute_input_edit(old, new);
        assert!(old.is_char_boundary(edit.start_byte));
        assert!(old.is_char_boundary(edit.old_end_byte));
        assert!(new.is_char_boundary(edit.new_end_byte));
        assert!(edit.start_position.row >= 1);
    }

    #[test]
    fn rust_textobjects_cover_function_argument_string_comment_and_field() {
        let mut syntax = IncrementalSyntax::new();
        assert!(syntax.sync("lib.rs", RUST_SOURCE));
        assert!(syntax.last_full_parse);

        let function = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "let name"),
                TextObject::Function { around: true },
            )
            .expect("function");
        let function_text = slice_of(RUST_SOURCE, &function);
        assert!(function_text.contains("fn outer"));
        assert!(function_text.contains("let name"));
        assert!(!function_text.contains("fn inner"));

        let body = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "let name"),
                TextObject::Function { around: false },
            )
            .expect("inner function");
        let body_text = slice_of(RUST_SOURCE, &body);
        assert!(body_text.contains("let name"));
        assert!(!body_text.contains("fn outer"));

        let argument = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "b + 1"),
                TextObject::Argument { around: true },
            )
            .expect("argument");
        assert!(slice_of(RUST_SOURCE, &argument).contains("b + 1"));

        let parameter = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "a: i32"),
                TextObject::Parameter { around: false },
            )
            .expect("parameter");
        assert_eq!(slice_of(RUST_SOURCE, &parameter).trim(), "a: i32");

        let string = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "glass"),
                TextObject::String { around: false },
            )
            .expect("string");
        assert_eq!(slice_of(RUST_SOURCE, &string), "glass");

        let comment = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "docs"),
                TextObject::Comment { around: false },
            )
            .expect("comment");
        assert!(slice_of(RUST_SOURCE, &comment).contains("docs"));

        let field = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "px: i32"),
                TextObject::Field { around: true },
            )
            .expect("field");
        assert!(slice_of(RUST_SOURCE, &field).contains("px: i32"));

        let pair = syntax
            .textobject(
                "lib.rs",
                RUST_SOURCE,
                position_in(RUST_SOURCE, "b + 1"),
                TextObject::Pair {
                    open: '(',
                    close: ')',
                    around: false,
                },
            )
            .expect("pair");
        assert!(slice_of(RUST_SOURCE, &pair).contains("a, b + 1"));
    }

    #[test]
    fn same_textobjects_collect_sibling_parameters_and_functions() {
        let mut syntax = IncrementalSyntax::new();
        let parameters = syntax.same_textobjects(
            "lib.rs",
            RUST_SOURCE,
            position_in(RUST_SOURCE, "a: i32"),
            TextObject::Parameter { around: false },
        );
        let texts = parameters
            .iter()
            .map(|selection| slice_of(RUST_SOURCE, selection).trim())
            .collect::<Vec<_>>();
        assert!(texts.contains(&"a: i32"));
        assert!(texts.contains(&"b: i32"));
        assert_eq!(parameters.len(), 2);

        let functions = syntax.same_textobjects(
            "lib.rs",
            RUST_SOURCE,
            position_in(RUST_SOURCE, "let name"),
            TextObject::Function { around: true },
        );
        assert_eq!(functions.len(), 2);
        let joined = functions
            .iter()
            .map(|selection| slice_of(RUST_SOURCE, selection))
            .collect::<String>();
        assert!(joined.contains("fn outer"));
        assert!(joined.contains("fn inner"));
    }

    #[test]
    fn nested_function_picks_the_innermost_item() {
        let source = "fn outer() {\n    fn inner() {\n        1\n    }\n}\n";
        let mut syntax = IncrementalSyntax::new();
        let selection = syntax
            .textobject(
                "nested.rs",
                source,
                position_in(source, "1"),
                TextObject::Function { around: true },
            )
            .expect("inner fn");
        let text = slice_of(source, &selection);
        assert!(text.contains("fn inner"));
        assert!(!text.contains("fn outer"));
    }

    #[test]
    fn incremental_edit_matches_full_reparse_and_is_not_a_full_parse() {
        let mut original = String::from("fn a() { 1 }\n");
        for index in 0..40 {
            original.push_str(&format!("fn f{index}(x: i32) -> i32 {{ x + {index} }}\n"));
        }
        let mut syntax = IncrementalSyntax::new();
        assert!(syntax.sync("big.rs", &original));
        assert!(syntax.last_full_parse);

        let mut edited = original.clone();
        let insert_at = edited.find("fn f10").expect("target");
        edited.insert_str(insert_at, "// patched\n");
        assert!(syntax.sync("big.rs", &edited));
        assert!(!syntax.last_full_parse, "edit must reuse the previous tree");
        assert!(syntax.last_edit.is_some());
        assert!(syntax.last_changed_ranges > 0);

        let mut fresh = IncrementalSyntax::new();
        assert!(fresh.sync("big.rs", &edited));
        let incremental = syntax
            .textobject(
                "big.rs",
                &edited,
                position_in(&edited, "fn f10"),
                TextObject::Function { around: true },
            )
            .expect("incremental function");
        let full = fresh
            .textobject(
                "big.rs",
                &edited,
                position_in(&edited, "fn f10"),
                TextObject::Function { around: true },
            )
            .expect("full function");
        assert_eq!(slice_of(&edited, &incremental), slice_of(&edited, &full));
        assert!(slice_of(&edited, &incremental).contains("fn f10"));
    }

    #[test]
    fn incremental_delete_and_replace_keep_textobjects() {
        let original = "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n";
        let deleted = "fn main() {\n    println!(\"two\");\n}\n";
        let replaced = "fn main() {\n    println!(\"two\");\n    println!(\"three\");\n}\n";
        let mut syntax = IncrementalSyntax::new();
        syntax.sync("main.rs", original);
        syntax.sync("main.rs", deleted);
        assert!(!syntax.last_full_parse);
        let function = syntax
            .textobject(
                "main.rs",
                deleted,
                position_in(deleted, "println"),
                TextObject::Function { around: true },
            )
            .expect("after delete");
        assert!(slice_of(deleted, &function).contains("two"));
        assert!(!slice_of(deleted, &function).contains("one"));

        syntax.sync("main.rs", replaced);
        assert!(!syntax.last_full_parse);
        let string = syntax
            .textobject(
                "main.rs",
                replaced,
                position_in(replaced, "three"),
                TextObject::String { around: false },
            )
            .expect("after replace");
        assert_eq!(slice_of(replaced, &string), "three");
    }

    #[test]
    fn switching_languages_reparses_from_scratch() {
        let mut syntax = IncrementalSyntax::new();
        assert!(syntax.sync("a.rs", "fn a() {}\n"));
        assert!(syntax.sync("b.py", "def b():\n    return 1\n"));
        assert!(syntax.last_full_parse);
        let function = syntax
            .textobject(
                "b.py",
                "def b():\n    return 1\n",
                position_in("def b():\n    return 1\n", "return"),
                TextObject::Function { around: true },
            )
            .expect("python def");
        assert!(slice_of("def b():\n    return 1\n", &function).contains("def b"));
    }

    #[test]
    fn javascript_and_go_and_json_textobjects_parse() {
        let mut syntax = IncrementalSyntax::new();
        let js = "function add(a, b) {\n  return a + b;\n}\n";
        let selection = syntax
            .textobject(
                "add.js",
                js,
                position_in(js, "return"),
                TextObject::Function { around: true },
            )
            .expect("js function");
        assert!(slice_of(js, &selection).contains("function add"));

        let go = "func Sum(a int, b int) int {\n\treturn a + b\n}\n";
        let selection = syntax
            .textobject(
                "sum.go",
                go,
                position_in(go, "return"),
                TextObject::Function { around: true },
            )
            .expect("go function");
        assert!(slice_of(go, &selection).contains("func Sum"));

        let json = "{\n  \"name\": \"glass\"\n}\n";
        let field = syntax
            .textobject(
                "pkg.json",
                json,
                position_in(json, "name"),
                TextObject::Field { around: true },
            )
            .expect("json pair");
        assert!(slice_of(json, &field).contains("name"));
    }

    #[test]
    fn unsupported_paths_do_not_parse() {
        let mut syntax = IncrementalSyntax::new();
        assert!(!syntax.sync("notes.md", "# heading\n"));
        assert!(
            syntax
                .textobject(
                    "notes.md",
                    "# heading\n",
                    TextPosition { line: 1, column: 1 },
                    TextObject::Function { around: true },
                )
                .is_none()
        );
    }

    #[test]
    fn identical_resync_is_a_cache_hit() {
        let mut syntax = IncrementalSyntax::new();
        syntax.sync("hit.rs", "fn a() {}\n");
        assert!(!syntax.last_cache_hit);
        syntax.sync("hit.rs", "fn a() {}\n");
        assert!(syntax.last_cache_hit, "unchanged source must not reparse");
    }
}
