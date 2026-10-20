use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeSelector {
    pub(super) compounds: Vec<NativeCompoundSelector>,
    pub(super) combinators: Vec<NativeSelectorCombinator>,
    pub(super) specificity: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeSelectorNamespace {
    Any,
    NoNamespace,
    Html,
    Svg,
    MathMl,
    Xml,
    Xmlns,
    Xlink,
}

impl NativeSelectorNamespace {
    pub(super) fn matches(self, namespace_uri: Option<&str>) -> bool {
        match self {
            Self::Any => true,
            Self::NoNamespace => namespace_uri.is_none(),
            Self::Html => namespace_uri == Some(HTML_NAMESPACE_URI),
            Self::Svg => namespace_uri == Some(SVG_NAMESPACE_URI),
            Self::MathMl => namespace_uri == Some(MATHML_NAMESPACE_URI),
            Self::Xml => namespace_uri == Some(XML_NAMESPACE_URI),
            Self::Xmlns => namespace_uri == Some(XMLNS_NAMESPACE_URI),
            Self::Xlink => namespace_uri == Some(XLINK_NAMESPACE_URI),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeAttributeNamespace {
    Unqualified,
    Any,
    NoNamespace,
    Html,
    Svg,
    MathMl,
    Xml,
    Xmlns,
    Xlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeAttributeOperator {
    Exists,
    Equals,
    Includes,
    DashMatch,
    Prefix,
    Suffix,
    Substring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeAttributeCaseSensitivity {
    Default,
    Insensitive,
    Sensitive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeAttributeSelector {
    pub(super) name: String,
    pub(super) namespace: NativeAttributeNamespace,
    pub(super) operator: NativeAttributeOperator,
    pub(super) value: Option<String>,
    pub(super) case_sensitivity: NativeAttributeCaseSensitivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeSelectorCombinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeRelativeSelector {
    pub(super) combinator: NativeSelectorCombinator,
    pub(super) selector: NativeSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NativePseudoClass {
    Root,
    Scope,
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    Empty,
    Checked,
    Disabled,
    Enabled,
    Required,
    Optional,
    Link,
    AnyLink,
    Lang(String),
    Dir(DirectionValue),
    Focus,
    FocusWithin,
    FocusVisible,
    Hover,
    Valid,
    Invalid,
    UserValid,
    UserInvalid,
    ReadOnly,
    ReadWrite,
    PlaceholderShown,
    Default,
    InRange,
    OutOfRange,
    Target,
    Indeterminate,
    Not(Vec<NativeSelector>),
    Is(Vec<NativeSelector>),
    Where(Vec<NativeSelector>),
    Has(Vec<NativeRelativeSelector>),
    NthChild {
        a: i32,
        b: i32,
        of: Option<Vec<NativeSelector>>,
    },
    NthLastChild {
        a: i32,
        b: i32,
        of: Option<Vec<NativeSelector>>,
    },
    NthOfType {
        a: i32,
        b: i32,
        of: Option<Vec<NativeSelector>>,
    },
    NthLastOfType {
        a: i32,
        b: i32,
        of: Option<Vec<NativeSelector>>,
    },
}

impl NativePseudoClass {
    pub(super) fn matches_local(&self, node: &NativeNode) -> bool {
        match self {
            Self::Empty => node.children().is_empty(),
            Self::Checked => {
                node.attribute("checked").is_some()
                    || node
                        .attribute("aria-checked")
                        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
            }
            Self::Disabled => node.attribute("disabled").is_some(),
            Self::Enabled => node.attribute("disabled").is_none(),
            Self::Required => node.attribute("required").is_some(),
            Self::Optional => node.attribute("required").is_none(),
            Self::Link | Self::AnyLink => node.attribute("href").is_some(),
            Self::Not(selectors) => selectors
                .iter()
                .all(|selector| !selector.matches_local(node)),
            Self::Is(selectors) | Self::Where(selectors) => selectors
                .iter()
                .any(|selector| selector.matches_local(node)),
            Self::Lang(_)
            | Self::Dir(_)
            | Self::Focus
            | Self::FocusWithin
            | Self::FocusVisible
            | Self::Hover
            | Self::Valid
            | Self::Invalid
            | Self::UserValid
            | Self::UserInvalid
            | Self::ReadOnly
            | Self::ReadWrite
            | Self::PlaceholderShown
            | Self::Default
            | Self::InRange
            | Self::OutOfRange
            | Self::Target
            | Self::Indeterminate
            | Self::Has(_)
            | Self::NthChild { .. }
            | Self::NthLastChild { .. }
            | Self::NthOfType { .. }
            | Self::NthLastOfType { .. } => false,
            Self::Root
            | Self::Scope
            | Self::FirstChild
            | Self::LastChild
            | Self::OnlyChild
            | Self::FirstOfType
            | Self::LastOfType
            | Self::OnlyOfType => false,
        }
    }

    pub(super) fn matches_in_document(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
    ) -> bool {
        let Some(node) = document.node(node_id) else {
            return false;
        };
        match self {
            Self::Root | Self::Scope => {
                node.parent() == Some(document.root())
                    && document.node(document.root()).is_some_and(|root| {
                        root.children().iter().copied().find(|child_id| {
                            document
                                .node(*child_id)
                                .is_some_and(|child| child.element_name().is_some())
                        }) == Some(node_id)
                    })
            }
            Self::FirstChild | Self::LastChild | Self::OnlyChild => {
                let Some(parent) = node.parent().and_then(|parent_id| document.node(parent_id))
                else {
                    return false;
                };
                let mut element_children = parent.children().iter().copied().filter(|child_id| {
                    document
                        .node(*child_id)
                        .is_some_and(|child| child.element_name().is_some())
                });
                match self {
                    Self::FirstChild => element_children.next() == Some(node_id),
                    Self::LastChild => element_children.next_back() == Some(node_id),
                    Self::OnlyChild => {
                        element_children.next() == Some(node_id)
                            && element_children.next().is_none()
                    }
                    _ => false,
                }
            }
            Self::FirstOfType => nth_pseudo_matches(document, node_id, 0, 1, false, true, None),
            Self::LastOfType => nth_pseudo_matches(document, node_id, 0, 1, true, true, None),
            Self::OnlyOfType => {
                nth_pseudo_matches(document, node_id, 0, 1, false, true, None)
                    && nth_pseudo_matches(document, node_id, 0, 1, true, true, None)
            }
            Self::Not(selectors) => selectors
                .iter()
                .all(|selector| !selector.matches_in_document(document, node_id)),
            Self::Is(selectors) | Self::Where(selectors) => selectors
                .iter()
                .any(|selector| selector.matches_in_document(document, node_id)),
            Self::Lang(range) => language_pseudo_matches(document, node_id, range),
            Self::Dir(direction) => direction_pseudo_matches(document, node_id, *direction),
            Self::Has(selectors) => selectors
                .iter()
                .any(|relative| relative_selector_matches(document, node_id, relative)),
            Self::NthChild { a, b, of } => {
                nth_pseudo_matches(document, node_id, *a, *b, false, false, of.as_deref())
            }
            Self::Focus | Self::FocusVisible => document.css_is_focused(node_id),
            Self::FocusWithin => document.css_focus_within(node_id),
            Self::Hover => document.css_is_hovered(node_id),
            Self::Valid => document.css_validity(node_id).is_some_and(|valid| valid),
            Self::Invalid => document.css_validity(node_id).is_some_and(|valid| !valid),
            Self::UserValid => document
                .css_user_validity(node_id)
                .is_some_and(|valid| valid),
            Self::UserInvalid => document
                .css_user_validity(node_id)
                .is_some_and(|valid| !valid),
            Self::ReadOnly => document
                .css_read_only(node_id)
                .is_some_and(|read_only| read_only),
            Self::ReadWrite => document
                .css_read_write(node_id)
                .is_some_and(|read_write| read_write),
            Self::PlaceholderShown => document.css_placeholder_shown(node_id),
            Self::Default => document.css_default(node_id),
            Self::InRange => document
                .css_range_validity(node_id)
                .is_some_and(|in_range| in_range),
            Self::OutOfRange => document
                .css_range_validity(node_id)
                .is_some_and(|in_range| !in_range),
            Self::Target => document.css_target(node_id),
            Self::Indeterminate => document.css_indeterminate(node_id),
            Self::NthLastChild { a, b, of } => {
                nth_pseudo_matches(document, node_id, *a, *b, true, false, of.as_deref())
            }
            Self::NthOfType { a, b, of } => {
                nth_pseudo_matches(document, node_id, *a, *b, false, true, of.as_deref())
            }
            Self::NthLastOfType { a, b, of } => {
                nth_pseudo_matches(document, node_id, *a, *b, true, true, of.as_deref())
            }
            _ => self.matches_local(node),
        }
    }
}

fn language_pseudo_matches(document: &NativeDocument, node_id: NativeNodeId, range: &str) -> bool {
    let mut current = Some(node_id);
    while let Some(current_id) = current {
        let Some(node) = document.node(current_id) else {
            return false;
        };
        if let Some(language) = node
            .attribute("lang")
            .or_else(|| node.attribute_in_namespace(Some(XML_NAMESPACE_URI), "lang"))
        {
            return language_range_matches(language, range);
        }
        current = node.parent();
    }
    false
}

fn language_range_matches(value: &str, range: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if range == "*" {
        return true;
    }
    let base = range.strip_suffix("-*").unwrap_or(range);
    value.eq_ignore_ascii_case(base)
        || (value.len() > base.len()
            && value.as_bytes().starts_with(base.as_bytes())
            && value.as_bytes().get(base.len()) == Some(&b'-'))
}

fn direction_pseudo_matches(
    document: &NativeDocument,
    node_id: NativeNodeId,
    direction: DirectionValue,
) -> bool {
    let mut current = Some(node_id);
    while let Some(current_id) = current {
        let Some(node) = document.node(current_id) else {
            return false;
        };
        if let Some(value) = node.attribute("dir") {
            let value = value.trim();
            return match value {
                value if value.eq_ignore_ascii_case("ltr") => direction == DirectionValue::Ltr,
                value if value.eq_ignore_ascii_case("rtl") => direction == DirectionValue::Rtl,
                _ => false,
            };
        }
        current = node.parent();
    }
    direction == DirectionValue::Ltr
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeCompoundSelector {
    pub(super) tag: Option<String>,
    pub(super) tag_namespace: NativeSelectorNamespace,
    pub(super) id: Option<String>,
    pub(super) classes: Vec<String>,
    pub(super) attributes: Vec<NativeAttributeSelector>,
    pub(super) pseudo_classes: Vec<NativePseudoClass>,
    pub(super) specificity: u16,
}

impl NativeSelector {
    pub(super) fn matches_local(&self, node: &NativeNode) -> bool {
        self.compounds
            .last()
            .is_some_and(|compound| compound.matches_local(node))
    }

    #[cfg(test)]
    pub(super) fn matches(&self, node: &NativeNode) -> bool {
        self.matches_local(node)
    }

    pub(super) fn matched_start_in_document(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
    ) -> Option<NativeNodeId> {
        let node = document.node(node_id)?;
        node.element_name()?;
        let target = self.compounds.last()?;
        if !target.matches_in_document(document, node_id) {
            return None;
        }

        let mut current = node_id;
        for index in (0..self.compounds.len().saturating_sub(1)).rev() {
            let relation = self
                .combinators
                .get(index)
                .copied()
                .unwrap_or(NativeSelectorCombinator::Descendant);
            let compound = &self.compounds[index];
            let matched = match relation {
                NativeSelectorCombinator::Descendant => {
                    ancestor_matching_compound(document, current, compound)
                }
                NativeSelectorCombinator::Child => {
                    child_matching_compound(document, current, compound)
                }
                NativeSelectorCombinator::NextSibling => {
                    sibling_matching_compound(document, current, compound, true)
                }
                NativeSelectorCombinator::SubsequentSibling => {
                    sibling_matching_compound(document, current, compound, false)
                }
            }?;
            current = matched;
        }
        Some(current)
    }

    pub(super) fn matches_in_document(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
    ) -> bool {
        self.matched_start_in_document(document, node_id).is_some()
    }
}

fn ancestor_matching_compound(
    document: &NativeDocument,
    start: NativeNodeId,
    compound: &NativeCompoundSelector,
) -> Option<NativeNodeId> {
    let mut candidate = document.node(start).and_then(|node| node.parent());
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let candidate_id = candidate?;
        let candidate_node = document.node(candidate_id)?;
        if candidate_node.element_name().is_some()
            && compound.matches_in_document(document, candidate_id)
        {
            return Some(candidate_id);
        }
        candidate = candidate_node.parent();
    }
    None
}

fn child_matching_compound(
    document: &NativeDocument,
    start: NativeNodeId,
    compound: &NativeCompoundSelector,
) -> Option<NativeNodeId> {
    let parent_id = document.node(start).and_then(|node| node.parent())?;
    let parent = document.node(parent_id)?;
    (parent.element_name().is_some() && compound.matches_in_document(document, parent_id))
        .then_some(parent_id)
}

fn sibling_matching_compound(
    document: &NativeDocument,
    start: NativeNodeId,
    compound: &NativeCompoundSelector,
    immediate: bool,
) -> Option<NativeNodeId> {
    let parent_id = document.node(start).and_then(|node| node.parent())?;
    let parent = document.node(parent_id)?;
    let position = parent
        .children()
        .iter()
        .position(|child_id| *child_id == start)?;
    let mut previous = parent.children()[..position]
        .iter()
        .rev()
        .copied()
        .filter(|candidate_id| {
            document
                .node(*candidate_id)
                .is_some_and(|candidate| candidate.element_name().is_some())
        });
    if immediate {
        let candidate_id = previous.next()?;
        return compound
            .matches_in_document(document, candidate_id)
            .then_some(candidate_id);
    }
    previous.find(|candidate_id| compound.matches_in_document(document, *candidate_id))
}

fn nth_pseudo_matches(
    document: &NativeDocument,
    node_id: NativeNodeId,
    a: i32,
    b: i32,
    from_end: bool,
    of_type: bool,
    of_selectors: Option<&[NativeSelector]>,
) -> bool {
    let Some(node) = document.node(node_id) else {
        return false;
    };
    let Some(node_name) = node.element_name() else {
        return false;
    };
    let Some(parent) = node.parent().and_then(|parent_id| document.node(parent_id)) else {
        return false;
    };
    let mut siblings = parent.children().iter().copied().filter(|candidate_id| {
        document.node(*candidate_id).is_some_and(|candidate| {
            candidate.element_name().is_some()
                && (!of_type || candidate.element_name() == Some(node_name))
                && of_selectors.is_none_or(|selectors| {
                    selectors
                        .iter()
                        .any(|selector| selector.matches_in_document(document, *candidate_id))
                })
        })
    });
    let position = if from_end {
        siblings
            .rev()
            .position(|candidate_id| candidate_id == node_id)
    } else {
        siblings.position(|candidate_id| candidate_id == node_id)
    };
    position
        .and_then(|position| position.checked_add(1))
        .is_some_and(|position| matches_nth_index(position, a, b))
}

fn matches_nth_index(position: usize, a: i32, b: i32) -> bool {
    let Ok(position) = i64::try_from(position) else {
        return false;
    };
    let a = i64::from(a);
    let b = i64::from(b);
    if a == 0 {
        return position == b;
    }
    let delta = position - b;
    if a > 0 {
        delta >= 0 && delta % a == 0
    } else {
        delta <= 0 && delta % a == 0
    }
}

fn relative_selector_matches(
    document: &NativeDocument,
    anchor_id: NativeNodeId,
    relative: &NativeRelativeSelector,
) -> bool {
    match relative.combinator {
        NativeSelectorCombinator::Descendant => {
            descendant_matching_selector(document, anchor_id, anchor_id, &relative.selector, 0)
        }
        NativeSelectorCombinator::Child => {
            child_matching_selector(document, anchor_id, &relative.selector)
        }
        NativeSelectorCombinator::NextSibling => {
            following_sibling_matching_selector(document, anchor_id, &relative.selector, true)
        }
        NativeSelectorCombinator::SubsequentSibling => {
            following_sibling_matching_selector(document, anchor_id, &relative.selector, false)
        }
    }
}

fn descendant_matching_selector(
    document: &NativeDocument,
    anchor_id: NativeNodeId,
    scope_id: NativeNodeId,
    selector: &NativeSelector,
    depth: usize,
) -> bool {
    if depth >= MAX_NATIVE_DOM_DEPTH {
        return false;
    }
    let Some(anchor) = document.node(anchor_id) else {
        return false;
    };
    for candidate_id in anchor.children().iter().copied() {
        let is_element = document
            .node(candidate_id)
            .is_some_and(|candidate| candidate.element_name().is_some());
        if is_element
            && selector
                .matched_start_in_document(document, candidate_id)
                .is_some_and(|matched_start| {
                    is_strict_descendant(document, scope_id, matched_start)
                })
        {
            return true;
        }
        if descendant_matching_selector(
            document,
            candidate_id,
            scope_id,
            selector,
            depth.saturating_add(1),
        ) {
            return true;
        }
    }
    false
}

fn is_strict_descendant(
    document: &NativeDocument,
    ancestor_id: NativeNodeId,
    candidate_id: NativeNodeId,
) -> bool {
    let mut current = document.node(candidate_id).and_then(|node| node.parent());
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        if current == Some(ancestor_id) {
            return true;
        }
        current = current.and_then(|node_id| document.node(node_id).and_then(|node| node.parent()));
    }
    false
}

fn child_matching_selector(
    document: &NativeDocument,
    anchor_id: NativeNodeId,
    selector: &NativeSelector,
) -> bool {
    let Some(anchor) = document.node(anchor_id) else {
        return false;
    };
    anchor.children().iter().copied().any(|candidate_id| {
        document.node(candidate_id).is_some_and(|candidate| {
            candidate.element_name().is_some()
                && selector.matched_start_in_document(document, candidate_id) == Some(candidate_id)
        })
    })
}

fn following_sibling_matching_selector(
    document: &NativeDocument,
    anchor_id: NativeNodeId,
    selector: &NativeSelector,
    immediate: bool,
) -> bool {
    let Some(parent_id) = document.node(anchor_id).and_then(|node| node.parent()) else {
        return false;
    };
    let Some(parent) = document.node(parent_id) else {
        return false;
    };
    let Some(position) = parent
        .children()
        .iter()
        .position(|child_id| *child_id == anchor_id)
    else {
        return false;
    };
    let mut following = parent
        .children()
        .iter()
        .copied()
        .skip(position.saturating_add(1))
        .filter(|candidate_id| {
            document
                .node(*candidate_id)
                .is_some_and(|candidate| candidate.element_name().is_some())
        });
    if immediate {
        following.next().is_some_and(|candidate_id| {
            selector.matched_start_in_document(document, candidate_id) == Some(candidate_id)
        })
    } else {
        following.any(|candidate_id| {
            selector.matched_start_in_document(document, candidate_id) == Some(candidate_id)
        })
    }
}

fn attribute_value_equals(
    actual: &str,
    expected: &str,
    case_sensitivity: NativeAttributeCaseSensitivity,
) -> bool {
    match case_sensitivity {
        NativeAttributeCaseSensitivity::Insensitive => actual.eq_ignore_ascii_case(expected),
        NativeAttributeCaseSensitivity::Default | NativeAttributeCaseSensitivity::Sensitive => {
            actual == expected
        }
    }
}

fn attribute_value_starts_with(
    actual: &str,
    expected: &str,
    case_sensitivity: NativeAttributeCaseSensitivity,
) -> bool {
    match case_sensitivity {
        NativeAttributeCaseSensitivity::Insensitive => actual
            .get(..expected.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected)),
        NativeAttributeCaseSensitivity::Default | NativeAttributeCaseSensitivity::Sensitive => {
            actual.starts_with(expected)
        }
    }
}

fn attribute_value_ends_with(
    actual: &str,
    expected: &str,
    case_sensitivity: NativeAttributeCaseSensitivity,
) -> bool {
    match case_sensitivity {
        NativeAttributeCaseSensitivity::Insensitive => actual
            .len()
            .checked_sub(expected.len())
            .and_then(|start| actual.get(start..))
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(expected)),
        NativeAttributeCaseSensitivity::Default | NativeAttributeCaseSensitivity::Sensitive => {
            actual.ends_with(expected)
        }
    }
}

fn attribute_value_contains(
    actual: &str,
    expected: &str,
    case_sensitivity: NativeAttributeCaseSensitivity,
) -> bool {
    match case_sensitivity {
        NativeAttributeCaseSensitivity::Insensitive => actual.char_indices().any(|(index, _)| {
            attribute_value_starts_with(&actual[index..], expected, case_sensitivity)
        }),
        NativeAttributeCaseSensitivity::Default | NativeAttributeCaseSensitivity::Sensitive => {
            actual.contains(expected)
        }
    }
}

pub(super) const HTML_CASE_INSENSITIVE_ATTRIBUTE_NAMES: &[&str] = &[
    "accept",
    "accept-charset",
    "align",
    "alink",
    "axis",
    "bgcolor",
    "charset",
    "checked",
    "clear",
    "codetype",
    "color",
    "compact",
    "declare",
    "defer",
    "dir",
    "direction",
    "disabled",
    "enctype",
    "face",
    "frame",
    "hreflang",
    "http-equiv",
    "lang",
    "language",
    "link",
    "media",
    "method",
    "multiple",
    "nohref",
    "noresize",
    "noshade",
    "nowrap",
    "readonly",
    "rel",
    "rev",
    "rules",
    "scope",
    "scrolling",
    "selected",
    "shape",
    "target",
    "text",
    "type",
    "valign",
    "valuetype",
    "vlink",
];

fn attribute_selector_case_sensitivity(
    selector: &NativeAttributeSelector,
    node: &NativeNode,
    attribute_namespace_uri: Option<&str>,
) -> NativeAttributeCaseSensitivity {
    match selector.case_sensitivity {
        NativeAttributeCaseSensitivity::Default
            if node.namespace_uri() == Some(HTML_NAMESPACE_URI)
                && attribute_namespace_uri.is_none()
                && HTML_CASE_INSENSITIVE_ATTRIBUTE_NAMES.contains(&selector.name.as_str()) =>
        {
            NativeAttributeCaseSensitivity::Insensitive
        }
        case_sensitivity => case_sensitivity,
    }
}

fn attribute_value_for_selector<'a>(
    selector: &NativeAttributeSelector,
    node: &'a NativeNode,
) -> Option<(&'a str, Option<&'a str>)> {
    match selector.namespace {
        NativeAttributeNamespace::Unqualified => node.attribute_with_namespace_uri(&selector.name),
        NativeAttributeNamespace::Any => node.attribute_in_any_namespace_with_uri(&selector.name),
        NativeAttributeNamespace::NoNamespace => node
            .attribute_in_namespace(None, &selector.name)
            .map(|value| (value, None)),
        NativeAttributeNamespace::Html => node
            .attribute_in_namespace(Some(HTML_NAMESPACE_URI), &selector.name)
            .map(|value| (value, Some(HTML_NAMESPACE_URI))),
        NativeAttributeNamespace::Svg => node
            .attribute_in_namespace(Some(SVG_NAMESPACE_URI), &selector.name)
            .map(|value| (value, Some(SVG_NAMESPACE_URI))),
        NativeAttributeNamespace::MathMl => node
            .attribute_in_namespace(Some(MATHML_NAMESPACE_URI), &selector.name)
            .map(|value| (value, Some(MATHML_NAMESPACE_URI))),
        NativeAttributeNamespace::Xml => node
            .attribute_in_namespace(Some(XML_NAMESPACE_URI), &selector.name)
            .map(|value| (value, Some(XML_NAMESPACE_URI))),
        NativeAttributeNamespace::Xmlns => node
            .attribute_in_namespace(Some(XMLNS_NAMESPACE_URI), &selector.name)
            .map(|value| (value, Some(XMLNS_NAMESPACE_URI))),
        NativeAttributeNamespace::Xlink => node
            .attribute_in_namespace(Some(XLINK_NAMESPACE_URI), &selector.name)
            .map(|value| (value, Some(XLINK_NAMESPACE_URI))),
    }
}

fn attribute_selector_matches(
    selector: &NativeAttributeSelector,
    actual: Option<&str>,
    case_sensitivity: NativeAttributeCaseSensitivity,
) -> bool {
    match selector.operator {
        NativeAttributeOperator::Exists => actual.is_some(),
        NativeAttributeOperator::Equals => actual.is_some_and(|actual| {
            selector
                .value
                .as_deref()
                .is_some_and(|expected| attribute_value_equals(actual, expected, case_sensitivity))
        }),
        NativeAttributeOperator::Includes => actual.is_some_and(|actual| {
            selector.value.as_deref().is_some_and(|expected| {
                !expected.is_empty()
                    && actual
                        .split_ascii_whitespace()
                        .any(|item| attribute_value_equals(item, expected, case_sensitivity))
            })
        }),
        NativeAttributeOperator::DashMatch => actual.is_some_and(|actual| {
            selector.value.as_deref().is_some_and(|expected| {
                !expected.is_empty()
                    && (attribute_value_equals(actual, expected, case_sensitivity)
                        || (attribute_value_starts_with(actual, expected, case_sensitivity)
                            && actual.as_bytes().get(expected.len()) == Some(&b'-')))
            })
        }),
        NativeAttributeOperator::Prefix => actual.is_some_and(|actual| {
            selector.value.as_deref().is_some_and(|expected| {
                !expected.is_empty()
                    && attribute_value_starts_with(actual, expected, case_sensitivity)
            })
        }),
        NativeAttributeOperator::Suffix => actual.is_some_and(|actual| {
            selector.value.as_deref().is_some_and(|expected| {
                !expected.is_empty()
                    && attribute_value_ends_with(actual, expected, case_sensitivity)
            })
        }),
        NativeAttributeOperator::Substring => actual.is_some_and(|actual| {
            selector.value.as_deref().is_some_and(|expected| {
                !expected.is_empty() && attribute_value_contains(actual, expected, case_sensitivity)
            })
        }),
    }
}

impl NativeCompoundSelector {
    fn matches_base(&self, node: &NativeNode) -> bool {
        if !self.tag_namespace.matches(node.namespace_uri()) {
            return false;
        }
        if self
            .tag
            .as_deref()
            .is_some_and(|tag| node.element_name() != Some(tag))
        {
            return false;
        }
        if self
            .id
            .as_deref()
            .is_some_and(|id| node.attribute("id") != Some(id))
        {
            return false;
        }
        if self.classes.iter().any(|class| {
            !node
                .attribute("class")
                .is_some_and(|value| value.split_ascii_whitespace().any(|item| item == class))
        }) {
            return false;
        }
        self.attributes.iter().all(|attribute| {
            let actual = attribute_value_for_selector(attribute, node);
            let case_sensitivity = attribute_selector_case_sensitivity(
                attribute,
                node,
                actual.and_then(|(_, namespace_uri)| namespace_uri),
            );
            attribute_selector_matches(attribute, actual.map(|(value, _)| value), case_sensitivity)
        })
    }

    pub(super) fn matches_local(&self, node: &NativeNode) -> bool {
        self.matches_base(node)
            && self
                .pseudo_classes
                .iter()
                .all(|pseudo| pseudo.matches_local(node))
    }

    pub(super) fn matches_in_document(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
    ) -> bool {
        let Some(node) = document.node(node_id) else {
            return false;
        };
        self.matches_base(node)
            && self
                .pseudo_classes
                .iter()
                .all(|pseudo| pseudo.matches_in_document(document, node_id))
    }
}
