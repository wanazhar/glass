use super::config::NativeEngineLimits;
use super::error::NativeEngineError;
use std::collections::BTreeMap;

const MAX_ATTRIBUTE_BYTES: usize = 1024;

/// Generational identity for one node in a native document arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NativeNodeId {
    generation: u32,
    index: u32,
}

impl NativeNodeId {
    pub const fn generation(self) -> u32 {
        self.generation
    }

    pub const fn index(self) -> u32 {
        self.index
    }
}

/// The initial DOM node kinds owned by the native engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeNodeKind {
    Document,
    Element {
        name: String,
        attributes: BTreeMap<String, String>,
    },
    Text(String),
}

/// One arena-owned node with parent and child links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNode {
    id: NativeNodeId,
    parent: Option<NativeNodeId>,
    children: Vec<NativeNodeId>,
    kind: NativeNodeKind,
}

impl NativeNode {
    pub const fn id(&self) -> NativeNodeId {
        self.id
    }

    pub const fn parent(&self) -> Option<NativeNodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NativeNodeId] {
        &self.children
    }

    pub const fn kind(&self) -> &NativeNodeKind {
        &self.kind
    }
}

/// A parsed document owned by one engine generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDocument {
    generation: u32,
    root: NativeNodeId,
    nodes: Vec<NativeNode>,
}

impl NativeDocument {
    /// Parse one bounded HTML document using the initial Glass-owned tree
    /// builder. This parser is intentionally not an HTML5 conformance claim.
    pub fn parse(source: &str, limits: &NativeEngineLimits) -> Result<Self, NativeEngineError> {
        Self::parse_with_generation(source, limits, 1)
    }

    pub(crate) fn parse_with_generation(
        source: &str,
        limits: &NativeEngineLimits,
        generation: u32,
    ) -> Result<Self, NativeEngineError> {
        limits.validate()?;
        if source.len() > limits.max_document_bytes {
            return Err(NativeEngineError::limit(
                "HTML document",
                limits.max_document_bytes,
                source.len(),
            ));
        }
        let tokens = tokenize(source, limits.max_nodes.saturating_mul(2).saturating_add(1))?;
        let root = NativeNodeId {
            generation,
            index: 0,
        };
        let mut document = Self {
            generation,
            root,
            nodes: vec![NativeNode {
                id: root,
                parent: None,
                children: Vec::new(),
                kind: NativeNodeKind::Document,
            }],
        };
        let mut stack = vec![root];

        for token in tokens {
            match token {
                HtmlToken::StartTag {
                    name,
                    attributes,
                    self_closing,
                } => {
                    while stack.len() > 1
                        && stack.last().is_some_and(|id| {
                            document
                                .node(*id)
                                .and_then(|node| match node.kind() {
                                    NativeNodeKind::Element { name, .. } => Some(name.as_str()),
                                    NativeNodeKind::Document | NativeNodeKind::Text(_) => None,
                                })
                                .is_some_and(|current| should_auto_close(current, &name))
                        })
                    {
                        stack.pop();
                    }
                    let current_depth = stack.len().saturating_sub(1);
                    if current_depth >= limits.max_dom_depth {
                        return Err(NativeEngineError::limit(
                            "DOM depth",
                            limits.max_dom_depth,
                            current_depth.saturating_add(1),
                        ));
                    }
                    let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "tree builder lost its document root".into(),
                    })?;
                    let id = document.add_node(
                        parent,
                        NativeNodeKind::Element {
                            name: name.clone(),
                            attributes,
                        },
                        limits.max_nodes,
                    )?;
                    if !self_closing && !is_void_element(&name) {
                        stack.push(id);
                    }
                }
                HtmlToken::EndTag(name) => {
                    if let Some(index) = stack.iter().rposition(|id| {
                        document
                            .node(*id)
                            .and_then(|node| match node.kind() {
                                NativeNodeKind::Element {
                                    name: node_name, ..
                                } => Some(node_name == &name),
                                NativeNodeKind::Document | NativeNodeKind::Text(_) => None,
                            })
                            .unwrap_or(false)
                    }) {
                        stack.truncate(index);
                    }
                }
                HtmlToken::Text(value) => {
                    if !value.is_empty() {
                        let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "tree builder lost its document root".into(),
                        })?;
                        document.add_node(
                            parent,
                            NativeNodeKind::Text(decode_entities(&value)),
                            limits.max_nodes,
                        )?;
                    }
                }
            }
        }
        Ok(document)
    }

    pub(crate) fn empty() -> Self {
        let root = NativeNodeId {
            generation: 1,
            index: 0,
        };
        Self {
            generation: 1,
            root,
            nodes: vec![NativeNode {
                id: root,
                parent: None,
                children: Vec::new(),
                kind: NativeNodeKind::Document,
            }],
        }
    }

    pub const fn generation(&self) -> u32 {
        self.generation
    }

    pub const fn root(&self) -> NativeNodeId {
        self.root
    }

    pub fn node(&self, id: NativeNodeId) -> Option<&NativeNode> {
        (id.generation == self.generation)
            .then(|| self.nodes.get(id.index as usize))
            .flatten()
    }

    pub const fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn title(&self, max_bytes: usize) -> (String, bool) {
        let Some(title_id) = self.find_element(self.root, "title") else {
            return (String::new(), false);
        };
        let mut content = String::new();
        self.collect_raw_text(title_id, &mut content);
        collapse_text(&content, max_bytes)
    }

    /// Return visible text with whitespace collapsed and an explicit truncation
    /// bit. Head, script, style, template, hidden, and aria-hidden subtrees are
    /// omitted until style/layout semantics exist.
    pub fn visible_text(&self, max_bytes: usize) -> (String, bool) {
        let mut output = String::new();
        let mut truncated = false;
        self.collect_visible(self.root, false, &mut output, &mut truncated, max_bytes);
        (output, truncated)
    }

    fn add_node(
        &mut self,
        parent: NativeNodeId,
        kind: NativeNodeKind,
        max_nodes: usize,
    ) -> Result<NativeNodeId, NativeEngineError> {
        if self.nodes.len() >= max_nodes {
            return Err(NativeEngineError::limit(
                "DOM nodes",
                max_nodes,
                self.nodes.len().saturating_add(1),
            ));
        }
        let index = u32::try_from(self.nodes.len()).map_err(|_| {
            NativeEngineError::limit("DOM node index", u32::MAX as usize, self.nodes.len())
        })?;
        let id = NativeNodeId {
            generation: self.generation,
            index,
        };
        self.nodes.push(NativeNode {
            id,
            parent: Some(parent),
            children: Vec::new(),
            kind,
        });
        let Some(parent_node) = self.node_mut(parent) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder referenced an unknown parent".into(),
            });
        };
        parent_node.children.push(id);
        Ok(id)
    }

    fn node_mut(&mut self, id: NativeNodeId) -> Option<&mut NativeNode> {
        (id.generation == self.generation)
            .then(|| self.nodes.get_mut(id.index as usize))
            .flatten()
    }

    fn find_element(&self, id: NativeNodeId, wanted: &str) -> Option<NativeNodeId> {
        let node = self.node(id)?;
        if matches!(node.kind(), NativeNodeKind::Element { name, .. } if name == wanted) {
            return Some(id);
        }
        node.children()
            .iter()
            .find_map(|child| self.find_element(*child, wanted))
    }

    fn collect_raw_text(&self, id: NativeNodeId, output: &mut String) {
        let Some(node) = self.node(id) else {
            return;
        };
        match node.kind() {
            NativeNodeKind::Text(value) => output.push_str(value),
            NativeNodeKind::Document | NativeNodeKind::Element { .. } => {
                for child in node.children() {
                    self.collect_raw_text(*child, output);
                }
            }
        }
    }

    fn collect_visible(
        &self,
        id: NativeNodeId,
        hidden_parent: bool,
        output: &mut String,
        truncated: &mut bool,
        max_bytes: usize,
    ) {
        if *truncated {
            return;
        }
        let Some(node) = self.node(id) else {
            return;
        };
        match node.kind() {
            NativeNodeKind::Text(value) if !hidden_parent => {
                append_collapsed_text(output, value, max_bytes, truncated);
            }
            NativeNodeKind::Document => {
                for child in node.children() {
                    self.collect_visible(*child, hidden_parent, output, truncated, max_bytes);
                }
            }
            NativeNodeKind::Element { name, attributes } => {
                let hidden = hidden_parent
                    || matches!(
                        name.as_str(),
                        "head" | "script" | "style" | "template" | "title"
                    )
                    || attributes.contains_key("hidden")
                    || attributes
                        .get("aria-hidden")
                        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
                for child in node.children() {
                    self.collect_visible(*child, hidden, output, truncated, max_bytes);
                }
            }
            NativeNodeKind::Text(_) => {}
        }
    }
}

#[derive(Debug)]
enum HtmlToken {
    StartTag {
        name: String,
        attributes: BTreeMap<String, String>,
        self_closing: bool,
    },
    EndTag(String),
    Text(String),
}

fn tokenize(source: &str, max_tokens: usize) -> Result<Vec<HtmlToken>, NativeEngineError> {
    let mut tokens = Vec::new();
    let mut position = 0;
    while position < source.len() {
        if source.as_bytes()[position] != b'<' {
            let end = source[position..]
                .find('<')
                .map_or(source.len(), |relative| position + relative);
            push_token(
                &mut tokens,
                HtmlToken::Text(source[position..end].into()),
                max_tokens,
            )?;
            position = end;
            continue;
        }
        if source[position..].starts_with("<!--") {
            let Some(relative_end) = source[position + 4..].find("-->") else {
                return Err(NativeEngineError::Parse {
                    offset: position,
                    reason: "unterminated HTML comment".into(),
                });
            };
            position += 4 + relative_end + 3;
            continue;
        }
        if source[position..].starts_with("<!") || source[position..].starts_with("<?") {
            let Some(end) = find_tag_end(source, position + 2) else {
                return Err(NativeEngineError::Parse {
                    offset: position,
                    reason: "unterminated document declaration".into(),
                });
            };
            position = end + 1;
            continue;
        }
        let is_end_tag = source[position..].starts_with("</");
        let name_position = position + if is_end_tag { 2 } else { 1 };
        if name_position >= source.len() || !is_tag_name_start(source.as_bytes()[name_position]) {
            push_token(&mut tokens, HtmlToken::Text("<".into()), max_tokens)?;
            position += 1;
            continue;
        }
        let Some(end) = find_tag_end(source, name_position) else {
            return Err(NativeEngineError::Parse {
                offset: position,
                reason: "unterminated HTML tag".into(),
            });
        };
        if is_end_tag {
            let raw = &source[name_position..end];
            let Some((name, _)) = read_name(raw, 0) else {
                return Err(NativeEngineError::Parse {
                    offset: position,
                    reason: "end tag has no element name".into(),
                });
            };
            push_token(&mut tokens, HtmlToken::EndTag(name), max_tokens)?;
        } else {
            let raw = &source[name_position..end];
            let (name, attributes, self_closing) = parse_start_tag(raw, position)?;
            push_token(
                &mut tokens,
                HtmlToken::StartTag {
                    name,
                    attributes,
                    self_closing,
                },
                max_tokens,
            )?;
        }
        position = end + 1;
    }
    Ok(tokens)
}

fn push_token(
    tokens: &mut Vec<HtmlToken>,
    token: HtmlToken,
    max_tokens: usize,
) -> Result<(), NativeEngineError> {
    if tokens.len() >= max_tokens {
        return Err(NativeEngineError::limit(
            "HTML tokens",
            max_tokens,
            tokens.len().saturating_add(1),
        ));
    }
    if matches!(&token, HtmlToken::Text(value) if value.is_empty()) {
        return Ok(());
    }
    tokens.push(token);
    Ok(())
}

fn find_tag_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote = None;
    for (offset, byte) in bytes[start..].iter().copied().enumerate() {
        match quote {
            Some(active) if byte == active => quote = None,
            None if byte == b'\'' || byte == b'"' => quote = Some(byte),
            None if byte == b'>' => return Some(start + offset),
            _ => {}
        }
    }
    None
}

fn parse_start_tag(
    raw: &str,
    offset: usize,
) -> Result<(String, BTreeMap<String, String>, bool), NativeEngineError> {
    let (name, mut cursor) = read_name(raw, 0).ok_or_else(|| NativeEngineError::Parse {
        offset,
        reason: "start tag has no element name".into(),
    })?;
    let mut attributes = BTreeMap::new();
    let mut self_closing = false;
    let bytes = raw.as_bytes();
    while cursor < raw.len() {
        skip_ascii_whitespace(bytes, &mut cursor);
        if cursor >= raw.len() {
            break;
        }
        if bytes[cursor] == b'/' {
            self_closing = true;
            cursor += 1;
            continue;
        }
        let Some((attribute_name, next)) = read_name(raw, cursor) else {
            cursor += 1;
            continue;
        };
        cursor = next;
        skip_ascii_whitespace(bytes, &mut cursor);
        let mut value = String::new();
        if cursor < raw.len() && bytes[cursor] == b'=' {
            cursor += 1;
            skip_ascii_whitespace(bytes, &mut cursor);
            if cursor >= raw.len() {
                return Err(NativeEngineError::Parse {
                    offset,
                    reason: "attribute is missing its value".into(),
                });
            }
            if bytes[cursor] == b'\'' || bytes[cursor] == b'"' {
                let quote = bytes[cursor];
                cursor += 1;
                let value_start = cursor;
                while cursor < raw.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                if cursor >= raw.len() {
                    return Err(NativeEngineError::Parse {
                        offset,
                        reason: "quoted attribute is unterminated".into(),
                    });
                }
                value.push_str(&raw[value_start..cursor]);
                cursor += 1;
            } else {
                let value_start = cursor;
                while cursor < raw.len() && !bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                value.push_str(&raw[value_start..cursor]);
            }
        }
        if attribute_name.len() > MAX_ATTRIBUTE_BYTES || value.len() > MAX_ATTRIBUTE_BYTES {
            return Err(NativeEngineError::limit(
                "HTML attribute",
                MAX_ATTRIBUTE_BYTES,
                attribute_name.len().max(value.len()),
            ));
        }
        attributes.insert(attribute_name, decode_entities(&value));
    }
    Ok((name, attributes, self_closing))
}

fn read_name(raw: &str, start: usize) -> Option<(String, usize)> {
    let bytes = raw.as_bytes();
    if start >= bytes.len() || !is_tag_name_start(bytes[start]) {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() && is_tag_name_char(bytes[end]) {
        end += 1;
    }
    Some((raw[start..end].to_ascii_lowercase(), end))
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
}

fn is_tag_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_tag_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':')
}

fn is_void_element(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn should_auto_close(current: &str, next: &str) -> bool {
    (current == "li" && next == "li")
        || (current == "p"
            && matches!(
                next,
                "p" | "div" | "section" | "article" | "h1" | "h2" | "h3"
            ))
        || (matches!(current, "dt" | "dd") && matches!(next, "dt" | "dd"))
        || (matches!(current, "tr") && next == "tr")
        || (matches!(current, "td" | "th") && matches!(next, "td" | "th"))
}

fn decode_entities(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let Some(relative_ampersand) = value[cursor..].find('&') else {
            output.push_str(&value[cursor..]);
            break;
        };
        let ampersand = cursor + relative_ampersand;
        output.push_str(&value[cursor..ampersand]);
        let Some(relative_semicolon) = value[ampersand + 1..].find(';') else {
            output.push('&');
            cursor = ampersand + 1;
            continue;
        };
        let semicolon = ampersand + 1 + relative_semicolon;
        let entity = &value[ampersand + 1..semicolon];
        if let Some(decoded) = decode_entity(entity) {
            output.push(decoded);
            cursor = semicolon + 1;
        } else {
            output.push('&');
            cursor = ampersand + 1;
        }
    }
    output
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
        _ => None,
    }
}

fn collapse_text(value: &str, max_bytes: usize) -> (String, bool) {
    let mut output = String::new();
    let mut truncated = false;
    append_collapsed_text(&mut output, value, max_bytes, &mut truncated);
    (output, truncated)
}

fn append_collapsed_text(output: &mut String, value: &str, max_bytes: usize, truncated: &mut bool) {
    let mut word = String::new();
    for character in value.chars() {
        if character.is_whitespace() {
            append_word(output, &word, max_bytes, truncated);
            word.clear();
            if *truncated {
                return;
            }
        } else {
            word.push(character);
        }
    }
    append_word(output, &word, max_bytes, truncated);
}

fn append_word(output: &mut String, word: &str, max_bytes: usize, truncated: &mut bool) {
    if word.is_empty() || *truncated {
        return;
    }
    let separator = usize::from(!output.is_empty());
    if output.len().saturating_add(separator) >= max_bytes {
        *truncated = true;
        return;
    }
    let available = max_bytes - output.len() - separator;
    if word.len() > available {
        if separator == 1 {
            output.push(' ');
        }
        let mut used = 0;
        for character in word.chars() {
            let width = character.len_utf8();
            if used + width > available {
                break;
            }
            output.push(character);
            used += width;
        }
        *truncated = true;
        return;
    }
    if separator == 1 {
        output.push(' ');
    }
    output.push_str(word);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_title_entities_and_visible_text() {
        let limits = NativeEngineLimits::default();
        let document = NativeDocument::parse(
            "<!doctype html><html><head><title>Example</title><style>.x{}</style></head><body><p>Hello &amp; <span>Glass</span></p><p hidden>secret</p></body></html>",
            &limits,
        )
        .unwrap();
        assert_eq!(document.title(1024), ("Example".into(), false));
        assert_eq!(document.visible_text(1024), ("Hello & Glass".into(), false));
        assert!(document.node_count() > 1);
    }

    #[test]
    fn document_generations_reject_old_node_ids() {
        let limits = NativeEngineLimits::default();
        let first = NativeDocument::parse_with_generation("<p>first</p>", &limits, 1).unwrap();
        let second = NativeDocument::parse_with_generation("<p>second</p>", &limits, 2).unwrap();
        assert!(first.node(first.root()).is_some());
        assert!(second.node(first.root()).is_none());
        assert_eq!(second.generation(), 2);
    }

    #[test]
    fn malformed_quoted_attribute_fails_without_partial_document() {
        let limits = NativeEngineLimits::default();
        let error = NativeDocument::parse("<button title='unfinished>", &limits).unwrap_err();
        assert!(matches!(error, NativeEngineError::Parse { .. }));
    }

    #[test]
    fn dom_depth_limit_counts_open_elements_without_the_document_root() {
        let limits = NativeEngineLimits {
            max_dom_depth: 1,
            ..NativeEngineLimits::default()
        };
        assert!(NativeDocument::parse("<section>one</section>", &limits).is_ok());
        assert!(matches!(
            NativeDocument::parse("<section><p>two</p></section>", &limits),
            Err(NativeEngineError::LimitExceeded { resource, .. }) if resource == "DOM depth"
        ));
    }
}
