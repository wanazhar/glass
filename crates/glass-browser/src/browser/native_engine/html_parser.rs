//! Standards-driven HTML tokenization and document tree construction.
//!
//! html5ever owns the parsing algorithm; this temporary tree is only the
//! `TreeSink` adapter. Parsed nodes are converted into Glass's bounded arena
//! before page resources or scripts are processed.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, EndTag, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::tree_builder::TreeBuilder;
use html5ever::{Attribute, ExpandedName, QualName, TokenizerResult, local_name, ns};

const MAX_RETAINED_PARSE_ERRORS: usize = 128;

type Handle = Rc<HtmlNode>;

struct HtmlNode {
    parent: Cell<Option<Weak<HtmlNode>>>,
    children: RefCell<Vec<Handle>>,
    element_depth: Cell<usize>,
    kind: HtmlNodeKind,
}

enum HtmlNodeKind {
    Document,
    DocumentFragment,
    DocumentType {
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    },
    Element {
        name: QualName,
        attributes: RefCell<Vec<Attribute>>,
        template_contents: RefCell<Option<Handle>>,
        mathml_annotation_xml_integration_point: bool,
    },
    Comment(StrTendril),
    ProcessingInstruction {
        target: StrTendril,
        data: StrTendril,
    },
    Text(RefCell<StrTendril>),
}

impl HtmlNode {
    fn new(kind: HtmlNodeKind) -> Handle {
        Rc::new(Self {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            element_depth: Cell::new(0),
            kind,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HtmlParsedAttribute {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HtmlParsedNodeKind {
    Document,
    DocumentFragment,
    DocumentType {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element {
        name: String,
        namespace: String,
        attributes: Vec<HtmlParsedAttribute>,
    },
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HtmlParsedNode {
    pub(crate) kind: HtmlParsedNodeKind,
    pub(crate) children: Vec<usize>,
    /// Index of the parser-only DocumentFragment associated with a template.
    pub(crate) template_contents: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum HtmlParsedQuirksMode {
    #[default]
    NoQuirks,
    LimitedQuirks,
    Quirks,
}

impl HtmlParsedQuirksMode {
    pub(crate) const fn compat_mode(self) -> &'static str {
        match self {
            Self::Quirks => "BackCompat",
            Self::NoQuirks | Self::LimitedQuirks => "CSS1Compat",
        }
    }
}

#[derive(Debug)]
pub(crate) struct HtmlParsedDocument {
    pub(crate) nodes: Vec<HtmlParsedNode>,
    pub(crate) quirks_mode: HtmlParsedQuirksMode,
    pub(crate) parse_error_count: usize,
    pub(crate) sink_failure: Option<HtmlTreeSinkFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HtmlTreeSinkFailure {
    InvalidTree(&'static str),
    NodeLimitExceeded { actual: usize },
    DomDepthExceeded { actual: usize },
}

struct NativeHtmlTreeSink {
    document: Handle,
    rejected_element: Handle,
    node_count: Cell<usize>,
    max_nodes: usize,
    max_dom_depth: usize,
    quirks_mode: RefCell<QuirksMode>,
    parse_error_count: Cell<usize>,
    sink_failure: Cell<Option<HtmlTreeSinkFailure>>,
    parser_token: Cell<ParserTokenContext>,
    virtual_mtext_name: QualName,
}

#[derive(Clone, Copy, Default)]
struct ParserTokenContext {
    foreign_breakout_end_tag: bool,
    initial_node_seen: bool,
    initial_node_is_foreign: bool,
    annotation_boundary_pending: bool,
}

struct NativeHtmlTokenSink {
    tree_builder: TreeBuilder<Handle, NativeHtmlTreeSink>,
}

impl TokenSink for NativeHtmlTokenSink {
    type Handle = Handle;

    fn process_token(&self, token: Token, line_number: u64) -> TokenSinkResult<Self::Handle> {
        self.tree_builder.sink.begin_token(&token);
        let result = self.tree_builder.process_token(token, line_number);
        self.tree_builder.sink.end_token();
        result
    }

    fn end(&self) {
        self.tree_builder.end();
    }

    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.tree_builder
            .adjusted_current_node_present_but_not_in_html_namespace()
    }
}

impl NativeHtmlTreeSink {
    fn new(max_nodes: usize, max_dom_depth: usize) -> Self {
        let rejected_element = HtmlNode::new(HtmlNodeKind::Element {
            name: QualName::new(None, ns!(html), local_name!("div")),
            attributes: RefCell::new(Vec::new()),
            template_contents: RefCell::new(None),
            mathml_annotation_xml_integration_point: false,
        });
        let sink = Self {
            document: HtmlNode::new(HtmlNodeKind::Document),
            rejected_element,
            node_count: Cell::new(1),
            max_nodes,
            max_dom_depth,
            quirks_mode: RefCell::new(QuirksMode::NoQuirks),
            parse_error_count: Cell::new(0),
            sink_failure: Cell::new(None),
            parser_token: Cell::new(ParserTokenContext::default()),
            virtual_mtext_name: QualName::new(None, ns!(mathml), local_name!("mtext")),
        };
        if max_nodes == 0 {
            sink.fail(HtmlTreeSinkFailure::NodeLimitExceeded { actual: 1 });
        }
        sink
    }

    fn fail(&self, failure: HtmlTreeSinkFailure) {
        if self.sink_failure.get().is_none() {
            self.sink_failure.set(Some(failure));
        }
    }

    fn begin_token(&self, token: &Token) {
        let foreign_breakout_end_tag = matches!(
            token,
            Token::TagToken(tag)
                if tag.kind == EndTag
                    && (tag.name == local_name!("br") || tag.name == local_name!("p"))
        );
        self.parser_token.set(ParserTokenContext {
            foreign_breakout_end_tag,
            ..ParserTokenContext::default()
        });
    }

    fn end_token(&self) {
        self.parser_token.set(ParserTokenContext::default());
    }

    fn create_node(&self, kind: HtmlNodeKind) -> Handle {
        if self.sink_failure.get().is_some() {
            return self.rejected_element.clone();
        }
        let actual = self.node_count.get().saturating_add(1);
        if actual > self.max_nodes {
            self.fail(HtmlTreeSinkFailure::NodeLimitExceeded { actual });
            return self.rejected_element.clone();
        }
        self.node_count.set(actual);
        HtmlNode::new(kind)
    }

    fn set_subtree_element_depth(&self, root: &Handle, parent_depth: usize) -> bool {
        let mut pending = vec![(root.clone(), parent_depth)];
        while let Some((node, parent_depth)) = pending.pop() {
            if self.sink_failure.get().is_some() {
                return false;
            }
            let is_element = matches!(&node.kind, HtmlNodeKind::Element { .. });
            let depth = parent_depth.saturating_add(usize::from(is_element));
            if depth > self.max_dom_depth {
                self.fail(HtmlTreeSinkFailure::DomDepthExceeded { actual: depth });
                return false;
            }
            node.element_depth.set(depth);
            let children = node.children.borrow().clone();
            pending.extend(children.into_iter().rev().map(|child| (child, depth)));
            if let HtmlNodeKind::Element {
                template_contents, ..
            } = &node.kind
                && let Some(contents) = template_contents.borrow().clone()
            {
                pending.push((contents, depth));
            }
        }
        true
    }

    fn append_node(&self, parent: &Handle, child: Handle) {
        if self.sink_failure.get().is_some() {
            return;
        }
        if Rc::ptr_eq(parent, &child) || would_create_cycle(parent, &child) {
            self.fail(HtmlTreeSinkFailure::InvalidTree(
                "tree sink rejected a cyclic node move",
            ));
            return;
        }
        if !remove_from_parent(&child, &self.sink_failure) {
            return;
        }
        if !self.set_subtree_element_depth(&child, parent.element_depth.get()) {
            return;
        }
        child.parent.set(Some(Rc::downgrade(parent)));
        parent.children.borrow_mut().push(child);
    }

    fn append_node_or_text(&self, parent: &Handle, child: NodeOrText<Handle>) {
        if self.sink_failure.get().is_some() {
            return;
        }
        match child {
            NodeOrText::AppendNode(node) => self.append_node(parent, node),
            NodeOrText::AppendText(text) => {
                if let Some(previous) = parent.children.borrow().last()
                    && append_to_text(previous, &text)
                {
                    return;
                }
                self.append_node(
                    parent,
                    self.create_node(HtmlNodeKind::Text(RefCell::new(text))),
                );
            }
        }
    }

    fn parsed_node(node: &Handle) -> HtmlParsedNode {
        let kind = match &node.kind {
            HtmlNodeKind::Document => HtmlParsedNodeKind::Document,
            HtmlNodeKind::DocumentFragment => HtmlParsedNodeKind::DocumentFragment,
            HtmlNodeKind::DocumentType {
                name,
                public_id,
                system_id,
            } => HtmlParsedNodeKind::DocumentType {
                name: name.to_string(),
                public_id: public_id.to_string(),
                system_id: system_id.to_string(),
            },
            HtmlNodeKind::Element {
                name, attributes, ..
            } => HtmlParsedNodeKind::Element {
                name: name.local.to_string(),
                namespace: name.ns.to_string(),
                attributes: attributes
                    .borrow()
                    .iter()
                    .map(|attribute| {
                        let mut name = String::new();
                        if let Some(prefix) = &attribute.name.prefix
                            && !prefix.is_empty()
                        {
                            name.push_str(prefix.as_ref());
                            name.push(':');
                        }
                        name.push_str(attribute.name.local.as_ref());
                        let namespace = attribute.name.ns.to_string();
                        HtmlParsedAttribute {
                            name,
                            value: attribute.value.to_string(),
                            namespace: (!namespace.is_empty()).then_some(namespace),
                        }
                    })
                    .collect(),
            },
            HtmlNodeKind::Comment(value) => HtmlParsedNodeKind::Comment(value.to_string()),
            HtmlNodeKind::ProcessingInstruction { target, data } => {
                HtmlParsedNodeKind::ProcessingInstruction {
                    target: target.to_string(),
                    data: data.to_string(),
                }
            }
            HtmlNodeKind::Text(value) => HtmlParsedNodeKind::Text(value.borrow().to_string()),
        };
        HtmlParsedNode {
            kind,
            children: Vec::new(),
            template_contents: None,
        }
    }

    fn into_parsed_document(self) -> HtmlParsedDocument {
        let NativeHtmlTreeSink {
            document,
            quirks_mode,
            parse_error_count,
            sink_failure,
            ..
        } = self;
        let mode = match *quirks_mode.borrow() {
            QuirksMode::NoQuirks => HtmlParsedQuirksMode::NoQuirks,
            QuirksMode::LimitedQuirks => HtmlParsedQuirksMode::LimitedQuirks,
            QuirksMode::Quirks => HtmlParsedQuirksMode::Quirks,
        };
        let mut nodes = vec![Self::parsed_node(&document)];
        let mut pending = vec![(document, 0_usize)];
        while let Some((node, output_index)) = pending.pop() {
            let children = node.children.borrow().clone();
            let template_contents = match &node.kind {
                HtmlNodeKind::Element {
                    template_contents, ..
                } => template_contents.borrow().clone(),
                _ => None,
            };
            let mut child_work =
                Vec::with_capacity(children.len() + usize::from(template_contents.is_some()));
            for child in children {
                let child_index = nodes.len();
                nodes.push(Self::parsed_node(&child));
                nodes[output_index].children.push(child_index);
                child_work.push((child, child_index));
            }
            if let Some(contents) = template_contents {
                let contents_index = nodes.len();
                nodes.push(Self::parsed_node(&contents));
                nodes[output_index].template_contents = Some(contents_index);
                child_work.push((contents, contents_index));
            }
            pending.extend(child_work.into_iter().rev());
        }
        HtmlParsedDocument {
            nodes,
            quirks_mode: mode,
            parse_error_count: parse_error_count.get().min(MAX_RETAINED_PARSE_ERRORS),
            sink_failure: sink_failure.get(),
        }
    }
}

impl TreeSink for NativeHtmlTreeSink {
    type Handle = Handle;
    type Output = HtmlParsedDocument;
    type ElemName<'a>
        = ExpandedName<'a>
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        self.into_parsed_document()
    }

    fn parse_error(&self, message: Cow<'static, str>) {
        let mut context = self.parser_token.get();
        if context.foreign_breakout_end_tag
            && context.initial_node_seen
            && context.initial_node_is_foreign
            && message == "Unexpected token"
        {
            context.annotation_boundary_pending = true;
            self.parser_token.set(context);
        }
        self.parse_error_count.set(
            self.parse_error_count
                .get()
                .saturating_add(1)
                .min(MAX_RETAINED_PARSE_ERRORS),
        );
    }

    fn get_document(&self) -> Self::Handle {
        self.document.clone()
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        let mut context = self.parser_token.get();
        match &target.kind {
            HtmlNodeKind::Element {
                name,
                mathml_annotation_xml_integration_point,
                ..
            } => {
                if context.foreign_breakout_end_tag && !context.initial_node_seen {
                    context.initial_node_seen = true;
                    context.initial_node_is_foreign = name.ns != ns!(html);
                    self.parser_token.set(context);
                }
                if context.annotation_boundary_pending && *mathml_annotation_xml_integration_point {
                    // html5ever 0.40.x omits MathML annotation-xml integration points
                    // from its foreign-content breakout loop. Expose the standard boundary
                    // for this loop query only; keep the stored QName and all later parser
                    // decisions unchanged.
                    context.annotation_boundary_pending = false;
                    self.parser_token.set(context);
                    self.virtual_mtext_name.expanded()
                } else {
                    name.expanded()
                }
            }
            _ => panic!("html5ever requested the name of a non-element node"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        if self.sink_failure.get().is_some() {
            return self.rejected_element.clone();
        }
        let element = self.create_node(HtmlNodeKind::Element {
            name,
            attributes: RefCell::new(attrs),
            template_contents: RefCell::new(None),
            mathml_annotation_xml_integration_point: flags.mathml_annotation_xml_integration_point,
        });
        if flags.template
            && self.sink_failure.get().is_none()
            && let HtmlNodeKind::Element {
                template_contents, ..
            } = &element.kind
        {
            *template_contents.borrow_mut() =
                Some(self.create_node(HtmlNodeKind::DocumentFragment));
        }
        element
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        self.create_node(HtmlNodeKind::Comment(text))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        self.create_node(HtmlNodeKind::ProcessingInstruction { target, data })
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.append_node_or_text(parent, child);
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        previous_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        if element.parent.take().is_some_and(|parent| {
            element.parent.set(Some(parent));
            true
        }) {
            self.append_before_sibling(element, child);
        } else {
            self.append(previous_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        self.append_node(
            &self.document,
            self.create_node(HtmlNodeKind::DocumentType {
                name,
                public_id,
                system_id,
            }),
        );
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        if self.sink_failure.get().is_some() {
            return self.rejected_element.clone();
        }
        let HtmlNodeKind::Element {
            template_contents, ..
        } = &target.kind
        else {
            self.fail(HtmlTreeSinkFailure::InvalidTree(
                "html5ever requested template contents for a non-element",
            ));
            return self.rejected_element.clone();
        };
        if let Some(contents) = template_contents.borrow().as_ref() {
            return contents.clone();
        }
        self.fail(HtmlTreeSinkFailure::InvalidTree(
            "html5ever requested missing template contents",
        ));
        self.rejected_element.clone()
    }

    fn same_node(&self, left: &Self::Handle, right: &Self::Handle) -> bool {
        Rc::ptr_eq(left, right)
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        *self.quirks_mode.borrow_mut() = mode;
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, child: NodeOrText<Self::Handle>) {
        if self.sink_failure.get().is_some() {
            return;
        }
        let Some(_) = parent_and_index(sibling, &self.sink_failure) else {
            return;
        };
        match child {
            NodeOrText::AppendText(text) => {
                let Some((parent, index)) = parent_and_index(sibling, &self.sink_failure) else {
                    return;
                };
                if index > 0 {
                    let previous = parent.children.borrow()[index - 1].clone();
                    if append_to_text(&previous, &text) {
                        return;
                    }
                }
                let node = self.create_node(HtmlNodeKind::Text(RefCell::new(text)));
                self.insert_node_before_sibling(sibling, node);
            }
            NodeOrText::AppendNode(node) => {
                self.insert_node_before_sibling(sibling, node);
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        if self.sink_failure.get().is_some() {
            return;
        }
        let HtmlNodeKind::Element {
            attributes: existing,
            ..
        } = &target.kind
        else {
            self.fail(HtmlTreeSinkFailure::InvalidTree(
                "html5ever added attributes to a non-element",
            ));
            return;
        };
        let mut existing = existing.borrow_mut();
        for attribute in attrs {
            if !existing
                .iter()
                .any(|current| current.name == attribute.name)
            {
                existing.push(attribute);
            }
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        if self.sink_failure.get().is_some() {
            return;
        }
        remove_from_parent(target, &self.sink_failure);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        if self.sink_failure.get().is_some() {
            return;
        }
        if Rc::ptr_eq(node, new_parent) || would_create_cycle(new_parent, node) {
            self.fail(HtmlTreeSinkFailure::InvalidTree(
                "html5ever requested an invalid child reparenting",
            ));
            return;
        }
        let children = node.children.borrow().clone();
        for child in &children {
            if !self.set_subtree_element_depth(child, new_parent.element_depth.get()) {
                return;
            }
        }
        let children = std::mem::take(&mut *node.children.borrow_mut());
        for child in &children {
            child.parent.set(Some(Rc::downgrade(new_parent)));
        }
        new_parent.children.borrow_mut().extend(children);
    }

    fn is_mathml_annotation_xml_integration_point(&self, target: &Self::Handle) -> bool {
        match &target.kind {
            HtmlNodeKind::Element {
                mathml_annotation_xml_integration_point,
                ..
            } => *mathml_annotation_xml_integration_point,
            _ => {
                self.fail(HtmlTreeSinkFailure::InvalidTree(
                    "html5ever queried a non-element MathML integration point",
                ));
                false
            }
        }
    }
}

impl NativeHtmlTreeSink {
    fn insert_node_before_sibling(&self, sibling: &Handle, child: Handle) {
        if self.sink_failure.get().is_some() {
            return;
        }
        let Some((parent, _)) = parent_and_index(sibling, &self.sink_failure) else {
            return;
        };
        if Rc::ptr_eq(sibling, &child) || would_create_cycle(&parent, &child) {
            self.fail(HtmlTreeSinkFailure::InvalidTree(
                "html5ever requested an invalid sibling move",
            ));
            return;
        }
        if !remove_from_parent(&child, &self.sink_failure)
            || !self.set_subtree_element_depth(&child, parent.element_depth.get())
        {
            return;
        }
        let Some((current_parent, index)) = parent_and_index(sibling, &self.sink_failure) else {
            return;
        };
        if !Rc::ptr_eq(&parent, &current_parent) {
            self.fail(HtmlTreeSinkFailure::InvalidTree(
                "html5ever moved a sibling during insertion",
            ));
            return;
        }
        child.parent.set(Some(Rc::downgrade(&parent)));
        parent.children.borrow_mut().insert(index, child);
    }
}

fn append_to_text(node: &Handle, text: &str) -> bool {
    if let HtmlNodeKind::Text(contents) = &node.kind {
        contents.borrow_mut().push_slice(text);
        true
    } else {
        false
    }
}

fn parent_and_index(
    node: &Handle,
    failure: &Cell<Option<HtmlTreeSinkFailure>>,
) -> Option<(Handle, usize)> {
    let parent = node.parent.take()?;
    node.parent.set(Some(parent.clone()));
    let Some(parent_handle) = parent.upgrade() else {
        set_tree_failure(
            failure,
            HtmlTreeSinkFailure::InvalidTree("HTML tree contains a detached parent link"),
        );
        return None;
    };
    let Some(index) = parent_handle
        .children
        .borrow()
        .iter()
        .position(|child| Rc::ptr_eq(child, node))
    else {
        set_tree_failure(
            failure,
            HtmlTreeSinkFailure::InvalidTree("HTML tree parent link has no matching child"),
        );
        return None;
    };
    Some((parent_handle, index))
}

fn remove_from_parent(node: &Handle, failure: &Cell<Option<HtmlTreeSinkFailure>>) -> bool {
    let Some(parent) = node.parent.take() else {
        return true;
    };
    let Some(parent_handle) = parent.upgrade() else {
        set_tree_failure(
            failure,
            HtmlTreeSinkFailure::InvalidTree("HTML tree contains a detached parent link"),
        );
        return false;
    };
    let Some(index) = parent_handle
        .children
        .borrow()
        .iter()
        .position(|child| Rc::ptr_eq(child, node))
    else {
        node.parent.set(Some(parent));
        set_tree_failure(
            failure,
            HtmlTreeSinkFailure::InvalidTree("HTML tree parent link has no matching child"),
        );
        return false;
    };
    parent_handle.children.borrow_mut().remove(index);
    true
}

fn set_tree_failure(failure: &Cell<Option<HtmlTreeSinkFailure>>, next: HtmlTreeSinkFailure) {
    if failure.get().is_none() {
        failure.set(Some(next));
    }
}

fn would_create_cycle(parent: &Handle, child: &Handle) -> bool {
    let mut cursor = Some(parent.clone());
    while let Some(node) = cursor {
        if Rc::ptr_eq(&node, child) {
            return true;
        }
        cursor = node.parent.take().and_then(|parent| {
            node.parent.set(Some(parent.clone()));
            parent.upgrade()
        });
    }
    false
}

pub(crate) fn parse_document_with_limits(
    source: &str,
    max_nodes: usize,
    max_dom_depth: usize,
) -> HtmlParsedDocument {
    let tree_builder = TreeBuilder::new(
        NativeHtmlTreeSink::new(max_nodes, max_dom_depth),
        Default::default(),
    );
    let token_sink = NativeHtmlTokenSink { tree_builder };
    let tokenizer = Tokenizer::new(token_sink, TokenizerOpts::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(source));
    while !matches!(tokenizer.feed(&input), TokenizerResult::Done) {}
    debug_assert!(input.is_empty());
    tokenizer.end();
    tokenizer.sink.tree_builder.sink.finish()
}

#[cfg(test)]
fn parse_document(source: &str) -> HtmlParsedDocument {
    parse_document_with_limits(source, 8_192, 128)
}

#[cfg(test)]
mod tests {
    use super::{
        HtmlParsedNodeKind, HtmlParsedQuirksMode, HtmlTreeSinkFailure, parse_document,
        parse_document_with_limits,
    };

    fn child<'a>(nodes: &'a [super::HtmlParsedNode], parent: usize, name: &str) -> usize {
        nodes[parent]
            .children
            .iter()
            .copied()
            .find(|index| {
                matches!(&nodes[*index].kind, HtmlParsedNodeKind::Element { name: actual, .. } if actual == name)
            })
            .unwrap_or_else(|| panic!("missing {name} child"))
    }

    fn element_by_id(nodes: &[super::HtmlParsedNode], id: &str) -> usize {
        nodes
            .iter()
            .position(|node| {
                matches!(
                    &node.kind,
                    HtmlParsedNodeKind::Element { attributes, .. }
                        if attributes.iter().any(|attribute| {
                            attribute.name == "id" && attribute.value == id
                        })
                )
            })
            .unwrap_or_else(|| panic!("missing element with id {id}"))
    }

    fn parent(nodes: &[super::HtmlParsedNode], child: usize) -> usize {
        nodes
            .iter()
            .position(|node| node.children.contains(&child))
            .expect("element must have a parent")
    }

    #[test]
    fn html_parser_builds_implied_document_structure() {
        let parsed = parse_document("<!doctype html><title>sample</title><p>body");
        assert_eq!(parsed.sink_failure, None);
        assert_eq!(parsed.quirks_mode, HtmlParsedQuirksMode::NoQuirks);
        let html = child(&parsed.nodes, 0, "html");
        let head = child(&parsed.nodes, html, "head");
        let body = child(&parsed.nodes, html, "body");
        assert_eq!(
            child(&parsed.nodes, head, "title"),
            parsed.nodes[head].children[0]
        );
        assert_eq!(
            child(&parsed.nodes, body, "p"),
            parsed.nodes[body].children[0]
        );
    }

    #[test]
    fn html_parser_template_contents_remain_a_distinct_parser_fragment() {
        let parsed = parse_document("<!doctype html><template><table><tr><td>x</template>");
        let html = child(&parsed.nodes, 0, "html");
        let head = child(&parsed.nodes, html, "head");
        let template = child(&parsed.nodes, head, "template");
        let fragment = parsed.nodes[template]
            .template_contents
            .expect("template insertion target must be a fragment");
        assert!(matches!(
            parsed.nodes[fragment].kind,
            HtmlParsedNodeKind::DocumentFragment
        ));
        let table = child(&parsed.nodes, fragment, "table");
        let tbody = child(&parsed.nodes, table, "tbody");
        assert!(parsed.nodes[tbody]
            .children
            .iter()
            .any(|index| matches!(parsed.nodes[*index].kind, HtmlParsedNodeKind::Element { ref name, .. } if name == "tr")));
    }

    #[test]
    fn html_parser_preserves_foreign_namespace_declaration_attributes() {
        let parsed = parse_document(
            "<svg id='svg' xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' xlink:href='#target'></svg>",
        );
        let html = child(&parsed.nodes, 0, "html");
        let body = child(&parsed.nodes, html, "body");
        let svg = child(&parsed.nodes, body, "svg");
        let HtmlParsedNodeKind::Element { attributes, .. } = &parsed.nodes[svg].kind else {
            panic!("svg node must be an element");
        };
        assert!(
            attributes.iter().any(|attribute| {
                attribute.name == "xmlns"
                    && attribute.namespace.as_deref() == Some("http://www.w3.org/2000/xmlns/")
            }),
            "TreeSink output did not preserve the xmlns attribute: {attributes:#?}"
        );
    }

    #[test]
    fn html_parser_keeps_annotation_xml_as_foreign_breakout_boundary() {
        let parsed = parse_document(concat!(
            "<!doctype html><math>",
            "<annotation-xml id='annotation' encoding='APPLICATION/XHTML+XML'>",
            "<svg><g></p><span id='after-nested-p'></span></g></svg>",
            "<svg><g></br><span id='after-nested-br'></span></g></svg>",
            "</annotation-xml></math>",
        ));
        assert_eq!(parsed.sink_failure, None);
        let annotation = element_by_id(&parsed.nodes, "annotation");
        let after_nested_p = element_by_id(&parsed.nodes, "after-nested-p");
        let after_nested_br = element_by_id(&parsed.nodes, "after-nested-br");
        assert_eq!(parent(&parsed.nodes, after_nested_p), annotation);
        assert_eq!(parent(&parsed.nodes, after_nested_br), annotation);

        let implied_p = child(&parsed.nodes, annotation, "p");
        let implied_br = child(&parsed.nodes, annotation, "br");
        for index in [after_nested_p, after_nested_br, implied_p, implied_br] {
            let HtmlParsedNodeKind::Element { namespace, .. } = &parsed.nodes[index].kind else {
                panic!("foreign-content breakout result must be an element");
            };
            assert_eq!(namespace, "http://www.w3.org/1999/xhtml");
        }
    }

    #[test]
    fn html_parser_counts_parse_errors_without_retaining_page_text() {
        let parsed = parse_document("<!doctype html><p><b>x</p>");
        assert!(parsed.parse_error_count > 0);
        assert!(parsed.parse_error_count <= super::MAX_RETAINED_PARSE_ERRORS);
    }

    #[test]
    fn html_parser_tree_sink_rejects_temporary_node_growth_at_the_configured_cap() {
        let parsed =
            parse_document_with_limits("<html><body>a<!--one--><!--two--></body></html>", 6, 128);
        assert_eq!(
            parsed.sink_failure,
            Some(HtmlTreeSinkFailure::NodeLimitExceeded { actual: 7 })
        );
    }

    #[test]
    fn html_parser_tree_sink_rejects_depth_growth_during_construction() {
        let parsed = parse_document_with_limits("<div><span>deep</span></div>", 128, 3);
        assert_eq!(
            parsed.sink_failure,
            Some(HtmlTreeSinkFailure::DomDepthExceeded { actual: 4 })
        );
    }
}
