use super::config::{
    NativeEngineLimits, validate_url_text, validate_window_name, without_fragment,
};
use super::css::{
    DEFAULT_NATIVE_FONT_SIZE_ADJUST, NativeFontDisplay, NativeFontFaceRule,
    NativeFontMetricOverrides, NativeStylesheet, NativeUnicodeRange, absolutize_stylesheet_urls,
    collect_background_image_sources, font_family_hash, format_font_face_unicode_ranges,
    format_font_feature_settings, format_font_metric_override, format_font_size_adjust,
    format_font_stretch_range, format_font_variation_settings, format_font_weight_range,
    parse_font_face_unicode_range, parse_font_face_variant, parse_font_feature_settings,
    parse_font_metric_override, parse_font_size_adjust, parse_font_stretch_range,
    parse_font_variation_settings, parse_font_weight_range,
};
use super::diagnostics::{NativeDiagnostic, NativeDiagnosticSink, NativeDiagnosticSource};
use super::error::NativeEngineError;
use super::image::{
    MAX_NATIVE_IMAGE_FRAMES, MAX_NATIVE_IMAGE_TRANSFER_BYTES, MAX_NATIVE_IMAGE_TRANSFER_PIXELS,
    NativeImage, NativeImageFrame, NativeImageResource, decode_data_image,
};
use super::interaction::{
    MAX_NATIVE_FORM_BODY_BYTES, NativeEventKind, NativeFile, validate_native_edit_key,
    validate_native_key,
};
use super::javascript::NativeScriptCommand;
use super::layout::{NativeLayoutSnapshot, NativePoint};
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
use super::resource_loader::{
    MAX_NATIVE_CSP_POLICIES, MAX_NATIVE_MEDIA_BYTES, NativeMediaMetadata, NativeNavigationRequest,
    NativeRequestBody,
};
use super::{
    config::{MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_NODES, TextFragmentTerms, Viewport},
    css::{
        AlignContentValue, AlignItemsValue, AlignSelfValue, DirectionValue, FlexBasisValue,
        FlexDirectionValue, FlexWrapValue, FontStyleValue, FontWeightValue, JustifyContentValue,
        NativeBorderRadius, NativeBorderStyleValue, NativeBoxSizing, NativeColor,
        NativeComputedStyle, NativeFontFamilyList, NativeFontFeatureSettings, NativeFontKerning,
        NativeFontLanguageOverride, NativeFontOpticalSizing, NativeFontPalette,
        NativeFontVariantAlternates, NativeFontVariantCaps, NativeFontVariantEastAsian,
        NativeFontVariantLigatures, NativeFontVariantNumeric, NativeFontVariantPosition,
        NativeFontVariationSettings, NativeFontWeightRange, NativeInheritedStyle,
        NativeMarginValue, NativeOrderValue, NativePointerEventsValue, NativeTextDecorationSkipInk,
        NativeTextDecorationSkipSpaces, NativeTextDecorationStyle, OverflowValue,
        TextAlignLastValue, TextAlignValue, TextDecorationValue, TextJustifyValue,
        TextOverflowValue, TextTransformValue, VerticalAlignValue, WhiteSpaceValue, WordBreakValue,
    },
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use url::Url;

use super::font::{
    MAX_NATIVE_FONT_BYTES, MAX_NATIVE_FONT_FACES, MAX_NATIVE_FONT_TOTAL_BYTES, NativeFontBook,
    NativeFontFaceResource, NativeTextMetrics,
};

const MAX_ATTRIBUTE_BYTES: usize = 1024;
const MAX_LOCATOR_BYTES: usize = crate::browser_backend::MAX_TEXT_BYTES;
const MAX_FORM_CONTROLS: usize = 128;
const MAX_IMAGE_SRCSET_CANDIDATES: usize = 32;
const MAX_IMAGE_DENSITY_MILLI: u32 = 64_000;
const DEFAULT_IMAGE_DENSITY_MILLI: u32 = 1_000;
/// Maximum logical pixels retained for one script-backed canvas.
pub(crate) const MAX_NATIVE_CANVAS_PIXELS: usize = 1_024 * 1_024;
pub(crate) const MAX_NATIVE_CANVAS_BYTES: usize = MAX_NATIVE_CANVAS_PIXELS * 4;
pub(crate) const MAX_NATIVE_CANVAS_DIMENSION: u32 = 4_096;
pub(crate) const HTML_NAMESPACE_URI: &str = "http://www.w3.org/1999/xhtml";
pub(crate) const SVG_NAMESPACE_URI: &str = "http://www.w3.org/2000/svg";
pub(crate) const MATHML_NAMESPACE_URI: &str = "http://www.w3.org/1998/Math/MathML";
pub(crate) const XML_NAMESPACE_URI: &str = "http://www.w3.org/XML/1998/namespace";
pub(crate) const XMLNS_NAMESPACE_URI: &str = "http://www.w3.org/2000/xmlns/";
pub(crate) const XLINK_NAMESPACE_URI: &str = "http://www.w3.org/1999/xlink";
// Script-created nodes use an index range that cannot collide with the
// document arena or the reserved window event target. They are resolved to
// real arena nodes while one command batch is committed.
const SCRIPT_TEMP_NODE_BASE: u32 = u32::MAX - 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeFormEncoding {
    UrlEncoded,
    Multipart,
    TextPlain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NativeFormValue {
    Text(String),
    File(NativeFile),
    EmptyFile,
}

const SUPPORTED_ROLES: [&str; 10] = [
    "button", "link", "textbox", "checkbox", "radio", "combobox", "option", "heading", "img",
    "file",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeImageCandidateDescriptor {
    Density(u32),
    Width(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeImageCandidate {
    source: String,
    descriptor: NativeImageCandidateDescriptor,
}

/// Generational identity for one node in a native document arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
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
    namespace_uri: Option<String>,
    attribute_namespaces: BTreeMap<String, String>,
    value: Option<String>,
    files: Vec<NativeFile>,
    checked: bool,
    focused: bool,
    selected: bool,
    custom_validity: String,
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    selection_direction: Option<String>,
    inline_style_allowed: bool,
}

impl NativeElementState {
    fn initial(name: &str, attributes: &BTreeMap<String, String>) -> Self {
        Self {
            namespace_uri: Some(HTML_NAMESPACE_URI.to_owned()),
            attribute_namespaces: attributes
                .keys()
                .filter_map(|name| {
                    inferred_attribute_namespace(name)
                        .map(|namespace| (name.clone(), namespace.to_owned()))
                })
                .collect(),
            value: (name == "input").then(|| attributes.get("value").cloned().unwrap_or_default()),
            files: Vec::new(),
            checked: attributes.contains_key("checked"),
            focused: false,
            selected: attributes.contains_key("selected"),
            custom_validity: String::new(),
            selection_start: None,
            selection_end: None,
            selection_direction: None,
            inline_style_allowed: true,
        }
    }
}

/// The initial DOM node kinds owned by the native engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeNodeKind {
    Document,
    DocumentType {
        name: String,
        public_id: Option<String>,
        system_id: Option<String>,
    },
    Element {
        name: String,
        attributes: BTreeMap<String, String>,
    },
    Comment(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeDocumentWire {
    pub(crate) nodes: Vec<NativeNodeWire>,
    pub(crate) computed_styles: Vec<NativeComputedStyle>,
    #[serde(default)]
    pub(crate) font_resources: Vec<NativeFontFaceResourceWire>,
    #[serde(default)]
    pub(crate) blocked_inline_style_nodes: Vec<u32>,
    #[serde(default)]
    pub(crate) script_nodes: Vec<NativeScriptNodeIdentity>,
    #[serde(default)]
    pub(crate) started_script_nodes: Vec<u32>,
    #[serde(default)]
    pub(crate) image_resources: Vec<NativeImageResourceWire>,
    #[serde(default)]
    pub(crate) background_image_sources: Vec<NativeBackgroundImageSourceWire>,
    #[serde(default)]
    pub(crate) background_image_resources: Vec<NativeImageResourceWire>,
    #[serde(default)]
    pub(crate) image_loads: Vec<NativeImageLoadWire>,
    #[serde(default)]
    pub(crate) canvas_resources: Vec<NativeCanvasResourceWire>,
    #[serde(default)]
    pub(crate) media_resources: Vec<NativeMediaResourceWire>,
    #[serde(default)]
    pub(crate) media_loads: Vec<NativeMediaLoadWire>,
    #[serde(default)]
    pub(crate) media_errors: Vec<NativeMediaLoadWire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontFaceResourceWire {
    pub(crate) family: String,
    pub(crate) family_key: u64,
    pub(crate) weight: FontWeightValue,
    #[serde(default)]
    pub(crate) weight_range: Option<NativeFontWeightRange>,
    pub(crate) style: FontStyleValue,
    #[serde(default)]
    pub(crate) stretch: super::css::NativeFontStretchRange,
    #[serde(default)]
    pub(crate) unicode_ranges: Vec<NativeUnicodeRange>,
    #[serde(default)]
    pub(crate) variation_settings: NativeFontVariationSettings,
    #[serde(default)]
    pub(crate) feature_settings: NativeFontFeatureSettings,
    #[serde(default = "default_native_font_size_adjust")]
    pub(crate) size_adjust: u16,
    #[serde(default)]
    pub(crate) metric_overrides: NativeFontMetricOverrides,
    #[serde(default)]
    pub(crate) font_display: NativeFontDisplay,
    pub(crate) data_base64: String,
}

fn default_native_font_size_adjust() -> u16 {
    DEFAULT_NATIVE_FONT_SIZE_ADJUST
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeFontFaceScriptDescriptor {
    pub(crate) family: String,
    pub(crate) weight: String,
    pub(crate) style: String,
    pub(crate) stretch: String,
    pub(crate) unicode_range: String,
    pub(crate) feature_settings: String,
    pub(crate) variation_settings: String,
    pub(crate) size_adjust: String,
    pub(crate) ascent_override: String,
    pub(crate) descent_override: String,
    pub(crate) line_gap_override: String,
    pub(crate) display: String,
    pub(crate) status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeBackgroundImageSourceWire {
    pub(crate) source_id: u32,
    pub(crate) source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeImageLoadWire {
    pub(crate) node_index: u32,
    pub(crate) source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeCanvasResourceWire {
    pub(crate) node_index: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels_base64: String,
    #[serde(default = "default_true")]
    pub(crate) origin_clean: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScriptImageResourceSnapshot {
    pub(crate) node_index: u32,
    pub(crate) source: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels_base64: String,
    #[serde(default)]
    pub(crate) frames: Vec<NativeScriptImageFrameSnapshot>,
    #[serde(default)]
    pub(crate) loop_count: Option<u32>,
    #[serde(default)]
    pub(crate) animation_elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScriptImageFrameSnapshot {
    pub(crate) delay_ms: u32,
    pub(crate) pixels_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeImageResourceWire {
    pub(crate) node_index: u32,
    pub(crate) source: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels_base64: String,
    #[serde(default)]
    pub(crate) frames: Vec<NativeImageFrameWire>,
    #[serde(default)]
    pub(crate) loop_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeImageFrameWire {
    pub(crate) delay_ms: u32,
    pub(crate) pixels_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeMediaResourceWire {
    pub(crate) node_index: u32,
    pub(crate) source: String,
    pub(crate) content_type: String,
    pub(crate) byte_length: usize,
    #[serde(default)]
    pub(crate) duration_millis: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeMediaLoadWire {
    pub(crate) node_index: u32,
    pub(crate) source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScriptNodeIdentity {
    pub(crate) temporary_index: u32,
    pub(crate) node_index: u32,
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
    DocumentType {
        name: String,
        public_id: Option<String>,
        system_id: Option<String>,
    },
    Element {
        name: String,
        attributes: BTreeMap<String, String>,
    },
    Comment(String),
    Text(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeElementStateWire {
    #[serde(default)]
    pub(crate) namespace_uri: Option<String>,
    #[serde(default)]
    pub(crate) attribute_namespaces: BTreeMap<String, String>,
    pub(crate) value: Option<String>,
    #[serde(default)]
    pub(crate) files: Vec<NativeFile>,
    pub(crate) checked: bool,
    pub(crate) focused: bool,
    pub(crate) selected: bool,
    pub(crate) custom_validity: String,
    #[serde(default)]
    pub(crate) selection_start: Option<usize>,
    #[serde(default)]
    pub(crate) selection_end: Option<usize>,
    #[serde(default)]
    pub(crate) selection_direction: Option<String>,
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
            NativeNodeKind::Document
            | NativeNodeKind::DocumentType { .. }
            | NativeNodeKind::Comment(_)
            | NativeNodeKind::Text(_) => None,
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
            NativeNodeKind::Document
            | NativeNodeKind::DocumentType { .. }
            | NativeNodeKind::Comment(_)
            | NativeNodeKind::Text(_) => None,
        }
    }

    /// Return the element namespace URI, or `None` for non-elements and
    /// namespace-less constructed elements.
    pub fn namespace_uri(&self) -> Option<&str> {
        matches!(self.kind, NativeNodeKind::Element { .. })
            .then(|| self.state.namespace_uri.as_deref())
            .flatten()
    }

    pub(crate) fn inline_style_allowed(&self) -> bool {
        self.state.inline_style_allowed
    }
}

/// Bounded semantic projection of one supported native element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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

/// Bounded document data used to refresh the native JavaScript host view.
/// These remain snapshots rather than live DOM identities; committed document
/// state is still owned by Rust while target-local event callbacks execute in
/// the persistent JavaScript realm.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScriptDocumentSnapshot {
    #[serde(default)]
    pub(crate) revision: u64,
    pub(crate) title: String,
    pub(crate) visible_text: String,
    pub(crate) elements: Vec<NativeScriptElementSnapshot>,
    #[serde(default)]
    pub(crate) nodes: Vec<NativeScriptNodeSnapshot>,
    #[serde(default)]
    pub(crate) geometry: Vec<NativeScriptGeometrySnapshot>,
    #[serde(default)]
    pub(crate) scroll_x: u32,
    #[serde(default)]
    pub(crate) scroll_y: u32,
    #[serde(default)]
    pub(crate) scroll_width: u32,
    #[serde(default)]
    pub(crate) scroll_height: u32,
    #[serde(default)]
    pub(crate) script_nodes: Vec<NativeScriptNodeIdentity>,
    #[serde(default)]
    pub(crate) canvas_resources: Vec<NativeCanvasResourceWire>,
    #[serde(default)]
    pub(crate) image_resources: Vec<NativeScriptImageResourceSnapshot>,
}

/// Layout-backed geometry transferred to one JavaScript document realm.
/// Coordinates are viewport-relative and may be negative after root scrolling;
/// dimensions remain non-negative integer CSS pixels in the bounded native
/// layout model.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScriptGeometrySnapshot {
    pub(crate) node_index: u32,
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) content_x: i64,
    pub(crate) content_y: i64,
    pub(crate) content_width: u32,
    pub(crate) content_height: u32,
    #[serde(default)]
    pub(crate) scroll_x: u32,
    #[serde(default)]
    pub(crate) scroll_y: u32,
    #[serde(default)]
    pub(crate) scroll_width: u32,
    #[serde(default)]
    pub(crate) scroll_height: u32,
    #[serde(default)]
    pub(crate) client_width: u32,
    #[serde(default)]
    pub(crate) client_height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScriptNodeSnapshot {
    pub(crate) node_index: u32,
    pub(crate) parent_index: Option<u32>,
    pub(crate) node_type: u8,
    pub(crate) node_name: String,
    pub(crate) node_value: Option<String>,
    #[serde(default)]
    pub(crate) public_id: Option<String>,
    #[serde(default)]
    pub(crate) system_id: Option<String>,
    pub(crate) children: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScriptElementSnapshot {
    pub(crate) node_index: u32,
    pub(crate) parent_index: Option<u32>,
    pub(crate) form_owner_index: Option<u32>,
    pub(crate) tag_name: String,
    #[serde(default)]
    pub(crate) namespace_uri: Option<String>,
    #[serde(default)]
    pub(crate) attribute_namespaces: BTreeMap<String, String>,
    pub(crate) attributes: BTreeMap<String, String>,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) inner_html: String,
    pub(crate) value: Option<String>,
    #[serde(default)]
    pub(crate) files: Vec<NativeFile>,
    pub(crate) checked: bool,
    pub(crate) selected: bool,
    pub(crate) disabled: bool,
    pub(crate) hidden: bool,
    pub(crate) focused: bool,
    pub(crate) validity: NativeValiditySnapshot,
    pub(crate) validation_message: String,
    pub(crate) custom_validity: String,
    pub(crate) will_validate: bool,
    pub(crate) selection_start: Option<usize>,
    pub(crate) selection_end: Option<usize>,
    pub(crate) selection_direction: Option<String>,
    #[serde(default)]
    pub(crate) image_complete: bool,
    #[serde(default)]
    pub(crate) image_natural_width: u32,
    #[serde(default)]
    pub(crate) image_natural_height: u32,
    #[serde(default)]
    pub(crate) image_current_src: String,
    #[serde(default)]
    pub(crate) media_ready_state: u8,
    #[serde(default)]
    pub(crate) media_network_state: u8,
    #[serde(default)]
    pub(crate) media_current_src: String,
    #[serde(default)]
    pub(crate) media_duration_millis: Option<u64>,
    #[serde(default)]
    pub(crate) media_error: Option<u16>,
    #[serde(default)]
    pub(crate) scroll_x: u32,
    #[serde(default)]
    pub(crate) scroll_y: u32,
    #[serde(default = "default_inline_style_allowed")]
    pub(crate) inline_style_allowed: bool,
    /// The layout owner's computed style for this element.  Keeping this in
    /// the same snapshot as attributes and geometry lets one JavaScript turn
    /// observe a coherent style/layout revision rather than reconstructing
    /// CSS from inline attributes alone.
    #[serde(default)]
    pub(crate) computed_style: NativeComputedStyle,
}

fn default_inline_style_allowed() -> bool {
    true
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeValiditySnapshot {
    pub(crate) bad_input: bool,
    pub(crate) custom_error: bool,
    pub(crate) pattern_mismatch: bool,
    pub(crate) range_overflow: bool,
    pub(crate) range_underflow: bool,
    pub(crate) step_mismatch: bool,
    pub(crate) too_long: bool,
    pub(crate) too_short: bool,
    pub(crate) type_mismatch: bool,
    pub(crate) valid: bool,
    pub(crate) value_missing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativePageScriptTiming {
    ParserBlocking,
    Async,
    Defer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativePageScriptSource {
    Inline {
        source: String,
        timing: NativePageScriptTiming,
        node_index: u32,
        nonce: Option<String>,
        parser_inserted: bool,
    },
    External {
        href: String,
        timing: NativePageScriptTiming,
        node_index: u32,
        nonce: Option<String>,
        integrity: Option<String>,
        crossorigin: Option<String>,
        parser_inserted: bool,
    },
    ModuleInline {
        source: String,
        timing: NativePageScriptTiming,
        node_index: u32,
        nonce: Option<String>,
        parser_inserted: bool,
    },
    ModuleExternal {
        href: String,
        timing: NativePageScriptTiming,
        node_index: u32,
        nonce: Option<String>,
        integrity: Option<String>,
        crossorigin: Option<String>,
        parser_inserted: bool,
    },
}

impl NativePageScriptSource {
    fn as_dynamic(mut self) -> Self {
        match &mut self {
            Self::Inline {
                parser_inserted, ..
            }
            | Self::External {
                parser_inserted, ..
            }
            | Self::ModuleInline {
                parser_inserted, ..
            }
            | Self::ModuleExternal {
                parser_inserted, ..
            } => *parser_inserted = false,
        }
        self
    }
}

/// A parsed document owned by one engine generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDocument {
    generation: u32,
    revision: u64,
    root: NativeNodeId,
    max_nodes: usize,
    max_dom_depth: usize,
    nodes: Vec<NativeNode>,
    stylesheet: NativeStylesheet,
    font_resources: Vec<NativeFontFaceResource>,
    font_book: NativeFontBook,
    external_stylesheet_states: BTreeMap<u32, NativeExternalStylesheetState>,
    computed_styles: Option<Vec<NativeComputedStyle>>,
    diagnostics: Vec<NativeDiagnostic>,
    diagnostics_truncated: bool,
    script_node_ids: BTreeMap<u32, NativeNodeId>,
    started_script_nodes: BTreeSet<u32>,
    processed_csp_meta_nodes: BTreeSet<u32>,
    pending_csp_meta_policies: BTreeMap<u32, String>,
    image_resources: BTreeMap<u32, NativeImageResource>,
    image_loads: BTreeMap<u32, String>,
    media_resources: BTreeMap<u32, NativeMediaResource>,
    media_loads: BTreeMap<u32, String>,
    media_errors: BTreeMap<u32, String>,
    background_image_sources: BTreeMap<u32, String>,
    background_image_resources: BTreeMap<u32, NativeImageResource>,
    canvas_resources: BTreeMap<u32, NativeCanvasResource>,
    inline_style_element_reports: BTreeMap<u32, (String, Option<String>)>,
    inline_style_attribute_reports: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeExternalStylesheetState {
    href: String,
    stylesheet_url: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeMediaResource {
    source: String,
    content_type: String,
    byte_length: usize,
    duration_millis: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeCanvasResource {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
    pub(crate) origin_clean: bool,
}

fn default_true() -> bool {
    true
}

fn image_from_wire(
    resource: &NativeImageResourceWire,
    max_encoded_bytes: usize,
) -> Result<NativeImage, NativeEngineError> {
    let expected_bytes = usize::try_from(resource.width)
        .ok()
        .and_then(|width| width.checked_mul(usize::try_from(resource.height).ok()?))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| NativeEngineError::limit("content-process image pixels", 0, usize::MAX))?;
    if resource.width == 0
        || resource.height == 0
        || expected_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES
        || expected_bytes / 4 > MAX_NATIVE_IMAGE_TRANSFER_PIXELS
    {
        return Err(NativeEngineError::limit(
            "content-process image pixels",
            MAX_NATIVE_IMAGE_TRANSFER_BYTES,
            expected_bytes,
        ));
    }
    let encoded_bytes = resource
        .frames
        .iter()
        .try_fold(resource.pixels_base64.len(), |total, frame| {
            total.checked_add(frame.pixels_base64.len())
        })
        .ok_or_else(|| NativeEngineError::limit("content-process image payload", 0, usize::MAX))?;
    if encoded_bytes > max_encoded_bytes {
        return Err(NativeEngineError::limit(
            "content-process image payload",
            max_encoded_bytes,
            encoded_bytes,
        ));
    }
    if resource.frames.len() > MAX_NATIVE_IMAGE_FRAMES {
        return Err(NativeEngineError::limit(
            "content-process image frames",
            MAX_NATIVE_IMAGE_FRAMES,
            resource.frames.len(),
        ));
    }
    let pixels = base64::engine::general_purpose::STANDARD
        .decode(&resource.pixels_base64)
        .map_err(|_| NativeEngineError::Parse {
            offset: 0,
            reason: "content process returned invalid image pixels".into(),
        })?;
    if pixels.len() != expected_bytes {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "content process returned image pixels with the wrong dimensions".into(),
        });
    }
    if resource.frames.is_empty() {
        if resource.loop_count.is_some() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned loop metadata without image frames".into(),
            });
        }
        return Ok(NativeImage::new(resource.width, resource.height, pixels));
    }
    let mut frames = Vec::with_capacity(resource.frames.len());
    for frame in &resource.frames {
        if frame.delay_ms == 0 || frame.delay_ms > super::image::MAX_NATIVE_IMAGE_FRAME_DELAY_MS {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid image frame delay".into(),
            });
        }
        let frame_pixels = base64::engine::general_purpose::STANDARD
            .decode(&frame.pixels_base64)
            .map_err(|_| NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned invalid image frame pixels".into(),
            })?;
        if frame_pixels.len() != expected_bytes {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned image frame pixels with the wrong dimensions"
                    .into(),
            });
        }
        frames.push(NativeImageFrame {
            delay_ms: frame.delay_ms,
            pixels: frame_pixels,
        });
    }
    let decoded_frame_bytes = frames
        .iter()
        .try_fold(0_usize, |total, frame| {
            total.checked_add(frame.pixels.len())
        })
        .ok_or_else(|| NativeEngineError::limit("content-process image frames", 0, usize::MAX))?;
    if decoded_frame_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES {
        return Err(NativeEngineError::limit(
            "content-process image frames",
            MAX_NATIVE_IMAGE_TRANSFER_BYTES,
            decoded_frame_bytes,
        ));
    }
    if frames.first().is_none_or(|frame| frame.pixels != pixels) {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: "content process returned a current image frame mismatch".into(),
        });
    }
    let image =
        NativeImage::with_frames(resource.width, resource.height, frames, resource.loop_count)
            .ok_or_else(|| NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned invalid image animation metadata".into(),
            })?;
    if image
        .decoded_bytes()
        .is_none_or(|decoded_bytes| decoded_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES)
    {
        return Err(NativeEngineError::limit(
            "content-process image frames",
            MAX_NATIVE_IMAGE_TRANSFER_BYTES,
            image.decoded_bytes().unwrap_or(usize::MAX),
        ));
    }
    Ok(image)
}

impl NativeDocument {
    /// Parse one bounded HTML document using the initial Glass-owned tree
    /// builder. This parser is intentionally not an HTML5 conformance claim.
    pub fn parse(source: &str, limits: &NativeEngineLimits) -> Result<Self, NativeEngineError> {
        Self::parse_with_stylesheets(source, limits, &[], 1)
    }

    pub(crate) fn parse_with_generation(
        source: &str,
        limits: &NativeEngineLimits,
        generation: u32,
    ) -> Result<Self, NativeEngineError> {
        Self::parse_with_stylesheets(source, limits, &[], generation)
    }

    pub(crate) fn parse_with_stylesheets(
        source: &str,
        limits: &NativeEngineLimits,
        external_stylesheets: &[String],
        generation: u32,
    ) -> Result<Self, NativeEngineError> {
        Self::parse_with_stylesheets_and_inline_style_policy(
            source,
            limits,
            external_stylesheets,
            generation,
            None,
        )
    }

    pub(crate) fn parse_with_stylesheets_and_inline_style_policy(
        source: &str,
        limits: &NativeEngineLimits,
        external_stylesheets: &[String],
        generation: u32,
        allowed_inline_style_nodes: Option<&BTreeSet<u32>>,
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
            max_nodes: limits.max_nodes,
            max_dom_depth: limits.max_dom_depth,
            nodes: vec![NativeNode {
                id: root,
                parent: None,
                children: Vec::new(),
                kind: NativeNodeKind::Document,
                state: NativeElementState::default(),
            }],
            stylesheet: NativeStylesheet::default(),
            font_resources: Vec::new(),
            font_book: NativeFontBook::system(),
            external_stylesheet_states: BTreeMap::new(),
            computed_styles: None,
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            script_node_ids: BTreeMap::new(),
            started_script_nodes: BTreeSet::new(),
            processed_csp_meta_nodes: BTreeSet::new(),
            pending_csp_meta_policies: BTreeMap::new(),
            image_resources: BTreeMap::new(),
            image_loads: BTreeMap::new(),
            media_resources: BTreeMap::new(),
            media_loads: BTreeMap::new(),
            media_errors: BTreeMap::new(),
            background_image_sources: BTreeMap::new(),
            background_image_resources: BTreeMap::new(),
            canvas_resources: BTreeMap::new(),
            inline_style_element_reports: BTreeMap::new(),
            inline_style_attribute_reports: BTreeMap::new(),
        };
        let mut stack = vec![root];
        let mut document_type_seen = false;

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
                                    NativeNodeKind::Document
                                    | NativeNodeKind::DocumentType { .. }
                                    | NativeNodeKind::Comment(_)
                                    | NativeNodeKind::Text(_) => None,
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
                                NativeNodeKind::Document
                                | NativeNodeKind::DocumentType { .. }
                                | NativeNodeKind::Comment(_)
                                | NativeNodeKind::Text(_) => None,
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
                HtmlToken::Comment(value) => {
                    let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "tree builder lost its document root".into(),
                    })?;
                    document.add_node(parent, NativeNodeKind::Comment(value), limits.max_nodes)?;
                }
                HtmlToken::Doctype {
                    name,
                    public_id,
                    system_id,
                } => {
                    // A document has at most one doctype, and HTML ignores a
                    // doctype token once the document element has started.
                    // Keeping that rule in the Rust owner prevents malformed
                    // input from creating ambiguous document.doctype state.
                    let document_element_started = document.node(root).is_some_and(|node| {
                        node.children().iter().any(|child| {
                            document.node(*child).is_some_and(|candidate| {
                                matches!(candidate.kind(), NativeNodeKind::Element { .. })
                            })
                        })
                    });
                    if !document_type_seen && !document_element_started {
                        document.add_node(
                            root,
                            NativeNodeKind::DocumentType {
                                name,
                                public_id,
                                system_id,
                            },
                            limits.max_nodes,
                        )?;
                        document_type_seen = true;
                    }
                }
            }
        }
        document.assign_parsed_namespaces();
        if let Some(allowed_inline_style_nodes) = allowed_inline_style_nodes {
            document.set_inline_style_policy(allowed_inline_style_nodes);
        }
        let mut style_sources = document
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("style") && node.inline_style_allowed())
            .map(|node| {
                let mut source = String::new();
                document.collect_raw_text(node.id(), &mut source);
                source
            })
            .collect::<Vec<_>>();
        style_sources.extend(external_stylesheets.iter().cloned());
        let mut diagnostics = NativeDiagnosticSink::default();
        document.stylesheet =
            NativeStylesheet::from_sources_with_diagnostics(style_sources, &mut diagnostics)?;
        document.background_image_sources = document.stylesheet.background_image_sources().clone();
        for node in &document.nodes {
            if !node.inline_style_allowed() {
                continue;
            }
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            collect_background_image_sources(inline_style, &mut document.background_image_sources);
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

    pub(crate) fn inline_style_elements(&self) -> Vec<(u32, String, Option<String>)> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()) && node.element_name() == Some("style"))
            .map(|node| {
                let mut source = String::new();
                self.collect_raw_text(node.id(), &mut source);
                (
                    node.id().index(),
                    source,
                    node.attribute("nonce").map(str::to_owned),
                )
            })
            .collect()
    }

    pub(crate) fn inline_style_attributes(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()) && node.attribute("style").is_some())
            .filter_map(|node| {
                node.attribute("style")
                    .map(|source| (node.id().index(), source.to_owned()))
            })
            .collect()
    }

    /// Mark the currently attached inline styles as already evaluated by the
    /// document loader. The initial parser probes styles once to determine
    /// enforced policy; the committed document must not report those same
    /// declarations again during its first refresh.
    pub(crate) fn mark_inline_style_reports_seen(&mut self) {
        self.inline_style_element_reports = self
            .inline_style_elements()
            .into_iter()
            .map(|(node_index, source, nonce)| (node_index, (source, nonce)))
            .collect();
        self.inline_style_attribute_reports = self.inline_style_attributes().into_iter().collect();
    }

    pub(crate) fn inline_style_element_reported(
        &self,
        node_index: u32,
        source: &str,
        nonce: Option<&str>,
    ) -> bool {
        self.inline_style_element_reports
            .get(&node_index)
            .is_some_and(|(reported_source, reported_nonce)| {
                reported_source == source && reported_nonce.as_deref() == nonce
            })
    }

    pub(crate) fn mark_inline_style_element_reported(
        &mut self,
        node_index: u32,
        source: String,
        nonce: Option<String>,
    ) {
        self.inline_style_element_reports
            .insert(node_index, (source, nonce));
    }

    pub(crate) fn inline_style_attribute_reported(&self, node_index: u32, source: &str) -> bool {
        self.inline_style_attribute_reports
            .get(&node_index)
            .is_some_and(|reported_source| reported_source == source)
    }

    pub(crate) fn mark_inline_style_attribute_reported(&mut self, node_index: u32, source: String) {
        self.inline_style_attribute_reports
            .insert(node_index, source);
    }

    pub(crate) fn prune_inline_style_reports(
        &mut self,
        element_nodes: &BTreeSet<u32>,
        attribute_nodes: &BTreeSet<u32>,
    ) {
        self.inline_style_element_reports
            .retain(|node_index, _| element_nodes.contains(node_index));
        self.inline_style_attribute_reports
            .retain(|node_index, _| attribute_nodes.contains(node_index));
    }

    /// Return parser-time enforced CSP policies declared by `meta` elements
    /// in the document head. Report-only values are intentionally excluded;
    /// the loader owns enforcement and reporting is a separate contract.
    /// Returning one extra value lets the loader reject an over-limit document
    /// instead of silently weakening its policy surface.
    pub(crate) fn content_security_policy_meta(&self) -> Vec<String> {
        self.content_security_policy_meta_entries()
            .into_iter()
            .take(MAX_NATIVE_CSP_POLICIES.saturating_add(1))
            .map(|(_, value)| value)
            .collect()
    }

    fn content_security_policy_meta_entries(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter(|node| {
                self.is_attached(node.id())
                    && node.element_name() == Some("meta")
                    && self.is_descendant_of_element(node.id(), "head")
                    && node.attribute("http-equiv").is_some_and(|value| {
                        value.trim().eq_ignore_ascii_case("content-security-policy")
                    })
            })
            .filter_map(|node| {
                node.attribute("content")
                    .filter(|value| !value.is_empty())
                    .map(|value| (node.id().index(), value.to_owned()))
            })
            .collect()
    }

    /// Mark the parser-discovered CSP meta elements as processed by the
    /// document policy container. Meta policies are additive and remain
    /// active after their element is removed; this ledger also makes later
    /// content attribute edits inert as required by CSP.
    pub(crate) fn mark_content_security_policy_meta_processed(&mut self) {
        self.processed_csp_meta_nodes.extend(
            self.content_security_policy_meta_entries()
                .into_iter()
                .map(|(node_index, _)| node_index),
        );
    }

    /// Return head meta policies that have not yet been handed to the policy
    /// container. Policies captured at insertion remain pending even if the
    /// node is later detached or edited. The caller commits the node indexes
    /// only after the loader accepts the append, preserving atomic failure.
    pub(crate) fn unprocessed_content_security_policy_meta(&self) -> Vec<(u32, String)> {
        let mut entries = self
            .pending_csp_meta_policies
            .iter()
            .map(|(node_index, value)| (*node_index, value.clone()))
            .collect::<Vec<_>>();
        entries.extend(
            self.content_security_policy_meta_entries()
                .into_iter()
                .filter(|(node_index, _)| {
                    !self.processed_csp_meta_nodes.contains(node_index)
                        && !self.pending_csp_meta_policies.contains_key(node_index)
                }),
        );
        entries.sort_by_key(|(node_index, _)| *node_index);
        entries
            .into_iter()
            .take(MAX_NATIVE_CSP_POLICIES.saturating_add(1))
            .collect()
    }

    pub(crate) fn mark_content_security_policy_meta_nodes_processed(
        &mut self,
        node_indexes: impl IntoIterator<Item = u32>,
    ) {
        for node_index in node_indexes {
            self.processed_csp_meta_nodes.insert(node_index);
            self.pending_csp_meta_policies.remove(&node_index);
        }
    }

    fn capture_attached_content_security_policy_meta(&mut self) {
        for (node_index, policy) in self.content_security_policy_meta_entries() {
            if !self.processed_csp_meta_nodes.contains(&node_index) {
                self.pending_csp_meta_policies
                    .entry(node_index)
                    .or_insert(policy);
            }
        }
    }

    pub(crate) fn set_inline_style_policy(&mut self, allowed_nodes: &BTreeSet<u32>) {
        for node in &mut self.nodes {
            if node.element_name() == Some("style") || node.attribute("style").is_some() {
                node.state.inline_style_allowed = allowed_nodes.contains(&node.id().index());
            }
        }
        self.computed_styles = None;
    }

    pub(crate) fn set_external_stylesheet_states(
        &mut self,
        states: impl IntoIterator<Item = (u32, String, Option<String>, Option<String>)>,
    ) {
        self.external_stylesheet_states = states
            .into_iter()
            .map(|(node_index, href, stylesheet_url, body)| {
                (
                    node_index,
                    NativeExternalStylesheetState {
                        href,
                        stylesheet_url,
                        body,
                    },
                )
            })
            .collect();
    }

    pub(crate) fn font_face_rules(&self) -> &[NativeFontFaceRule] {
        self.stylesheet.font_face_rules()
    }

    pub(crate) fn font_face_script_descriptors(&self) -> Vec<NativeFontFaceScriptDescriptor> {
        self.stylesheet
            .font_face_rules()
            .iter()
            .map(|rule| NativeFontFaceScriptDescriptor {
                family: rule.family.clone(),
                weight: format_font_weight_range(rule.weight),
                style: match rule.style {
                    FontStyleValue::Normal => "normal".into(),
                    FontStyleValue::Italic => "italic".into(),
                },
                stretch: format_font_stretch_range(rule.stretch),
                unicode_range: format_font_face_unicode_ranges(&rule.unicode_ranges),
                feature_settings: format_font_feature_settings(rule.feature_settings),
                variation_settings: format_font_variation_settings(rule.variation_settings),
                size_adjust: format_font_size_adjust(rule.size_adjust),
                ascent_override: format_font_metric_override(rule.metric_overrides.ascent),
                descent_override: format_font_metric_override(rule.metric_overrides.descent),
                line_gap_override: format_font_metric_override(rule.metric_overrides.line_gap),
                display: rule.font_display.as_str().into(),
                status: if self.font_resources.iter().any(|resource| {
                    resource.family_key == rule.family_key
                        && resource.weight == rule.weight
                        && resource.style == rule.style
                        && resource.variation_settings == rule.variation_settings
                        && resource.feature_settings == rule.feature_settings
                        && resource.size_adjust == rule.size_adjust
                        && resource.font_display == rule.font_display
                        && resource.metric_overrides == rule.metric_overrides
                }) {
                    "loaded".into()
                } else {
                    "error".into()
                },
            })
            .collect()
    }

    pub(crate) fn set_font_resources(
        &mut self,
        resources: Vec<NativeFontFaceResource>,
    ) -> Result<(), NativeEngineError> {
        if resources.len() > MAX_NATIVE_FONT_FACES {
            return Err(NativeEngineError::limit(
                "native font faces",
                MAX_NATIVE_FONT_FACES,
                resources.len(),
            ));
        }
        let total_bytes = resources.iter().try_fold(0usize, |total, resource| {
            if resource.family.is_empty() || resource.family.len() > MAX_ATTRIBUTE_BYTES {
                return None;
            }
            if resource.weight.min < 1
                || resource.weight.min > resource.weight.max
                || resource.weight.max > 1000
            {
                return None;
            }
            if resource.stretch.min < 500
                || resource.stretch.min > resource.stretch.max
                || resource.stretch.max > 2000
            {
                return None;
            }
            if resource.bytes.is_empty() || resource.bytes.len() > MAX_NATIVE_FONT_BYTES {
                return None;
            }
            if !resource.variation_settings.is_valid() {
                return None;
            }
            total.checked_add(resource.bytes.len())
        });
        let Some(total_bytes) = total_bytes else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "native font resource metadata is invalid".into(),
            });
        };
        if total_bytes > MAX_NATIVE_FONT_TOTAL_BYTES {
            return Err(NativeEngineError::limit(
                "native font resource bytes",
                MAX_NATIVE_FONT_TOTAL_BYTES,
                total_bytes,
            ));
        }
        self.font_book = NativeFontBook::from_resources(&resources);
        self.font_resources = resources;
        Ok(())
    }

    /// Install one script-created `FontFace` after its source bytes have
    /// already passed the page/network loader. Keeping the final admission in
    /// the document owner makes local and process-backed documents share the
    /// same bounds, descriptor normalization, and font-book rebuild.
    fn apply_script_font_face_install(
        &mut self,
        request_id: u32,
        family: &str,
        weight: &str,
        style: &str,
        stretch: &str,
        unicode_range: &str,
        variant: &str,
        feature_settings: &str,
        size_adjust: &str,
        ascent_override: &str,
        descent_override: &str,
        line_gap_override: &str,
        display: &str,
        variation_settings: &str,
        body_base64: &str,
    ) -> Result<(), NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native FontFace request id",
                "must be positive",
            ));
        }
        let family = family.trim();
        if family.is_empty()
            || family.len() > MAX_ATTRIBUTE_BYTES
            || family.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NativeEngineError::invalid(
                "native FontFace family",
                "must be a bounded non-empty name",
            ));
        }
        let weight = parse_font_weight_range(weight).ok_or_else(|| {
            NativeEngineError::invalid(
                "native FontFace weight",
                "must be one absolute weight or an ascending 1-1000 range",
            )
        })?;
        let style = match style.trim().to_ascii_lowercase().as_str() {
            "normal" => FontStyleValue::Normal,
            "italic" => FontStyleValue::Italic,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native FontFace style",
                    "must be normal or italic",
                ));
            }
        };
        let stretch = if stretch.trim().is_empty() {
            super::css::NativeFontStretchRange::default()
        } else {
            parse_font_stretch_range(stretch).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace stretch",
                    "must be a bounded CSS font-stretch value",
                )
            })?
        };
        let unicode_ranges = if unicode_range.trim().is_empty() {
            Vec::new()
        } else {
            parse_font_face_unicode_range(unicode_range).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace unicodeRange",
                    "must be a bounded CSS unicode range list",
                )
            })?
        };
        let variant_settings = if variant.trim().is_empty() {
            NativeFontFeatureSettings::default()
        } else {
            parse_font_face_variant(variant).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace variant",
                    "must be a bounded CSS font-variant value",
                )
            })?
        };
        let variation_settings = if variation_settings.trim().is_empty() {
            NativeFontVariationSettings::default()
        } else {
            parse_font_variation_settings(variation_settings).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace variationSettings",
                    "must be normal or a bounded CSS font-variation-settings value",
                )
            })?
        };
        let feature_settings = if feature_settings.trim().is_empty() {
            NativeFontFeatureSettings::default()
        } else {
            parse_font_feature_settings(feature_settings).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace featureSettings",
                    "must be normal or a bounded CSS font-feature-settings value",
                )
            })?
        };
        let feature_settings = variant_settings.with_overrides(feature_settings);
        let size_adjust = if size_adjust.trim().is_empty() {
            DEFAULT_NATIVE_FONT_SIZE_ADJUST
        } else {
            parse_font_size_adjust(size_adjust).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace sizeAdjust",
                    "must be a bounded CSS percentage from 25% through 400%",
                )
            })?
        };
        let parse_metric_override = |value: &str, field: &str| {
            if value.trim().is_empty() {
                Ok(None)
            } else {
                parse_font_metric_override(value).ok_or_else(|| {
                    NativeEngineError::invalid(
                        field,
                        "must be normal or a bounded CSS percentage from 0% through 1000%",
                    )
                })
            }
        };
        let metric_overrides = NativeFontMetricOverrides {
            ascent: parse_metric_override(ascent_override, "native FontFace ascentOverride")?,
            descent: parse_metric_override(descent_override, "native FontFace descentOverride")?,
            line_gap: parse_metric_override(line_gap_override, "native FontFace lineGapOverride")?,
        };
        let font_display = if display.trim().is_empty() {
            NativeFontDisplay::Auto
        } else {
            NativeFontDisplay::parse(display).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native FontFace display",
                    "must be one of auto, block, swap, fallback, or optional",
                )
            })?
        };
        let max_encoded_bytes = (MAX_NATIVE_FONT_BYTES.saturating_add(2) / 3).saturating_mul(4);
        if body_base64.len() > max_encoded_bytes {
            return Err(NativeEngineError::limit(
                "native FontFace bytes",
                max_encoded_bytes,
                body_base64.len(),
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body_base64)
            .map_err(|_| {
                NativeEngineError::invalid("native FontFace bytes", "must be valid base64")
            })?;
        if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
            return Err(NativeEngineError::limit(
                "native FontFace bytes",
                MAX_NATIVE_FONT_BYTES,
                bytes.len(),
            ));
        }
        if !NativeFontBook::is_parseable_font_bytes(&bytes) {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "native FontFace bytes are not a supported font format".into(),
            });
        }
        let resource = NativeFontFaceResource {
            family: family.to_owned(),
            family_key: font_family_hash(family),
            weight,
            style,
            stretch,
            size_adjust,
            metric_overrides,
            font_display,
            bytes: Arc::from(bytes),
            unicode_ranges,
            variation_settings,
            feature_settings,
        };
        let mut resources = self.font_resources.clone();
        resources.push(resource);
        self.set_font_resources(resources)
    }

    pub(crate) fn apply_script_font_face_installs(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<(), NativeEngineError> {
        for command in commands {
            let NativeScriptCommand::FontFaceInstall {
                request_id,
                family,
                weight,
                style,
                stretch,
                unicode_range,
                variant,
                feature_settings,
                size_adjust,
                ascent_override,
                descent_override,
                line_gap_override,
                display,
                variation_settings,
                body_base64,
            } = command
            else {
                continue;
            };
            self.apply_script_font_face_install(
                *request_id,
                family,
                weight,
                style,
                stretch,
                unicode_range,
                variant,
                feature_settings,
                size_adjust,
                ascent_override,
                descent_override,
                line_gap_override,
                display,
                variation_settings,
                body_base64,
            )?;
        }
        Ok(())
    }

    pub(crate) fn external_stylesheet_states(
        &self,
    ) -> Vec<(u32, String, Option<String>, Option<String>)> {
        self.external_stylesheet_states
            .iter()
            .map(|(node_index, state)| {
                (
                    *node_index,
                    state.href.clone(),
                    state.stylesheet_url.clone(),
                    state.body.clone(),
                )
            })
            .collect()
    }

    pub(crate) fn rebuild_external_stylesheet(
        &mut self,
        document_url: &str,
    ) -> Result<(), NativeEngineError> {
        let external_sources = self
            .external_stylesheet_states
            .values()
            .filter_map(|state| {
                state.body.as_ref().map(|body| {
                    let stylesheet_url = state.stylesheet_url.as_deref().unwrap_or(&state.href);
                    absolutize_stylesheet_urls(body, document_url, stylesheet_url)
                })
            })
            .collect::<Vec<_>>();
        let mut style_sources = self
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("style") && node.inline_style_allowed())
            .map(|node| {
                let mut source = String::new();
                self.collect_raw_text(node.id(), &mut source);
                source
            })
            .collect::<Vec<_>>();
        style_sources.extend(external_sources);
        let mut diagnostics = NativeDiagnosticSink::default();
        let stylesheet =
            NativeStylesheet::from_sources_with_diagnostics(style_sources, &mut diagnostics)?;
        let mut background_image_sources = stylesheet.background_image_sources().clone();
        for node in &self.nodes {
            if !node.inline_style_allowed() {
                continue;
            }
            let Some(inline_style) = node.attribute("style") else {
                continue;
            };
            collect_background_image_sources(inline_style, &mut background_image_sources);
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
        self.stylesheet = stylesheet;
        self.font_resources.clear();
        self.font_book = NativeFontBook::system();
        self.background_image_sources = background_image_sources;
        self.diagnostics = diagnostics;
        self.diagnostics_truncated = diagnostics_truncated;
        self.computed_styles = None;
        Ok(())
    }

    pub(crate) fn external_stylesheet_links(
        &self,
    ) -> Vec<(u32, String, Option<String>, Option<String>)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                (node.element_name() == Some("link")
                    && node.attribute("rel").is_some_and(|rel| {
                        rel.split_ascii_whitespace()
                            .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                    }))
                .then(|| {
                    node.attribute("href").map(|href| {
                        (
                            node.id().index(),
                            href.to_owned(),
                            node.attribute("integrity").map(str::to_owned),
                            node.attribute("crossorigin").map(str::to_owned),
                        )
                    })
                })
                .flatten()
            })
            .collect()
    }

    pub(crate) fn external_image_links(&self, viewport: Viewport) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                let source = self.selected_image_source(node.id(), viewport)?;
                if source.is_empty()
                    || source
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
                {
                    return None;
                }
                (node.element_name() == Some("img")).then_some((node.id().index(), source))
            })
            .collect()
    }

    /// Return media elements whose selected source needs native resource
    /// admission. Source children are considered in document order after the
    /// element's own src attribute, matching the bounded HTML media source
    /// selection model.
    pub(crate) fn external_media_links(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                let source = self.selected_media_source(node.id())?;
                (node.element_name() == Some("audio") || node.element_name() == Some("video"))
                    .then_some((node.id().index(), source))
            })
            .filter(|(_, source)| !source.is_empty())
            .collect()
    }

    fn selected_media_source(&self, node_id: NativeNodeId) -> Option<String> {
        let node = self.node(node_id)?;
        if node.element_name() != Some("audio") && node.element_name() != Some("video") {
            return None;
        }
        if let Some(source) = node.attribute("src").filter(|source| !source.is_empty()) {
            return Some(source.to_owned());
        }
        for child_id in node.children() {
            let child = self.node(*child_id)?;
            if child.element_name() != Some("source")
                || !media_type_is_supported(child.attribute("type"))
            {
                continue;
            }
            if let Some(source) = child.attribute("src").filter(|source| !source.is_empty()) {
                return Some(source.to_owned());
            }
        }
        None
    }

    pub(crate) fn media_current_src(&self, node_id: NativeNodeId) -> String {
        self.media_loads
            .get(&node_id.index())
            .cloned()
            .or_else(|| self.media_errors.get(&node_id.index()).cloned())
            .or_else(|| self.selected_media_source(node_id))
            .unwrap_or_default()
    }

    /// Return the bounded HTMLMediaElement resource state:
    /// (ready_state, network_state, duration_millis, current_src, error).
    pub(crate) fn media_properties(
        &self,
        node_id: NativeNodeId,
    ) -> Option<(u8, u8, Option<u64>, String, Option<u16>)> {
        let node = self.node(node_id)?;
        if node.element_name() != Some("audio") && node.element_name() != Some("video") {
            return None;
        }
        let source = self.media_current_src(node_id);
        if source.is_empty() {
            return Some((0, 0, None, String::new(), None));
        }
        if let Some(resource) = self.media_resources.get(&node_id.index())
            && resource.source == source
        {
            return Some((1, 1, resource.duration_millis, source, None));
        }
        if self
            .media_errors
            .get(&node_id.index())
            .is_some_and(|loaded_source| loaded_source == &source)
        {
            return Some((0, 3, None, source, Some(4)));
        }
        Some((0, 2, None, source, None))
    }

    pub(crate) fn mark_media_load(
        &mut self,
        node_index: u32,
        source: String,
    ) -> Result<(), NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
            || self.selected_media_source(node_id).as_deref() != Some(source.as_str())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media load state does not match its media element".into(),
            });
        }
        self.media_resources.remove(&node_index);
        self.media_errors.remove(&node_index);
        self.media_loads.insert(node_index, source);
        Ok(())
    }

    pub(crate) fn refresh_media_loads(&mut self) {
        let retained_loads = self
            .media_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                let node = self.node(node_id)?;
                ((node.element_name() == Some("audio") || node.element_name() == Some("video"))
                    && self.selected_media_source(node_id).as_deref() == Some(source.as_str()))
                .then(|| (*node_index, source.clone()))
            })
            .collect();
        self.media_loads = retained_loads;
        let retained_errors = self
            .media_errors
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                let node = self.node(node_id)?;
                ((node.element_name() == Some("audio") || node.element_name() == Some("video"))
                    && self.selected_media_source(node_id).as_deref() == Some(source.as_str()))
                .then(|| (*node_index, source.clone()))
            })
            .collect();
        self.media_errors = retained_errors;
        self.media_resources.retain(|node_index, resource| {
            self.media_loads
                .get(node_index)
                .is_some_and(|source| source == &resource.source)
        });
    }

    pub(crate) fn has_media_resource_for_node(&self, node_id: NativeNodeId) -> bool {
        self.media_resources
            .get(&node_id.index())
            .is_some_and(|resource| {
                self.media_loads.get(&node_id.index()) == Some(&resource.source)
            })
    }

    pub(crate) fn set_media_resource(
        &mut self,
        node_index: u32,
        source: String,
        metadata: NativeMediaMetadata,
    ) -> Result<(), NativeEngineError> {
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
            || metadata.content_type.len() > MAX_ATTRIBUTE_BYTES
            || !media_type_is_supported(Some(&metadata.content_type))
            || metadata.byte_length == 0
            || metadata.byte_length > MAX_NATIVE_MEDIA_BYTES
        {
            return Err(NativeEngineError::invalid(
                "native media resource",
                "contains an invalid bounded media resource",
            ));
        }
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
            || self.media_loads.get(&node_index) != Some(&source)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media resource does not match its media element".into(),
            });
        }
        self.media_errors.remove(&node_index);
        self.media_resources.insert(
            node_index,
            NativeMediaResource {
                source,
                content_type: metadata.content_type,
                byte_length: metadata.byte_length,
                duration_millis: metadata.duration_millis,
            },
        );
        Ok(())
    }

    pub(crate) fn set_media_error(
        &mut self,
        node_index: u32,
        source: String,
    ) -> Result<(), NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
            || self.media_loads.get(&node_index) != Some(&source)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media error does not match its media element".into(),
            });
        }
        self.media_resources.remove(&node_index);
        self.media_errors.insert(node_index, source);
        Ok(())
    }

    fn selected_image_source(&self, node_id: NativeNodeId, viewport: Viewport) -> Option<String> {
        let node = self.node(node_id)?;
        if node.element_name() != Some("img") {
            return None;
        }
        let fallback = node.attribute("src").map(str::to_owned);
        if let Some((srcset, sizes)) = self.picture_image_source_set(node_id, viewport)
            && let Some(source) = select_image_srcset_source(&srcset, sizes.as_deref(), viewport)
        {
            return Some(source);
        }
        if let Some(srcset) = node.attribute("srcset")
            && let Some(source) =
                select_image_srcset_source(srcset, node.attribute("sizes"), viewport)
        {
            return Some(source);
        }
        fallback
    }

    fn picture_image_source_set(
        &self,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> Option<(String, Option<String>)> {
        let node = self.node(node_id)?;
        let parent_id = node.parent()?;
        let parent = self.node(parent_id)?;
        if parent.element_name() != Some("picture") {
            return None;
        }
        for child_id in parent.children() {
            if *child_id == node_id {
                break;
            }
            let source = self.node(*child_id)?;
            if source.element_name() != Some("source")
                || !image_media_matches(source.attribute("media"), viewport)
                || !image_type_is_supported(source.attribute("type"))
            {
                continue;
            }
            let Some(srcset) = source.attribute("srcset") else {
                continue;
            };
            if parse_image_srcset(srcset).is_empty() {
                continue;
            }
            return Some((
                srcset.to_owned(),
                source.attribute("sizes").map(str::to_owned),
            ));
        }
        None
    }

    pub(crate) fn image_current_src(&self, node_id: NativeNodeId, viewport: Viewport) -> String {
        self.image_loads
            .get(&node_id.index())
            .cloned()
            .or_else(|| self.selected_image_source(node_id, viewport))
            .unwrap_or_default()
    }

    pub(crate) fn image_properties(
        &self,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> Option<(bool, u32, u32)> {
        let node = self.node(node_id)?;
        if node.element_name() != Some("img") {
            return None;
        }
        let source = self.image_current_src(node_id, viewport);
        if source.is_empty() {
            return Some((true, 0, 0));
        }
        if let Some(image) = self.image_resource_for_node(node_id) {
            return Some((true, image.width, image.height));
        }
        if let Some(image) = decode_data_image(&source) {
            return Some((true, image.width, image.height));
        }
        Some((
            self.image_loads
                .get(&node_id.index())
                .is_some_and(|loaded_source| loaded_source == &source),
            0,
            0,
        ))
    }

    pub(crate) fn mark_image_load(
        &mut self,
        node_index: u32,
        source: String,
        viewport: Viewport,
    ) -> Result<(), NativeEngineError> {
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("img")
            || self.selected_image_source(node_id, viewport).as_deref() != Some(source.as_str())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "image load state does not match its image element".into(),
            });
        }
        self.image_loads.insert(node_index, source);
        Ok(())
    }

    pub(crate) fn refresh_image_loads(&mut self, viewport: Viewport) {
        let retained = self
            .image_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                let node = self.node(node_id)?;
                (node.element_name() == Some("img")
                    && self.selected_image_source(node_id, viewport).as_deref()
                        == Some(source.as_str()))
                .then(|| (*node_index, source.clone()))
            })
            .collect();
        self.image_loads = retained;
    }

    pub(crate) fn background_image_source_for_node(&self, node_id: NativeNodeId) -> Option<&str> {
        let source_id = self.computed_style_for_layout(node_id).background_image()?;
        self.background_image_sources
            .get(&source_id)
            .map(String::as_str)
    }

    pub(crate) fn external_background_image_links(&self) -> Vec<(u32, String)> {
        self.nodes
            .iter()
            .filter_map(|node| {
                let node_id = node.id();
                self.node(node_id)?;
                let source = self.background_image_source_for_node(node_id)?;
                if source.is_empty()
                    || source
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
                {
                    return None;
                }
                Some((node_id.index(), source.to_owned()))
            })
            .collect()
    }

    pub(crate) fn image_resource_for_node(&self, node_id: NativeNodeId) -> Option<&NativeImage> {
        let resource = self.image_resources.get(&node_id.index())?;
        let node = self.node(node_id)?;
        (node.element_name() == Some("img")
            && self.image_loads.get(&node_id.index()) == Some(&resource.source))
        .then_some(&resource.image)
    }

    pub(crate) fn canvas_resource_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> Option<&NativeCanvasResource> {
        let node = self.node(node_id)?;
        (node.element_name() == Some("canvas"))
            .then(|| self.canvas_resources.get(&node_id.index()))
            .flatten()
    }

    pub(crate) fn set_image_resource(
        &mut self,
        node_index: u32,
        source: String,
        image: NativeImage,
    ) -> Result<(), NativeEngineError> {
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NativeEngineError::invalid(
                "external image source",
                "must be a bounded printable URL attribute",
            ));
        }
        let expected_bytes = usize::try_from(image.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(image.height).ok()?))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| NativeEngineError::limit("external image pixels", 0, usize::MAX))?;
        let decoded_bytes = image
            .decoded_bytes()
            .ok_or_else(|| NativeEngineError::limit("external image frames", 0, usize::MAX))?;
        if image.width == 0
            || image.height == 0
            || expected_bytes != image.pixels.len()
            || decoded_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES
            || expected_bytes / 4 > MAX_NATIVE_IMAGE_TRANSFER_PIXELS
        {
            return Err(NativeEngineError::limit(
                "external image pixels",
                MAX_NATIVE_IMAGE_TRANSFER_BYTES,
                decoded_bytes,
            ));
        }
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        let node = self
            .node(node_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("img") || self.image_loads.get(&node_index) != Some(&source)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "external image resource does not match its image element".into(),
            });
        }
        self.image_resources
            .insert(node_index, NativeImageResource { source, image });
        Ok(())
    }

    pub(crate) fn background_image_resource_for_node(
        &self,
        node_id: NativeNodeId,
    ) -> Option<&NativeImage> {
        let resource = self.background_image_resources.get(&node_id.index())?;
        (self.background_image_source_for_node(node_id) == Some(resource.source.as_str()))
            .then_some(&resource.image)
    }

    pub(crate) fn set_background_image_resource(
        &mut self,
        node_index: u32,
        source: String,
        image: NativeImage,
    ) -> Result<(), NativeEngineError> {
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NativeEngineError::invalid(
                "background image source",
                "must be a bounded printable URL",
            ));
        }
        let expected_bytes = usize::try_from(image.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(image.height).ok()?))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| NativeEngineError::limit("background image pixels", 0, usize::MAX))?;
        let decoded_bytes = image
            .decoded_bytes()
            .ok_or_else(|| NativeEngineError::limit("background image frames", 0, usize::MAX))?;
        if image.width == 0
            || image.height == 0
            || expected_bytes != image.pixels.len()
            || decoded_bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES
            || expected_bytes / 4 > MAX_NATIVE_IMAGE_TRANSFER_PIXELS
        {
            return Err(NativeEngineError::limit(
                "background image pixels",
                MAX_NATIVE_IMAGE_TRANSFER_BYTES,
                decoded_bytes,
            ));
        }
        let node_id = NativeNodeId::from_parts(self.generation, node_index);
        if self.background_image_source_for_node(node_id) != Some(source.as_str()) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "background image resource does not match its computed style".into(),
            });
        }
        self.background_image_resources
            .insert(node_index, NativeImageResource { source, image });
        Ok(())
    }

    pub(crate) fn refresh_background_image_sources(&mut self) {
        let mut sources = self.stylesheet.background_image_sources().clone();
        let inline_sources = self
            .nodes
            .iter()
            .filter(|node| node.inline_style_allowed())
            .filter_map(|node| node.attribute("style").map(str::to_owned))
            .collect::<Vec<_>>();
        for inline_style in inline_sources {
            collect_background_image_sources(&inline_style, &mut sources);
        }
        self.background_image_sources = sources;
        let retained = self
            .background_image_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                (self.background_image_source_for_node(node_id) == Some(resource.source.as_str()))
                    .then(|| (*node_index, resource.clone()))
            })
            .collect();
        self.background_image_resources = retained;
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
                    NativeNodeKind::DocumentType {
                        name,
                        public_id,
                        system_id,
                    } => NativeNodeKindWire::DocumentType {
                        name: name.clone(),
                        public_id: public_id.clone(),
                        system_id: system_id.clone(),
                    },
                    NativeNodeKind::Comment(value) => NativeNodeKindWire::Comment(value.clone()),
                    NativeNodeKind::Text(value) => NativeNodeKindWire::Text(value.clone()),
                },
                state: NativeElementStateWire {
                    namespace_uri: node
                        .state
                        .namespace_uri
                        .clone()
                        .or_else(|| Some(String::new())),
                    attribute_namespaces: node.state.attribute_namespaces.clone(),
                    value: node.state.value.clone(),
                    files: node.state.files.clone(),
                    checked: node.state.checked,
                    focused: node.state.focused,
                    selected: node.state.selected,
                    custom_validity: node.state.custom_validity.clone(),
                    selection_start: node.state.selection_start,
                    selection_end: node.state.selection_end,
                    selection_direction: node.state.selection_direction.clone(),
                },
            })
            .collect();
        let image_resources = self
            .image_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                (node.element_name() == Some("img")
                    && self.image_loads.get(node_index) == Some(&resource.source))
                .then(|| NativeImageResourceWire {
                    node_index: *node_index,
                    source: resource.source.clone(),
                    width: resource.image.width,
                    height: resource.image.height,
                    pixels_base64: base64::engine::general_purpose::STANDARD
                        .encode(&resource.image.pixels),
                    frames: resource
                        .image
                        .frames
                        .iter()
                        .map(|frame| NativeImageFrameWire {
                            delay_ms: frame.delay_ms,
                            pixels_base64: base64::engine::general_purpose::STANDARD
                                .encode(&frame.pixels),
                        })
                        .collect(),
                    loop_count: resource.image.loop_count,
                })
            })
            .collect();
        let image_loads = self
            .image_loads
            .iter()
            .filter_map(|(node_index, source)| {
                let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                (node.element_name() == Some("img")).then(|| NativeImageLoadWire {
                    node_index: *node_index,
                    source: source.clone(),
                })
            })
            .collect();
        let media_resources = self
            .media_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                ((node.element_name() == Some("audio") || node.element_name() == Some("video"))
                    && self.media_loads.get(node_index) == Some(&resource.source))
                .then(|| NativeMediaResourceWire {
                    node_index: *node_index,
                    source: resource.source.clone(),
                    content_type: resource.content_type.clone(),
                    byte_length: resource.byte_length,
                    duration_millis: resource.duration_millis,
                })
            })
            .collect();
        let media_loads =
            self.media_loads
                .iter()
                .filter_map(|(node_index, source)| {
                    let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                    (node.element_name() == Some("audio") || node.element_name() == Some("video"))
                        .then(|| NativeMediaLoadWire {
                            node_index: *node_index,
                            source: source.clone(),
                        })
                })
                .collect();
        let media_errors =
            self.media_errors
                .iter()
                .filter_map(|(node_index, source)| {
                    let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                    (node.element_name() == Some("audio") || node.element_name() == Some("video"))
                        .then(|| NativeMediaLoadWire {
                            node_index: *node_index,
                            source: source.clone(),
                        })
                })
                .collect();
        let canvas_resources = self
            .canvas_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                (node.element_name() == Some("canvas")).then(|| NativeCanvasResourceWire {
                    node_index: *node_index,
                    width: resource.width,
                    height: resource.height,
                    pixels_base64: base64::engine::general_purpose::STANDARD
                        .encode(&resource.pixels),
                    origin_clean: resource.origin_clean,
                })
            })
            .collect();
        let background_image_sources = self
            .background_image_sources
            .iter()
            .map(|(source_id, source)| NativeBackgroundImageSourceWire {
                source_id: *source_id,
                source: source.clone(),
            })
            .collect();
        let background_image_resources = self
            .background_image_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node_id = NativeNodeId::from_parts(self.generation, *node_index);
                (self.background_image_source_for_node(node_id) == Some(resource.source.as_str()))
                    .then(|| NativeImageResourceWire {
                        node_index: *node_index,
                        source: resource.source.clone(),
                        width: resource.image.width,
                        height: resource.image.height,
                        pixels_base64: base64::engine::general_purpose::STANDARD
                            .encode(&resource.image.pixels),
                        frames: resource
                            .image
                            .frames
                            .iter()
                            .map(|frame| NativeImageFrameWire {
                                delay_ms: frame.delay_ms,
                                pixels_base64: base64::engine::general_purpose::STANDARD
                                    .encode(&frame.pixels),
                            })
                            .collect(),
                        loop_count: resource.image.loop_count,
                    })
            })
            .collect();
        let font_resources = self
            .font_resources
            .iter()
            .map(|resource| NativeFontFaceResourceWire {
                family: resource.family.clone(),
                family_key: resource.family_key,
                weight: FontWeightValue::from_numeric(resource.weight.nominal())
                    .unwrap_or(FontWeightValue::Normal),
                weight_range: (resource.weight.min != resource.weight.max)
                    .then_some(resource.weight),
                style: resource.style,
                stretch: resource.stretch,
                unicode_ranges: resource.unicode_ranges.clone(),
                variation_settings: resource.variation_settings,
                feature_settings: resource.feature_settings,
                size_adjust: resource.size_adjust,
                metric_overrides: resource.metric_overrides,
                font_display: resource.font_display,
                data_base64: base64::engine::general_purpose::STANDARD
                    .encode(resource.bytes.as_ref()),
            })
            .collect();
        NativeDocumentWire {
            nodes,
            computed_styles,
            font_resources,
            blocked_inline_style_nodes: self
                .nodes
                .iter()
                .filter(|node| {
                    (node.element_name() == Some("style") || node.attribute("style").is_some())
                        && !node.inline_style_allowed()
                })
                .map(|node| node.id().index())
                .collect(),
            script_nodes: self
                .script_node_ids
                .iter()
                .filter(|(_, id)| id.generation == self.generation)
                .map(|(temporary_index, id)| NativeScriptNodeIdentity {
                    temporary_index: *temporary_index,
                    node_index: id.index,
                })
                .collect(),
            started_script_nodes: self.started_script_nodes.iter().copied().collect(),
            image_resources,
            background_image_sources,
            background_image_resources,
            image_loads,
            canvas_resources,
            media_resources,
            media_loads,
            media_errors,
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
        if wire.font_resources.len() > MAX_NATIVE_FONT_FACES {
            return Err(NativeEngineError::limit(
                "content-process font faces",
                MAX_NATIVE_FONT_FACES,
                wire.font_resources.len(),
            ));
        }
        let max_encoded_font_bytes =
            (MAX_NATIVE_FONT_BYTES.saturating_add(2).saturating_div(3)).saturating_mul(4);
        let mut font_resources = Vec::with_capacity(wire.font_resources.len());
        let mut total_font_bytes = 0usize;
        for resource in wire.font_resources {
            let weight = resource
                .weight_range
                .unwrap_or_else(|| NativeFontWeightRange::singleton(resource.weight));
            if resource.family.is_empty()
                || resource.family.len() > MAX_ATTRIBUTE_BYTES
                || resource.family.bytes().any(|byte| byte.is_ascii_control())
                || resource.family_key != super::css::font_family_hash(&resource.family)
                || weight.min < 1
                || weight.min > weight.max
                || weight.max > 1000
                || resource.stretch.min < 500
                || resource.stretch.min > resource.stretch.max
                || resource.stretch.max > 2000
                || !(250..=4000).contains(&resource.size_adjust)
                || resource.unicode_ranges.len() > super::css::MAX_NATIVE_FONT_FACE_UNICODE_RANGES
                || resource
                    .unicode_ranges
                    .iter()
                    .any(|range| range.start > range.end || range.end > 0x10_FFFF)
                || !resource.variation_settings.is_valid()
                || !resource.feature_settings.is_valid()
                || resource
                    .metric_overrides
                    .ascent
                    .is_some_and(|value| value > 10_000)
                || resource
                    .metric_overrides
                    .descent
                    .is_some_and(|value| value > 10_000)
                || resource
                    .metric_overrides
                    .line_gap
                    .is_some_and(|value| value > 10_000)
                || resource.data_base64.len() > max_encoded_font_bytes
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid font metadata".into(),
                });
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&resource.data_base64)
                .map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid font bytes".into(),
                })?;
            if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
                return Err(NativeEngineError::limit(
                    "content-process font bytes",
                    MAX_NATIVE_FONT_BYTES,
                    bytes.len(),
                ));
            }
            total_font_bytes = total_font_bytes.saturating_add(bytes.len());
            if total_font_bytes > MAX_NATIVE_FONT_TOTAL_BYTES {
                return Err(NativeEngineError::limit(
                    "content-process font bytes",
                    MAX_NATIVE_FONT_TOTAL_BYTES,
                    total_font_bytes,
                ));
            }
            font_resources.push(NativeFontFaceResource {
                family: resource.family,
                family_key: resource.family_key,
                weight,
                style: resource.style,
                stretch: resource.stretch,
                size_adjust: resource.size_adjust,
                metric_overrides: resource.metric_overrides,
                font_display: resource.font_display,
                unicode_ranges: resource.unicode_ranges,
                variation_settings: resource.variation_settings,
                feature_settings: resource.feature_settings,
                bytes: Arc::from(bytes),
            });
        }
        let font_book = NativeFontBook::from_resources(&font_resources);
        if wire.background_image_sources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process background image sources",
                limits.max_nodes,
                wire.background_image_sources.len(),
            ));
        }
        let mut background_image_sources = BTreeMap::new();
        for source in &wire.background_image_sources {
            if source.source_id == 0
                || source.source.is_empty()
                || source.source.len() > MAX_ATTRIBUTE_BYTES
                || source.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid background image source".into(),
                });
            }
            if background_image_sources
                .insert(source.source_id, source.source.clone())
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate background image sources".into(),
                });
            }
        }
        if wire.computed_styles.iter().any(|style| {
            style
                .background_image()
                .is_some_and(|source_id| !background_image_sources.contains_key(&source_id))
        }) {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an unknown background image source".into(),
            });
        }
        if wire.blocked_inline_style_nodes.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process blocked inline styles",
                limits.max_nodes,
                wire.blocked_inline_style_nodes.len(),
            ));
        }
        let blocked_inline_style_nodes = wire
            .blocked_inline_style_nodes
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if blocked_inline_style_nodes.len() != wire.blocked_inline_style_nodes.len()
            || blocked_inline_style_nodes.iter().any(|node_index| {
                let Ok(index) = usize::try_from(*node_index) else {
                    return true;
                };
                let Some(node) = wire.nodes.get(index) else {
                    return true;
                };
                match &node.kind {
                    NativeNodeKindWire::Element { name, attributes } => {
                        !name.eq_ignore_ascii_case("style")
                            && !attributes
                                .iter()
                                .any(|(name, _)| name.eq_ignore_ascii_case("style"))
                    }
                    NativeNodeKindWire::Document
                    | NativeNodeKindWire::DocumentType { .. }
                    | NativeNodeKindWire::Comment(_)
                    | NativeNodeKindWire::Text(_) => true,
                }
            })
        {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid blocked inline style node".into(),
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
                NativeNodeKindWire::DocumentType {
                    name,
                    public_id,
                    system_id,
                } => {
                    if name.len() > MAX_ATTRIBUTE_BYTES
                        || public_id
                            .as_ref()
                            .is_some_and(|value| value.len() > MAX_ATTRIBUTE_BYTES)
                        || system_id
                            .as_ref()
                            .is_some_and(|value| value.len() > MAX_ATTRIBUTE_BYTES)
                    {
                        return Err(NativeEngineError::limit(
                            "content-process document type",
                            MAX_ATTRIBUTE_BYTES,
                            MAX_ATTRIBUTE_BYTES.saturating_add(1),
                        ));
                    }
                    NativeNodeKind::DocumentType {
                        name: name.clone(),
                        public_id: public_id.clone(),
                        system_id: system_id.clone(),
                    }
                }
                NativeNodeKindWire::Comment(value) => {
                    if value.len() > MAX_ATTRIBUTE_BYTES {
                        return Err(NativeEngineError::limit(
                            "content-process comment",
                            MAX_ATTRIBUTE_BYTES,
                            value.len(),
                        ));
                    }
                    NativeNodeKind::Comment(value.clone())
                }
                NativeNodeKindWire::Text(value) => NativeNodeKind::Text(value.clone()),
            };
            let namespace_uri = if matches!(&kind, NativeNodeKind::Element { .. }) {
                match wire_node.state.namespace_uri.as_deref() {
                    Some("") => None,
                    Some(value) => Some(validate_namespace_uri(value)?),
                    None => Some(HTML_NAMESPACE_URI.to_owned()),
                }
            } else {
                None
            };
            let attribute_namespaces = if matches!(&kind, NativeNodeKind::Element { .. }) {
                wire_node
                    .state
                    .attribute_namespaces
                    .iter()
                    .map(|(name, namespace)| {
                        Ok((name.clone(), validate_attribute_namespace_uri(namespace)?))
                    })
                    .collect::<Result<BTreeMap<_, _>, NativeEngineError>>()?
            } else {
                BTreeMap::new()
            };
            if !wire_node.state.files.is_empty() {
                NativeFile::validate_many(&wire_node.state.files)?;
            }
            nodes.push(NativeNode {
                id: NativeNodeId { generation, index },
                parent,
                children,
                kind,
                state: NativeElementState {
                    namespace_uri,
                    attribute_namespaces,
                    value: wire_node.state.value.clone(),
                    files: wire_node.state.files.clone(),
                    checked: wire_node.state.checked,
                    focused: wire_node.state.focused,
                    selected: wire_node.state.selected,
                    custom_validity: wire_node.state.custom_validity.clone(),
                    selection_start: wire_node.state.selection_start,
                    selection_end: wire_node.state.selection_end,
                    selection_direction: wire_node.state.selection_direction.clone(),
                    inline_style_allowed: !blocked_inline_style_nodes.contains(&index),
                },
            });
        }
        if !matches!(nodes[0].kind, NativeNodeKind::Document) || nodes[0].parent.is_some() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid document root".into(),
            });
        }
        if wire.image_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process image resources",
                limits.max_nodes,
                wire.image_resources.len(),
            ));
        }
        let max_encoded_pixels = (MAX_NATIVE_IMAGE_TRANSFER_BYTES
            .saturating_add(2)
            .saturating_div(3))
        .saturating_mul(4);
        let mut image_resources = BTreeMap::new();
        for resource in wire.image_resources {
            if resource.source.is_empty()
                || resource.source.len() > MAX_ATTRIBUTE_BYTES
                || resource.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image source".into(),
                });
            }
            if resource.pixels_base64.len() > max_encoded_pixels {
                return Err(NativeEngineError::limit(
                    "content-process image pixels",
                    max_encoded_pixels,
                    resource.pixels_base64.len(),
                ));
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image node index".into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range image node index".into(),
                })?;
            if node.element_name() != Some("img")
                || !image_source_is_declared(&nodes, node_index, &resource.source)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an image resource for a different node"
                        .into(),
                });
            }
            let image = image_from_wire(&resource, max_encoded_pixels)?;
            if image_resources
                .insert(
                    resource.node_index,
                    NativeImageResource {
                        source: resource.source,
                        image,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate image resources".into(),
                });
            }
        }
        if wire.background_image_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process background image resources",
                limits.max_nodes,
                wire.background_image_resources.len(),
            ));
        }
        let mut background_image_resources = BTreeMap::new();
        for resource in wire.background_image_resources {
            if resource.source.is_empty()
                || resource.source.len() > MAX_ATTRIBUTE_BYTES
                || resource.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid background image resource".into(),
                });
            }
            if resource.pixels_base64.len() > max_encoded_pixels {
                return Err(NativeEngineError::limit(
                    "content-process background image pixels",
                    max_encoded_pixels,
                    resource.pixels_base64.len(),
                ));
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid background image node index"
                        .into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range background image node index"
                        .into(),
                })?;
            let style = wire
                .computed_styles
                .get(node_index)
                .copied()
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range background image style"
                        .into(),
                })?;
            let source_id = style
                .background_image()
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a background image for a node without one"
                        .into(),
                })?;
            if background_image_sources.get(&source_id) != Some(&resource.source)
                || !matches!(node.kind(), NativeNodeKind::Element { .. })
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason:
                        "content process returned a background image resource for a different style"
                            .into(),
                });
            }
            let image = image_from_wire(&resource, max_encoded_pixels)?;
            if background_image_resources
                .insert(
                    resource.node_index,
                    NativeImageResource {
                        source: resource.source,
                        image,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate background image resources".into(),
                });
            }
        }
        if wire.image_loads.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process image load states",
                limits.max_nodes,
                wire.image_loads.len(),
            ));
        }
        let mut image_loads = BTreeMap::new();
        for load in wire.image_loads {
            if load.source.is_empty()
                || load.source.len() > MAX_ATTRIBUTE_BYTES
                || load.source.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image load source".into(),
                });
            }
            let node_index =
                usize::try_from(load.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid image load node index".into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range image load node index".into(),
                })?;
            if node.element_name() != Some("img")
                || !image_source_is_declared(&nodes, node_index, &load.source)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an image load for a different node".into(),
                });
            }
            if image_loads.insert(load.node_index, load.source).is_some() {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate image load states".into(),
                });
            }
        }
        if wire.media_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process media resources",
                limits.max_nodes,
                wire.media_resources.len(),
            ));
        }
        let mut media_resources = BTreeMap::new();
        for resource in wire.media_resources {
            if resource.source.is_empty()
                || resource.source.len() > MAX_ATTRIBUTE_BYTES
                || resource.source.bytes().any(|byte| byte.is_ascii_control())
                || resource.content_type.is_empty()
                || resource.content_type.len() > MAX_ATTRIBUTE_BYTES
                || resource.byte_length == 0
                || resource.byte_length > MAX_NATIVE_MEDIA_BYTES
                || !media_type_is_supported(Some(&resource.content_type))
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid media resource".into(),
                });
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid media node index".into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range media node index".into(),
                })?;
            if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
                || !media_source_is_declared(&nodes, node_index, &resource.source)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a media resource for a different node".into(),
                });
            }
            if media_resources
                .insert(
                    resource.node_index,
                    NativeMediaResource {
                        source: resource.source,
                        content_type: resource.content_type,
                        byte_length: resource.byte_length,
                        duration_millis: resource.duration_millis,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate media resources".into(),
                });
            }
        }
        let decode_media_loads = |loads: Vec<NativeMediaLoadWire>,
                                  kind: &'static str|
         -> Result<BTreeMap<u32, String>, NativeEngineError> {
            if loads.len() > limits.max_nodes {
                return Err(NativeEngineError::limit(
                    kind,
                    limits.max_nodes,
                    loads.len(),
                ));
            }
            let mut result = BTreeMap::new();
            for load in loads {
                if load.source.is_empty()
                    || load.source.len() > MAX_ATTRIBUTE_BYTES
                    || load.source.bytes().any(|byte| byte.is_ascii_control())
                {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an invalid media load source".into(),
                    });
                }
                let node_index =
                    usize::try_from(load.node_index).map_err(|_| NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an invalid media load node index".into(),
                    })?;
                let node = nodes
                    .get(node_index)
                    .ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned an out-of-range media load node index"
                            .into(),
                    })?;
                if (node.element_name() != Some("audio") && node.element_name() != Some("video"))
                    || !media_source_is_declared(&nodes, node_index, &load.source)
                {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned a media load for a different node".into(),
                    });
                }
                if result.insert(load.node_index, load.source).is_some() {
                    return Err(NativeEngineError::Parse {
                        offset: 0,
                        reason: "content process returned duplicate media load states".into(),
                    });
                }
            }
            Ok(result)
        };
        let media_loads =
            decode_media_loads(wire.media_loads, "content-process media load states")?;
        let media_errors =
            decode_media_loads(wire.media_errors, "content-process media error states")?;
        if media_resources
            .iter()
            .any(|(node_index, resource)| media_loads.get(node_index) != Some(&resource.source))
        {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned media resources without matching loads".into(),
            });
        }
        if wire.canvas_resources.len() > limits.max_nodes {
            return Err(NativeEngineError::limit(
                "content-process canvas resources",
                limits.max_nodes,
                wire.canvas_resources.len(),
            ));
        }
        let max_canvas_encoded_bytes =
            (MAX_NATIVE_CANVAS_BYTES.saturating_add(2).saturating_div(3)).saturating_mul(4);
        let mut canvas_resources = BTreeMap::new();
        for resource in wire.canvas_resources {
            if resource.width == 0
                || resource.height == 0
                || resource.width > MAX_NATIVE_CANVAS_DIMENSION
                || resource.height > MAX_NATIVE_CANVAS_DIMENSION
            {
                return Err(NativeEngineError::invalid(
                    "content-process canvas dimensions",
                    format!("must be between 1 and {MAX_NATIVE_CANVAS_DIMENSION} pixels per axis"),
                ));
            }
            if resource.pixels_base64.len() > max_canvas_encoded_bytes {
                return Err(NativeEngineError::limit(
                    "content-process canvas pixels",
                    max_canvas_encoded_bytes,
                    resource.pixels_base64.len(),
                ));
            }
            let expected_bytes = usize::try_from(resource.width)
                .ok()
                .and_then(|width| {
                    usize::try_from(resource.height)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| {
                    NativeEngineError::limit(
                        "content-process canvas pixels",
                        MAX_NATIVE_CANVAS_BYTES,
                        usize::MAX,
                    )
                })?;
            if expected_bytes > MAX_NATIVE_CANVAS_BYTES {
                return Err(NativeEngineError::limit(
                    "content-process canvas pixels",
                    MAX_NATIVE_CANVAS_BYTES,
                    expected_bytes,
                ));
            }
            let node_index =
                usize::try_from(resource.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid canvas node index".into(),
                })?;
            let node = nodes
                .get(node_index)
                .ok_or_else(|| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an out-of-range canvas node index".into(),
                })?;
            if node.element_name() != Some("canvas") {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned a canvas resource for a different node"
                        .into(),
                });
            }
            let pixels = base64::engine::general_purpose::STANDARD
                .decode(&resource.pixels_base64)
                .map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned invalid canvas pixels".into(),
                })?;
            if pixels.len() != expected_bytes {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned canvas pixels with the wrong dimensions"
                        .into(),
                });
            }
            if canvas_resources
                .insert(
                    resource.node_index,
                    NativeCanvasResource {
                        width: resource.width,
                        height: resource.height,
                        pixels,
                        origin_clean: resource.origin_clean,
                    },
                )
                .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate canvas resources".into(),
                });
            }
        }
        let root = NativeNodeId {
            generation,
            index: 0,
        };
        let mut diagnostics = NativeDiagnosticSink::default();
        for node in &nodes {
            if !node.inline_style_allowed() {
                continue;
            }
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
        let mut script_node_ids = BTreeMap::new();
        for identity in wire.script_nodes {
            if identity.temporary_index < SCRIPT_TEMP_NODE_BASE {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid script node identity".into(),
                });
            }
            let index =
                usize::try_from(identity.node_index).map_err(|_| NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid script node identity".into(),
                })?;
            if index >= nodes.len()
                || script_node_ids
                    .insert(
                        identity.temporary_index,
                        NativeNodeId {
                            generation,
                            index: identity.node_index,
                        },
                    )
                    .is_some()
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned duplicate script node identity".into(),
                });
            }
        }
        let mut started_script_nodes = BTreeSet::new();
        for node_index in wire.started_script_nodes {
            let index = usize::try_from(node_index).map_err(|_| NativeEngineError::Parse {
                offset: 0,
                reason: "content process returned an invalid started script node".into(),
            })?;
            if nodes.get(index).and_then(NativeNode::element_name) != Some("script")
                || !started_script_nodes.insert(node_index)
            {
                return Err(NativeEngineError::Parse {
                    offset: 0,
                    reason: "content process returned an invalid started script node".into(),
                });
            }
        }
        let mut document = Self {
            generation,
            revision: u64::from(generation),
            root,
            max_nodes: limits.max_nodes,
            max_dom_depth: limits.max_dom_depth,
            nodes,
            stylesheet: NativeStylesheet::default(),
            font_resources: font_resources.clone(),
            font_book,
            external_stylesheet_states: BTreeMap::new(),
            computed_styles: Some(wire.computed_styles),
            diagnostics,
            diagnostics_truncated,
            script_node_ids,
            started_script_nodes,
            processed_csp_meta_nodes: BTreeSet::new(),
            pending_csp_meta_policies: BTreeMap::new(),
            image_resources,
            image_loads,
            media_resources,
            media_loads,
            media_errors,
            background_image_sources,
            background_image_resources,
            canvas_resources,
            inline_style_element_reports: BTreeMap::new(),
            inline_style_attribute_reports: BTreeMap::new(),
        };
        document.normalize_select_defaults();
        document.mark_inline_style_reports_seen();
        document.mark_content_security_policy_meta_processed();
        Ok(document)
    }

    fn assign_parsed_namespaces(&mut self) {
        let children = self
            .raw_node(self.root)
            .map(NativeNode::children)
            .unwrap_or_default()
            .to_vec();
        for child in children {
            self.assign_parsed_namespace_subtree(child, HTML_NAMESPACE_URI);
        }
    }

    fn assign_parsed_namespace_subtree(&mut self, id: NativeNodeId, parent_namespace: &str) {
        let Some(node) = self.raw_node(id) else {
            return;
        };
        let Some(name) = node.element_name().map(str::to_owned) else {
            return;
        };
        let children = node.children().to_vec();
        let namespace = if parent_namespace == SVG_NAMESPACE_URI || name == "svg" {
            SVG_NAMESPACE_URI
        } else if name == "math" || parent_namespace == MATHML_NAMESPACE_URI {
            MATHML_NAMESPACE_URI
        } else {
            HTML_NAMESPACE_URI
        };
        let child_namespace = if namespace == SVG_NAMESPACE_URI && name == "foreignobject" {
            HTML_NAMESPACE_URI
        } else {
            namespace
        };
        if let Some(node) = self.raw_node_mut(id) {
            node.state.namespace_uri = Some(namespace.to_owned());
        }
        for child in children {
            self.assign_parsed_namespace_subtree(child, child_namespace);
        }
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
            max_nodes: MAX_NATIVE_NODES,
            max_dom_depth: MAX_NATIVE_DOM_DEPTH,
            nodes: vec![NativeNode {
                id: root,
                parent: None,
                children: Vec::new(),
                kind: NativeNodeKind::Document,
                state: NativeElementState::default(),
            }],
            stylesheet: NativeStylesheet::default(),
            font_resources: Vec::new(),
            font_book: NativeFontBook::system(),
            external_stylesheet_states: BTreeMap::new(),
            computed_styles: None,
            diagnostics: Vec::new(),
            diagnostics_truncated: false,
            script_node_ids: BTreeMap::new(),
            started_script_nodes: BTreeSet::new(),
            processed_csp_meta_nodes: BTreeSet::new(),
            pending_csp_meta_policies: BTreeMap::new(),
            image_resources: BTreeMap::new(),
            image_loads: BTreeMap::new(),
            media_resources: BTreeMap::new(),
            media_loads: BTreeMap::new(),
            media_errors: BTreeMap::new(),
            background_image_sources: BTreeMap::new(),
            background_image_resources: BTreeMap::new(),
            canvas_resources: BTreeMap::new(),
            inline_style_element_reports: BTreeMap::new(),
            inline_style_attribute_reports: BTreeMap::new(),
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
        let node = self.raw_node(id)?;
        self.is_attached(id).then_some(node)
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

    pub(crate) fn element_node_ids(&self) -> impl Iterator<Item = NativeNodeId> + '_ {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.element_name().is_some())
            .map(NativeNode::id)
    }

    /// Return the embedded browsing-context sources in document order. The
    /// frame owner resolves these references against the current document;
    /// an omitted or empty `src` is the standard initial blank document.
    pub(crate) fn embedded_frame_sources(&self) -> Vec<(NativeNodeId, String)> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| matches!(node.element_name(), Some("iframe" | "frame")))
            .map(|node| {
                if let Some(srcdoc) = node.attribute("srcdoc") {
                    return (
                        node.id(),
                        format!("data:text/html,{}", encode_data_url_payload(srcdoc)),
                    );
                }
                (
                    node.id(),
                    node.attribute("src")
                        .map(str::trim)
                        .filter(|source| !source.is_empty())
                        .unwrap_or("about:blank")
                        .to_owned(),
                )
            })
            .collect()
    }

    fn script_image_resources(&self, viewport: Viewport) -> Vec<NativeScriptImageResourceSnapshot> {
        let snapshot =
            |node_index, source: String, image: &NativeImage| NativeScriptImageResourceSnapshot {
                node_index,
                source,
                width: image.width,
                height: image.height,
                pixels_base64: base64::engine::general_purpose::STANDARD
                    .encode(image.current_pixels()),
                frames: image
                    .frames
                    .iter()
                    .map(|frame| NativeScriptImageFrameSnapshot {
                        delay_ms: frame.delay_ms,
                        pixels_base64: base64::engine::general_purpose::STANDARD
                            .encode(&frame.pixels),
                    })
                    .collect(),
                loop_count: image.loop_count,
                animation_elapsed_ms: image.animation_elapsed_ms(),
            };
        let mut resources = Vec::new();
        let mut included = BTreeSet::new();
        for (node_index, resource) in &self.image_resources {
            let node_id = NativeNodeId::from_parts(self.generation, *node_index);
            let Some(node) = self.node(node_id) else {
                continue;
            };
            if node.element_name() != Some("img")
                || self.image_loads.get(node_index) != Some(&resource.source)
            {
                continue;
            }
            resources.push(snapshot(
                *node_index,
                resource.source.clone(),
                &resource.image,
            ));
            included.insert(*node_index);
        }
        for node in &self.nodes {
            let node_index = node.id().index();
            if included.contains(&node_index) || node.element_name() != Some("img") {
                continue;
            }
            let source = self.image_current_src(node.id(), viewport);
            let Some(image) = decode_data_image(&source) else {
                continue;
            };
            resources.push(snapshot(node_index, source, &image));
        }
        resources
    }

    pub(crate) fn script_snapshot_for_viewport(
        &self,
        max_text_bytes: usize,
        viewport: Viewport,
    ) -> NativeScriptDocumentSnapshot {
        let (title, _) = self.title(max_text_bytes);
        let (visible_text, _) = self.visible_text(max_text_bytes);
        let nodes = self
            .nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                let (node_type, node_name, node_value) = match node.kind() {
                    NativeNodeKind::Document => (9, "#document".to_owned(), None),
                    NativeNodeKind::DocumentType { name, .. } => (10, name.clone(), None),
                    NativeNodeKind::Element { name, .. } => (1, name.to_ascii_uppercase(), None),
                    NativeNodeKind::Comment(value) => {
                        (8, "#comment".to_owned(), Some(value.clone()))
                    }
                    NativeNodeKind::Text(value) => (3, "#text".to_owned(), Some(value.clone())),
                };
                let (public_id, system_id) = match node.kind() {
                    NativeNodeKind::DocumentType {
                        public_id,
                        system_id,
                        ..
                    } => (public_id.clone(), system_id.clone()),
                    _ => (None, None),
                };
                let children = node
                    .children()
                    .iter()
                    .copied()
                    .filter(|child| self.node(*child).is_some())
                    .map(NativeNodeId::index)
                    .collect();
                Some(NativeScriptNodeSnapshot {
                    node_index: node.id().index(),
                    parent_index: node.parent().map(NativeNodeId::index),
                    node_type,
                    node_name,
                    node_value,
                    public_id,
                    system_id,
                    children,
                })
            })
            .collect();
        let elements = self
            .nodes
            .iter()
            .filter_map(|node| {
                self.node(node.id())?;
                let tag_name = node.element_name()?.to_owned();
                let (text, _) = self
                    .element_text(node.id(), max_text_bytes)
                    .unwrap_or_default();
                let (validity, validation_message, will_validate) =
                    self.script_validation_snapshot(node.id());
                let (selection_start, selection_end, selection_direction) = self
                    .selection_snapshot(node.id())
                    .map(|(start, end, direction)| (Some(start), Some(end), Some(direction)))
                    .unwrap_or((None, None, None));
                let (image_complete, image_natural_width, image_natural_height) = self
                    .image_properties(node.id(), viewport)
                    .unwrap_or((false, 0, 0));
                let image_current_src = if tag_name == "img" {
                    self.image_current_src(node.id(), viewport)
                } else {
                    String::new()
                };
                let (
                    media_ready_state,
                    media_network_state,
                    media_duration_millis,
                    media_current_src,
                    media_error,
                ) = self
                    .media_properties(node.id())
                    .unwrap_or((0, 0, None, String::new(), None));
                let computed_style = self.computed_style_for_layout(node.id());
                Some(NativeScriptElementSnapshot {
                    node_index: node.id().index(),
                    parent_index: self.parent_element_index(node.id()),
                    form_owner_index: self.form_owner(node.id()).map(NativeNodeId::index),
                    tag_name,
                    namespace_uri: node.state.namespace_uri.clone(),
                    attribute_namespaces: node.state.attribute_namespaces.clone(),
                    attributes: node.attributes()?.clone(),
                    text,
                    inner_html: self.element_inner_html(node.id(), max_text_bytes),
                    value: self.current_value(node.id()),
                    files: node.state.files.clone(),
                    checked: node.state.checked,
                    selected: node.state.selected,
                    disabled: self.is_disabled(node.id()),
                    hidden: self.is_hidden(node.id()),
                    focused: node.state.focused,
                    validity,
                    validation_message,
                    custom_validity: node.state.custom_validity.clone(),
                    will_validate,
                    selection_start,
                    selection_end,
                    selection_direction,
                    image_complete,
                    image_natural_width,
                    image_natural_height,
                    image_current_src,
                    media_ready_state,
                    media_network_state,
                    media_current_src,
                    media_duration_millis,
                    media_error,
                    scroll_x: 0,
                    scroll_y: 0,
                    inline_style_allowed: node.inline_style_allowed(),
                    computed_style,
                })
            })
            .collect();
        let canvas_resources = self
            .canvas_resources
            .iter()
            .filter_map(|(node_index, resource)| {
                let node = self.node(NativeNodeId::from_parts(self.generation, *node_index))?;
                (node.element_name() == Some("canvas")).then(|| NativeCanvasResourceWire {
                    node_index: *node_index,
                    width: resource.width,
                    height: resource.height,
                    pixels_base64: base64::engine::general_purpose::STANDARD
                        .encode(&resource.pixels),
                    origin_clean: resource.origin_clean,
                })
            })
            .collect();
        NativeScriptDocumentSnapshot {
            revision: self.revision,
            title,
            visible_text,
            elements,
            nodes,
            geometry: Vec::new(),
            scroll_x: 0,
            scroll_y: 0,
            scroll_width: 0,
            scroll_height: 0,
            script_nodes: self
                .script_node_ids
                .iter()
                .filter(|(_, id)| id.generation == self.generation)
                .map(|(temporary_index, id)| NativeScriptNodeIdentity {
                    temporary_index: *temporary_index,
                    node_index: id.index,
                })
                .collect(),
            canvas_resources,
            image_resources: self.script_image_resources(viewport),
        }
    }

    pub(crate) fn script_snapshot_with_layout(
        &self,
        max_text_bytes: usize,
        viewport: Viewport,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) -> Result<NativeScriptDocumentSnapshot, NativeEngineError> {
        let mut snapshot = self.script_snapshot_for_viewport(max_text_bytes, viewport);
        let layout = self.layout(viewport)?.with_scroll_offset(
            self,
            scroll_offset,
            nested_scroll_offsets,
        )?;
        snapshot.geometry = layout
            .boxes
            .iter()
            .map(|layout_box| {
                let scroll_container = layout.scroll_container_for(layout_box.node_id);
                let nested_scroll_offset = if layout_box.fixed {
                    NativePoint { x: 0, y: 0 }
                } else {
                    layout.nested_scroll_offset_for(self, layout_box.node_id, false)
                };
                let is_root = self
                    .node(layout_box.node_id)
                    .and_then(|node| node.element_name())
                    == Some("html");
                let client_width = if is_root {
                    viewport.width
                } else {
                    scroll_container
                        .map(|container| container.client_width)
                        .unwrap_or(layout_box.rect.width)
                };
                let client_height = if is_root {
                    viewport.height
                } else {
                    scroll_container
                        .map(|container| container.client_height)
                        .unwrap_or(layout_box.rect.height)
                };
                NativeScriptGeometrySnapshot {
                    node_index: layout_box.node_id.index(),
                    x: i64::from(layout_box.rect.x)
                        - i64::from(layout.scroll_offset.x)
                        - i64::from(nested_scroll_offset.x),
                    y: i64::from(layout_box.rect.y)
                        - i64::from(layout.scroll_offset.y)
                        - i64::from(nested_scroll_offset.y),
                    width: layout_box.rect.width,
                    height: layout_box.rect.height,
                    content_x: i64::from(layout_box.content_rect.x)
                        - i64::from(layout.scroll_offset.x)
                        - i64::from(nested_scroll_offset.x),
                    content_y: i64::from(layout_box.content_rect.y)
                        - i64::from(layout.scroll_offset.y)
                        - i64::from(nested_scroll_offset.y),
                    content_width: layout_box.content_rect.width,
                    content_height: layout_box.content_rect.height,
                    scroll_x: if is_root {
                        layout.scroll_offset.x
                    } else {
                        scroll_container
                            .map(|container| container.scroll_offset.x)
                            .unwrap_or(0)
                    },
                    scroll_y: if is_root {
                        layout.scroll_offset.y
                    } else {
                        scroll_container
                            .map(|container| container.scroll_offset.y)
                            .unwrap_or(0)
                    },
                    scroll_width: if is_root {
                        layout.content_width
                    } else {
                        scroll_container
                            .map(|container| container.scroll_width)
                            .unwrap_or(layout_box.rect.width)
                    },
                    scroll_height: if is_root {
                        layout.content_height
                    } else {
                        scroll_container
                            .map(|container| container.scroll_height)
                            .unwrap_or(layout_box.rect.height)
                    },
                    client_width,
                    client_height,
                }
            })
            .collect();
        for element in &mut snapshot.elements {
            if let Some(geometry) = snapshot
                .geometry
                .iter()
                .find(|geometry| geometry.node_index == element.node_index)
            {
                element.scroll_x = geometry.scroll_x;
                element.scroll_y = geometry.scroll_y;
            }
        }
        snapshot.scroll_x = layout.scroll_offset.x;
        snapshot.scroll_y = layout.scroll_offset.y;
        snapshot.scroll_width = layout.content_width;
        snapshot.scroll_height = layout.content_height;
        Ok(snapshot)
    }

    pub(crate) fn page_script_sources(
        &self,
        max_scripts: usize,
        max_source_bytes: usize,
    ) -> Vec<NativePageScriptSource> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.element_name() == Some("script"))
            .filter_map(|node| {
                let module = match node.attribute("type") {
                    None | Some("") => false,
                    Some(value) if value.eq_ignore_ascii_case("module") => true,
                    Some(value)
                        if value.eq_ignore_ascii_case("text/javascript")
                            || value.eq_ignore_ascii_case("application/javascript") =>
                    {
                        false
                    }
                    Some(_) => return None,
                };
                let external = node
                    .attribute("src")
                    .is_some_and(|source| !source.is_empty());
                let timing = if external && node.attribute("async").is_some() {
                    NativePageScriptTiming::Async
                } else if module || (external && node.attribute("defer").is_some()) {
                    NativePageScriptTiming::Defer
                } else {
                    NativePageScriptTiming::ParserBlocking
                };
                Some((node, module, timing))
            })
            .take(max_scripts)
            .filter_map(|(node, module, timing)| {
                if let Some(source) = node.attribute("src") {
                    return (!source.is_empty()).then(|| {
                        if module {
                            NativePageScriptSource::ModuleExternal {
                                href: source.to_owned(),
                                timing,
                                node_index: node.id().index(),
                                nonce: node.attribute("nonce").map(str::to_owned),
                                integrity: node.attribute("integrity").map(str::to_owned),
                                crossorigin: node.attribute("crossorigin").map(str::to_owned),
                                parser_inserted: true,
                            }
                        } else {
                            NativePageScriptSource::External {
                                href: source.to_owned(),
                                timing,
                                node_index: node.id().index(),
                                nonce: node.attribute("nonce").map(str::to_owned),
                                integrity: node.attribute("integrity").map(str::to_owned),
                                crossorigin: node.attribute("crossorigin").map(str::to_owned),
                                parser_inserted: true,
                            }
                        }
                    });
                }
                let mut source = String::new();
                self.collect_raw_text(node.id(), &mut source);
                (source.len() <= max_source_bytes && !source.is_empty()).then_some(if module {
                    NativePageScriptSource::ModuleInline {
                        source,
                        timing,
                        node_index: node.id().index(),
                        nonce: node.attribute("nonce").map(str::to_owned),
                        parser_inserted: true,
                    }
                } else {
                    NativePageScriptSource::Inline {
                        source,
                        timing,
                        node_index: node.id().index(),
                        nonce: node.attribute("nonce").map(str::to_owned),
                        parser_inserted: true,
                    }
                })
            })
            .collect()
    }

    /// Mark every parser-discovered script as started before page lifecycle
    /// events run. A script element is single-shot even when its text or
    /// parent is later changed by script.
    pub(crate) fn mark_attached_scripts_started(&mut self) {
        let started = self
            .nodes
            .iter()
            .filter(|node| self.is_attached(node.id()) && node.element_name() == Some("script"))
            .map(|node| node.id().index())
            .collect::<Vec<_>>();
        self.started_script_nodes.extend(started);
    }

    /// Commit the script-start side effect for nodes touched by one host
    /// mutation and return the newly attached page-script sources. Temporary
    /// script nodes may be created in one evaluation and attached in a later
    /// evaluation, so both creation and insertion commands are considered.
    pub(crate) fn take_newly_attached_page_script_sources(
        &mut self,
        commands: &[NativeScriptCommand],
        max_scripts: usize,
        max_source_bytes: usize,
    ) -> Vec<NativePageScriptSource> {
        let mut roots = Vec::new();
        let mut realm_started = BTreeSet::new();
        for command in commands {
            let node_index = match command {
                NativeScriptCommand::CreateElement {
                    node_index,
                    tag_name,
                    ..
                } if tag_name.eq_ignore_ascii_case("script") => Some(*node_index),
                NativeScriptCommand::AppendChild { child_index, .. }
                | NativeScriptCommand::InsertBefore { child_index, .. } => Some(*child_index),
                NativeScriptCommand::StartScript { node_index } => {
                    let id = self
                        .script_node_ids
                        .get(node_index)
                        .copied()
                        .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, *node_index));
                    realm_started.insert(id.index());
                    Some(*node_index)
                }
                _ => None,
            };
            let Some(node_index) = node_index else {
                continue;
            };
            let id = self
                .script_node_ids
                .get(&node_index)
                .copied()
                .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, node_index));
            if !roots.contains(&id) {
                roots.push(id);
            }
        }

        let mut candidate_nodes = BTreeSet::new();
        for root in roots {
            self.collect_attached_script_nodes(root, &mut candidate_nodes);
        }
        if candidate_nodes.is_empty() {
            return Vec::new();
        }

        let sources = self
            .page_script_sources(self.nodes.len(), max_source_bytes)
            .into_iter()
            .filter(|source| {
                let node_index = match source {
                    NativePageScriptSource::Inline { node_index, .. }
                    | NativePageScriptSource::External { node_index, .. }
                    | NativePageScriptSource::ModuleInline { node_index, .. }
                    | NativePageScriptSource::ModuleExternal { node_index, .. } => *node_index,
                };
                candidate_nodes.contains(&node_index)
                    && !self.started_script_nodes.contains(&node_index)
                    && !realm_started.contains(&node_index)
            })
            .take(max_scripts)
            .map(NativePageScriptSource::as_dynamic)
            .collect::<Vec<_>>();
        self.started_script_nodes.extend(candidate_nodes);
        sources
    }

    fn collect_attached_script_nodes(&self, id: NativeNodeId, output: &mut BTreeSet<u32>) {
        if !self.is_attached(id) {
            return;
        }
        let Some(node) = self.node(id) else {
            return;
        };
        if node.element_name() == Some("script") {
            output.insert(id.index());
        }
        for child in node.children() {
            self.collect_attached_script_nodes(*child, output);
        }
    }

    fn parent_element_index(&self, id: NativeNodeId) -> Option<u32> {
        let mut parent = self.node(id).and_then(NativeNode::parent);
        while let Some(parent_id) = parent {
            let parent_node = self.node(parent_id)?;
            if parent_node.element_name().is_some() {
                return Some(parent_id.index());
            }
            parent = parent_node.parent();
        }
        None
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
            NativeLocator::Css(selector) => {
                unique_match(super::css::selector_matches_in_document(self, selector)?)
            }
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
            Some(self.select_for_option(id)?)
        } else {
            None
        };

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
                let multiple = self
                    .node(select_id)
                    .is_some_and(|node| node.attribute("multiple").is_some());
                let mut selection_changed = false;
                for option_id in option_ids {
                    let should_be_selected = if multiple {
                        if option_id == id {
                            !self
                                .node(option_id)
                                .ok_or(NativeEngineError::DetachedTarget)?
                                .state
                                .selected
                        } else {
                            self.node(option_id)
                                .ok_or(NativeEngineError::DetachedTarget)?
                                .state
                                .selected
                        }
                    } else {
                        option_id == id
                    };
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

    /// Deliver the pointer-enter events for one visible element.
    ///
    /// Pointer movement is represented as a semantic action rather than raw
    /// coordinates here. Layout hit testing and target routing are performed
    /// by the engine/backend; the document owns the DOM event order.
    pub(crate) fn apply_hover(
        &self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name().is_none() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hover requires an element target".into(),
            });
        }
        if self.is_hidden(id) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets are not actionable".into(),
            });
        }
        if !self
            .computed_style_for_layout(id)
            .pointer_events()
            .allows_hit_testing()
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "target does not accept pointer events".into(),
            });
        }
        Ok(vec![
            (id, NativeEventKind::MouseOver),
            (id, NativeEventKind::MouseEnter),
        ])
    }

    /// Deliver the DOM drag event sequence for one visible source and target.
    ///
    /// The current native action is semantic, so it intentionally does not
    /// synthesize coordinates or a `DataTransfer` object. It does preserve the
    /// browser-visible event ordering and bubbling/cancellation metadata.
    pub(crate) fn apply_drag(
        &self,
        source: NativeNodeId,
        destination: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        for (label, id) in [("drag source", source), ("drag destination", destination)] {
            let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
            if node.element_name().is_none() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: format!("{label} requires an element target"),
                });
            }
            if self.is_hidden(id) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: format!("hidden {label} is not actionable"),
                });
            }
            if !self
                .computed_style_for_layout(id)
                .pointer_events()
                .allows_hit_testing()
            {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: format!("{label} does not accept pointer events"),
                });
            }
        }
        Ok(vec![
            (source, NativeEventKind::DragStart),
            (destination, NativeEventKind::DragEnter),
            (destination, NativeEventKind::DragOver),
            (destination, NativeEventKind::Drop),
            (source, NativeEventKind::DragEnd),
        ])
    }

    /// Attach bounded file objects to one native file input and return the
    /// browser-visible input/change event order. File selection is a direct
    /// browser-owner operation, so it remains valid for hidden file inputs;
    /// the file contents never become DOM attributes or locator text.
    pub(crate) fn apply_upload(
        &mut self,
        id: NativeNodeId,
        files: &[NativeFile],
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        NativeFile::validate_many(files)?;
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("input")
            || !node
                .attribute("type")
                .unwrap_or("text")
                .eq_ignore_ascii_case("file")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "upload requires an input type=file control".into(),
            });
        }
        if self.is_disabled(id) {
            return Err(NativeEngineError::DisabledTarget);
        }
        if node.attribute("multiple").is_none() && files.len() > 1 {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "file input does not allow multiple files".into(),
            });
        }
        let fake_path = format!(r"C:\fakepath\{}", files[0].name);
        let state = &mut self
            .node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state;
        state.files = files.to_vec();
        state.value = Some(fake_path);
        Ok(vec![
            (id, NativeEventKind::Input),
            (id, NativeEventKind::Change),
        ])
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
        self.set_selection_state(id, text.chars().count(), text.chars().count(), "none")?;
        events.push((id, NativeEventKind::Input));
        events.push((id, NativeEventKind::Change));
        Ok(events)
    }

    /// Clear a supported editable control and return its native event order.
    /// An already-empty control remains a successful, focus-preserving no-op.
    pub(crate) fn apply_clear(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported semantic text-control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot be cleared".into(),
            });
        }
        if semantic.role != "textbox" || !matches!(semantic.tag_name.as_str(), "input" | "textarea")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "clear requires an input or textarea textbox".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.read_only {
            return Err(NativeEngineError::ReadOnlyTarget);
        }

        let mut events = self.focus_element(id);
        let current = self.current_value(id).unwrap_or_default();
        if !current.is_empty() {
            self.node_mut(id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .value = Some(String::new());
            self.set_selection_state(id, 0, 0, "none")?;
            events.push((id, NativeEventKind::Input));
            events.push((id, NativeEventKind::Change));
        }
        Ok(events)
    }

    /// Return the checked state of one visible, enabled checkbox or radio.
    pub(crate) fn checked_control_state(
        &self,
        id: NativeNodeId,
    ) -> Result<bool, NativeEngineError> {
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported checkable control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot be checked".into(),
            });
        }
        if !matches!(semantic.role.as_str(), "checkbox" | "radio") || semantic.tag_name != "input" {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "check and uncheck require a checkbox or radio input".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        Ok(semantic.checked.unwrap_or(false))
    }

    /// Select one exact option value and return its native event order.
    pub(crate) fn apply_select(
        &mut self,
        id: NativeNodeId,
        value: &str,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        if value.is_empty() || value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::invalid(
                "select value",
                "must be 1..=16384 bytes",
            ));
        }
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported select control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot be selected".into(),
            });
        }
        if semantic.role != "combobox" || semantic.tag_name != "select" {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "select requires a select control".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        let selected_option_id = self
            .select_option_ids(id)
            .into_iter()
            .find(|option_id| {
                self.option_value(*option_id).as_deref() == Some(value)
                    && !self.is_disabled(*option_id)
            })
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "select option value was not found or is disabled".into(),
            })?;

        let mut events = self.focus_element(id);
        let option_ids = self.select_option_ids(id);
        let mut changed = false;
        for option_id in option_ids {
            let should_be_selected = option_id == selected_option_id;
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
                changed = true;
            }
        }
        if changed {
            events.push((id, NativeEventKind::Input));
            events.push((id, NativeEventKind::Change));
        }
        Ok(events)
    }

    pub(crate) fn focused_text_control(&self) -> Result<NativeNodeId, NativeEngineError> {
        self.nodes
            .iter()
            .find(|node| {
                self.is_attached(node.id())
                    && node.state.focused
                    && matches!(node.element_name(), Some("input" | "textarea"))
                    && self
                        .semantic_node(node.id())
                        .is_some_and(|semantic| semantic.role == "textbox")
            })
            .map(NativeNode::id)
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "key press requires a focused text control".into(),
            })
    }

    /// Return the active page target for a raw key event. Browsers deliver
    /// keyboard events to the focused element and otherwise to the document;
    /// the native host uses the document root as that no-focus target.
    pub(crate) fn focused_node(&self) -> NativeNodeId {
        self.nodes
            .iter()
            .find(|node| self.is_attached(node.id()) && node.state.focused)
            .map(NativeNode::id)
            .unwrap_or(self.root)
    }

    /// Move focus through the bounded sequentially focusable controls in
    /// document order. Positive `tabindex` values are ordered before the
    /// natural zero-order controls; negative values are skipped.
    pub(crate) fn apply_tab_focus(
        &mut self,
        reverse: bool,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let mut focusable = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(order, node)| {
                if !self.is_attached(node.id())
                    || self.is_hidden(node.id())
                    || self.is_disabled(node.id())
                {
                    return None;
                }
                let role = self.semantic_role(node.id())?;
                if !matches!(
                    role,
                    "button" | "link" | "textbox" | "checkbox" | "radio" | "combobox"
                ) {
                    return None;
                }
                let tab_index = node
                    .attribute("tabindex")
                    .and_then(|value| value.parse::<i32>().ok())
                    .unwrap_or(0);
                (tab_index >= 0).then_some((tab_index, order, node.id()))
            })
            .collect::<Vec<_>>();
        focusable.sort_by_key(|(tab_index, order, _)| {
            (
                if *tab_index > 0 { 0 } else { 1 },
                (*tab_index).max(0),
                *order,
            )
        });
        let Some((current_index, _)) =
            focusable
                .iter()
                .enumerate()
                .find_map(|(index, (_, _, id))| {
                    self.node(*id)
                        .is_some_and(|node| node.state.focused)
                        .then_some((index, *id))
                })
        else {
            let Some((_, _, id)) = (if reverse {
                focusable.last().copied()
            } else {
                focusable.first().copied()
            }) else {
                return Ok(Vec::new());
            };
            return Ok(self.focus_element(id));
        };
        let next_index = if reverse {
            current_index.checked_sub(1).unwrap_or(focusable.len() - 1)
        } else {
            (current_index + 1) % focusable.len()
        };
        Ok(self.focus_element(focusable[next_index].2))
    }

    /// Return the bounded text-control selection as character offsets and a
    /// direction. Non-text elements have no selection API in the native host.
    fn selection_snapshot(&self, id: NativeNodeId) -> Option<(usize, usize, String)> {
        let node = self.node(id)?;
        if !matches!(node.element_name(), Some("input" | "textarea"))
            || !self
                .semantic_node(id)
                .is_some_and(|semantic| semantic.role == "textbox")
        {
            return None;
        }
        let length = self.current_value(id).unwrap_or_default().chars().count();
        let start = node.state.selection_start.unwrap_or(0).min(length);
        let end = node.state.selection_end.unwrap_or(start).min(length);
        Some((
            start.min(end),
            start.max(end),
            node.state
                .selection_direction
                .clone()
                .unwrap_or_else(|| "none".into()),
        ))
    }

    fn set_selection_state(
        &mut self,
        id: NativeNodeId,
        start: usize,
        end: usize,
        direction: &str,
    ) -> Result<(), NativeEngineError> {
        if !matches!(
            self.node(id).and_then(NativeNode::element_name),
            Some("input" | "textarea")
        ) || !self
            .semantic_node(id)
            .is_some_and(|semantic| semantic.role == "textbox")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "selection requires an input or textarea".into(),
            });
        }
        if !matches!(direction, "none" | "forward" | "backward") {
            return Err(NativeEngineError::invalid(
                "selection direction",
                "must be none, forward, or backward",
            ));
        }
        let length = self.current_value(id).unwrap_or_default().chars().count();
        let start = start.min(length);
        let end = end.min(length);
        let (start, end) = (start.min(end), start.max(end));
        let node = self.node_mut(id).ok_or(NativeEngineError::DetachedTarget)?;
        node.state.selection_start = Some(start);
        node.state.selection_end = Some(end);
        node.state.selection_direction = Some(direction.to_owned());
        Ok(())
    }

    fn initialize_selection_if_needed(&mut self, id: NativeNodeId) {
        let Some((length, needs_initialization)) = self.node(id).and_then(|node| {
            matches!(node.element_name(), Some("input" | "textarea")).then(|| {
                (
                    self.current_value(id).unwrap_or_default().chars().count(),
                    node.state.selection_start.is_none() || node.state.selection_end.is_none(),
                )
            })
        }) else {
            return;
        };
        if needs_initialization && let Some(node) = self.node_mut(id) {
            node.state.selection_start = Some(length);
            node.state.selection_end = Some(length);
            node.state.selection_direction = Some("none".into());
        }
    }

    /// Apply the bounded default action for one keyboard sequence. The
    /// returned input event is present only when the value changed; selection
    /// and caret movement themselves remain silent DOM state changes.
    pub(crate) fn apply_key_default(
        &mut self,
        id: NativeNodeId,
        key: &str,
        modifiers: i64,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        validate_native_key(key)?;
        let semantic = self
            .semantic_node(id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot receive key input".into(),
            });
        }
        if semantic.role != "textbox" || !matches!(semantic.tag_name.as_str(), "input" | "textarea")
        {
            return Ok(Vec::new());
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.read_only {
            return Err(NativeEngineError::ReadOnlyTarget);
        }
        if !self.node(id).is_some_and(|node| node.state.focused) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "key default target is not focused".into(),
            });
        }
        self.initialize_selection_if_needed(id);
        let (start, end, direction) = self
            .selection_snapshot(id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let value = self.current_value(id).unwrap_or_default();
        let characters = value.chars().collect::<Vec<_>>();
        let length = characters.len();
        let primary_modifier = modifiers & (2 | 4) != 0;
        let shift = modifiers & 8 != 0;
        let alt = modifiers & 1 != 0;

        if primary_modifier && key.eq_ignore_ascii_case("a") {
            self.set_selection_state(id, 0, length, "forward")?;
            return Ok(Vec::new());
        }
        if primary_modifier || alt {
            return Ok(Vec::new());
        }

        if matches!(key, "ArrowLeft" | "ArrowRight" | "Home" | "End") {
            let current_focus = if direction == "backward" { start } else { end };
            let focus = match key {
                "ArrowLeft" => current_focus.saturating_sub(1),
                "ArrowRight" => (current_focus + 1).min(length),
                "Home" => 0,
                "End" => length,
                _ => unreachable!("matched edge key"),
            };
            if shift {
                let anchor = if direction == "backward" { end } else { start };
                let direction = if focus < anchor {
                    "backward"
                } else if focus > anchor {
                    "forward"
                } else {
                    "none"
                };
                self.set_selection_state(id, anchor, focus, direction)?;
            } else {
                let caret = if start != end {
                    if matches!(key, "ArrowLeft" | "Home") {
                        start
                    } else {
                        end
                    }
                } else {
                    focus
                };
                self.set_selection_state(id, caret, caret, "none")?;
            }
            return Ok(Vec::new());
        }

        let (next_value, next_caret) = if key.chars().count() == 1 {
            let mut next = Vec::with_capacity(length + 1);
            next.extend_from_slice(&characters[..start]);
            next.extend(key.chars());
            next.extend_from_slice(&characters[end..]);
            (next.into_iter().collect::<String>(), start + 1)
        } else if key == "Backspace" {
            let remove_start = if start != end {
                start
            } else {
                start.saturating_sub(1)
            };
            let remove_end = if start != end { end } else { start };
            if remove_start == remove_end {
                self.set_selection_state(id, start, start, "none")?;
                return Ok(Vec::new());
            }
            let mut next = Vec::with_capacity(length);
            next.extend_from_slice(&characters[..remove_start]);
            next.extend_from_slice(&characters[remove_end..]);
            (next.into_iter().collect::<String>(), remove_start)
        } else if key == "Delete" {
            let remove_start = start;
            let remove_end = if start != end {
                end
            } else {
                (start + 1).min(length)
            };
            if remove_start == remove_end {
                self.set_selection_state(id, start, start, "none")?;
                return Ok(Vec::new());
            }
            let mut next = Vec::with_capacity(length);
            next.extend_from_slice(&characters[..remove_start]);
            next.extend_from_slice(&characters[remove_end..]);
            (next.into_iter().collect::<String>(), remove_start)
        } else {
            return Ok(Vec::new());
        };
        if next_value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "native text value",
                MAX_LOCATOR_BYTES,
                next_value.len(),
            ));
        }
        self.node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .value = Some(next_value);
        self.set_selection_state(id, next_caret, next_caret, "none")?;
        Ok(vec![(id, NativeEventKind::Input)])
    }

    /// Apply the bounded default edit for one key to a focused text control.
    /// Selection, caret movement, composition, and form submission are kept
    /// out of this slice; printable keys append at the current value end.
    pub(crate) fn apply_key_press(
        &mut self,
        id: NativeNodeId,
        key: &str,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        validate_native_edit_key(key)?;
        self.apply_key_default(id, key, 0)
    }

    /// Apply validated commands emitted by one JavaScript evaluation.
    /// Commands run against the caller's document clone so the engine can
    /// commit the complete script batch as one revision.
    pub(crate) fn apply_script_commands(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        self.apply_script_commands_with_link_policy(commands, false)
    }

    pub(crate) fn apply_script_commands_allowing_links(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        self.apply_script_commands_with_link_policy(commands, true)
    }

    fn apply_script_commands_with_link_policy(
        &mut self,
        commands: &[NativeScriptCommand],
        allow_script_navigation: bool,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let mut events = Vec::new();
        let mut script_nodes = self.script_node_ids.clone();
        for command in commands {
            match command {
                NativeScriptCommand::Focus { node_index } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    events.extend(self.apply_script_focus(id)?);
                }
                NativeScriptCommand::Blur { node_index } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    events.extend(self.apply_script_blur(id)?);
                }
                NativeScriptCommand::Click { node_index } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    if !allow_script_navigation
                        && self.link_href(id).is_some_and(|href| !href.is_empty())
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-driven link navigation is not available".into(),
                        });
                    }
                    events.extend(self.apply_click(id)?);
                }
                NativeScriptCommand::SubmitForm { node_index } => {
                    if !allow_script_navigation {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "script-driven form submission is not available in this event phase"
                                    .into(),
                        });
                    }
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    if self
                        .node(id)
                        .is_none_or(|node| node.element_name() != Some("form"))
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script form submission target is not a form".into(),
                        });
                    }
                }
                NativeScriptCommand::RequestSubmitForm {
                    node_index,
                    submitter_index,
                } => {
                    if !allow_script_navigation {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "script-driven form submission is not available in this event phase"
                                    .into(),
                        });
                    }
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    if self
                        .node(id)
                        .is_none_or(|node| node.element_name() != Some("form"))
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script form submission target is not a form".into(),
                        });
                    }
                    if let Some(submitter_index) = submitter_index {
                        let submitter = NativeNodeId::from_parts(self.generation, *submitter_index);
                        if self.submit_control_form(submitter) != Some(id) {
                            return Err(NativeEngineError::TargetNotActionable {
                                reason:
                                    "requestSubmit submitter must be a submit control for the form"
                                        .into(),
                            });
                        }
                    }
                }
                NativeScriptCommand::Fetch { .. } => {}
                NativeScriptCommand::FontFaceInstall {
                    request_id,
                    family,
                    weight,
                    style,
                    stretch,
                    unicode_range,
                    variant,
                    feature_settings,
                    size_adjust,
                    ascent_override,
                    descent_override,
                    line_gap_override,
                    display,
                    variation_settings,
                    body_base64,
                } => {
                    self.apply_script_font_face_install(
                        *request_id,
                        family,
                        weight,
                        style,
                        stretch,
                        unicode_range,
                        variant,
                        feature_settings,
                        size_adjust,
                        ascent_override,
                        descent_override,
                        line_gap_override,
                        display,
                        variation_settings,
                        body_base64,
                    )?;
                }
                NativeScriptCommand::ServiceWorkerRegister { .. }
                | NativeScriptCommand::ServiceWorkerUnregister { .. }
                | NativeScriptCommand::ServiceWorkerUpdate { .. }
                | NativeScriptCommand::ServiceWorkerSkipWaiting { .. }
                | NativeScriptCommand::ServiceWorkerClientsClaim { .. }
                | NativeScriptCommand::ServiceWorkerPostMessage { .. }
                | NativeScriptCommand::ServiceWorkerClientPostMessage { .. }
                | NativeScriptCommand::ServiceWorkerCacheOpen { .. }
                | NativeScriptCommand::ServiceWorkerCacheDelete { .. }
                | NativeScriptCommand::ServiceWorkerCacheHas { .. }
                | NativeScriptCommand::ServiceWorkerCacheKeys { .. }
                | NativeScriptCommand::ServiceWorkerCacheMatch { .. }
                | NativeScriptCommand::ServiceWorkerCachePut { .. }
                | NativeScriptCommand::ServiceWorkerCachePutAll { .. }
                | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { .. }
                | NativeScriptCommand::ServiceWorkerCacheEntries { .. }
                | NativeScriptCommand::ServiceWorkerOpenWindow { .. }
                | NativeScriptCommand::MessagePortPostMessage { .. } => {}
                NativeScriptCommand::SharedWorkerCreate { .. }
                | NativeScriptCommand::WorkerCreate { .. }
                | NativeScriptCommand::WorkerPostMessage { .. }
                | NativeScriptCommand::WorkerTerminate { .. }
                | NativeScriptCommand::WorkerClose { .. } => {}
                NativeScriptCommand::WebSocketOpen { .. }
                | NativeScriptCommand::WebSocketSend { .. }
                | NativeScriptCommand::WebSocketClose { .. }
                | NativeScriptCommand::EventSourceOpen { .. }
                | NativeScriptCommand::EventSourceClose { .. }
                | NativeScriptCommand::FetchStreamRead { .. }
                | NativeScriptCommand::FetchStreamCancel { .. }
                | NativeScriptCommand::FetchUploadChunk { .. }
                | NativeScriptCommand::FetchUploadEnd { .. }
                | NativeScriptCommand::FetchUploadError { .. }
                | NativeScriptCommand::FetchUploadCancel { .. } => {}
                NativeScriptCommand::Navigate { .. } => {
                    if !allow_script_navigation {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "script location navigation is not available in this event phase"
                                    .into(),
                        });
                    }
                }
                NativeScriptCommand::HistoryPushState { .. }
                | NativeScriptCommand::HistoryReplaceState { .. }
                | NativeScriptCommand::HistoryGo { .. }
                | NativeScriptCommand::ScrollTo { .. } => {}
                NativeScriptCommand::StorageSet { .. }
                | NativeScriptCommand::StorageRemove { .. }
                | NativeScriptCommand::StorageClear { .. }
                | NativeScriptCommand::CookieSet { .. }
                | NativeScriptCommand::Dialog { .. }
                | NativeScriptCommand::OpenWindow { .. }
                | NativeScriptCommand::SetWindowName { .. }
                | NativeScriptCommand::CloseWindow { .. }
                | NativeScriptCommand::NavigateWindow { .. }
                | NativeScriptCommand::PostMessage { .. }
                | NativeScriptCommand::FrameScriptBatch { .. }
                | NativeScriptCommand::FrameScript { .. } => {}
                NativeScriptCommand::ClearFileInput { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    let node = self
                        .script_node(id, &script_nodes)
                        .ok_or(NativeEngineError::DetachedTarget)?;
                    if node.element_name() != Some("input")
                        || !node
                            .attribute("type")
                            .unwrap_or("text")
                            .eq_ignore_ascii_case("file")
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "clear file input requires an input type=file control".into(),
                        });
                    }
                    let node = self
                        .script_node_mut(id, &script_nodes)
                        .ok_or(NativeEngineError::DetachedTarget)?;
                    node.state.files.clear();
                    node.state.value = Some(String::new());
                }
                NativeScriptCommand::SetValue { node_index, value } => {
                    self.apply_script_value(*node_index, value, &script_nodes)?;
                }
                NativeScriptCommand::SetSelection {
                    node_index,
                    start,
                    end,
                    direction,
                } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    self.set_selection_state(id, *start, *end, direction)?;
                }
                NativeScriptCommand::CanvasCommit {
                    node_index,
                    width,
                    height,
                    pixels_base64,
                    origin_clean,
                } => {
                    self.apply_script_canvas(
                        *node_index,
                        *width,
                        *height,
                        pixels_base64,
                        *origin_clean,
                        &script_nodes,
                    )?;
                }
                NativeScriptCommand::MediaLoad { node_index } => {
                    self.reset_media_load(*node_index, &script_nodes)?;
                }
                NativeScriptCommand::SetChecked {
                    node_index,
                    checked,
                } => {
                    self.apply_script_checked(*node_index, *checked, &script_nodes)?;
                }
                NativeScriptCommand::SetSelected {
                    node_index,
                    selected,
                } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    self.apply_script_selected(id, *selected)?;
                }
                NativeScriptCommand::SetAttribute {
                    node_index,
                    name,
                    value,
                    namespace_uri,
                } => {
                    self.apply_script_attribute(
                        *node_index,
                        name,
                        value,
                        namespace_uri.as_deref(),
                        &script_nodes,
                    )?;
                }
                NativeScriptCommand::RemoveAttribute {
                    node_index,
                    name,
                    namespace_uri,
                } => {
                    self.remove_script_attribute(
                        *node_index,
                        name,
                        namespace_uri.as_deref(),
                        &script_nodes,
                    )?;
                }
                NativeScriptCommand::SetTextContent { node_index, value } => {
                    self.apply_script_text_content(*node_index, value, &script_nodes)?;
                }
                NativeScriptCommand::SetDocumentTitle { value } => {
                    self.apply_script_document_title(value)?;
                }
                NativeScriptCommand::SetInnerHtml { node_index, value } => {
                    self.apply_script_inner_html(*node_index, value, &script_nodes)?;
                }
                NativeScriptCommand::RemoveNode { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    self.apply_script_remove_node(id, &script_nodes)?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateElement {
                    node_index,
                    tag_name,
                    namespace_uri,
                } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    let name = validate_script_element_name(tag_name)?;
                    let namespace_uri = match namespace_uri.as_deref() {
                        None => Some(HTML_NAMESPACE_URI.to_owned()),
                        Some("") => None,
                        Some(value) => Some(validate_namespace_uri(value)?),
                    };
                    let id = self.add_detached_node(
                        NativeNodeKind::Element {
                            name,
                            attributes: BTreeMap::new(),
                        },
                        self.max_nodes,
                    )?;
                    if let Some(node) = self.raw_node_mut(id) {
                        node.state.namespace_uri = namespace_uri;
                    }
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateTextNode { node_index, value } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    if value.len() > MAX_LOCATOR_BYTES {
                        return Err(NativeEngineError::limit(
                            "script text node",
                            MAX_LOCATOR_BYTES,
                            value.len(),
                        ));
                    }
                    let id = self
                        .add_detached_node(NativeNodeKind::Text(value.clone()), self.max_nodes)?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateComment { node_index, value } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    if value.len() > MAX_LOCATOR_BYTES {
                        return Err(NativeEngineError::limit(
                            "script comment node",
                            MAX_LOCATOR_BYTES,
                            value.len(),
                        ));
                    }
                    let id = self.add_detached_node(
                        NativeNodeKind::Comment(value.clone()),
                        self.max_nodes,
                    )?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateDocumentType {
                    node_index,
                    name,
                    public_id,
                    system_id,
                } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    let name = validate_script_element_name(name)?;
                    for (label, value) in [
                        ("public identifier", public_id),
                        ("system identifier", system_id),
                    ] {
                        if value.len() > MAX_LOCATOR_BYTES {
                            return Err(NativeEngineError::limit(
                                format!("script document type {label}"),
                                MAX_LOCATOR_BYTES,
                                value.len(),
                            ));
                        }
                    }
                    let id = self.add_detached_node(
                        NativeNodeKind::DocumentType {
                            name,
                            public_id: (!public_id.is_empty()).then(|| public_id.clone()),
                            system_id: (!system_id.is_empty()).then(|| system_id.clone()),
                        },
                        self.max_nodes,
                    )?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::AppendChild {
                    parent_index,
                    child_index,
                } => {
                    let parent = self.resolve_script_node_id(*parent_index, &script_nodes);
                    let child = self.resolve_script_node_id(*child_index, &script_nodes);
                    self.append_script_child(parent, child, None, &script_nodes)?;
                }
                NativeScriptCommand::InsertBefore {
                    parent_index,
                    child_index,
                    before_index,
                } => {
                    let parent = self.resolve_script_node_id(*parent_index, &script_nodes);
                    let child = self.resolve_script_node_id(*child_index, &script_nodes);
                    let before =
                        before_index.map(|index| self.resolve_script_node_id(index, &script_nodes));
                    self.append_script_child(parent, child, before, &script_nodes)?;
                }
                NativeScriptCommand::StartScript { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    if self
                        .script_node(id, &script_nodes)
                        .and_then(NativeNode::element_name)
                        != Some("script")
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-start marker must target a script element".into(),
                        });
                    }
                }
                NativeScriptCommand::SetCustomValidity {
                    node_index,
                    message,
                } => {
                    self.apply_script_custom_validity(*node_index, message, &script_nodes)?;
                }
                NativeScriptCommand::CheckValidity { node_index }
                | NativeScriptCommand::ReportValidity { node_index } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    events.extend(self.validation_events(id)?);
                }
            }
            if events.len() > super::interaction::MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "script mutation effects",
                    super::interaction::MAX_NATIVE_EFFECTS,
                    events.len(),
                ));
            }
        }
        self.script_node_ids = script_nodes
            .into_iter()
            .filter(|(node_index, _)| *node_index >= SCRIPT_TEMP_NODE_BASE)
            .collect();
        Ok(events)
    }

    fn reset_media_load(
        &mut self,
        node_index: u32,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let node_id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node(node_id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("audio") && node.element_name() != Some("video") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "media load requires an audio or video element".into(),
            });
        }
        let source = self.selected_media_source(node_id);
        self.media_resources.remove(&node_index);
        self.media_errors.remove(&node_index);
        self.media_loads.remove(&node_index);
        if let Some(source) = source.filter(|source| !source.is_empty()) {
            self.media_loads.insert(node_index, source);
        }
        Ok(())
    }

    fn apply_script_canvas(
        &mut self,
        node_index: u32,
        width: u32,
        height: u32,
        pixels_base64: &str,
        origin_clean: bool,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if width == 0
            || height == 0
            || width > MAX_NATIVE_CANVAS_DIMENSION
            || height > MAX_NATIVE_CANVAS_DIMENSION
        {
            return Err(NativeEngineError::invalid(
                "script canvas dimensions",
                format!("must be between 1 and {MAX_NATIVE_CANVAS_DIMENSION} pixels per axis"),
            ));
        }
        let expected_bytes = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "script canvas pixels",
                    MAX_NATIVE_CANVAS_BYTES,
                    usize::MAX,
                )
            })?;
        if expected_bytes > MAX_NATIVE_CANVAS_BYTES {
            return Err(NativeEngineError::limit(
                "script canvas pixels",
                MAX_NATIVE_CANVAS_BYTES,
                expected_bytes,
            ));
        }
        let encoded_limit =
            (MAX_NATIVE_CANVAS_BYTES.saturating_add(2).saturating_div(3)).saturating_mul(4);
        if pixels_base64.len() > encoded_limit {
            return Err(NativeEngineError::limit(
                "script canvas pixels",
                encoded_limit,
                pixels_base64.len(),
            ));
        }
        let pixels = base64::engine::general_purpose::STANDARD
            .decode(pixels_base64)
            .map_err(|_| NativeEngineError::invalid("script canvas pixels", "invalid base64"))?;
        if pixels.len() != expected_bytes {
            return Err(NativeEngineError::invalid(
                "script canvas pixels",
                "pixel data does not match the canvas dimensions",
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("canvas") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "canvas commit requires a canvas element".into(),
            });
        }
        self.canvas_resources.insert(
            id.index(),
            NativeCanvasResource {
                width,
                height,
                pixels,
                origin_clean,
            },
        );
        Ok(())
    }

    fn apply_script_custom_validity(
        &mut self,
        node_index: u32,
        message: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if message.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "custom validity message",
                MAX_LOCATOR_BYTES,
                message.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if !matches!(
            node.element_name(),
            Some("button" | "input" | "select" | "textarea")
        ) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "custom validity requires a form-associated control".into(),
            });
        }
        node.state.custom_validity = message.to_owned();
        Ok(())
    }

    fn validation_events(
        &self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() == Some("form") {
            return Ok(self
                .invalid_form_controls_for_api(id)?
                .into_iter()
                .map(|id| (id, NativeEventKind::Invalid))
                .collect());
        }
        let (validity, _, will_validate) = self.script_validation_snapshot(id);
        if !will_validate || validity.valid {
            return Ok(Vec::new());
        }
        Ok(vec![(id, NativeEventKind::Invalid)])
    }

    pub(crate) fn apply_script_focus(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let Some(_node) = self.node(id) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported semantic control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot receive focus".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if !matches!(
            semantic.role.as_str(),
            "button" | "link" | "checkbox" | "radio" | "textbox" | "combobox" | "option"
        ) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "only supported semantic controls can receive focus".into(),
            });
        }
        Ok(self.focus_element(id))
    }

    pub(crate) fn apply_script_blur(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if !node.state.focused {
            return Ok(Vec::new());
        }
        self.node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .focused = false;
        Ok(vec![(id, NativeEventKind::Blur)])
    }

    fn apply_script_value(
        &mut self,
        node_index: u32,
        value: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script value",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let element_name = self
            .script_node(id, script_nodes)
            .and_then(NativeNode::element_name)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if element_name == "select" {
            let matching = self.select_option_ids(id).into_iter().find(|option_id| {
                self.option_value(*option_id)
                    .is_some_and(|option_value| option_value == value)
            });
            for option_id in self.select_option_ids(id) {
                self.node_mut(option_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .selected = matching == Some(option_id);
            }
            return Ok(());
        }
        if element_name == "input"
            && self
                .node(id)
                .and_then(|node| node.attribute("type"))
                .is_some_and(|input_type| input_type.eq_ignore_ascii_case("file"))
        {
            if !value.is_empty() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "script cannot assign a non-empty file input value".into(),
                });
            }
            let node = self
                .script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            node.state.files.clear();
            node.state.value = Some(String::new());
            return Ok(());
        }
        if !matches!(element_name, "input" | "textarea") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script value requires an input or textarea".into(),
            });
        }
        {
            let node = self
                .script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            node.state.value = Some(value.to_owned());
        }
        if self
            .semantic_node(id)
            .is_some_and(|semantic| semantic.role == "textbox")
        {
            let end = value.chars().count();
            self.set_selection_state(id, end, end, "none")?;
        }
        Ok(())
    }

    fn apply_script_checked(
        &mut self,
        node_index: u32,
        checked: bool,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("input")
            || !matches!(
                node.attribute("type").unwrap_or("text"),
                "checkbox" | "radio"
            )
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script checked state requires a checkbox or radio input".into(),
            });
        }
        if checked && node.attribute("type") == Some("radio") {
            let group_name = node.attribute("name").map(str::to_owned);
            let radio_ids = self
                .nodes
                .iter()
                .filter(|candidate| {
                    candidate.element_name() == Some("input")
                        && candidate.attribute("type") == Some("radio")
                })
                .filter(|candidate| {
                    candidate.id() == id
                        || group_name.as_deref().is_some_and(|name| {
                            !name.is_empty() && candidate.attribute("name") == Some(name)
                        })
                })
                .map(NativeNode::id)
                .collect::<Vec<_>>();
            for radio_id in radio_ids {
                self.node_mut(radio_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .checked = radio_id == id;
            }
        } else {
            self.script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .checked = checked;
        }
        Ok(())
    }

    fn apply_script_selected(
        &mut self,
        id: NativeNodeId,
        selected: bool,
    ) -> Result<(), NativeEngineError> {
        if self.node(id).and_then(NativeNode::element_name) != Some("option") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script selected state requires an option".into(),
            });
        }
        let select_id = self.select_for_option(id)?;
        let multiple = self
            .node(select_id)
            .is_some_and(|node| node.attribute("multiple").is_some());
        if selected && multiple {
            self.node_mut(id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .selected = true;
        } else if selected {
            for option_id in self.select_option_ids(select_id) {
                self.node_mut(option_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .selected = option_id == id;
            }
        } else {
            self.node_mut(id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .selected = false;
        }
        Ok(())
    }

    fn apply_script_attribute(
        &mut self,
        node_index: u32,
        name: &str,
        value: &str,
        namespace_uri: Option<&str>,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let name = validate_script_attribute(name)?;
        let namespace_uri = match namespace_uri {
            None | Some("") => None,
            Some(value) => Some(validate_attribute_namespace_uri(value)?),
        };
        if value.len() > MAX_ATTRIBUTE_BYTES {
            return Err(NativeEngineError::limit(
                "script attribute value",
                MAX_ATTRIBUTE_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let NativeNodeKind::Element { attributes, .. } = &mut node.kind else {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script attributes require an element".into(),
            });
        };
        attributes.insert(name.clone(), value.to_owned());
        if let Some(namespace_uri) = namespace_uri {
            node.state
                .attribute_namespaces
                .insert(name.clone(), namespace_uri);
        } else {
            node.state.attribute_namespaces.remove(&name);
        }
        Ok(())
    }

    fn remove_script_attribute(
        &mut self,
        node_index: u32,
        name: &str,
        namespace_uri: Option<&str>,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let name = validate_script_attribute(name)?;
        let namespace_uri = match namespace_uri {
            None | Some("") => None,
            Some(value) => Some(validate_attribute_namespace_uri(value)?),
        };
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let node = self
            .script_node_mut(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let NativeNodeKind::Element { attributes, .. } = &mut node.kind else {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script attributes require an element".into(),
            });
        };
        if namespace_uri.as_deref()
            != node
                .state
                .attribute_namespaces
                .get(&name)
                .map(String::as_str)
            && namespace_uri.is_some()
        {
            return Ok(());
        }
        attributes.remove(&name);
        node.state.attribute_namespaces.remove(&name);
        Ok(())
    }

    fn apply_script_text_content(
        &mut self,
        node_index: u32,
        value: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script text content",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let kind = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?
            .kind()
            .clone();
        if matches!(kind, NativeNodeKind::Text(_) | NativeNodeKind::Comment(_)) {
            let node = self
                .script_node_mut(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            node.kind = match node.kind() {
                NativeNodeKind::Comment(_) => NativeNodeKind::Comment(value.to_owned()),
                _ => NativeNodeKind::Text(value.to_owned()),
            };
            return Ok(());
        }
        if !matches!(kind, NativeNodeKind::Element { .. }) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script text content requires an element".into(),
            });
        }
        let children = self
            .script_node(id, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?
            .children
            .to_vec();
        for child in children {
            self.detach_subtree(child)?;
        }
        if !value.is_empty() {
            self.add_node(id, NativeNodeKind::Text(value.to_owned()), self.max_nodes)?;
        }
        Ok(())
    }

    fn apply_script_document_title(&mut self, value: &str) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script document title",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let title_id = if let Some(title_id) = self.find_element(self.root, "title") {
            title_id
        } else {
            let parent = if let Some(head_id) = self.find_element(self.root, "head") {
                head_id
            } else if let Some(html_id) = self.find_element(self.root, "html") {
                if self.element_depth(html_id) >= self.max_dom_depth {
                    return Err(NativeEngineError::limit(
                        "DOM depth",
                        self.max_dom_depth,
                        self.element_depth(html_id).saturating_add(1),
                    ));
                }
                self.add_node(
                    html_id,
                    NativeNodeKind::Element {
                        name: "head".into(),
                        attributes: BTreeMap::new(),
                    },
                    self.max_nodes,
                )?
            } else {
                self.root
            };
            if parent != self.root && self.element_depth(parent) >= self.max_dom_depth {
                return Err(NativeEngineError::limit(
                    "DOM depth",
                    self.max_dom_depth,
                    self.element_depth(parent).saturating_add(1),
                ));
            }
            self.add_node(
                parent,
                NativeNodeKind::Element {
                    name: "title".into(),
                    attributes: BTreeMap::new(),
                },
                self.max_nodes,
            )?
        };
        self.apply_script_text_content(title_id.index(), value, &BTreeMap::new())
    }

    fn apply_script_inner_html(
        &mut self,
        node_index: u32,
        value: &str,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "script innerHTML",
                MAX_LOCATOR_BYTES,
                value.len(),
            ));
        }
        let id = self.resolve_script_node_id(node_index, script_nodes);
        let (target_name, old_children) = {
            let target = self
                .script_node(id, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            let Some(target_name) = target.element_name() else {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "script innerHTML requires an element".into(),
                });
            };
            (target_name.to_owned(), target.children().to_vec())
        };
        if target_name.is_empty() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script innerHTML requires an element".into(),
            });
        }
        let available_nodes = self.max_nodes.saturating_sub(self.nodes.len());
        let max_tokens = available_nodes.saturating_mul(2).saturating_add(1).max(1);
        let tokens = tokenize(value, max_tokens)?;
        let target_depth = self.element_depth(id);
        for child in old_children {
            self.detach_subtree(child)?;
        }
        if is_void_element(&target_name) {
            return Ok(());
        }

        let mut stack = vec![id];
        for token in tokens {
            match token {
                HtmlToken::StartTag {
                    name,
                    attributes,
                    self_closing,
                } => {
                    while stack.len() > 1
                        && stack.last().is_some_and(|current| {
                            self.raw_node(*current)
                                .and_then(NativeNode::element_name)
                                .is_some_and(|current_name| should_auto_close(current_name, &name))
                        })
                    {
                        stack.pop();
                    }
                    let current_depth = target_depth.saturating_add(stack.len().saturating_sub(1));
                    if current_depth >= self.max_dom_depth {
                        return Err(NativeEngineError::limit(
                            "DOM depth",
                            self.max_dom_depth,
                            current_depth.saturating_add(1),
                        ));
                    }
                    let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "fragment parser lost its element parent".into(),
                    })?;
                    let child = self.add_node(
                        parent,
                        NativeNodeKind::Element {
                            name: name.clone(),
                            attributes,
                        },
                        self.max_nodes,
                    )?;
                    if !self_closing && !is_void_element(&name) {
                        stack.push(child);
                    }
                }
                HtmlToken::EndTag(name) => {
                    if let Some(index) = stack.iter().rposition(|current| {
                        self.raw_node(*current)
                            .and_then(NativeNode::element_name)
                            .is_some_and(|current_name| current_name == name)
                    }) && index > 0
                    {
                        stack.truncate(index);
                    }
                }
                HtmlToken::Text(value) => {
                    if !value.is_empty() {
                        let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "fragment parser lost its text parent".into(),
                        })?;
                        self.add_node(
                            parent,
                            NativeNodeKind::Text(decode_entities(&value)),
                            self.max_nodes,
                        )?;
                    }
                }
                HtmlToken::RawText(value) => {
                    if !value.is_empty() {
                        let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                            offset: 0,
                            reason: "fragment parser lost its raw-text parent".into(),
                        })?;
                        self.add_node(parent, NativeNodeKind::Text(value), self.max_nodes)?;
                    }
                }
                HtmlToken::Comment(value) => {
                    let parent = *stack.last().ok_or_else(|| NativeEngineError::Parse {
                        offset: 0,
                        reason: "fragment parser lost its comment parent".into(),
                    })?;
                    self.add_node(parent, NativeNodeKind::Comment(value), self.max_nodes)?;
                }
                HtmlToken::Doctype { .. } => {}
            }
        }
        self.capture_attached_content_security_policy_meta();
        Ok(())
    }

    fn apply_script_remove_node(
        &mut self,
        id: NativeNodeId,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        if id == self.root {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "the document root cannot be removed".into(),
            });
        }
        let is_script_node = script_nodes.values().any(|candidate| *candidate == id);
        if (!is_script_node && self.node(id).is_none()) || self.raw_node(id).is_none() {
            return Err(NativeEngineError::DetachedTarget);
        }
        self.detach_subtree(id)
    }

    fn element_depth(&self, id: NativeNodeId) -> usize {
        let mut depth: usize = 0;
        let mut current = Some(id);
        while let Some(current_id) = current {
            let Some(node) = self.raw_node(current_id) else {
                break;
            };
            if node.element_name().is_some() {
                depth = depth.saturating_add(1);
            }
            current = node.parent();
        }
        depth
    }

    fn element_inner_html(&self, id: NativeNodeId, max_bytes: usize) -> String {
        let mut output = String::new();
        let mut truncated = false;
        let children = self
            .node(id)
            .map(NativeNode::children)
            .unwrap_or_default()
            .to_vec();
        for child in children {
            self.append_serialized_node(child, max_bytes, &mut output, &mut truncated, false);
            if truncated {
                break;
            }
        }
        output
    }

    fn append_serialized_node(
        &self,
        id: NativeNodeId,
        max_bytes: usize,
        output: &mut String,
        truncated: &mut bool,
        raw_text: bool,
    ) {
        if *truncated {
            return;
        }
        let Some(node) = self.node(id) else {
            return;
        };
        match node.kind() {
            NativeNodeKind::Comment(value) => {
                append_bounded_markup(output, "<!--", max_bytes, truncated);
                append_bounded_markup(output, value, max_bytes, truncated);
                append_bounded_markup(output, "-->", max_bytes, truncated);
            }
            NativeNodeKind::Text(value) => {
                let value = if raw_text {
                    value.clone()
                } else {
                    escape_html_text(value)
                };
                append_bounded_markup(output, &value, max_bytes, truncated);
            }
            NativeNodeKind::Document => {
                let children = node.children().to_vec();
                for child in children {
                    self.append_serialized_node(child, max_bytes, output, truncated, false);
                    if *truncated {
                        break;
                    }
                }
            }
            NativeNodeKind::DocumentType {
                name,
                public_id,
                system_id,
            } => {
                append_bounded_markup(output, "<!DOCTYPE ", max_bytes, truncated);
                append_bounded_markup(output, name, max_bytes, truncated);
                if let Some(public_id) = public_id {
                    append_bounded_markup(output, " PUBLIC \"", max_bytes, truncated);
                    append_bounded_markup(output, public_id, max_bytes, truncated);
                    append_bounded_markup(output, "\"", max_bytes, truncated);
                    if let Some(system_id) = system_id {
                        append_bounded_markup(output, " \"", max_bytes, truncated);
                        append_bounded_markup(output, system_id, max_bytes, truncated);
                        append_bounded_markup(output, "\"", max_bytes, truncated);
                    }
                } else if let Some(system_id) = system_id {
                    append_bounded_markup(output, " SYSTEM \"", max_bytes, truncated);
                    append_bounded_markup(output, system_id, max_bytes, truncated);
                    append_bounded_markup(output, "\"", max_bytes, truncated);
                }
                append_bounded_markup(output, ">", max_bytes, truncated);
            }
            NativeNodeKind::Element { name, attributes } => {
                append_bounded_markup(output, "<", max_bytes, truncated);
                append_bounded_markup(output, name, max_bytes, truncated);
                for (attribute, value) in attributes {
                    if *truncated {
                        break;
                    }
                    append_bounded_markup(output, " ", max_bytes, truncated);
                    append_bounded_markup(output, attribute, max_bytes, truncated);
                    append_bounded_markup(output, "=\"", max_bytes, truncated);
                    append_bounded_markup(
                        output,
                        &escape_html_attribute(value),
                        max_bytes,
                        truncated,
                    );
                    append_bounded_markup(output, "\"", max_bytes, truncated);
                }
                append_bounded_markup(output, ">", max_bytes, truncated);
                if *truncated || is_void_element(name) {
                    return;
                }
                let children = node.children().to_vec();
                let child_raw_text = matches!(name.as_str(), "script" | "style");
                for child in children {
                    self.append_serialized_node(
                        child,
                        max_bytes,
                        output,
                        truncated,
                        child_raw_text,
                    );
                    if *truncated {
                        return;
                    }
                }
                append_bounded_markup(output, "</", max_bytes, truncated);
                append_bounded_markup(output, name, max_bytes, truncated);
                append_bounded_markup(output, ">", max_bytes, truncated);
            }
        }
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
        if self.raw_node(parent).is_none() {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder referenced an unknown parent".into(),
            });
        }
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
            NativeNodeKind::Document
            | NativeNodeKind::DocumentType { .. }
            | NativeNodeKind::Comment(_)
            | NativeNodeKind::Text(_) => NativeElementState::default(),
        };
        self.nodes.push(NativeNode {
            id,
            parent: Some(parent),
            children: Vec::new(),
            kind,
            state,
        });
        let Some(parent_node) = self.raw_node_mut(parent) else {
            return Err(NativeEngineError::Parse {
                offset: 0,
                reason: "tree builder referenced an unknown parent".into(),
            });
        };
        parent_node.children.push(id);
        Ok(id)
    }

    fn add_detached_node(
        &mut self,
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
            NativeNodeKind::Document
            | NativeNodeKind::DocumentType { .. }
            | NativeNodeKind::Comment(_)
            | NativeNodeKind::Text(_) => NativeElementState::default(),
        };
        self.nodes.push(NativeNode {
            id,
            parent: None,
            children: Vec::new(),
            kind,
            state,
        });
        Ok(id)
    }

    fn raw_node(&self, id: NativeNodeId) -> Option<&NativeNode> {
        (id.generation == self.generation)
            .then(|| self.nodes.get(id.index as usize))
            .flatten()
    }

    fn raw_node_mut(&mut self, id: NativeNodeId) -> Option<&mut NativeNode> {
        (id.generation == self.generation)
            .then(|| self.nodes.get_mut(id.index as usize))
            .flatten()
    }

    fn is_attached(&self, id: NativeNodeId) -> bool {
        if id.generation != self.generation {
            return false;
        }
        let mut current = id;
        for _ in 0..=self.nodes.len() {
            if current == self.root {
                return true;
            }
            let Some(parent) = self.raw_node(current).and_then(NativeNode::parent) else {
                return false;
            };
            if parent == current {
                return false;
            }
            current = parent;
        }
        false
    }

    fn is_descendant_of_element(&self, id: NativeNodeId, wanted: &str) -> bool {
        let mut current = self.raw_node(id).and_then(NativeNode::parent);
        for _ in 0..=self.nodes.len() {
            let Some(current_id) = current else {
                return false;
            };
            let Some(node) = self.raw_node(current_id) else {
                return false;
            };
            if node.element_name() == Some(wanted) {
                return true;
            }
            current = node.parent();
        }
        false
    }

    fn node_mut(&mut self, id: NativeNodeId) -> Option<&mut NativeNode> {
        self.is_attached(id)
            .then(|| self.raw_node_mut(id))
            .flatten()
    }

    fn resolve_script_node_id(
        &self,
        node_index: u32,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> NativeNodeId {
        script_nodes
            .get(&node_index)
            .copied()
            .unwrap_or_else(|| NativeNodeId::from_parts(self.generation, node_index))
    }

    fn script_node<'a>(
        &'a self,
        id: NativeNodeId,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Option<&'a NativeNode> {
        if script_nodes.values().any(|candidate| *candidate == id) {
            self.raw_node(id)
        } else {
            self.node(id)
        }
    }

    fn script_node_mut<'a>(
        &'a mut self,
        id: NativeNodeId,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Option<&'a mut NativeNode> {
        if script_nodes.values().any(|candidate| *candidate == id) {
            self.raw_node_mut(id)
        } else {
            self.node_mut(id)
        }
    }

    fn detach_subtree(&mut self, id: NativeNodeId) -> Result<(), NativeEngineError> {
        let parent = self
            .raw_node(id)
            .and_then(NativeNode::parent)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let Some(parent_node) = self.raw_node_mut(parent) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        parent_node.children.retain(|child| *child != id);

        let mut pending = vec![id];
        let mut detached = Vec::new();
        while let Some(current) = pending.pop() {
            let children = self
                .raw_node(current)
                .ok_or(NativeEngineError::DetachedTarget)?
                .children
                .clone();
            pending.extend(children);
            detached.push(current);
        }
        for current in &detached {
            self.canvas_resources.remove(&current.index());
            self.media_resources.remove(&current.index());
            self.media_loads.remove(&current.index());
            self.media_errors.remove(&current.index());
        }
        for current in detached {
            let node = self
                .raw_node_mut(current)
                .ok_or(NativeEngineError::DetachedTarget)?;
            node.parent = None;
            node.children.clear();
            node.state.focused = false;
        }
        Ok(())
    }

    fn append_script_child(
        &mut self,
        parent: NativeNodeId,
        child: NativeNodeId,
        before: Option<NativeNodeId>,
        script_nodes: &BTreeMap<u32, NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        let parent_node = self
            .script_node(parent, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let parent_is_document = matches!(parent_node.kind(), NativeNodeKind::Document);
        if !parent_is_document && !matches!(parent_node.kind(), NativeNodeKind::Element { .. }) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "appendChild parent must be an element or document".into(),
            });
        }
        let child_node = self
            .script_node(child, script_nodes)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let child_is_document_type =
            matches!(child_node.kind(), NativeNodeKind::DocumentType { .. });
        if !matches!(
            child_node.kind(),
            NativeNodeKind::Element { .. } | NativeNodeKind::Text(_) | NativeNodeKind::Comment(_)
        ) && !(parent_is_document && child_is_document_type)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "appendChild cannot insert a document node".into(),
            });
        }
        if child_is_document_type {
            if !parent_is_document {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "document type must be a child of the document".into(),
                });
            }
            if parent_node
                .children()
                .iter()
                .filter_map(|id| self.script_node(*id, script_nodes))
                .any(|node| {
                    matches!(node.kind(), NativeNodeKind::DocumentType { .. }) && node.id() != child
                })
            {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "document cannot have more than one document type".into(),
                });
            }
        } else if parent_is_document {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "document only accepts a document type in this native surface".into(),
            });
        }
        if child == self.root || parent == child {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "appendChild would create a DOM hierarchy cycle".into(),
            });
        }
        if before == Some(child) {
            return Ok(());
        }
        if let Some(before) = before {
            let before_node = self
                .script_node(before, script_nodes)
                .ok_or(NativeEngineError::DetachedTarget)?;
            if before_node.parent() != Some(parent) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "insertBefore reference is not a child of the parent".into(),
                });
            }
        }

        let mut ancestor = Some(parent);
        for _ in 0..=self.nodes.len() {
            if ancestor == Some(child) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "appendChild would create a DOM hierarchy cycle".into(),
                });
            }
            let Some(current) = ancestor else {
                break;
            };
            ancestor = self.raw_node(current).and_then(NativeNode::parent);
        }

        let old_parent = self.raw_node(child).and_then(NativeNode::parent);
        if let Some(old_parent) = old_parent {
            let Some(old_parent_node) = self.raw_node_mut(old_parent) else {
                return Err(NativeEngineError::DetachedTarget);
            };
            old_parent_node
                .children
                .retain(|candidate| *candidate != child);
        }
        let Some(child_node) = self.raw_node_mut(child) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        child_node.parent = Some(parent);
        let Some(parent_node) = self.raw_node_mut(parent) else {
            return Err(NativeEngineError::DetachedTarget);
        };
        let insertion_index = before
            .and_then(|before| parent_node.children.iter().position(|id| *id == before))
            .unwrap_or(parent_node.children.len());
        parent_node.children.insert(insertion_index, child);
        self.capture_attached_content_security_policy_meta();
        Ok(())
    }

    fn normalize_select_defaults(&mut self) {
        let select_ids = self
            .nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
            .filter(|node| node.element_name() == Some("select"))
            .map(NativeNode::id)
            .collect::<Vec<_>>();
        for select_id in select_ids {
            let option_ids = self.select_option_ids(select_id);
            if self
                .node(select_id)
                .is_some_and(|node| node.attribute("multiple").is_some())
            {
                continue;
            }
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

    fn select_for_option(
        &self,
        option_id: NativeNodeId,
    ) -> Result<NativeNodeId, NativeEngineError> {
        self.find_ancestor_element(option_id, "select")
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "option must belong to a select control".into(),
            })
    }

    fn option_value(&self, option_id: NativeNodeId) -> Option<String> {
        let option = self.node(option_id)?;
        option.attribute("value").map(str::to_owned).or_else(|| {
            self.element_text(option_id, MAX_LOCATOR_BYTES)
                .map(|(value, _)| value)
        })
    }

    fn select_option_ids(&self, select_id: NativeNodeId) -> Vec<NativeNodeId> {
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()))
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
            .filter(|node| self.is_attached(node.id()))
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
        self.initialize_selection_if_needed(id);
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
            .then(|| self.current_value(id).is_none_or(|value| value.is_empty()))
            .or_else(|| (role == "file").then_some(node.state.files.is_empty()));
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
        let mut inherited_font_variant_ligatures = NativeFontVariantLigatures::default();
        let mut inherited_font_variant_caps = NativeFontVariantCaps::Normal;
        let mut inherited_font_variant_position = NativeFontVariantPosition::Normal;
        let mut inherited_font_variant_alternates = NativeFontVariantAlternates::Normal;
        let mut inherited_font_language_override = NativeFontLanguageOverride::default();
        let mut inherited_font_variation_settings = NativeFontVariationSettings::default();
        let mut inherited_font_variant_east_asian = NativeFontVariantEastAsian::default();
        let mut inherited_font_variant_numeric = NativeFontVariantNumeric::default();
        let mut inherited_font_feature_settings = NativeFontFeatureSettings::default();
        let mut inherited_font_kerning = NativeFontKerning::Auto;
        let mut inherited_font_optical_sizing = NativeFontOpticalSizing::Auto;
        let mut inherited_font_palette = NativeFontPalette::Normal;
        let mut inherited_font_weight = FontWeightValue::Normal;
        let mut inherited_font_style = FontStyleValue::Normal;
        let mut inherited_font_stretch = super::css::NativeFontStretchRange::default();
        let mut inherited_font_family = NativeFontFamilyList::default();
        let mut inherited_font_size = super::font::DEFAULT_NATIVE_FONT_SIZE;
        let mut inherited_word_break = WordBreakValue::Normal;
        let mut inherited_text_overflow = TextOverflowValue::Clip;
        let mut inherited_overflow_x = OverflowValue::Other;
        let mut inherited_overflow_y = OverflowValue::Other;
        let mut inherited_vertical_align = VerticalAlignValue::Baseline;
        let mut inherited_text_indent = 0;
        let mut inherited_word_spacing = 0;
        let mut inherited_letter_spacing = 0;
        let mut inherited_pointer_events = NativePointerEventsValue::Auto;
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
                    font_variant_ligatures: inherited_font_variant_ligatures,
                    font_variant_caps: inherited_font_variant_caps,
                    font_variant_position: inherited_font_variant_position,
                    font_variant_alternates: inherited_font_variant_alternates,
                    font_language_override: inherited_font_language_override,
                    font_variation_settings: inherited_font_variation_settings,
                    font_variant_east_asian: inherited_font_variant_east_asian,
                    font_variant_numeric: inherited_font_variant_numeric,
                    font_feature_settings: inherited_font_feature_settings,
                    font_kerning: inherited_font_kerning,
                    font_optical_sizing: inherited_font_optical_sizing,
                    font_palette: inherited_font_palette,
                    font_weight: inherited_font_weight,
                    font_style: inherited_font_style,
                    font_stretch: inherited_font_stretch,
                    font_family: inherited_font_family,
                    font_size: inherited_font_size,
                    word_break: inherited_word_break,
                    text_overflow: inherited_text_overflow,
                    overflow_x: inherited_overflow_x,
                    overflow_y: inherited_overflow_y,
                    vertical_align: inherited_vertical_align,
                    text_indent: inherited_text_indent,
                    word_spacing: inherited_word_spacing,
                    letter_spacing: inherited_letter_spacing,
                    pointer_events: inherited_pointer_events,
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
            inherited_font_variant_ligatures = style.font_variant_ligatures();
            inherited_font_variant_caps = style.font_variant_caps();
            inherited_font_variant_position = style.font_variant_position();
            inherited_font_variant_alternates = style.font_variant_alternates();
            inherited_font_language_override = style.font_language_override();
            inherited_font_variation_settings = style.font_variation_settings();
            inherited_font_variant_east_asian = style.font_variant_east_asian();
            inherited_font_variant_numeric = style.font_variant_numeric();
            inherited_font_feature_settings = style.font_feature_settings();
            inherited_font_kerning = style.font_kerning();
            inherited_font_optical_sizing = style.font_optical_sizing();
            inherited_font_palette = style.font_palette();
            inherited_font_weight = style.font_weight();
            inherited_font_style = style.font_style();
            inherited_font_stretch = style.font_stretch();
            inherited_font_family = style.font_family();
            inherited_font_size = style.font_size();
            inherited_word_break = style.word_break();
            inherited_text_overflow = style.text_overflow();
            inherited_overflow_x = style.overflow_x();
            inherited_overflow_y = style.overflow_y();
            inherited_vertical_align = style.vertical_align();
            inherited_text_indent = style.text_indent();
            inherited_word_spacing = style.word_spacing();
            inherited_letter_spacing = style.letter_spacing();
            inherited_pointer_events = style.pointer_events();
            if current_id == id {
                return style;
            }
        }
        NativeComputedStyle::default()
    }

    pub(crate) fn is_hidden_for_layout(&self, id: NativeNodeId) -> bool {
        self.is_hidden(id)
    }

    pub(crate) fn text_metrics_for_layout(&self, id: NativeNodeId) -> NativeTextMetrics {
        let style = self.computed_style_for_layout(id);
        let metrics = if style
            .font_family()
            .iter()
            .any(|family| !matches!(family, super::css::NativeFontFamilyValue::Fallback))
        {
            NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
                style.font_family(),
                style.font_size(),
                style.font_weight(),
                style.font_style(),
                style.font_stretch().min,
                style.font_variant_ligatures(),
                style.font_feature_settings(),
                style.font_kerning(),
                style.font_variant_caps(),
                style.font_variant_position(),
                style.font_variant_numeric(),
                style.font_variant_alternates(),
                style.font_variant_east_asian(),
                style.font_language_override(),
                style.font_variation_settings(),
                style.direction(),
                &self.font_book,
            )
        } else {
            NativeTextMetrics::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
                style.font_size(),
                style.font_stretch().min,
                style.font_variant_ligatures(),
                style.font_feature_settings(),
                style.font_kerning(),
                style.font_variant_caps(),
                style.font_variant_position(),
                style.font_variant_numeric(),
                style.font_variant_alternates(),
                style.font_variant_east_asian(),
                style.font_language_override(),
                style.font_variation_settings(),
                style.direction(),
            )
        };
        let palette = style.font_palette();
        metrics
            .with_optical_sizing(style.font_optical_sizing())
            .with_palette(self.stylesheet.resolve_font_palette(palette))
            .with_palette_overrides(self.stylesheet.resolve_font_palette_overrides(palette))
    }

    pub(crate) fn text_line_height_for_layout(&self, id: NativeNodeId) -> u32 {
        self.text_metrics_for_layout(id).line_height()
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

    pub(crate) fn is_descendant_or_self(&self, id: NativeNodeId, ancestor: NativeNodeId) -> bool {
        let mut current = Some(id);
        while let Some(current_id) = current {
            if current_id == ancestor {
                return true;
            }
            current = self.node(current_id).and_then(NativeNode::parent);
        }
        false
    }

    pub(crate) fn link_href(&self, id: NativeNodeId) -> Option<&str> {
        let node = self.node(id)?;
        (node.element_name() == Some("a") && self.semantic_role(id) == Some("link"))
            .then(|| node.attribute("href"))
            .flatten()
    }

    pub(crate) fn link_download_attribute(&self, id: NativeNodeId) -> Option<&str> {
        let node = self.node(id)?;
        if node.element_name() != Some("a") || self.semantic_role(id) != Some("link") {
            return None;
        }
        let attributes = node.attributes()?;
        attributes
            .contains_key("download")
            .then(|| node.attribute("download").unwrap_or(""))
    }

    /// Return whether an anchor explicitly requests a fresh browsing context.
    ///
    /// Named browsing contexts are intentionally kept out of this bounded
    /// topology projection until they have a stable public identity. The
    /// reserved `_blank` target is the interoperable new-page contract used by
    /// the native target owner today.
    pub(crate) fn link_opens_new_target(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        node.element_name() == Some("a")
            && self.semantic_role(id) == Some("link")
            && node
                .attribute("target")
                .is_some_and(|target| target.trim().eq_ignore_ascii_case("_blank"))
    }

    pub(crate) fn form_submission_target(
        &self,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
    ) -> Result<String, NativeEngineError> {
        let form = self
            .node(form_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if form.element_name() != Some("form") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "form submission target is not a form".into(),
            });
        }
        let submitter_node = submitter.and_then(|id| self.node(id));
        let raw_target = submitter_node
            .and_then(|node| node.attribute("formtarget"))
            .or_else(|| form.attribute("target"))
            .unwrap_or("_self")
            .trim();
        if raw_target.is_empty() {
            return Ok("_self".into());
        }
        validate_url_text("form target", raw_target)?;
        validate_window_name(raw_target)?;
        let lower = raw_target.to_ascii_lowercase();
        Ok(match lower.as_str() {
            "_self" | "_parent" | "_top" | "_blank" | "_unfencedtop" => lower,
            _ => raw_target.to_owned(),
        })
    }

    pub(crate) fn form_submission_request(
        &self,
        id: NativeNodeId,
        document_url: &str,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        self.form_submission_request_with_submitter(id, document_url, None)
    }

    pub(crate) fn form_submission_request_with_submitter(
        &self,
        id: NativeNodeId,
        document_url: &str,
        submitter: Option<NativeNodeId>,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        let form_id = if self
            .node(id)
            .is_some_and(|node| node.element_name() == Some("form"))
        {
            id
        } else {
            self.submit_control_form(id)
                .ok_or(NativeEngineError::DetachedTarget)?
        };
        let form = self
            .node(form_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if form.element_name() != Some("form") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script form submission target is not a form".into(),
            });
        }
        if let Some(submitter) = submitter
            && self.submit_control_form(submitter) != Some(form_id)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "form submitter must be a submit control for the form".into(),
            });
        }
        let form_target = self.form_submission_target(form_id, submitter)?;
        let submitter_node = submitter.and_then(|id| self.node(id));
        let method = submitter_node
            .and_then(|node| node.attribute("formmethod"))
            .unwrap_or_else(|| form.attribute("method").unwrap_or("get"));
        let method = if method.eq_ignore_ascii_case("get") {
            super::resource_loader::NativeNavigationMethod::Get
        } else if method.eq_ignore_ascii_case("post") {
            super::resource_loader::NativeNavigationMethod::Post
        } else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native form submission supports only GET and POST".into(),
            });
        };
        let encoding = if method == super::resource_loader::NativeNavigationMethod::Post {
            let enctype = submitter_node
                .and_then(|node| node.attribute("formenctype"))
                .unwrap_or_else(|| {
                    form.attribute("enctype")
                        .unwrap_or("application/x-www-form-urlencoded")
                });
            let media_type = enctype.split(';').next().unwrap_or_default().trim();
            if media_type.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
                NativeFormEncoding::UrlEncoded
            } else if media_type.eq_ignore_ascii_case("multipart/form-data") {
                NativeFormEncoding::Multipart
            } else if media_type.eq_ignore_ascii_case("text/plain") {
                NativeFormEncoding::TextPlain
            } else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "native POST form submission supports only urlencoded, multipart, or text/plain encoding".into(),
                });
            }
        } else {
            NativeFormEncoding::UrlEncoded
        };
        let action = submitter_node
            .and_then(|node| node.attribute("formaction"))
            .unwrap_or_else(|| form.attribute("action").unwrap_or(document_url));
        validate_url_text("form action", action)?;
        let base = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "form submission owner URL is not valid URL syntax".into(),
            }
        })?;
        let mut target = if let Ok(absolute) = Url::parse(action) {
            absolute
        } else {
            base.join(action)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "form action could not be resolved against the document".into(),
                })?
        };
        if !target.username().is_empty() || target.password().is_some() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "form action must not contain userinfo credentials".into(),
            });
        }
        let mut pairs = Vec::new();
        self.collect_form_data(form_id, &mut pairs, submitter)?;
        let mut request = match method {
            super::resource_loader::NativeNavigationMethod::Get => {
                let query = encode_urlencoded_form_data(&pairs)?;
                if query.is_empty() {
                    target.set_query(None);
                } else {
                    target.set_query(Some(&query));
                }
                let target = target.to_string();
                validate_url_text("form submission URL", &target)?;
                Ok(NativeNavigationRequest::get(target))
            }
            super::resource_loader::NativeNavigationMethod::Post => {
                let target = target.to_string();
                validate_url_text("form submission URL", &target)?;
                let (body, content_type) = encode_form_data(&pairs, encoding)?;
                NativeNavigationRequest::post_with_body(target, body, content_type)
            }
            _ => unreachable!("form method was validated above"),
        }?;
        request.target = Some(form_target);
        Ok(request)
    }

    pub(crate) fn submit_control_form(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let node = self.node(id)?;
        let tag_name = node.element_name()?;
        let is_submit = match tag_name {
            "button" => node
                .attribute("type")
                .is_none_or(|kind| kind.eq_ignore_ascii_case("submit")),
            "input" => node.attribute("type").is_some_and(|kind| {
                kind.eq_ignore_ascii_case("submit") || kind.eq_ignore_ascii_case("image")
            }),
            _ => false,
        };
        if !is_submit {
            return None;
        }
        self.form_owner(id)
    }

    /// Return required controls that block an interactive form submission.
    /// This is deliberately a bounded validity subset: disabled controls and
    /// read-only controls do not participate, while required text controls,
    /// checkboxes, radio groups, selects, and textareas do.
    pub(crate) fn invalid_form_controls(
        &self,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        self.invalid_form_controls_with_policy(form_id, submitter, true)
    }

    pub(crate) fn invalid_form_controls_for_api(
        &self,
        form_id: NativeNodeId,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        self.invalid_form_controls_with_policy(form_id, None, false)
    }

    fn invalid_form_controls_with_policy(
        &self,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
        honor_submit_bypass: bool,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        let form = self
            .node(form_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if form.element_name() != Some("form") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "form validation target is not a form".into(),
            });
        }
        if honor_submit_bypass
            && (form.attribute("novalidate").is_some()
                || submitter.is_some_and(|id| {
                    self.node(id)
                        .is_some_and(|node| node.attribute("formnovalidate").is_some())
                }))
        {
            return Ok(Vec::new());
        }
        let controls = self.form_controls_in_document_order(form_id)?;
        let mut invalid = Vec::new();
        for id in controls {
            let Some(_node) = self.node(id) else {
                continue;
            };
            if self.is_disabled(id) || self.is_read_only(id) {
                continue;
            }
            let valid = self.control_validity(id).0.valid;
            if !valid {
                if invalid.len() >= MAX_FORM_CONTROLS {
                    return Err(NativeEngineError::limit(
                        "invalid form controls",
                        MAX_FORM_CONTROLS,
                        invalid.len().saturating_add(1),
                    ));
                }
                invalid.push(id);
            }
        }
        Ok(invalid)
    }

    fn script_validation_snapshot(
        &self,
        id: NativeNodeId,
    ) -> (NativeValiditySnapshot, String, bool) {
        let Some(node) = self.node(id) else {
            return (NativeValiditySnapshot::default(), String::new(), false);
        };
        if node.element_name() == Some("form") {
            let valid = self
                .invalid_form_controls_for_api(id)
                .is_ok_and(|invalid| invalid.is_empty());
            return (
                NativeValiditySnapshot {
                    valid,
                    ..NativeValiditySnapshot::default()
                },
                String::new(),
                false,
            );
        }
        let (validity, will_validate, message) = self.control_validity(id);
        (validity, message, will_validate)
    }

    fn control_validity(&self, id: NativeNodeId) -> (NativeValiditySnapshot, bool, String) {
        let Some(node) = self.node(id) else {
            return (NativeValiditySnapshot::default(), false, String::new());
        };
        let will_validate = match node.element_name() {
            Some("input") => {
                let input_type = node.attribute("type").unwrap_or("text");
                !matches!(
                    input_type.to_ascii_lowercase().as_str(),
                    "hidden" | "button" | "submit" | "reset" | "image"
                ) && !self.is_disabled(id)
                    && !self.is_read_only(id)
            }
            Some("select") | Some("textarea") => !self.is_disabled(id) && !self.is_read_only(id),
            Some("button") => !self.is_disabled(id),
            _ => false,
        };
        if !will_validate {
            return (
                NativeValiditySnapshot {
                    valid: true,
                    ..NativeValiditySnapshot::default()
                },
                false,
                String::new(),
            );
        }
        let mut validity = match node.element_name() {
            Some("input") => {
                let input_type = node.attribute("type").unwrap_or("text");
                if input_type.eq_ignore_ascii_case("checkbox") {
                    NativeValiditySnapshot {
                        value_missing: node.attribute("required").is_some() && !node.state.checked,
                        ..NativeValiditySnapshot::default()
                    }
                } else if input_type.eq_ignore_ascii_case("radio") {
                    let checked = self
                        .form_owner(id)
                        .is_some_and(|form_id| self.radio_group_has_checked(form_id, id))
                        || node.state.checked;
                    NativeValiditySnapshot {
                        value_missing: node.attribute("required").is_some() && !checked,
                        ..NativeValiditySnapshot::default()
                    }
                } else {
                    let value = self.current_value(id).unwrap_or_default();
                    text_input_validity(node, &value)
                }
            }
            Some("textarea") => {
                let value = self.current_value(id).unwrap_or_default();
                textarea_validity(node, &value)
            }
            Some("select") => NativeValiditySnapshot {
                value_missing: node.attribute("required").is_some()
                    && self.current_value(id).is_none_or(|value| value.is_empty()),
                ..NativeValiditySnapshot::default()
            },
            _ => NativeValiditySnapshot::default(),
        };
        validity.custom_error = !node.state.custom_validity.is_empty();
        validity.valid = !validity.bad_input
            && !validity.custom_error
            && !validity.pattern_mismatch
            && !validity.range_overflow
            && !validity.range_underflow
            && !validity.step_mismatch
            && !validity.too_long
            && !validity.too_short
            && !validity.type_mismatch
            && !validity.value_missing;
        let message = validation_message(&validity, &node.state.custom_validity);
        (validity, true, message)
    }

    fn collect_form_controls(
        &self,
        parent_id: NativeNodeId,
        controls: &mut Vec<NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        self.node(parent_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        for node in &self.nodes {
            if !self.is_attached(node.id()) {
                continue;
            }
            if self.form_owner(node.id()) == Some(parent_id) {
                let child_id = node.id();
                controls.push(child_id);
                if controls.len() > MAX_FORM_CONTROLS {
                    return Err(NativeEngineError::limit(
                        "form controls",
                        MAX_FORM_CONTROLS,
                        controls.len(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn form_controls_in_document_order(
        &self,
        form_id: NativeNodeId,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        let mut controls = Vec::new();
        self.collect_form_controls(form_id, &mut controls)?;
        Ok(controls)
    }

    fn form_owner(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let node = self.node(id)?;
        if !matches!(
            node.element_name(),
            Some("button" | "input" | "select" | "textarea")
        ) {
            return None;
        }
        if let Some(form_reference) = node.attribute("form") {
            return self.nodes.iter().find_map(|candidate| {
                (self.is_attached(candidate.id())
                    && candidate.element_name() == Some("form")
                    && candidate.attribute("id") == Some(form_reference))
                .then_some(candidate.id())
            });
        }
        self.find_ancestor_element(id, "form")
    }

    fn radio_group_has_checked(&self, form_id: NativeNodeId, radio_id: NativeNodeId) -> bool {
        let Some(radio) = self.node(radio_id) else {
            return false;
        };
        let name = radio.attribute("name");
        self.nodes.iter().any(|candidate| {
            self.is_attached(candidate.id())
                && candidate.element_name() == Some("input")
                && candidate
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("radio"))
                && self.form_owner(candidate.id()) == Some(form_id)
                && candidate.attribute("name") == name
                && !self.is_disabled(candidate.id())
                && candidate.state.checked
        })
    }

    fn collect_form_data(
        &self,
        form_id: NativeNodeId,
        pairs: &mut Vec<(String, NativeFormValue)>,
        submitter: Option<NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        for child_id in self.form_controls_in_document_order(form_id)? {
            let Some(child) = self.node(child_id) else {
                continue;
            };
            if self.is_disabled(child_id) {
                continue;
            }
            let name = child.attribute("name").filter(|name| !name.is_empty());
            match child.element_name() {
                Some("input") => {
                    let input_type = child.attribute("type").unwrap_or("text");
                    if input_type.eq_ignore_ascii_case("submit")
                        || input_type.eq_ignore_ascii_case("button")
                        || input_type.eq_ignore_ascii_case("reset")
                        || input_type.eq_ignore_ascii_case("image")
                    {
                        if Some(child_id) == submitter && input_type.eq_ignore_ascii_case("submit")
                        {
                            self.append_submitter_data(child_id, pairs)?;
                        }
                        continue;
                    }
                    if input_type.eq_ignore_ascii_case("file") {
                        if let Some(name) = name {
                            if child.state.files.is_empty() {
                                append_form_value(pairs, name, NativeFormValue::EmptyFile)?;
                            } else {
                                for file in &child.state.files {
                                    append_form_value(
                                        pairs,
                                        name,
                                        NativeFormValue::File(file.clone()),
                                    )?;
                                }
                            }
                        }
                        continue;
                    }
                    if (input_type.eq_ignore_ascii_case("checkbox")
                        || input_type.eq_ignore_ascii_case("radio"))
                        && !child.state.checked
                    {
                        continue;
                    }
                    if let Some(name) = name {
                        let value = child
                            .state
                            .value
                            .clone()
                            .or_else(|| child.attribute("value").map(str::to_owned))
                            .unwrap_or_default();
                        append_form_value(pairs, name, NativeFormValue::Text(value))?;
                    }
                }
                Some("textarea") => {
                    if let Some(name) = name {
                        append_form_value(
                            pairs,
                            name,
                            NativeFormValue::Text(self.current_value(child_id).unwrap_or_default()),
                        )?;
                    }
                }
                Some("button") => {
                    let button_type = child.attribute("type").unwrap_or("submit");
                    if Some(child_id) == submitter && button_type.eq_ignore_ascii_case("submit") {
                        self.append_submitter_data(child_id, pairs)?;
                    }
                }
                Some("select") => {
                    if let Some(name) = name {
                        for option_id in self.select_option_ids(child_id) {
                            let Some(option) = self.node(option_id) else {
                                continue;
                            };
                            if !option.state.selected {
                                continue;
                            }
                            let value = option
                                .attribute("value")
                                .map(str::to_owned)
                                .or_else(|| {
                                    self.element_text(option_id, MAX_LOCATOR_BYTES)
                                        .map(|(value, _)| value)
                                })
                                .unwrap_or_default();
                            append_form_value(pairs, name, NativeFormValue::Text(value))?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn append_submitter_data(
        &self,
        id: NativeNodeId,
        pairs: &mut Vec<(String, NativeFormValue)>,
    ) -> Result<(), NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        let Some(name) = node.attribute("name").filter(|name| !name.is_empty()) else {
            return Ok(());
        };
        let value = match node.element_name() {
            Some("input") => node
                .state
                .value
                .clone()
                .or_else(|| node.attribute("value").map(str::to_owned))
                .unwrap_or_default(),
            Some("button") => node.attribute("value").unwrap_or_default().to_owned(),
            _ => String::new(),
        };
        append_form_value(pairs, name, NativeFormValue::Text(value))
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
            if !self.is_attached(node.id())
                || node.element_name().is_none()
                || node.attribute("id") != Some(fragment)
            {
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
            if !self.is_attached(node.id())
                || node.element_name() != Some("a")
                || node.attribute("name") != Some(fragment)
            {
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
            "img" => Some("img"),
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
                "file" => Some("file"),
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
        if role == "img"
            && let Some(value) = node.attribute("alt")
        {
            return collapse_text(value, MAX_LOCATOR_BYTES);
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
            (self.is_attached(node.id())
                && node.element_name().is_some()
                && node.attribute("id") == Some(value))
            .then_some(node.id())
        })
    }

    fn find_label_for(&self, value: &str) -> Option<NativeNodeId> {
        self.nodes.iter().find_map(|node| {
            (self.is_attached(node.id())
                && node.element_name() == Some("label")
                && node.attribute("for") == Some(value))
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
            NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
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
            NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
        }
    }
}

fn text_input_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot::default();
    let required = node.attribute("required").is_some();
    if required && value.is_empty() {
        validity.value_missing = true;
    }
    if value.is_empty() {
        return finalize_validity(validity);
    }
    let input_type = node.attribute("type").unwrap_or("text");
    if input_type.eq_ignore_ascii_case("email") {
        let values = if node.attribute("multiple").is_some() {
            value.split(',').map(str::trim).collect::<Vec<_>>()
        } else {
            vec![value]
        };
        if values.is_empty() || values.iter().any(|item| !valid_email_value(item)) {
            validity.type_mismatch = true;
        }
    } else if input_type.eq_ignore_ascii_case("url") && url::Url::parse(value).is_err() {
        validity.type_mismatch = true;
    }
    if pattern_applies(input_type) {
        merge_validity(&mut validity, pattern_validity(node, value));
    }
    match input_type.to_ascii_lowercase().as_str() {
        "date" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::Date),
        ),
        "month" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::Month),
        ),
        "time" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::Time),
        ),
        "datetime-local" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::DateTimeLocal),
        ),
        "number" | "range" => merge_validity(&mut validity, numeric_validity(node, value)),
        _ => {}
    }
    merge_validity(&mut validity, length_validity(node, value));
    finalize_validity(validity)
}

fn textarea_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot {
        value_missing: node.attribute("required").is_some() && value.is_empty(),
        ..NativeValiditySnapshot::default()
    };
    merge_validity(&mut validity, length_validity(node, value));
    finalize_validity(validity)
}

fn pattern_applies(input_type: &str) -> bool {
    !matches!(
        input_type.to_ascii_lowercase().as_str(),
        "hidden"
            | "button"
            | "submit"
            | "reset"
            | "image"
            | "checkbox"
            | "radio"
            | "file"
            | "date"
            | "month"
            | "week"
            | "time"
            | "datetime-local"
            | "number"
            | "range"
            | "color"
    )
}

fn pattern_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let Some(pattern) = node.attribute("pattern") else {
        return NativeValiditySnapshot::default();
    };
    let expression = format!(r"\A(?:{pattern})\z");
    let Ok(regex) = regex::Regex::new(&expression) else {
        return NativeValiditySnapshot::default();
    };
    finalize_validity(NativeValiditySnapshot {
        pattern_mismatch: !regex.is_match(value),
        ..NativeValiditySnapshot::default()
    })
}

fn finalize_validity(mut validity: NativeValiditySnapshot) -> NativeValiditySnapshot {
    validity.valid = !validity.bad_input
        && !validity.custom_error
        && !validity.pattern_mismatch
        && !validity.range_overflow
        && !validity.range_underflow
        && !validity.step_mismatch
        && !validity.too_long
        && !validity.too_short
        && !validity.type_mismatch
        && !validity.value_missing;
    validity
}

fn merge_validity(target: &mut NativeValiditySnapshot, source: NativeValiditySnapshot) {
    target.bad_input |= source.bad_input;
    target.custom_error |= source.custom_error;
    target.pattern_mismatch |= source.pattern_mismatch;
    target.range_overflow |= source.range_overflow;
    target.range_underflow |= source.range_underflow;
    target.step_mismatch |= source.step_mismatch;
    target.too_long |= source.too_long;
    target.too_short |= source.too_short;
    target.type_mismatch |= source.type_mismatch;
    target.value_missing |= source.value_missing;
}

fn validation_message(validity: &NativeValiditySnapshot, custom_validity: &str) -> String {
    if !custom_validity.is_empty() {
        return custom_validity.to_owned();
    }
    if validity.value_missing {
        return "Please fill out this field.".into();
    }
    if validity.type_mismatch || validity.bad_input {
        return "Please enter a valid value.".into();
    }
    if validity.too_short {
        return "Value is too short.".into();
    }
    if validity.too_long {
        return "Value is too long.".into();
    }
    if validity.range_underflow {
        return "Value is below the minimum.".into();
    }
    if validity.range_overflow {
        return "Value is above the maximum.".into();
    }
    if validity.step_mismatch {
        return "Value does not match the required step.".into();
    }
    if validity.pattern_mismatch {
        return "Please match the requested format.".into();
    }
    String::new()
}

fn valid_email_value(value: &str) -> bool {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

fn length_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let length = value.encode_utf16().count();
    let min_length = node
        .attribute("minlength")
        .and_then(|value| value.parse::<usize>().ok());
    let max_length = node
        .attribute("maxlength")
        .and_then(|value| value.parse::<usize>().ok());
    finalize_validity(NativeValiditySnapshot {
        too_short: min_length.is_some_and(|minimum| !value.is_empty() && length < minimum),
        too_long: max_length.is_some_and(|maximum| length > maximum),
        ..NativeValiditySnapshot::default()
    })
}

fn numeric_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot::default();
    if value.is_empty() {
        return finalize_validity(validity);
    }
    let Ok(number) = value.parse::<f64>() else {
        validity.bad_input = true;
        return finalize_validity(validity);
    };
    if !number.is_finite() {
        validity.bad_input = true;
        return finalize_validity(validity);
    }
    let minimum = node
        .attribute("min")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite());
    let maximum = node
        .attribute("max")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite());
    validity.range_underflow = minimum.is_some_and(|minimum| number < minimum);
    validity.range_overflow = maximum.is_some_and(|maximum| number > maximum);
    let step_value = node.attribute("step").unwrap_or("1");
    if step_value.eq_ignore_ascii_case("any") {
        return finalize_validity(validity);
    }
    let step = step_value
        .parse::<f64>()
        .ok()
        .filter(|step| step.is_finite() && *step > 0.0)
        .unwrap_or(1.0);
    let base = minimum.unwrap_or(0.0);
    let remainder = ((number - base) / step).fract().abs();
    validity.step_mismatch =
        remainder > f64::EPSILON * 32.0 && (1.0 - remainder) > f64::EPSILON * 32.0;
    finalize_validity(validity)
}

#[derive(Debug, Clone, Copy)]
enum TemporalInputKind {
    Date,
    Month,
    Time,
    DateTimeLocal,
}

fn temporal_validity(
    node: &NativeNode,
    value: &str,
    kind: TemporalInputKind,
) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot::default();
    if value.is_empty() {
        return finalize_validity(validity);
    }
    let Some(number) = parse_temporal_value(value, kind) else {
        validity.bad_input = true;
        return finalize_validity(validity);
    };
    let minimum = node
        .attribute("min")
        .and_then(|value| parse_temporal_value(value, kind));
    let maximum = node
        .attribute("max")
        .and_then(|value| parse_temporal_value(value, kind));
    validity.range_underflow = minimum.is_some_and(|minimum| number < minimum);
    validity.range_overflow = maximum.is_some_and(|maximum| number > maximum);
    let default_step = match kind {
        TemporalInputKind::Date => 86_400.0,
        TemporalInputKind::Month => 1.0,
        TemporalInputKind::Time | TemporalInputKind::DateTimeLocal => 60.0,
    };
    let step_unit = match kind {
        TemporalInputKind::Date | TemporalInputKind::DateTimeLocal => 86_400.0,
        TemporalInputKind::Month | TemporalInputKind::Time => 1.0,
    };
    let step_value = node.attribute("step").unwrap_or_default();
    if step_value.eq_ignore_ascii_case("any") {
        return finalize_validity(validity);
    }
    let step = if step_value.is_empty() {
        default_step
    } else {
        step_value
            .parse::<f64>()
            .ok()
            .filter(|step| step.is_finite() && *step > 0.0)
            .map(|step| step * step_unit)
            .filter(|step| step.is_finite() && *step > 0.0)
            .unwrap_or(default_step)
    };
    let base = minimum.unwrap_or_else(|| match kind {
        TemporalInputKind::Date | TemporalInputKind::DateTimeLocal => parse_temporal_value(
            if matches!(kind, TemporalInputKind::Date) {
                "1970-01-01"
            } else {
                "1970-01-01T00:00"
            },
            kind,
        )
        .unwrap_or(0.0),
        TemporalInputKind::Month => 1970.0 * 12.0,
        TemporalInputKind::Time => 0.0,
    });
    let remainder = ((number - base) / step).fract().abs();
    validity.step_mismatch =
        remainder > f64::EPSILON * 32.0 && (1.0 - remainder) > f64::EPSILON * 32.0;
    finalize_validity(validity)
}

fn parse_temporal_value(value: &str, kind: TemporalInputKind) -> Option<f64> {
    match kind {
        TemporalInputKind::Date => parse_date_value(value).map(|days| days as f64 * 86_400.0),
        TemporalInputKind::Month => parse_month_value(value).map(|months| months as f64),
        TemporalInputKind::Time => parse_time_value(value),
        TemporalInputKind::DateTimeLocal => {
            let (date, time) = value.split_once('T')?;
            Some(parse_date_value(date)? as f64 * 86_400.0 + parse_time_value(time)?)
        }
    }
}

fn parse_date_value(value: &str) -> Option<i64> {
    if value.len() != 10 || value.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    if value.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let year = value.get(..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..)?.parse::<i64>().ok()?;
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let first = days_from_civil(year, month, 1);
    let next_month = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    (1..=(next_month - first))
        .contains(&day)
        .then_some(first + day - 1)
}

fn parse_month_value(value: &str) -> Option<i64> {
    if value.len() != 7 || value.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    let year = value.get(..4)?.parse::<i64>().ok()?;
    let month = value.get(5..)?.parse::<i64>().ok()?;
    (1..=9999)
        .contains(&year)
        .then_some(month)
        .filter(|month| (1..=12).contains(month))
        .map(|month| year * 12 + month - 1)
}

fn parse_time_value(value: &str) -> Option<f64> {
    let (hour, remainder) = value.split_once(':')?;
    let (minute, seconds) = remainder
        .split_once(':')
        .map_or((remainder, None), |(m, s)| (m, Some(s)));
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let second = seconds.unwrap_or("0");
    let (whole, fraction) = second
        .split_once('.')
        .map_or((second, None), |(s, f)| (s, Some(f)));
    let whole = whole.parse::<u32>().ok()?;
    if whole > 59 || fraction.is_some_and(|fraction| fraction.is_empty()) {
        return None;
    }
    let fraction = match fraction {
        Some(fraction) => format!("0.{fraction}").parse::<f64>().ok()?,
        None => 0.0,
    };
    Some(hour as f64 * 3600.0 + minute as f64 * 60.0 + whole as f64 + fraction)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year / 400
    } else {
        (adjusted_year - 399) / 400
    };
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era
}

fn append_form_value(
    pairs: &mut Vec<(String, NativeFormValue)>,
    name: &str,
    value: NativeFormValue,
) -> Result<(), NativeEngineError> {
    if pairs.len() >= MAX_FORM_CONTROLS {
        return Err(NativeEngineError::limit(
            "form controls",
            MAX_FORM_CONTROLS,
            pairs.len().saturating_add(1),
        ));
    }
    pairs.push((name.to_owned(), value));
    Ok(())
}

fn form_value_text(value: &NativeFormValue) -> &str {
    match value {
        NativeFormValue::Text(value) => value,
        NativeFormValue::File(file) => &file.name,
        NativeFormValue::EmptyFile => "",
    }
}

fn encode_urlencoded_form_data(
    pairs: &[(String, NativeFormValue)],
) -> Result<String, NativeEngineError> {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        query.append_pair(name, form_value_text(value));
    }
    let query = query.finish();
    if query.len() > MAX_LOCATOR_BYTES {
        return Err(NativeEngineError::limit(
            "form submission data",
            MAX_LOCATOR_BYTES,
            query.len(),
        ));
    }
    Ok(query)
}

fn encode_form_data(
    pairs: &[(String, NativeFormValue)],
    encoding: NativeFormEncoding,
) -> Result<(NativeRequestBody, String), NativeEngineError> {
    match encoding {
        NativeFormEncoding::UrlEncoded => Ok((
            NativeRequestBody::Text(encode_urlencoded_form_data(pairs)?),
            "application/x-www-form-urlencoded".into(),
        )),
        NativeFormEncoding::TextPlain => {
            let mut body = String::new();
            for (name, value) in pairs {
                body.push_str(name);
                body.push('=');
                body.push_str(form_value_text(value));
                body.push_str("\r\n");
            }
            if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
                return Err(NativeEngineError::limit(
                    "form submission data",
                    MAX_NATIVE_FORM_BODY_BYTES,
                    body.len(),
                ));
            }
            Ok((NativeRequestBody::Text(body), "text/plain".into()))
        }
        NativeFormEncoding::Multipart => {
            if pairs.iter().any(|(name, _)| name.contains(['\r', '\n'])) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "multipart form field names must not contain line breaks".into(),
                });
            }
            let boundary = (0..16).find_map(|suffix| {
                let candidate = if suffix == 0 {
                    "----glass-native-form-boundary".to_owned()
                } else {
                    format!("----glass-native-form-boundary-{suffix}")
                };
                pairs
                    .iter()
                    .all(|(name, value)| {
                        !name.contains(&candidate)
                            && match value {
                                NativeFormValue::Text(value) => !value.contains(&candidate),
                                NativeFormValue::File(file) => {
                                    !file.name.contains(&candidate)
                                        && !file
                                            .bytes
                                            .windows(candidate.len())
                                            .any(|window| window == candidate.as_bytes())
                                }
                                NativeFormValue::EmptyFile => true,
                            }
                    })
                    .then_some(candidate)
            });
            let Some(boundary) = boundary else {
                return Err(NativeEngineError::limit(
                    "multipart form boundary attempts",
                    16,
                    17,
                ));
            };
            let mut body = Vec::new();
            for (name, value) in pairs {
                body.extend_from_slice(b"--");
                body.extend_from_slice(boundary.as_bytes());
                body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
                body.extend_from_slice(name.replace('\\', "\\\\").replace('"', "\\\"").as_bytes());
                body.extend_from_slice(b"\"");
                match value {
                    NativeFormValue::Text(value) => {
                        body.extend_from_slice(b"\r\n\r\n");
                        body.extend_from_slice(value.as_bytes());
                    }
                    NativeFormValue::File(file) => {
                        body.extend_from_slice(b"; filename=\"");
                        body.extend_from_slice(
                            file.name
                                .replace('\\', "\\\\")
                                .replace('"', "\\\"")
                                .as_bytes(),
                        );
                        body.extend_from_slice(b"\"\r\nContent-Type: ");
                        body.extend_from_slice(if file.media_type.is_empty() {
                            b"application/octet-stream".as_slice()
                        } else {
                            file.media_type.as_bytes()
                        });
                        body.extend_from_slice(b"\r\n\r\n");
                        body.extend_from_slice(&file.bytes);
                    }
                    NativeFormValue::EmptyFile => {
                        body.extend_from_slice(
                            b"; filename=\"\"\r\nContent-Type: application/octet-stream\r\n\r\n",
                        );
                    }
                }
                body.extend_from_slice(b"\r\n");
                if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
                    return Err(NativeEngineError::limit(
                        "form submission data",
                        MAX_NATIVE_FORM_BODY_BYTES,
                        body.len(),
                    ));
                }
            }
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"--\r\n");
            if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
                return Err(NativeEngineError::limit(
                    "form submission data",
                    MAX_NATIVE_FORM_BODY_BYTES,
                    body.len(),
                ));
            }
            Ok((
                NativeRequestBody::Bytes(body),
                format!("multipart/form-data; boundary={boundary}"),
            ))
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
    Css(&'a str),
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
    if let Some(value) = locator.strip_prefix("css=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Css(value))
            .ok_or_else(|| {
                NativeEngineError::invalid("action locator", "CSS selector must not be empty")
            });
    }
    Err(NativeEngineError::invalid(
        "action locator",
        "use ref=, id=, role=, name=, text=, or css=",
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
    Comment(String),
    Doctype {
        name: String,
        public_id: Option<String>,
        system_id: Option<String>,
    },
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
            let relative_end = source[position + 4..].find("-->");
            let comment_end =
                relative_end.unwrap_or_else(|| source.len().saturating_sub(position + 4));
            push_token(
                &mut tokens,
                HtmlToken::Comment(source[position + 4..position + 4 + comment_end].to_owned()),
                max_tokens,
            )?;
            position = relative_end.map_or(source.len(), |end| position + 4 + end + 3);
            continue;
        }
        let doctype_prefix = position.saturating_add(9);
        if source
            .as_bytes()
            .get(position..doctype_prefix)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"<!doctype"))
            && source
                .as_bytes()
                .get(doctype_prefix)
                .is_none_or(|byte| byte.is_ascii_whitespace() || *byte == b'>')
        {
            let end = find_tag_end(source, doctype_prefix).unwrap_or(source.len());
            if let Some((name, public_id, system_id)) = parse_doctype(&source[doctype_prefix..end])
            {
                push_token(
                    &mut tokens,
                    HtmlToken::Doctype {
                        name,
                        public_id,
                        system_id,
                    },
                    max_tokens,
                )?;
            }
            position = if end < source.len() {
                end + 1
            } else {
                source.len()
            };
            continue;
        }
        if source[position..].starts_with("<!") || source[position..].starts_with("<?") {
            // HTML treats unknown declarations and processing-instruction
            // syntax as bogus comments. Recover to EOF when the declaration
            // is incomplete instead of discarding the remainder or failing a
            // whole navigation.
            let end = find_tag_end(source, position + 2);
            let comment_end = end.unwrap_or(source.len());
            push_token(
                &mut tokens,
                HtmlToken::Comment(source[position + 2..comment_end].to_owned()),
                max_tokens,
            )?;
            position = end.map_or(source.len(), |value| value + 1);
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
            // EOF in a tag is a parse error, but it is still recoverable HTML
            // input. Preserve the source as text so a malformed response does
            // not turn into a synthetic partial element.
            push_token(
                &mut tokens,
                HtmlToken::Text(source[position..].to_owned()),
                max_tokens,
            )?;
            break;
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
            if matches!(name.as_str(), "iframe" | "frame") && !self_closing {
                let text_start = end + 1;
                if let Some(text_end) = find_raw_text_end(source, text_start, &name) {
                    next_position = text_end;
                } else {
                    break;
                }
            }
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
        // The HTML tokenizer keeps the first attribute with a given
        // ASCII-case-insensitive name and ignores later duplicates.
        attributes
            .entry(attribute_name)
            .or_insert_with(|| decode_entities(&value));
    }
    Ok((name, attributes, self_closing))
}

fn parse_doctype(raw: &str) -> Option<(String, Option<String>, Option<String>)> {
    let bytes = raw.as_bytes();
    let mut cursor = 0;
    skip_ascii_whitespace(bytes, &mut cursor);
    let (name, next) = read_name(raw, cursor)?;
    cursor = next;
    skip_ascii_whitespace(bytes, &mut cursor);
    if cursor >= raw.len() {
        return Some((name, None, None));
    }

    let (keyword, next) = read_name(raw, cursor)?;
    cursor = next;
    skip_ascii_whitespace(bytes, &mut cursor);
    if keyword.eq_ignore_ascii_case("public") {
        let (public_id, next) = read_quoted_value(raw, cursor)?;
        cursor = next;
        skip_ascii_whitespace(bytes, &mut cursor);
        let system_id = read_quoted_value(raw, cursor).map(|(value, _)| value);
        return Some((name, Some(public_id), system_id));
    }
    if keyword.eq_ignore_ascii_case("system") {
        let (system_id, _) = read_quoted_value(raw, cursor)?;
        return Some((name, None, Some(system_id)));
    }
    Some((name, None, None))
}

fn read_quoted_value(raw: &str, start: usize) -> Option<(String, usize)> {
    let bytes = raw.as_bytes();
    let quote = *bytes.get(start)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let value_start = start + 1;
    let relative_end = raw[value_start..].find(quote as char)?;
    let end = value_start + relative_end;
    Some((raw[value_start..end].to_owned(), end + 1))
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

fn encode_data_url_payload(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
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

fn select_image_srcset_source(
    srcset: &str,
    sizes: Option<&str>,
    viewport: Viewport,
) -> Option<String> {
    let candidates = parse_image_srcset(srcset);
    if candidates.is_empty() {
        return None;
    }
    let source_size = image_source_size(sizes, viewport);
    let device_scale_factor = viewport
        .device_scale_factor_milli
        .max(DEFAULT_IMAGE_DENSITY_MILLI);
    let selected = match candidates[0].descriptor {
        NativeImageCandidateDescriptor::Density(_) => candidates.iter().min_by_key(|candidate| {
            let density = match candidate.descriptor {
                NativeImageCandidateDescriptor::Density(value) => value,
                NativeImageCandidateDescriptor::Width(_) => {
                    unreachable!("mixed image candidate descriptors were rejected during parsing")
                }
            };
            (
                density < device_scale_factor,
                density.abs_diff(device_scale_factor),
            )
        }),
        NativeImageCandidateDescriptor::Width(_) => {
            let target_width = u64::from(source_size)
                .saturating_mul(u64::from(device_scale_factor))
                .saturating_add(u64::from(DEFAULT_IMAGE_DENSITY_MILLI - 1))
                / u64::from(DEFAULT_IMAGE_DENSITY_MILLI);
            candidates.iter().min_by_key(|candidate| {
                let width = match candidate.descriptor {
                    NativeImageCandidateDescriptor::Width(value) => value,
                    NativeImageCandidateDescriptor::Density(_) => unreachable!(
                        "mixed image candidate descriptors were rejected during parsing"
                    ),
                };
                (
                    u64::from(width) < target_width,
                    u64::from(width).abs_diff(target_width),
                )
            })
        }
    }?;
    Some(selected.source.clone())
}

fn image_media_matches(media: Option<&str>, viewport: Viewport) -> bool {
    media.is_none_or(|media| {
        let media = media.trim();
        media.is_empty() || image_size_condition_matches(media, viewport)
    })
}

fn image_type_is_supported(image_type: Option<&str>) -> bool {
    image_type.is_none_or(|image_type| {
        let image_type = image_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        image_type.is_empty()
            || matches!(
                image_type.as_str(),
                "image/png"
                    | "image/apng"
                    | "image/jpeg"
                    | "image/webp"
                    | "image/gif"
                    | "image/svg+xml"
            )
    })
}

fn media_type_is_supported(media_type: Option<&str>) -> bool {
    media_type.is_none_or(|media_type| {
        let media_type = media_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        media_type.is_empty()
            || matches!(
                media_type.as_str(),
                "audio/mpeg"
                    | "audio/mp3"
                    | "audio/ogg"
                    | "application/ogg"
                    | "audio/wav"
                    | "audio/wave"
                    | "audio/x-wav"
                    | "audio/webm"
                    | "audio/mp4"
                    | "video/mp4"
                    | "video/ogg"
                    | "video/webm"
                    | "application/vnd.apple.mpegurl"
                    | "application/x-mpegurl"
            )
    })
}

fn parse_image_srcset(value: &str) -> Vec<NativeImageCandidate> {
    let mut candidates = Vec::new();
    let mut descriptor_kind = None;
    for component in value.split(',').take(MAX_IMAGE_SRCSET_CANDIDATES) {
        let mut tokens = component.split_ascii_whitespace();
        let Some(source) = tokens.next() else {
            continue;
        };
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
            || tokens.next().is_some_and(|_| tokens.next().is_some())
        {
            continue;
        }
        let descriptor_token = component
            .split_ascii_whitespace()
            .nth(1)
            .unwrap_or_default();
        let descriptor = if descriptor_token.is_empty() {
            NativeImageCandidateDescriptor::Density(DEFAULT_IMAGE_DENSITY_MILLI)
        } else if let Some(value) = descriptor_token.strip_suffix('w') {
            let Some(width) = value.parse::<u32>().ok().filter(|width| *width > 0) else {
                continue;
            };
            NativeImageCandidateDescriptor::Width(width)
        } else if let Some(value) = descriptor_token.strip_suffix('x') {
            let Some(density) = parse_image_density(value) else {
                continue;
            };
            NativeImageCandidateDescriptor::Density(density)
        } else {
            continue;
        };
        let is_density = matches!(descriptor, NativeImageCandidateDescriptor::Density(_));
        if descriptor_kind.is_some_and(|kind| kind != is_density) {
            return Vec::new();
        }
        descriptor_kind = Some(is_density);
        if candidates
            .iter()
            .any(|candidate: &NativeImageCandidate| candidate.descriptor == descriptor)
        {
            continue;
        }
        candidates.push(NativeImageCandidate {
            source: source.to_owned(),
            descriptor,
        });
    }
    candidates
}

fn image_source_is_declared(nodes: &[NativeNode], node_index: usize, source: &str) -> bool {
    let Some(node) = nodes.get(node_index) else {
        return false;
    };
    if node.attribute("src") == Some(source)
        || node.attribute("srcset").is_some_and(|srcset| {
            parse_image_srcset(srcset)
                .iter()
                .any(|candidate| candidate.source == source)
        })
    {
        return true;
    }
    let Some(parent_index) = node.parent().map(NativeNodeId::index) else {
        return false;
    };
    let Some(parent) = usize::try_from(parent_index)
        .ok()
        .and_then(|index| nodes.get(index))
    else {
        return false;
    };
    if parent.element_name() != Some("picture") {
        return false;
    }
    for child_id in parent.children() {
        let Some(child_index) = usize::try_from(child_id.index()).ok() else {
            continue;
        };
        if child_index == node_index {
            break;
        }
        if nodes
            .get(child_index)
            .is_some_and(|child| child.element_name() == Some("source"))
            && nodes[child_index]
                .attribute("srcset")
                .is_some_and(|srcset| {
                    parse_image_srcset(srcset)
                        .iter()
                        .any(|candidate| candidate.source == source)
                })
        {
            return true;
        }
    }
    false
}

fn media_source_is_declared(nodes: &[NativeNode], node_index: usize, source: &str) -> bool {
    let Some(node) = nodes.get(node_index) else {
        return false;
    };
    if node.attribute("src") == Some(source) {
        return true;
    }
    node.children().iter().any(|child_id| {
        usize::try_from(child_id.index())
            .ok()
            .and_then(|index| nodes.get(index))
            .is_some_and(|child| {
                child.element_name() == Some("source") && child.attribute("src") == Some(source)
            })
    })
}

fn parse_image_density(value: &str) -> Option<u32> {
    let density = value.parse::<f64>().ok()?;
    if !density.is_finite()
        || density <= 0.0
        || density * 1000.0 > f64::from(MAX_IMAGE_DENSITY_MILLI)
    {
        return None;
    }
    let density = (density * 1000.0).round();
    (density >= 1.0).then_some(density as u32)
}

fn image_source_size(sizes: Option<&str>, viewport: Viewport) -> u32 {
    let default_size = viewport.width.max(1);
    let Some(sizes) = sizes else {
        return default_size;
    };
    for component in sizes.split(',') {
        let component = component.trim();
        if component.is_empty() {
            continue;
        }
        let (condition, length) = if component.starts_with('(') {
            let Some(condition_end) = component.find(')') else {
                continue;
            };
            (
                Some(&component[..=condition_end]),
                component[condition_end.saturating_add(1)..].trim(),
            )
        } else {
            (None, component)
        };
        if condition.is_some_and(|condition| !image_size_condition_matches(condition, viewport)) {
            continue;
        }
        if let Some(size) = parse_image_source_length(length, viewport) {
            return size.max(1);
        }
    }
    default_size
}

fn image_size_condition_matches(condition: &str, viewport: Viewport) -> bool {
    let Some(condition) = condition
        .strip_prefix('(')
        .and_then(|condition| condition.strip_suffix(')'))
    else {
        return false;
    };
    let Some((feature, value)) = condition.split_once(':') else {
        return false;
    };
    let Some(value) = parse_fixed_css_pixels(value.trim()) else {
        return false;
    };
    let viewport_width = f64::from(viewport.width);
    match feature.trim().to_ascii_lowercase().as_str() {
        "max-width" => viewport_width <= value,
        "min-width" => viewport_width >= value,
        "width" => (viewport_width - value).abs() < f64::EPSILON,
        _ => false,
    }
}

fn parse_image_source_length(value: &str, viewport: Viewport) -> Option<u32> {
    let value = value.trim();
    let (number, unit) = if let Some(number) = value.strip_suffix("px") {
        (number, "px")
    } else {
        let number = value.strip_suffix("vw")?;
        (number, "vw")
    };
    let number = number.trim().parse::<f64>().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    let pixels = if unit == "vw" {
        number * f64::from(viewport.width) / 100.0
    } else {
        number
    };
    if !pixels.is_finite() {
        return None;
    }
    Some(pixels.round().clamp(1.0, f64::from(u32::MAX)) as u32)
}

fn parse_fixed_css_pixels(value: &str) -> Option<f64> {
    let number = value.strip_suffix("px")?.trim().parse::<f64>().ok()?;
    (!number.is_nan() && number.is_finite() && number >= 0.0).then_some(number)
}

fn should_auto_close(current: &str, next: &str) -> bool {
    (current == "li" && next == "li")
        || (current == "p"
            && matches!(
                next,
                "address"
                    | "article"
                    | "aside"
                    | "blockquote"
                    | "details"
                    | "div"
                    | "dl"
                    | "fieldset"
                    | "figcaption"
                    | "figure"
                    | "footer"
                    | "form"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "header"
                    | "hgroup"
                    | "hr"
                    | "main"
                    | "menu"
                    | "nav"
                    | "ol"
                    | "p"
                    | "pre"
                    | "section"
                    | "table"
                    | "ul"
            ))
        || (matches!(current, "dt" | "dd") && matches!(next, "dt" | "dd"))
        || (current == "rt" && next == "rt")
        || (current == "rp" && next == "rp")
        || (current == "option" && matches!(next, "option" | "optgroup"))
        || (current == "optgroup" && next == "optgroup")
        || (current == "tr" && matches!(next, "tr" | "tbody" | "thead" | "tfoot"))
        || (matches!(current, "td" | "th")
            && matches!(next, "td" | "th" | "tr" | "tbody" | "thead" | "tfoot"))
        || (current == "thead" && matches!(next, "tbody" | "tfoot"))
        || (current == "tbody" && matches!(next, "tbody" | "tfoot"))
        || (current == "tfoot" && next == "tbody")
        || (current == "colgroup" && matches!(next, "colgroup" | "tbody" | "thead" | "tfoot"))
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

fn validate_script_attribute(name: &str) -> Result<String, NativeEngineError> {
    if name.is_empty() {
        return Err(NativeEngineError::invalid(
            "script attribute name",
            "must not be empty",
        ));
    }
    if name.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "script attribute name",
            MAX_ATTRIBUTE_BYTES,
            name.len(),
        ));
    }
    if name
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\'' | b'<' | b'>' | b'='))
    {
        return Err(NativeEngineError::invalid(
            "script attribute name",
            "contains a forbidden character",
        ));
    }
    Ok(name.to_ascii_lowercase())
}

fn validate_script_element_name(name: &str) -> Result<String, NativeEngineError> {
    if name.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "script element name",
            MAX_ATTRIBUTE_BYTES,
            name.len(),
        ));
    }
    let Some((normalized, end)) = read_name(name, 0) else {
        return Err(NativeEngineError::invalid(
            "script element name",
            "must start with an ASCII letter",
        ));
    };
    if end != name.len() {
        return Err(NativeEngineError::invalid(
            "script element name",
            "contains an unsupported character",
        ));
    }
    Ok(normalized)
}

fn validate_namespace_uri(value: &str) -> Result<String, NativeEngineError> {
    if value.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "element namespace URI",
            MAX_ATTRIBUTE_BYTES,
            value.len(),
        ));
    }
    if matches!(
        value,
        HTML_NAMESPACE_URI | SVG_NAMESPACE_URI | MATHML_NAMESPACE_URI
    ) {
        return Ok(value.to_owned());
    }
    Err(NativeEngineError::invalid(
        "element namespace URI",
        "is not a supported HTML, SVG, or MathML namespace",
    ))
}

fn validate_attribute_namespace_uri(value: &str) -> Result<String, NativeEngineError> {
    if value.len() > MAX_ATTRIBUTE_BYTES {
        return Err(NativeEngineError::limit(
            "attribute namespace URI",
            MAX_ATTRIBUTE_BYTES,
            value.len(),
        ));
    }
    if matches!(
        value,
        HTML_NAMESPACE_URI
            | SVG_NAMESPACE_URI
            | MATHML_NAMESPACE_URI
            | XML_NAMESPACE_URI
            | XMLNS_NAMESPACE_URI
            | XLINK_NAMESPACE_URI
    ) {
        return Ok(value.to_owned());
    }
    Err(NativeEngineError::invalid(
        "attribute namespace URI",
        "is not a supported HTML, SVG, MathML, XML, XMLNS, or XLink namespace",
    ))
}

fn inferred_attribute_namespace(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if lower == "xmlns" || lower.starts_with("xmlns:") {
        Some(XMLNS_NAMESPACE_URI)
    } else if lower.starts_with("xml:") {
        Some(XML_NAMESPACE_URI)
    } else if lower.starts_with("xlink:") {
        Some(XLINK_NAMESPACE_URI)
    } else {
        None
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

fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn append_bounded_markup(output: &mut String, value: &str, max_bytes: usize, truncated: &mut bool) {
    if *truncated {
        return;
    }
    let available = max_bytes.saturating_sub(output.len());
    if value.len() <= available {
        output.push_str(value);
        return;
    }
    let mut end = available.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    output.push_str(&value[..end]);
    *truncated = true;
}

#[cfg(test)]
mod tests {
    use super::super::css::NativeFontPaletteName;
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
    fn named_font_palette_resolves_base_palette_for_text_metrics() {
        let document = NativeDocument::parse(
            "<style>@font-palette-values --brand { base-palette: 2; override-colors: 0 #010203; } #target { font-palette: --brand; } #missing { font-palette: --missing; }</style><p id='target'>Text</p><p id='missing'>Missing</p>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let target = document.resolve_target("id=target").unwrap();
        let missing = document.resolve_target("id=missing").unwrap();
        let brand = NativeFontPaletteName::parse("--brand").unwrap();
        assert_eq!(
            document.computed_style_for_layout(target).font_palette(),
            NativeFontPalette::Named(brand)
        );
        assert_eq!(
            document.text_metrics_for_layout(target).palette(),
            NativeFontPalette::Base(2)
        );
        assert_eq!(
            document.text_metrics_for_layout(target).palette_overrides()[0]
                .map(|value| (value.palette_index, value.color)),
            Some((
                0,
                NativeColor {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: u8::MAX,
                }
            ))
        );
        assert_eq!(
            document.text_metrics_for_layout(missing).palette(),
            NativeFontPalette::Normal
        );
        assert!(
            document
                .text_metrics_for_layout(missing)
                .palette_overrides()[0]
                .is_none()
        );
    }

    #[test]
    fn discovers_only_enforced_csp_meta_policies_in_head() {
        let limits = NativeEngineLimits::default();
        let document = NativeDocument::parse(
            "<html><head><meta http-equiv='Content-Security-Policy' content=\"script-src 'self'\"><meta http-equiv='Content-Security-Policy-Report-Only' content=\"script-src 'none'\"></head><body><meta http-equiv='Content-Security-Policy' content=\"script-src 'none'\"></body></html>",
            &limits,
        )
        .unwrap();
        assert_eq!(
            document.content_security_policy_meta(),
            vec!["script-src 'self'".to_owned()]
        );
    }

    #[test]
    fn csp_meta_policy_ledger_only_processes_each_node_once() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse(
            "<html><head><meta http-equiv='Content-Security-Policy' content=\"script-src 'self'\"></head><body></body></html>",
            &limits,
        )
        .unwrap();

        let pending = document.unprocessed_content_security_policy_meta();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, "script-src 'self'");
        document.mark_content_security_policy_meta_processed();
        assert!(
            document
                .unprocessed_content_security_policy_meta()
                .is_empty()
        );
    }

    #[test]
    fn preserves_comments_and_doctype_without_affecting_visible_text() {
        let document = NativeDocument::parse(
            "<!DOCTYPE html PUBLIC \"public-id\" \"system-id\"><!--before--><html><body><!--inside-->Visible</body></html>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

        let root = document.root();
        let root_children = document.node(root).unwrap().children();
        assert!(matches!(
            document.node(root_children[0]).map(NativeNode::kind),
            Some(NativeNodeKind::DocumentType {
                name,
                public_id: Some(public_id),
                system_id: Some(system_id),
            }) if name == "html" && public_id == "public-id" && system_id == "system-id"
        ));
        assert!(matches!(
            document.node(root_children[1]).map(NativeNode::kind),
            Some(NativeNodeKind::Comment(value)) if value == "before"
        ));

        let snapshot = document.script_snapshot_for_viewport(1024, Viewport::default());
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.node_type == 10 && node.node_name == "html")
        );
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.node_type == 8 && node.node_value.as_deref() == Some("inside"))
        );
        assert_eq!(document.visible_text(1024), ("Visible".into(), false));
        assert_eq!(
            document.element_inner_html(root, 1024),
            "<!DOCTYPE html PUBLIC \"public-id\" \"system-id\"><!--before--><html><body><!--inside-->Visible</body></html>"
        );
    }

    #[test]
    fn script_document_title_materializes_missing_title_node() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse("<body>Content</body>", &limits).unwrap();

        document
            .apply_script_commands(&[NativeScriptCommand::SetDocumentTitle {
                value: "Native title".into(),
            }])
            .unwrap();

        let title = document.find_element(document.root, "title").unwrap();
        assert_eq!(document.title(1024), ("Native title".into(), false));
        assert_eq!(document.node(title).unwrap().parent(), Some(document.root));
        assert_eq!(
            document.element_text(title, 1024),
            Some(("Native title".into(), false))
        );
    }

    #[test]
    fn script_text_content_replaces_subtree_and_detaches_old_nodes() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse(
            "<body><p id='root'>before <span id='old'>child</span></p></body>",
            &limits,
        )
        .unwrap();
        let root = document.find_element_by_id("root").unwrap();
        let old = document.find_element_by_id("old").unwrap();

        document
            .apply_script_commands(&[NativeScriptCommand::SetTextContent {
                node_index: root.index(),
                value: "after & literal".into(),
            }])
            .unwrap();

        assert_eq!(
            document.element_text(root, 1024),
            Some(("after & literal".into(), false))
        );
        assert!(document.find_element_by_id("old").is_none());
        assert!(document.node(old).is_none());
        assert_eq!(
            document.visible_text(1024),
            ("after & literal".into(), false)
        );
        assert_eq!(
            document
                .script_snapshot_for_viewport(1024, Viewport::default())
                .elements
                .len(),
            2
        );
    }

    #[test]
    fn script_inner_html_replaces_subtree_and_serializes_markup() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse(
            "<body><p id='root'>before <span id='old'>child</span></p></body>",
            &limits,
        )
        .unwrap();
        let root = document.find_element_by_id("root").unwrap();
        let old = document.find_element_by_id("old").unwrap();

        document
            .apply_script_commands(&[NativeScriptCommand::SetInnerHtml {
                node_index: root.index(),
                value: "<strong id='new'>after &amp; literal</strong> tail".into(),
            }])
            .unwrap();

        let new = document.find_element_by_id("new").unwrap();
        assert_eq!(
            document.element_text(root, 1024),
            Some(("after & literal tail".into(), false))
        );
        assert_eq!(
            document.element_inner_html(root, 1024),
            "<strong id=\"new\">after &amp; literal</strong> tail"
        );
        assert_eq!(document.node(old), None);
        assert_eq!(document.node(new).and_then(NativeNode::parent), Some(root));
    }

    #[test]
    fn script_remove_detaches_existing_subtree() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse(
            "<body><section id='remove'><span id='nested'>gone</span></section><p id='keep'>stay</p></body>",
            &limits,
        )
        .unwrap();
        let remove = document.find_element_by_id("remove").unwrap();
        let nested = document.find_element_by_id("nested").unwrap();

        document
            .apply_script_commands(&[NativeScriptCommand::RemoveNode {
                node_index: remove.index(),
            }])
            .unwrap();

        assert!(document.find_element_by_id("remove").is_none());
        assert!(document.find_element_by_id("nested").is_none());
        assert!(document.node(remove).is_none());
        assert!(document.node(nested).is_none());
        assert_eq!(document.visible_text(1024), ("stay".into(), false));
    }

    #[test]
    fn script_create_element_and_insert_child_commits_owned_nodes() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse(
            "<body><p id='before'>before</p><p id='after'>after</p></body>",
            &limits,
        )
        .unwrap();
        let body = document.find_element(document.root(), "body").unwrap();
        let first = u32::MAX - 1;
        let child = u32::MAX - 2;
        let inserted = u32::MAX - 3;
        let child_text = u32::MAX - 4;

        document
            .apply_script_commands(&[
                NativeScriptCommand::CreateElement {
                    node_index: first,
                    tag_name: "section".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::CreateElement {
                    node_index: child,
                    tag_name: "strong".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::CreateTextNode {
                    node_index: child_text,
                    value: "created text".into(),
                },
                NativeScriptCommand::SetAttribute {
                    node_index: first,
                    name: "id".into(),
                    value: "created".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::AppendChild {
                    parent_index: child,
                    child_index: child_text,
                },
                NativeScriptCommand::AppendChild {
                    parent_index: first,
                    child_index: child,
                },
                NativeScriptCommand::AppendChild {
                    parent_index: body.index(),
                    child_index: first,
                },
                NativeScriptCommand::CreateElement {
                    node_index: inserted,
                    tag_name: "em".into(),
                    namespace_uri: None,
                },
                NativeScriptCommand::SetTextContent {
                    node_index: inserted,
                    value: "inserted".into(),
                },
                NativeScriptCommand::InsertBefore {
                    parent_index: body.index(),
                    child_index: inserted,
                    before_index: Some(first),
                },
            ])
            .unwrap();

        let created = document.find_element_by_id("created").unwrap();
        let created_child = document.find_element(created, "strong").unwrap();
        let inserted_node = document.find_element(body, "em").unwrap();
        assert_eq!(
            document.element_text(created, 1024),
            Some(("created text".into(), false))
        );
        assert_eq!(
            document.element_inner_html(created, 1024),
            "<strong>created text</strong>"
        );
        assert_eq!(
            document.node(created_child).and_then(NativeNode::parent),
            Some(created)
        );
        assert_eq!(
            document.node(inserted_node).and_then(NativeNode::parent),
            Some(body)
        );
        assert_eq!(
            document
                .node(body)
                .unwrap()
                .children()
                .iter()
                .position(|id| *id == inserted_node),
            document
                .node(body)
                .unwrap()
                .children()
                .iter()
                .position(|id| *id == created)
                .map(|position| position.saturating_sub(1))
        );
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
    fn malformed_markup_recovers_without_partial_elements() {
        let document = NativeDocument::parse(
            "<!--unterminated<!unknown declaration<div title='unfinished>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

        assert!(document.semantic_nodes().is_empty());
        let root_children = document.node(document.root()).unwrap().children();
        assert_eq!(root_children.len(), 1);
        assert!(matches!(
            document.node(root_children[0]).map(NativeNode::kind),
            Some(NativeNodeKind::Comment(value)) if value == "unterminated<!unknown declaration<div title='unfinished>"
        ));
    }

    #[test]
    fn duplicate_html_attributes_keep_the_first_value_and_late_doctype_is_ignored() {
        let document = NativeDocument::parse(
            "<!doctype html><button ID='first' id='second'>Action</button><!doctype svg>",
            &NativeEngineLimits::default(),
        )
        .unwrap();

        let root_children = document.node(document.root()).unwrap().children();
        assert_eq!(
            root_children
                .iter()
                .filter(|id| matches!(
                    document.node(**id).map(NativeNode::kind),
                    Some(NativeNodeKind::DocumentType { .. })
                ))
                .count(),
            1
        );
        let button = document
            .node(document.resolve_target("role=button[name=Action]").unwrap())
            .unwrap();
        assert_eq!(button.attribute("id"), Some("first"));
    }

    #[test]
    fn implied_end_tags_close_common_list_option_table_and_paragraph_items() {
        let document = NativeDocument::parse(
            "<ul><li>one<li>two</ul><select><option>one<option>two</select><p>first<div>second</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let list_items = document
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("li"))
            .collect::<Vec<_>>();
        assert_eq!(list_items.len(), 2);
        assert!(list_items.iter().all(|node| node.parent().is_some()));
        let options = document
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("option"))
            .collect::<Vec<_>>();
        assert_eq!(options.len(), 2);
        assert!(options.iter().all(|node| node.parent().is_some()));
        let paragraphs = document
            .nodes
            .iter()
            .filter(|node| node.element_name() == Some("p"))
            .collect::<Vec<_>>();
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(
            document.visible_text(1024).0,
            "one two one two first second"
        );
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
    fn content_wire_round_trips_document_font_resources() {
        let limits = NativeEngineLimits::default();
        let mut document = NativeDocument::parse("<p>embedded</p>", &limits).unwrap();
        let variation_settings = parse_font_variation_settings(r#""wght" 620"#).unwrap();
        let feature_settings = parse_font_feature_settings(r#""liga" off, "kern" 1"#).unwrap();
        document
            .set_font_resources(vec![NativeFontFaceResource {
                family: "Embedded Sans".into(),
                family_key: super::super::css::font_family_hash("Embedded Sans"),
                weight: super::super::css::NativeFontWeightRange { min: 300, max: 700 },
                style: FontStyleValue::Normal,
                stretch: super::super::css::NativeFontStretchRange::default(),
                variation_settings,
                feature_settings,
                size_adjust: 625,
                metric_overrides: NativeFontMetricOverrides {
                    ascent: Some(800),
                    descent: Some(200),
                    line_gap: None,
                },
                font_display: NativeFontDisplay::Fallback,
                bytes: Arc::from(vec![0_u8, 1, 2, 3]),
                unicode_ranges: Vec::new(),
            }])
            .unwrap();
        let wire = document.to_content_wire();
        assert_eq!(wire.font_resources.len(), 1);
        assert_eq!(wire.font_resources[0].data_base64, "AAECAw==");
        assert_eq!(wire.font_resources[0].stretch.min, 1000);
        assert_eq!(wire.font_resources[0].stretch.max, 1000);
        assert_eq!(
            wire.font_resources[0].weight,
            super::super::css::FontWeightValue::Numeric(500)
        );
        assert_eq!(
            wire.font_resources[0].weight_range,
            Some(super::super::css::NativeFontWeightRange { min: 300, max: 700 })
        );
        assert_eq!(
            wire.font_resources[0].variation_settings,
            variation_settings
        );
        assert_eq!(wire.font_resources[0].feature_settings, feature_settings);
        assert_eq!(wire.font_resources[0].size_adjust, 625);
        assert_eq!(
            wire.font_resources[0].metric_overrides,
            NativeFontMetricOverrides {
                ascent: Some(800),
                descent: Some(200),
                line_gap: None,
            }
        );
        assert_eq!(
            wire.font_resources[0].font_display,
            NativeFontDisplay::Fallback
        );
        let restored =
            NativeDocument::from_content_wire(wire.clone(), &limits, document.generation())
                .unwrap();
        assert_eq!(restored.font_resources.len(), 1);
        assert_eq!(restored.font_resources[0].family, "Embedded Sans");
        assert_eq!(
            restored.font_resources[0].weight,
            super::super::css::NativeFontWeightRange { min: 300, max: 700 }
        );
        assert_eq!(restored.font_resources[0].bytes.as_ref(), &[0, 1, 2, 3]);
        assert_eq!(
            restored.font_resources[0].feature_settings,
            feature_settings
        );
        assert_eq!(restored.font_resources[0].size_adjust, 625);
        assert_eq!(
            restored.font_resources[0].metric_overrides,
            NativeFontMetricOverrides {
                ascent: Some(800),
                descent: Some(200),
                line_gap: None,
            }
        );
        assert_eq!(
            restored.font_resources[0].font_display,
            NativeFontDisplay::Fallback
        );

        let mut legacy_font_json =
            serde_json::to_value(&wire.font_resources[0]).expect("font wire serializes");
        legacy_font_json
            .as_object_mut()
            .expect("font wire object")
            .remove("feature_settings");
        legacy_font_json
            .as_object_mut()
            .expect("font wire object")
            .remove("size_adjust");
        legacy_font_json
            .as_object_mut()
            .expect("font wire object")
            .remove("metric_overrides");
        legacy_font_json
            .as_object_mut()
            .expect("font wire object")
            .remove("font_display");
        let legacy_font: NativeFontFaceResourceWire =
            serde_json::from_value(legacy_font_json).expect("legacy font wire decodes");
        assert_eq!(
            legacy_font.feature_settings,
            NativeFontFeatureSettings::default()
        );
        assert_eq!(
            legacy_font.size_adjust,
            super::super::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST
        );
        assert_eq!(
            legacy_font.metric_overrides,
            NativeFontMetricOverrides::default()
        );
        assert_eq!(legacy_font.font_display, NativeFontDisplay::Auto);
        let mut legacy_wire = wire;
        legacy_wire.font_resources[0].weight_range = None;
        let legacy =
            NativeDocument::from_content_wire(legacy_wire, &limits, document.generation()).unwrap();
        assert_eq!(
            legacy.font_resources[0].weight,
            super::super::css::NativeFontWeightRange::singleton(
                super::super::css::FontWeightValue::Numeric(500)
            )
        );
    }

    #[test]
    fn script_font_face_install_rejects_malformed_unicode_range() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Range".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "normal".into(),
                unicode_range: "U+4?A".into(),
                variant: "normal".into(),
                variation_settings: "normal".into(),
                feature_settings: "normal".into(),
                size_adjust: "100%".into(),
                ascent_override: "normal".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "auto".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script unicodeRange must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace unicodeRange"
        ));
        assert!(document.font_resources.is_empty());
    }

    #[test]
    fn script_font_face_install_rejects_malformed_stretch() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Stretch".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "201%".into(),
                unicode_range: String::new(),
                variant: "normal".into(),
                variation_settings: "normal".into(),
                feature_settings: "normal".into(),
                size_adjust: "100%".into(),
                ascent_override: "normal".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "auto".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script stretch must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace stretch"
        ));
        assert!(document.font_resources.is_empty());
    }

    #[test]
    fn script_font_face_install_rejects_malformed_size_adjust() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Size Adjust".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "normal".into(),
                unicode_range: String::new(),
                variant: "normal".into(),
                variation_settings: "normal".into(),
                feature_settings: "normal".into(),
                size_adjust: "24.9%".into(),
                ascent_override: "normal".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "auto".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script sizeAdjust must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace sizeAdjust"
        ));
        assert!(document.font_resources.is_empty());
    }

    #[test]
    fn script_font_face_install_rejects_malformed_metric_override() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Metric Override".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "normal".into(),
                unicode_range: String::new(),
                variant: "normal".into(),
                variation_settings: "normal".into(),
                feature_settings: "normal".into(),
                size_adjust: "100%".into(),
                ascent_override: "1000.1%".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "auto".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script metric override must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace ascentOverride"
        ));
        assert!(document.font_resources.is_empty());
    }

    #[test]
    fn script_font_face_install_rejects_malformed_display() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Display".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "normal".into(),
                unicode_range: String::new(),
                variant: "normal".into(),
                variation_settings: "normal".into(),
                feature_settings: "normal".into(),
                size_adjust: "100%".into(),
                ascent_override: "normal".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "blink".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script display must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace display"
        ));
        assert!(document.font_resources.is_empty());
    }

    #[test]
    fn script_font_face_install_rejects_malformed_variation_settings() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Variations".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "normal".into(),
                unicode_range: String::new(),
                variant: "normal".into(),
                variation_settings: r#""wght" 700 escape"#.into(),
                feature_settings: "normal".into(),
                size_adjust: "100%".into(),
                ascent_override: "normal".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "auto".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script variationSettings must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace variationSettings"
        ));
        assert!(document.font_resources.is_empty());
    }
    #[test]
    fn script_font_face_install_rejects_malformed_feature_settings() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Features".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "normal".into(),
                unicode_range: String::new(),
                variant: "normal".into(),
                feature_settings: r#""liga" 65536"#.into(),
                size_adjust: "100%".into(),
                ascent_override: "normal".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "auto".into(),
                variation_settings: "normal".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script featureSettings must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace featureSettings"
        ));
        assert!(document.font_resources.is_empty());
    }
    #[test]
    fn script_font_face_install_rejects_malformed_variant() {
        let mut document = NativeDocument::empty();
        let error = document
            .apply_script_font_face_installs(&[NativeScriptCommand::FontFaceInstall {
                request_id: 1,
                family: "Rejected Variant".into(),
                weight: "normal".into(),
                style: "normal".into(),
                stretch: "normal".into(),
                unicode_range: String::new(),
                variant: "small-caps all-small-caps".into(),
                feature_settings: "normal".into(),
                size_adjust: "100%".into(),
                ascent_override: "normal".into(),
                descent_override: "normal".into(),
                line_gap_override: "normal".into(),
                display: "auto".into(),
                variation_settings: "normal".into(),
                body_base64: "AA==".into(),
            }])
            .expect_err("malformed script variant must be rejected");

        assert!(matches!(
            error,
            NativeEngineError::InvalidConfiguration { field, .. }
                if field == "native FontFace variant"
        ));
        assert!(document.font_resources.is_empty());
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
    fn malformed_quoted_attribute_recovers_as_text_without_partial_document() {
        let limits = NativeEngineLimits::default();
        let document = NativeDocument::parse("<button title='unfinished>", &limits).unwrap();
        assert!(document.semantic_nodes().is_empty());
        assert_eq!(document.visible_text(1024).0, "<button title='unfinished>");
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
        let name_node = document
            .semantic_nodes()
            .into_iter()
            .find(|node| node.role == "textbox")
            .unwrap();
        let reference = name_node.reference;
        assert!(document.resolve_target(&reference).is_ok());
        document.revision = 2;
        assert!(matches!(
            document.resolve_target(&reference),
            Err(NativeEngineError::DetachedTarget)
        ));
        assert_eq!(document.resolve_target("css=#name"), Ok(name_node.node_id));
    }
}
