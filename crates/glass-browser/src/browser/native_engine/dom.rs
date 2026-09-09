use super::config::NativeEngineLimits;
use super::css::NativeStylesheet;
use super::diagnostics::{NativeDiagnostic, NativeDiagnosticSink, NativeDiagnosticSource};
use super::error::NativeEngineError;
use super::interaction::NativeEventKind;
use super::layout::NativeLayoutSnapshot;
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
use super::{
    config::{MAX_NATIVE_DOM_DEPTH, TextFragmentTerms, Viewport},
    css::{
        AlignContentValue, AlignItemsValue, AlignSelfValue, DirectionValue, FlexBasisValue,
        FlexDirectionValue, FlexWrapValue, FontStyleValue, FontWeightValue, JustifyContentValue,
        NativeBorderRadius, NativeBorderStyleValue, NativeBoxSizing, NativeColor,
        NativeComputedStyle, NativeInheritedStyle, NativeMarginValue, NativeOrderValue,
        NativeTextDecorationSkipInk, NativeTextDecorationSkipSpaces, NativeTextDecorationStyle,
        OverflowValue, TextAlignLastValue, TextAlignValue, TextDecorationValue, TextJustifyValue,
        TextOverflowValue, TextTransformValue, VerticalAlignValue, WhiteSpaceValue, WordBreakValue,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_ATTRIBUTE_BYTES: usize = 1024;
const MAX_LOCATOR_BYTES: usize = crate::browser_backend::MAX_TEXT_BYTES;

const SUPPORTED_ROLES: [&str; 8] = [
    "button", "link", "textbox", "checkbox", "radio", "combobox", "option", "heading",
];

/// Generational identity for one node in a native document arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NativeNodeId {
    generation: u32,
    index: u32,
}

impl NativeNodeId {
    pub(crate) const fn from_parts(generation: u32, index: u32) -> Self {
        Self { generation, index }
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }

    pub const fn index(self) -> u32 {
        self.index
    }
}

/// Mutable state associated with a native element. Raw values remain inside
/// the document owner and are not included in semantic projections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NativeElementState {
    value: Option<String>,
    checked: bool,
    focused: bool,
    selected: bool,
}

impl NativeElementState {
    fn initial(name: &str, attributes: &BTreeMap<String, String>) -> Self {
        Self {
            value: (name == "input").then(|| attributes.get("value").cloned().unwrap_or_default()),
            checked: attributes.contains_key("checked"),
            focused: false,
            selected: attributes.contains_key("selected"),
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeDocumentWire {
    pub(crate) nodes: Vec<NativeNodeWire>,
    pub(crate) computed_styles: Vec<NativeComputedStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeNodeWire {
    pub(crate) parent: Option<u32>,
    pub(crate) children: Vec<u32>,
    pub(crate) kind: NativeNodeKindWire,
    pub(crate) state: NativeElementStateWire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeNodeKindWire {
    Document,
    Element {
        name: String,
        attributes: BTreeMap<String, String>,
    },
    Text(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeElementStateWire {
    pub(crate) value: Option<String>,
    pub(crate) checked: bool,
    pub(crate) focused: bool,
    pub(crate) selected: bool,
}

/// One arena-owned node with parent and child links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNode {
    id: NativeNodeId,
    parent: Option<NativeNodeId>,
    children: Vec<NativeNodeId>,
    kind: NativeNodeKind,
    state: NativeElementState,
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

    /// Return the element's attribute map, or `None` for document/text nodes.
    pub fn attributes(&self) -> Option<&BTreeMap<String, String>> {
        match &self.kind {
            NativeNodeKind::Element { attributes, .. } => Some(attributes),
            NativeNodeKind::Document | NativeNodeKind::Text(_) => None,
        }
    }

    /// Return a case-insensitive attribute value for an element.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes()?.iter().find_map(|(attribute, value)| {
            attribute
                .eq_ignore_ascii_case(name)
                .then_some(value.as_str())
        })
    }

    /// Return the lower-case element name, or `None` for document/text nodes.
    pub fn element_name(&self) -> Option<&str> {
        match &self.kind {
            NativeNodeKind::Element { name, .. } => Some(name),
            NativeNodeKind::Document | NativeNodeKind::Text(_) => None,
        }
    }
}

/// Bounded semantic projection of one supported native element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSemanticNode {
    pub node_id: NativeNodeId,
    pub reference: String,
    pub tag_name: String,
    pub role: String,
    pub name: String,
    pub name_truncated: bool,
    pub input_type: Option<String>,
    pub empty: Option<bool>,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub hidden: bool,
    pub disabled: bool,
    pub read_only: bool,
    pub required: bool,
    pub focused: bool,
}

/// A parsed document owned by one engine generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDocument {
    generation: u32,
    revision: u64,
    root: NativeNodeId,
    nodes: Vec<NativeNode>,
    stylesheet: NativeStylesheet,
    computed_styles: Option<Vec<NativeComputedStyle>>,
    diagnostics: Vec<NativeDiagnostic>,
    diagnostics_truncated: bool,
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
            revision: u64::from(generation),
            root,
            nodes: vec![NativeNode {
                id: root,
                parent: None,
                children: Vec::new(),
                kind: NativeNodeKind::Document,
                state: NativeElementState::default(),
            }],
            stylesheet: NativeStylesheet::default(),
            computed_styles: None,
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
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
                HtmlToken::RawText(value) => {
                    if !value.is_empty() {
                        let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "tree builder lost its document root".into(),
                        })?;
                        document.add_node(parent, NativeNodeKind::Text(value), limits.max_nodes)?;
                    }
                }
            }
        }
        let style_sources = document
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("style"))
            .map(|node| {
                let mut source = String::new();
                document.collect_raw_text(node.id(), &mut source);
                source
            })
            .collect::<Vec<_>>();
        let mut diagnostics = NativeDiagnosticSink::default();
        document.stylesheet =
            NativeStylesheet::from_sources_with_diagnostics(style_sources, &mut diagnostics)?;
        for node in &document.nodes {
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            super::css::collect_declaration_diagnostics(
                inline_style,
                NativeDiagnosticSource::InlineStyle {
                    node_index: node.id().index(),
                },
                0,
                &mut diagnostics,
            );
        }
        let (diagnostics, diagnostics_truncated) = diagnostics.finish();
        document.diagnostics = diagnostics;
        document.diagnostics_truncated = diagnostics_truncated;
        document.normalize_select_defaults();
        Ok(document)
    }

    pub(crate) fn to_content_wire(&self) -> NativeDocumentWire {
        let computed_styles = (0..self.nodes.len())
            .map(|index| {
                self.computed_style_for_layout(NativeNodeId {
                    generation: self.generation,
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                })
            })
            .collect();
        let nodes = self
            .nodes
            .iter()
            .map(|node| NativeNodeWire {
                parent: node.parent.map(|parent| parent.index),
                children: node.children.iter().map(|child| child.index).collect(),
                kind: match &node.kind {
                    NativeNodeKind::Document => NativeNodeKindWire::Document,
                    NativeNodeKind::Element { name, attributes } => NativeNodeKindWire::Element {
                        name: name.clone(),
                        attributes: attributes.clone(),
                    },
                    NativeNodeKind::Text(value) => NativeNodeKindWire::Text(value.clone()),
                },
                state: NativeElementStateWire {
                    value: node.state.value.clone(),
                    checked: node.state.checked,
                    focused: node.state.focused,
                    selected: node.state.selected,
                },
            })
            .collect();
        NativeDocumentWire {
            nodes,
            computed_styles,
        }
    }

    pub(crate) fn from_content_wire(
        wire: NativeDocumentWire,
        limits: &NativeEngineLimits,
        generation: u32,
    ) -> Result<Self, NativeEngineError> {
        limits.validate()?;
        if wire.nodes.is_empty() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an empty document snapshot".into(),
            });
        }
        if wire.nodes.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process DOM nodes",
                limits.max_nodes,
                wire.nodes.len(),
            ));
        }
        if wire.computed_styles.len() != wire.nodes.len() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned incomplete computed styles".into(),
            });
        }
        let node_id = |index: u32| -> Result<NativeNodeId, NativeEngineError> {
            let index_usize = usize::try_from(index).map_err(|_| NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid node index".into(),
            })?;
            if index_usize >= wire.nodes.len() {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range node link".into(),
                });
            }
            Ok(NativeNodeId { generation, index })
        };
        let mut nodes = Vec::with_capacity(wire.nodes.len());
        for (index, wire_node) in wire.nodes.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| {
                NativeEngineError::limit("content-process DOM node index", u32::MAX as usize, index)
            })?;
            let parent = wire_node.parent.map(&node_id).transpose()?;
            let children = wire_node
                .children
                .iter()
                .copied()
                .map(node_id)
                .collect::<Result<Vec<_>, _>>()?;
            let kind = match &wire_node.kind {
                NativeNodeKindWire::Document => NativeNodeKind::Document,
                NativeNodeKindWire::Element { name, attributes } => {
                    if name.len() > MAX_ATTRIBUTE_BYTES
                        || attributes.iter().any(|(name, value)| {
                            name.len() > MAX_ATTRIBUTE_BYTES || value.len() > MAX_ATTRIBUTE_BYTES
                        })
                    {
                        return Err(NativeEngineError::limit(
                            "content-process attribute",
                            MAX_ATTRIBUTE_BYTES,
                            MAX_ATTRIBUTE_BYTES.saturating_add(1),
                        ));
                    }
                    NativeNodeKind::Element {
                        name: name.clone(),
                        attributes: attributes.clone(),
                    }
                }
                NativeNodeKindWire::Text(value) => NativeNodeKind::Text(value.clone()),
            };
            nodes.push(NativeNode {
                id: NativeNodeId { generation, index },
                parent,
                children,
                kind,
                state: NativeElementState {
                    value: wire_node.state.value.clone(),
                    checked: wire_node.state.checked,
                    focused: wire_node.state.focused,
                    selected: wire_node.state.selected,
                },
            });
        }
        if !matches!(nodes[0].kind, NativeNodeKind::Document) || nodes[0].parent.is_some() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid document root".into(),
            });
        }
        let root = NativeNodeId {
            generation,
            index: 0,
        };
        let mut diagnostics = NativeDiagnosticSink::default();
        for node in &nodes {
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            super::css::collect_declaration_diagnostics(
                inline_style,
                NativeDiagnosticSource::InlineStyle {
                    node_index: node.id().index(),
                },
                0,
                &mut diagnostics,
            );
        }
        let (diagnostics, diagnostics_truncated) = diagnostics.finish();
        let mut document = Self {
            generation,
            revision: u64::from(generation),
            root,
            nodes,
            stylesheet: NativeStylesheet::default(),
            computed_styles: Some(wire.computed_styles),
            diagnostics,
            diagnostics_truncated,
        };
        document.normalize_select_defaults();
        Ok(document)
    }

    pub(crate) fn empty() -> Self {
        let root = NativeNodeId {
            generation: 1,
            index: 0,
        };
        Self {
            generation: 1,
            revision: 0,
            root,
            nodes: vec![NativeNode {
                id: root,
                parent: None,
                children: Vec::new(),
                kind: NativeNodeKind::Document,
                state: NativeElementState::default(),
            }],
            stylesheet: NativeStylesheet::default(),
            computed_styles: None,
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
        }
    }

    /// Return the bounded diagnostics collected while parsing this document.
    pub fn diagnostics(&self) -> &[NativeDiagnostic] {
        &self.diagnostics
    }

    pub const fn diagnostics_truncated(&self) -> bool {
        self.diagnostics_truncated
    }

    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// Return the revision represented by this document's semantic references.
    pub const fn revision(&self) -> u64 {
        self.revision
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

    /// Return a revision-bound reference for a current arena node.
    pub fn node_reference(&self, id: NativeNodeId) -> Option<String> {
        self.node(id)
            .map(|_| format!("ref=r{}:n{}", self.revision, id.index))
    }

    /// Return a bounded semantic projection in deterministic document order.
    pub fn semantic_nodes(&self) -> Vec<NativeSemanticNode> {
        self.nodes
            .iter()
            .filter_map(|node| self.semantic_node(node.id))
            .collect()
    }

    /// Derive the current document's bounded integer-pixel layout.
    pub fn layout(&self, viewport: Viewport) -> Result<NativeLayoutSnapshot, NativeEngineError> {
        NativeLayoutSnapshot::compute(self, viewport)
    }

    /// Hit test one point in the supplied viewport without scrolling or
    /// adjusting the requested coordinates.
    pub fn hit_test(
        &self,
        viewport: Viewport,
        x: i64,
        y: i64,
    ) -> Result<Option<NativeNodeId>, NativeEngineError> {
        self.layout(viewport)?.hit_test(x, y)
    }

    /// Derive an immutable display list from the current layout revision.
    pub fn display_list(&self, viewport: Viewport) -> Result<NativeDisplayList, NativeEngineError> {
        let layout = self.layout(viewport)?;
        NativeDisplayList::build(self, &layout)
    }

    /// Derive and replay the current document into a bounded logical surface.
    pub fn rasterize(&self, viewport: Viewport) -> Result<NativeSurface, NativeEngineError> {
        self.display_list(viewport)?.rasterize()
    }

    /// Resolve one explicit semantic locator to exactly one current element.
    pub fn resolve_target(&self, locator: &str) -> Result<NativeNodeId, NativeEngineError> {
        let locator = parse_locator(locator)?;
        match locator {
            NativeLocator::Reference { revision, index } => {
                if revision != self.revision {
                    return Err(NativeEngineError::DetachedTarget);
                }
                let id = NativeNodeId {
                    generation: self.generation,
                    index,
                };
                self.node(id)
                    .filter(|node| node.element_name().is_some())
                    .map(|node| node.id())
                    .ok_or(NativeEngineError::TargetNotFound)
            }
            NativeLocator::Id(value) => {
                self.unique_element_matches(|node| node.attribute("id") == Some(value))
            }
            NativeLocator::Role { role, name } => self.unique_semantic_matches(|node| {
                node.role == role && name.is_none_or(|expected| node.name == expected)
            }),
            NativeLocator::Name(value) => self.unique_semantic_matches(|node| node.name == value),
            NativeLocator::Text(value) => self.unique_semantic_matches(|node| {
                self.element_text(node.node_id, MAX_LOCATOR_BYTES)
                    .is_some_and(|(text, truncated)| !truncated && text == value)
            }),
        }
    }

    pub(crate) fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }

    /// Apply a bounded semantic click after the engine has resolved the target.
    /// Validation is completed before focus or control state is mutated.
    pub(crate) fn apply_click(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported semantic control role".into(),
                })?;
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets are not actionable".into(),
            });
        }
        if !matches!(
            semantic.role.as_str(),
            "button" | "link" | "checkbox" | "radio" | "textbox" | "combobox" | "option"
        ) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "only supported semantic controls accept click".into(),
            });
        }
        let option_select_id = if semantic.role == "option" {
            Some(self.single_select_for_option(id)?)
        } else {
            None
        };
        if semantic.role == "combobox"
            && self
                .node(id)
                .is_some_and(|node| node.attribute("multiple").is_some())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "multiple select controls are not supported".into(),
            });
        }

        let mut events = self.focus_element(id);
        events.push((id, NativeEventKind::Click));
        match semantic.role.as_str() {
            "checkbox" => {
                let node = self.node_mut(id).ok_or(NativeEngineError::DetachedTarget)?;
                node.state.checked = !node.state.checked;
                events.push((id, NativeEventKind::Change));
            }
            "radio" => {
                let group_name = self
                    .node(id)
                    .and_then(|node| node.attribute("name"))
                    .map(str::to_owned);
                let radio_ids = self
                    .nodes
                    .iter()
                    .filter(|node| self.semantic_role(node.id()) == Some("radio"))
                    .filter(|node| {
                        node.id() == id
                            || group_name.as_deref().is_some_and(|name| {
                                !name.is_empty() && node.attribute("name") == Some(name)
                            })
                    })
                    .map(NativeNode::id)
                    .collect::<Vec<_>>();
                for radio_id in radio_ids {
                    let should_be_checked = radio_id == id;
                    let was_checked = self
                        .node(radio_id)
                        .ok_or(NativeEngineError::DetachedTarget)?
                        .state
                        .checked;
                    if was_checked != should_be_checked {
                        self.node_mut(radio_id)
                            .ok_or(NativeEngineError::DetachedTarget)?
                            .state
                            .checked = should_be_checked;
                        events.push((radio_id, NativeEventKind::Change));
                    }
                }
            }
            "option" => {
                let select_id = option_select_id.ok_or(NativeEngineError::DetachedTarget)?;
                let option_ids = self.select_option_ids(select_id);
                let mut selection_changed = false;
                for option_id in option_ids {
                    let should_be_selected = option_id == id;
                    let was_selected = self
                        .node(option_id)
                        .ok_or(NativeEngineError::DetachedTarget)?
                        .state
                        .selected;
                    if was_selected != should_be_selected {
                        self.node_mut(option_id)
                            .ok_or(NativeEngineError::DetachedTarget)?
                            .state
                            .selected = should_be_selected;
                        selection_changed = true;
                    }
                }
                if selection_changed {
                    events.push((select_id, NativeEventKind::Change));
                }
            }
            _ => {}
        }
        Ok(events)
    }

    /// Replace a supported text control's value with bounded input text.
    /// The value itself stays private; semantic projections expose only state.
    pub(crate) fn apply_type(
        &mut self,
        id: NativeNodeId,
        text: &str,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported semantic text-control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets are not actionable".into(),
            });
        }
        if semantic.role != "textbox" || !matches!(semantic.tag_name.as_str(), "input" | "textarea")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "type requires an input or textarea textbox".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.read_only {
            return Err(NativeEngineError::ReadOnlyTarget);
        }
        if text.is_empty() {
            return Err(NativeEngineError::invalid(
                "action text",
                "must not be empty",
            ));
        }
        if text.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "action text",
                MAX_LOCATOR_BYTES,
                text.len(),
            ));
        }

        let mut events = self.focus_element(id);
        self.node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .value = Some(text.to_owned());
        events.push((id, NativeEventKind::Input));
        events.push((id, NativeEventKind::Change));
        Ok(events)
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
        let state = match &kind {
            NativeNodeKind::Element { name, attributes } => {
                NativeElementState::initial(name, attributes)
            }
            NativeNodeKind::Document | NativeNodeKind::Text(_) => NativeElementState::default(),
        };
        self.nodes.push(NativeNode {
            id,
            parent: Some(parent),
            children: Vec::new(),
            kind,
            state,
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

    fn normalize_select_defaults(&mut self) {
        let select_ids = self
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("select"))
            .map(NativeNode::id)
            .collect::<Vec<_>>();
        for select_id in select_ids {
            let option_ids = self.select_option_ids(select_id);
            let selected_id = option_ids
                .iter()
                .find(|option_id| {
                    self.node(**option_id)
                        .is_some_and(|node| node.state.selected)
                })
                .copied()
                .or_else(|| option_ids.first().copied());
            for option_id in option_ids {
                if let Some(node) = self.node_mut(option_id) {
                    node.state.selected = selected_id == Some(option_id);
                }
            }
        }
    }

    fn single_select_for_option(
        &self,
        option_id: NativeNodeId,
    ) -> Result<NativeNodeId, NativeEngineError> {
        let select_id = self
            .find_ancestor_element(option_id, "select")
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "option must belong to a single-select control".into(),
            })?;
        if self
            .node(select_id)
            .is_some_and(|node| node.attribute("multiple").is_some())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "multiple select controls are not supported".into(),
            });
        }
        Ok(select_id)
    }

    fn select_option_ids(&self, select_id: NativeNodeId) -> Vec<NativeNodeId> {
        self.nodes
            .iter()
            .filter(|node| self.semantic_role(node.id()) == Some("option"))
            .filter(|node| self.is_descendant_of(node.id(), select_id))
            .map(NativeNode::id)
            .collect()
    }

    fn is_descendant_of(&self, id: NativeNodeId, ancestor: NativeNodeId) -> bool {
        let mut parent = self.node(id).and_then(NativeNode::parent);
        while let Some(parent_id) = parent {
            if parent_id == ancestor {
                return true;
            }
            parent = self.node(parent_id).and_then(NativeNode::parent);
        }
        false
    }

    fn find_ancestor_element(&self, id: NativeNodeId, wanted: &str) -> Option<NativeNodeId> {
        let mut parent = self.node(id).and_then(NativeNode::parent);
        while let Some(parent_id) = parent {
            let parent_node = self.node(parent_id)?;
            if parent_node.element_name() == Some(wanted) {
                return Some(parent_id);
            }
            parent = parent_node.parent();
        }
        None
    }

    fn focus_element(&mut self, id: NativeNodeId) -> Vec<(NativeNodeId, NativeEventKind)> {
        let focused_ids = self
            .nodes
            .iter()
            .filter(|node| node.state.focused && node.id() != id)
            .map(NativeNode::id)
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for focused_id in focused_ids {
            if let Some(node) = self.node_mut(focused_id) {
                node.state.focused = false;
                events.push((focused_id, NativeEventKind::Blur));
            }
        }
        if let Some(node) = self.node_mut(id)
            && !node.state.focused
        {
            node.state.focused = true;
            events.push((id, NativeEventKind::Focus));
        }
        events
    }

    fn unique_element_matches(
        &self,
        predicate: impl Fn(&NativeNode) -> bool,
    ) -> Result<NativeNodeId, NativeEngineError> {
        let matches = self
            .nodes
            .iter()
            .filter(|node| node.element_name().is_some() && predicate(node))
            .map(NativeNode::id)
            .collect::<Vec<_>>();
        unique_match(matches)
    }

    fn unique_semantic_matches(
        &self,
        predicate: impl Fn(&NativeSemanticNode) -> bool,
    ) -> Result<NativeNodeId, NativeEngineError> {
        let matches = self
            .semantic_nodes()
            .into_iter()
            .filter(predicate)
            .map(|node| node.node_id)
            .collect::<Vec<_>>();
        unique_match(matches)
    }

    fn semantic_node(&self, id: NativeNodeId) -> Option<NativeSemanticNode> {
        let node = self.node(id)?;
        let tag_name = node.element_name()?.to_string();
        let role = self.semantic_role(id)?.to_string();
        let (name, name_truncated) = self.accessible_name(id, &role);
        let input_type = (tag_name == "input").then(|| {
            node.attribute("type")
                .unwrap_or("text")
                .to_ascii_lowercase()
        });
        let empty = matches!(role.as_str(), "textbox" | "combobox")
            .then(|| self.current_value(id).is_none_or(|value| value.is_empty()));
        let checked = matches!(role.as_str(), "checkbox" | "radio").then(|| node.state.checked);
        let selected = (role == "option").then_some(node.state.selected);
        Some(NativeSemanticNode {
            node_id: id,
            reference: self.node_reference(id)?,
            tag_name,
            role,
            name,
            name_truncated,
            input_type,
            empty,
            checked,
            selected,
            hidden: self.is_hidden(id),
            disabled: self.is_disabled(id),
            read_only: self.is_read_only(id),
            required: self.is_required(id),
            focused: node.state.focused,
        })
    }

    pub(crate) fn computed_style_for_layout(&self, id: NativeNodeId) -> NativeComputedStyle {
        if self.node(id).is_none() {
            return NativeComputedStyle::default();
        }
        if let Some(computed_styles) = &self.computed_styles {
            return computed_styles
                .get(id.index as usize)
                .copied()
                .unwrap_or_default();
        }
        let mut chain = Vec::new();
        let mut current = Some(id);
        for _ in 0..=MAX_NATIVE_DOM_DEPTH {
            let Some(current_id) = current else {
                break;
            };
            chain.push(current_id);
            current = self.node(current_id).and_then(NativeNode::parent);
        }

        let mut inherited_color = Some(NativeColor::BLACK);
        let mut inherited_background_color = None;
        let mut inherited_text_decoration_color = NativeColor::BLACK;
        let mut inherited_border_color = [NativeColor::BLACK; 4];
        let mut inherited_border_width = [0; 4];
        let mut inherited_border_style = [NativeBorderStyleValue::None; 4];
        let mut inherited_border_radius = NativeBorderRadius::default();
        let mut inherited_padding = [0; 4];
        let mut inherited_margin = [NativeMarginValue::Length(0); 4];
        let mut inherited_box_sizing = NativeBoxSizing::ContentBox;
        let mut inherited_width = None;
        let mut inherited_height = None;
        let mut inherited_min_width = None;
        let mut inherited_max_width = None;
        let mut inherited_min_height = None;
        let mut inherited_max_height = None;
        let mut inherited_justify_content = JustifyContentValue::FlexStart;
        let mut inherited_align_items = AlignItemsValue::FlexStart;
        let mut inherited_align_self = AlignSelfValue::Auto;
        let mut inherited_align_content = AlignContentValue::FlexStart;
        let mut inherited_flex_direction = FlexDirectionValue::Row;
        let mut inherited_flex_wrap = FlexWrapValue::NoWrap;
        let mut inherited_flex_item_order = NativeOrderValue::default();
        let mut inherited_row_gap = 0;
        let mut inherited_column_gap = 0;
        let mut inherited_direction = DirectionValue::Ltr;
        let mut inherited_flex_grow = 0;
        let mut inherited_flex_shrink = 1;
        let mut inherited_flex_basis = FlexBasisValue::Auto;
        let mut inherited_white_space = WhiteSpaceValue::Normal;
        let mut inherited_line_height = None;
        let mut inherited_text_align = TextAlignValue::Left;
        let mut inherited_text_align_last = TextAlignLastValue::Auto;
        let mut inherited_text_justify = TextJustifyValue::Auto;
        let mut inherited_text_decoration = TextDecorationValue::none();
        let mut inherited_text_decoration_style = NativeTextDecorationStyle::Solid;
        let mut inherited_text_decoration_skip_ink = NativeTextDecorationSkipInk::Auto;
        let mut inherited_text_decoration_skip_spaces = NativeTextDecorationSkipSpaces::None;
        let mut inherited_text_decoration_thickness = 1;
        let mut inherited_text_underline_offset = 0;
        let mut inherited_text_transform = TextTransformValue::None;
        let mut inherited_font_weight = FontWeightValue::Normal;
        let mut inherited_font_style = FontStyleValue::Normal;
        let mut inherited_word_break = WordBreakValue::Normal;
        let mut inherited_text_overflow = TextOverflowValue::Clip;
        let mut inherited_overflow_x = OverflowValue::Other;
        let mut inherited_overflow_y = OverflowValue::Other;
        let mut inherited_vertical_align = VerticalAlignValue::Baseline;
        let mut inherited_text_indent = 0;
        let mut inherited_word_spacing = 0;
        let mut inherited_letter_spacing = 0;
        for current_id in chain.into_iter().rev() {
            let Some(_) = self.node(current_id) else {
                continue;
            };
            let style = self.stylesheet.computed_for_in_document_with_inheritance(
                self,
                current_id,
                NativeInheritedStyle {
                    color: inherited_color,
                    background_color: inherited_background_color,
                    text_decoration_color: inherited_text_decoration_color,
                    border_color: inherited_border_color,
                    border_width: inherited_border_width,
                    border_style: inherited_border_style,
                    border_radius: inherited_border_radius,
                    padding: inherited_padding,
                    margin: inherited_margin,
                    box_sizing: inherited_box_sizing,
                    width: inherited_width,
                    height: inherited_height,
                    min_width: inherited_min_width,
                    max_width: inherited_max_width,
                    min_height: inherited_min_height,
                    max_height: inherited_max_height,
                    justify_content: inherited_justify_content,
                    align_items: inherited_align_items,
                    align_self: inherited_align_self,
                    align_content: inherited_align_content,
                    flex_direction: inherited_flex_direction,
                    flex_wrap: inherited_flex_wrap,
                    flex_item_order: inherited_flex_item_order,
                    row_gap: inherited_row_gap,
                    column_gap: inherited_column_gap,
                    direction: inherited_direction,
                    flex_grow: inherited_flex_grow,
                    flex_shrink: inherited_flex_shrink,
                    flex_basis: inherited_flex_basis,
                    white_space: inherited_white_space,
                    line_height: inherited_line_height,
                    text_align: inherited_text_align,
                    text_align_last: inherited_text_align_last,
                    text_justify: inherited_text_justify,
                    text_decoration: inherited_text_decoration,
                    text_decoration_style: inherited_text_decoration_style,
                    text_decoration_skip_ink: inherited_text_decoration_skip_ink,
                    text_decoration_skip_spaces: inherited_text_decoration_skip_spaces,
                    text_decoration_thickness: inherited_text_decoration_thickness,
                    text_underline_offset: inherited_text_underline_offset,
                    text_transform: inherited_text_transform,
                    font_weight: inherited_font_weight,
                    font_style: inherited_font_style,
                    word_break: inherited_word_break,
                    text_overflow: inherited_text_overflow,
                    overflow_x: inherited_overflow_x,
                    overflow_y: inherited_overflow_y,
                    vertical_align: inherited_vertical_align,
                    text_indent: inherited_text_indent,
                    word_spacing: inherited_word_spacing,
                    letter_spacing: inherited_letter_spacing,
                },
            );
            inherited_color = style.color().or(inherited_color);
            inherited_background_color = style.background_color();
            inherited_text_decoration_color = style
                .text_decoration_color()
                .unwrap_or(style.color().unwrap_or(NativeColor::BLACK));
            inherited_border_color = style.border_colors();
            inherited_border_width = style.border_widths();
            inherited_border_style = style.border_styles();
            inherited_border_radius = style.border_radius();
            inherited_padding = [
                style.padding().top(),
                style.padding().right(),
                style.padding().bottom(),
                style.padding().left(),
            ];
            inherited_margin = style.margin_values();
            inherited_box_sizing = style.box_sizing();
            inherited_width = style.width();
            inherited_height = style.height();
            inherited_min_width = style.min_width();
            inherited_max_width = style.max_width();
            inherited_min_height = style.min_height();
            inherited_max_height = style.max_height();
            inherited_justify_content = style.justify_content();
            inherited_align_items = style.align_items();
            inherited_align_self = style.align_self();
            inherited_align_content = style.align_content();
            inherited_flex_direction = style.flex_direction();
            inherited_flex_wrap = style.flex_wrap();
            inherited_flex_item_order = style.flex_item_order();
            inherited_row_gap = style.row_gap();
            inherited_column_gap = style.column_gap();
            inherited_direction = style.direction();
            inherited_flex_grow = style.flex_grow();
            inherited_flex_shrink = style.flex_shrink();
            inherited_flex_basis = style.flex_basis();
            inherited_white_space = style.white_space();
            inherited_line_height = style.line_height().or(inherited_line_height);
            inherited_text_align = style.text_align();
            inherited_text_align_last = style.text_align_last();
            inherited_text_justify = style.text_justify();
            inherited_text_decoration = style.text_decoration();
            inherited_text_decoration_style = style.text_decoration_style();
            inherited_text_decoration_skip_ink = style.text_decoration_skip_ink();
            inherited_text_decoration_skip_spaces = style.text_decoration_skip_spaces();
            inherited_text_decoration_thickness = style.text_decoration_thickness();
            inherited_text_underline_offset = style.text_underline_offset();
            inherited_text_transform = style.text_transform();
            inherited_font_weight = style.font_weight();
            inherited_font_style = style.font_style();
            inherited_word_break = style.word_break();
            inherited_text_overflow = style.text_overflow();
            inherited_overflow_x = if style.overflow_clip_x() {
                OverflowValue::Clip
            } else {
                OverflowValue::Other
            };
            inherited_overflow_y = if style.overflow_clip_y() {
                OverflowValue::Clip
            } else {
                OverflowValue::Other
            };
            inherited_vertical_align = style.vertical_align();
            inherited_text_indent = style.text_indent();
            inherited_word_spacing = style.word_spacing();
            inherited_letter_spacing = style.letter_spacing();
            if current_id == id {
                return style;
            }
        }
        NativeComputedStyle::default()
    }

    pub(crate) fn is_hidden_for_layout(&self, id: NativeNodeId) -> bool {
        self.is_hidden(id)
    }

    pub(crate) fn raw_text_for_layout(&self, id: NativeNodeId) -> Option<String> {
        self.node(id)?;
        let mut text = String::new();
        self.collect_raw_text(id, &mut text);
        Some(text)
    }

    pub(crate) fn collapse_text_for_layout(value: &str) -> (String, bool) {
        collapse_text(value, MAX_LOCATOR_BYTES)
    }

    pub(crate) fn nearest_clickable_ancestor(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let mut current = Some(id);
        while let Some(current_id) = current {
            if let Some(semantic) = self.semantic_node(current_id)
                && matches!(
                    semantic.role.as_str(),
                    "button" | "link" | "checkbox" | "radio" | "textbox" | "combobox" | "option"
                )
            {
                return Some(current_id);
            }
            current = self.node(current_id).and_then(NativeNode::parent);
        }
        None
    }

    pub(crate) fn link_href(&self, id: NativeNodeId) -> Option<&str> {
        let node = self.node(id)?;
        (node.element_name() == Some("a") && self.semantic_role(id) == Some("link"))
            .then(|| node.attribute("href"))
            .flatten()
    }

    /// Resolve one decoded, exact local fragment target. A unique `id` wins;
    /// when no `id` matches, a unique legacy `<a name>` anchor is accepted.
    /// Duplicate targets are rejected so the bounded engine never invents
    /// browser recovery rules.
    pub(crate) fn fragment_target(&self, fragment: &str) -> Option<NativeNodeId> {
        if fragment.is_empty() {
            return None;
        }
        let mut target = None;
        for node in &self.nodes {
            if node.element_name().is_none() || node.attribute("id") != Some(fragment) {
                continue;
            }
            if target.is_some() {
                return None;
            }
            target = Some(node.id());
        }
        if target.is_some() {
            return target;
        }

        for node in &self.nodes {
            if node.element_name() != Some("a") || node.attribute("name") != Some(fragment) {
                continue;
            }
            if target.is_some() {
                return None;
            }
            target = Some(node.id());
        }
        target
    }

    /// Resolve a bounded text fragment against the first complete visible
    /// layout run in document order. Ranges stay inside one run so matching
    /// never invents cross-node whitespace or browser range semantics.
    pub(crate) fn text_fragment_target(
        &self,
        layout: &NativeLayoutSnapshot,
        terms: &TextFragmentTerms,
    ) -> Option<NativeNodeId> {
        if terms.start.is_empty() {
            return None;
        }
        layout
            .text_runs
            .iter()
            .filter(|run| !run.truncated)
            .find_map(|run| {
                Self::text_fragment_run_matches(&run.text, terms).then_some(run.node_id)
            })
    }

    fn text_fragment_run_matches(text: &str, terms: &TextFragmentTerms) -> bool {
        let mut start_search = 0;
        while start_search < text.len() {
            let Some(relative_start) = text[start_search..].find(&terms.start) else {
                return false;
            };
            let start_offset = start_search.saturating_add(relative_start);
            let start_end = start_offset.saturating_add(terms.start.len());
            let prefix_matches = terms.prefix.as_ref().is_none_or(|prefix| {
                start_offset
                    .checked_sub(prefix.len())
                    .and_then(|prefix_start| text.get(prefix_start..start_offset))
                    == Some(prefix.as_str())
            });
            if prefix_matches {
                let (match_end, range_matches) = if let Some(end) = terms.end.as_deref() {
                    match text
                        .get(start_end..)
                        .and_then(|remaining| remaining.find(end))
                        .map(|relative_end| start_end.saturating_add(relative_end))
                    {
                        Some(end_offset) => (end_offset.saturating_add(end.len()), true),
                        None => (start_end, false),
                    }
                } else {
                    (start_end, true)
                };
                let suffix_matches = terms.suffix.as_ref().is_none_or(|suffix| {
                    text.get(match_end..)
                        .is_some_and(|remaining| remaining.starts_with(suffix))
                });
                if range_matches && suffix_matches {
                    return true;
                }
            }
            start_search = start_end;
        }
        false
    }

    fn semantic_role(&self, id: NativeNodeId) -> Option<&'static str> {
        let node = self.node(id)?;
        if let Some(role) = node
            .attribute("role")
            .and_then(|role| supported_role(role.trim()))
        {
            return Some(role);
        }
        let name = node.element_name()?;
        match name {
            "button" => Some("button"),
            "a" if node.attribute("href").is_some() => Some("link"),
            "textarea" => Some("textbox"),
            "select" => Some("combobox"),
            "option" => Some("option"),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some("heading"),
            "input" => match node
                .attribute("type")
                .unwrap_or("text")
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "button" | "submit" | "reset" | "image" => Some("button"),
                "checkbox" => Some("checkbox"),
                "radio" => Some("radio"),
                "text" | "email" | "password" | "search" | "tel" | "url" => Some("textbox"),
                _ => None,
            },
            _ => None,
        }
    }

    fn accessible_name(&self, id: NativeNodeId, role: &str) -> (String, bool) {
        let Some(node) = self.node(id) else {
            return (String::new(), false);
        };
        if let Some(value) = node
            .attribute("aria-label")
            .filter(|value| !value.is_empty())
        {
            return collapse_text(value, MAX_LOCATOR_BYTES);
        }
        if let Some(value) = node.attribute("aria-labelledby") {
            let mut labelled = String::new();
            for label_id in value.split_ascii_whitespace() {
                if let Some(label) = self.find_element_by_id(label_id) {
                    self.collect_raw_text(label, &mut labelled);
                    labelled.push(' ');
                }
            }
            let collapsed = collapse_text(&labelled, MAX_LOCATOR_BYTES);
            if !collapsed.0.is_empty() {
                return collapsed;
            }
        }
        if let Some(value) = node.attribute("id")
            && let Some(label) = self.find_label_for(value)
        {
            let mut text = String::new();
            self.collect_raw_text(label, &mut text);
            let collapsed = collapse_text(&text, MAX_LOCATOR_BYTES);
            if !collapsed.0.is_empty() {
                return collapsed;
            }
        }
        if let Some(label) = self.find_ancestor_label(id) {
            let mut text = String::new();
            self.collect_raw_text(label, &mut text);
            let collapsed = collapse_text(&text, MAX_LOCATOR_BYTES);
            if !collapsed.0.is_empty() {
                return collapsed;
            }
        }
        if role == "textbox"
            && let Some(value) = node
                .attribute("placeholder")
                .filter(|value| !value.is_empty())
        {
            return collapse_text(value, MAX_LOCATOR_BYTES);
        }
        let mut text = String::new();
        self.collect_raw_text(id, &mut text);
        collapse_text(&text, MAX_LOCATOR_BYTES)
    }

    fn element_text(&self, id: NativeNodeId, max_bytes: usize) -> Option<(String, bool)> {
        self.node(id)?;
        let mut text = String::new();
        self.collect_raw_text(id, &mut text);
        Some(collapse_text(&text, max_bytes))
    }

    fn current_value(&self, id: NativeNodeId) -> Option<String> {
        let node = self.node(id)?;
        match node.element_name()? {
            "input" => node.state.value.clone(),
            "textarea" => node.state.value.clone().or_else(|| {
                let mut text = String::new();
                self.collect_raw_text(id, &mut text);
                Some(text)
            }),
            "select" => {
                let option_ids = self.select_option_ids(id);
                let option_id = option_ids
                    .iter()
                    .copied()
                    .find(|option_id| {
                        self.node(*option_id)
                            .is_some_and(|option| option.state.selected)
                    })
                    .or_else(|| option_ids.first().copied());
                let Some(option_id) = option_id else {
                    return Some(String::new());
                };
                let option = self.node(option_id)?;
                option.attribute("value").map(str::to_owned).or_else(|| {
                    self.element_text(option_id, MAX_LOCATOR_BYTES)
                        .map(|(value, _)| value)
                })
            }
            _ => None,
        }
    }

    fn is_disabled(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return true;
        };
        if node.attribute("disabled").is_some()
            || node
                .attribute("aria-disabled")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            return true;
        }
        let mut parent = node.parent();
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else {
                break;
            };
            if parent_node.element_name() == Some("fieldset")
                && parent_node.attribute("disabled").is_some()
            {
                return true;
            }
            if parent_node.element_name() == Some("select")
                && parent_node.attribute("disabled").is_some()
            {
                return true;
            }
            parent = parent_node.parent();
        }
        false
    }

    fn is_hidden(&self, id: NativeNodeId) -> bool {
        let mut current = Some(id);
        while let Some(current_id) = current {
            let Some(node) = self.node(current_id) else {
                break;
            };
            if let Some(attributes) = node.attributes()
                && has_hidden_signal(attributes)
            {
                return true;
            }
            if self.computed_style_for_layout(current_id).hidden() {
                return true;
            }
            current = node.parent();
        }
        false
    }

    fn is_read_only(&self, id: NativeNodeId) -> bool {
        self.node(id).is_some_and(|node| {
            node.attribute("readonly").is_some()
                || node
                    .attribute("aria-readonly")
                    .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        })
    }

    fn is_required(&self, id: NativeNodeId) -> bool {
        self.node(id)
            .is_some_and(|node| node.attribute("required").is_some())
    }

    fn find_element_by_id(&self, value: &str) -> Option<NativeNodeId> {
        self.nodes.iter().find_map(|node| {
            (node.element_name().is_some() && node.attribute("id") == Some(value))
                .then_some(node.id())
        })
    }

    fn find_label_for(&self, value: &str) -> Option<NativeNodeId> {
        self.nodes.iter().find_map(|node| {
            (node.element_name() == Some("label") && node.attribute("for") == Some(value))
                .then_some(node.id())
        })
    }

    fn find_ancestor_label(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let mut parent = self.node(id)?.parent();
        while let Some(parent_id) = parent {
            let parent_node = self.node(parent_id)?;
            if parent_node.element_name() == Some("label") {
                return Some(parent_id);
            }
            parent = parent_node.parent();
        }
        None
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
            NativeNodeKind::Element { name, .. } => {
                let hidden = hidden_parent
                    || matches!(
                        name.as_str(),
                        "head" | "script" | "style" | "template" | "title"
                    )
                    || self.is_hidden(id);
                for child in node.children() {
                    self.collect_visible(*child, hidden, output, truncated, max_bytes);
                }
            }
            NativeNodeKind::Text(_) => {}
        }
    }
}

#[derive(Debug)]
enum NativeLocator<'a> {
    Reference {
        revision: u64,
        index: u32,
    },
    Id(&'a str),
    Role {
        role: &'a str,
        name: Option<&'a str>,
    },
    Name(&'a str),
    Text(&'a str),
}

fn parse_locator(locator: &str) -> Result<NativeLocator<'_>, NativeEngineError> {
    if locator.is_empty() {
        return Err(NativeEngineError::invalid(
            "action locator",
            "must not be empty",
        ));
    }
    if locator.len() > MAX_LOCATOR_BYTES {
        return Err(NativeEngineError::limit(
            "action locator",
            MAX_LOCATOR_BYTES,
            locator.len(),
        ));
    }
    if let Some(value) = locator.strip_prefix("ref=") {
        let Some(value) = value.strip_prefix('r') else {
            return Err(NativeEngineError::invalid(
                "action locator",
                "reference must use ref=r<revision>:n<arena-index>",
            ));
        };
        let Some((revision, index)) = value.split_once(":n") else {
            return Err(NativeEngineError::invalid(
                "action locator",
                "reference must use ref=r<revision>:n<arena-index>",
            ));
        };
        if revision.is_empty() || index.is_empty() || index.contains(":n") {
            return Err(NativeEngineError::invalid(
                "action locator",
                "reference must use ref=r<revision>:n<arena-index>",
            ));
        }
        let revision = revision.parse::<u64>().map_err(|_| {
            NativeEngineError::invalid(
                "action locator",
                "reference revision must be an unsigned integer",
            )
        })?;
        let index = index.parse::<u32>().map_err(|_| {
            NativeEngineError::invalid(
                "action locator",
                "reference node index must be an unsigned integer",
            )
        })?;
        return Ok(NativeLocator::Reference { revision, index });
    }
    if let Some(value) = locator.strip_prefix("id=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Id(value))
            .ok_or_else(|| NativeEngineError::invalid("action locator", "id must not be empty"));
    }
    if let Some(value) = locator.strip_prefix("role=") {
        if let Some((role, name)) = value.split_once("[name=") {
            if !name.ends_with(']') || name.len() == 1 || role.is_empty() {
                return Err(NativeEngineError::invalid(
                    "action locator",
                    "role/name locator is malformed",
                ));
            }
            let name = &name[..name.len() - 1];
            if name.is_empty() || !SUPPORTED_ROLES.contains(&role) {
                return Err(NativeEngineError::invalid(
                    "action locator",
                    "role/name locator uses an unsupported or empty value",
                ));
            }
            return Ok(NativeLocator::Role {
                role,
                name: Some(name),
            });
        }
        if value.is_empty() || !SUPPORTED_ROLES.contains(&value) {
            return Err(NativeEngineError::invalid(
                "action locator",
                "role locator uses an unsupported or empty role",
            ));
        }
        return Ok(NativeLocator::Role {
            role: value,
            name: None,
        });
    }
    if let Some(value) = locator.strip_prefix("name=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Name(value))
            .ok_or_else(|| NativeEngineError::invalid("action locator", "name must not be empty"));
    }
    if let Some(value) = locator.strip_prefix("text=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Text(value))
            .ok_or_else(|| NativeEngineError::invalid("action locator", "text must not be empty"));
    }
    Err(NativeEngineError::invalid(
        "action locator",
        "use ref=, id=, role=, name=, or text=",
    ))
}

fn unique_match(matches: Vec<NativeNodeId>) -> Result<NativeNodeId, NativeEngineError> {
    match matches.as_slice() {
        [] => Err(NativeEngineError::TargetNotFound),
        [id] => Ok(*id),
        _ => Err(NativeEngineError::AmbiguousTarget {
            matches: matches.len(),
        }),
    }
}

fn supported_role(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();
    SUPPORTED_ROLES
        .iter()
        .copied()
        .find(|role| *role == normalized)
}

fn has_hidden_signal(attributes: &BTreeMap<String, String>) -> bool {
    if attributes.contains_key("hidden")
        || attributes
            .get("aria-hidden")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        return true;
    }
    false
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
    RawText(String),
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
        let mut next_position = end + 1;
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
            let special_text_mode = special_text_mode(&name);
            push_token(
                &mut tokens,
                HtmlToken::StartTag {
                    name: name.clone(),
                    attributes,
                    self_closing,
                },
                max_tokens,
            )?;
            if let Some(decode_entities) = special_text_mode
                && !self_closing
                && !is_void_element(&name)
            {
                let text_start = end + 1;
                let Some(text_end) = find_raw_text_end(source, text_start, &name) else {
                    let value = source[text_start..].to_owned();
                    push_token(
                        &mut tokens,
                        if decode_entities {
                            HtmlToken::Text(value)
                        } else {
                            HtmlToken::RawText(value)
                        },
                        max_tokens,
                    )?;
                    break;
                };
                let value = source[text_start..text_end].to_owned();
                push_token(
                    &mut tokens,
                    if decode_entities {
                        HtmlToken::Text(value)
                    } else {
                        HtmlToken::RawText(value)
                    },
                    max_tokens,
                )?;
                next_position = text_end;
            }
        }
        position = next_position;
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
    if matches!(&token, HtmlToken::Text(value) | HtmlToken::RawText(value) if value.is_empty()) {
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

fn special_text_mode(name: &str) -> Option<bool> {
    match name {
        "script" | "style" => Some(false),
        "title" | "textarea" => Some(true),
        _ => None,
    }
}

fn find_raw_text_end(source: &str, start: usize, name: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut candidate = start;
    while candidate < bytes.len() {
        let relative = source[candidate..].find('<')?;
        let opening = candidate + relative;
        let name_start = opening.checked_add(2)?;
        let name_end = name_start.checked_add(name.len())?;
        if bytes.get(opening + 1) == Some(&b'/')
            && name_end <= bytes.len()
            && bytes[name_start..name_end].eq_ignore_ascii_case(name.as_bytes())
            && bytes
                .get(name_end)
                .is_none_or(|byte| byte.is_ascii_whitespace() || *byte == b'>')
        {
            return Some(opening);
        }
        candidate = opening.saturating_add(1);
    }
    None
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
    fn raw_text_and_rcdata_elements_do_not_create_nested_semantic_nodes() {
        let limits = NativeEngineLimits::default();
        let document = NativeDocument::parse(
            "<ScRiPt>const markup = '<button id=\"fake-script\">Fake</button>';</SCRIPT><STYLE><button id=\"fake-style\">Fake</button></style><title>Doc &amp; <b>Title</b></TITLE><textarea>&lt;button id='fake-textarea'&gt;Fake&lt;/button&gt;</textarea><button id='real'>Real</button>",
            &limits,
        )
        .unwrap();

        assert_eq!(document.title(1024), ("Doc & <b>Title</b>".into(), false));
        let nodes = document.semantic_nodes();
        assert_eq!(nodes.iter().filter(|node| node.role == "button").count(), 1);
        let textarea = nodes.iter().find(|node| node.role == "textbox").unwrap();
        assert_eq!(textarea.name, "<button id='fake-textarea'>Fake</button>");
        assert_eq!(
            nodes
                .iter()
                .find(|node| node.name == "Real")
                .map(|node| node.role.as_str()),
            Some("button")
        );
    }

    #[test]
    fn unterminated_raw_text_consumes_the_bounded_remainder() {
        let document = NativeDocument::parse(
            "<script><button id='fake'>Fake</button>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

        assert!(document.semantic_nodes().is_empty());
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
    fn fragment_target_uses_unique_legacy_name_after_id_precedence() {
        let document = NativeDocument::parse(
            "<a name='legacy'>Legacy</a><a name='café'>UTF-8 legacy</a><div name='not-anchor'>Not an anchor</div><div id='same'>ID wins</div><a name='same'>Name loses</a><a name='duplicate'>One</a><a name='duplicate'>Two</a><p id='duplicate-id'>ID target</p>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let legacy = document
            .node(document.fragment_target("legacy").unwrap())
            .unwrap();
        assert_eq!(legacy.element_name(), Some("a"));
        assert_eq!(legacy.attribute("name"), Some("legacy"));
        assert_eq!(
            document
                .node(document.fragment_target("café").unwrap())
                .unwrap()
                .attribute("name"),
            Some("café")
        );
        assert_eq!(
            document.fragment_target("same"),
            Some(document.resolve_target("id=same").unwrap())
        );
        assert_eq!(document.fragment_target("not-anchor"), None);
        assert_eq!(document.fragment_target("duplicate"), None);
        assert_eq!(document.fragment_target("DUPLICATE"), None);
        assert_eq!(
            document.fragment_target("duplicate-id"),
            Some(document.resolve_target("id=duplicate-id").unwrap())
        );
    }

    #[test]
    fn text_fragment_target_stays_inside_one_visible_layout_run() {
        let document = NativeDocument::parse(
            "<p style='white-space:pre'>target phrase anchor</p><p hidden>hidden target</p><p><span>cross </span><span>run target</span></p>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let layout = document
            .layout(Viewport {
                width: 240,
                height: 100,
                device_scale_factor_milli: 1000,
            })
            .unwrap();
        let first = layout
            .text_runs
            .iter()
            .find(|run| run.text == "target phrase anchor")
            .unwrap();
        let start = TextFragmentTerms {
            prefix: None,
            start: "target phrase".into(),
            end: None,
            suffix: None,
        };
        assert_eq!(
            document.text_fragment_target(&layout, &start),
            Some(first.node_id)
        );
        let range = TextFragmentTerms {
            prefix: None,
            start: "target phrase".into(),
            end: Some("anchor".into()),
            suffix: None,
        };
        assert_eq!(
            document.text_fragment_target(&layout, &range),
            Some(first.node_id)
        );
        let reversed = TextFragmentTerms {
            prefix: None,
            start: "anchor".into(),
            end: Some("target".into()),
            suffix: None,
        };
        assert_eq!(document.text_fragment_target(&layout, &reversed), None);
        let hidden = TextFragmentTerms {
            prefix: None,
            start: "hidden target".into(),
            end: None,
            suffix: None,
        };
        assert_eq!(document.text_fragment_target(&layout, &hidden), None);
        let cross_run = TextFragmentTerms {
            prefix: None,
            start: "cross".into(),
            end: Some("target".into()),
            suffix: None,
        };
        assert_eq!(document.text_fragment_target(&layout, &cross_run), None);
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

    #[test]
    fn semantic_projection_infers_roles_labels_attributes_and_state() {
        let limits = NativeEngineLimits::default();
        let document = NativeDocument::parse(
            r#"<label for="email">Email address</label><input id="email" type="email" placeholder="name@example.com" required><button id="save" aria-label="Save changes">Save</button><a href="/next">Next</a><input id="remember" type="checkbox" checked>"#,
            &limits,
        )
        .unwrap();

        let nodes = document.semantic_nodes();
        let email = nodes.iter().find(|node| node.tag_name == "input").unwrap();
        assert_eq!(email.role, "textbox");
        assert_eq!(email.name, "Email address");
        assert_eq!(email.input_type.as_deref(), Some("email"));
        assert_eq!(email.empty, Some(true));
        assert!(email.required);
        assert_eq!(
            email.reference,
            format!("ref=r1:n{}", email.node_id.index())
        );
        assert_eq!(
            document.node(email.node_id).unwrap().attribute("ID"),
            Some("email")
        );

        let save = nodes.iter().find(|node| node.role == "button").unwrap();
        assert_eq!(save.name, "Save changes");
        assert_eq!(document.resolve_target("id=save"), Ok(save.node_id));
        assert_eq!(
            document.resolve_target("role=button[name=Save changes]"),
            Ok(save.node_id)
        );

        let checkbox = nodes.iter().find(|node| node.role == "checkbox").unwrap();
        assert_eq!(checkbox.checked, Some(true));
        assert!(
            nodes
                .iter()
                .any(|node| node.role == "link" && node.name == "Next")
        );
    }

    #[test]
    fn locators_reject_duplicates_stale_references_and_unknown_forms() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse(
            "<button>Save</button><button>Save</button><input id='name'>",
            &limits,
        )
        .unwrap();
        assert!(matches!(
            document.resolve_target("role=button[name=Save]"),
            Err(NativeEngineError::AmbiguousTarget { matches: 2 })
        ));
        let reference = document
            .semantic_nodes()
            .into_iter()
            .find(|node| node.role == "textbox")
            .unwrap()
            .reference;
        assert!(document.resolve_target(&reference).is_ok());
        document.revision = 2;
        assert!(matches!(
            document.resolve_target(&reference),
            Err(NativeEngineError::DetachedTarget)
        ));
        assert!(matches!(
            document.resolve_target("css=#name"),
            Err(NativeEngineError::InvalidConfiguration { field, .. }) if field == "action locator"
        ));
    }
}
