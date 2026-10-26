use super::config::{
    NativeEngineLimits, decode_percent_encoded_fragment, validate_url_text, validate_window_name,
    without_fragment,
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
use super::diagnostics::{
    NativeDiagnostic, NativeDiagnosticCode, NativeDiagnosticSink, NativeDiagnosticSource,
};
use super::error::NativeEngineError;
use super::html_parser::{
    HtmlFragmentOptions, HtmlParsedAttribute, HtmlParsedFragment, HtmlParsedNodeKind,
    HtmlTreeSinkFailure, parse_fragment_with_limits as parse_html5_fragment,
};
use super::image::{
    MAX_NATIVE_IMAGE_FRAMES, MAX_NATIVE_IMAGE_TRANSFER_BYTES, MAX_NATIVE_IMAGE_TRANSFER_PIXELS,
    NativeImage, NativeImageFrame, NativeImageResource, decode_data_image,
};
use super::interaction::{
    MAX_NATIVE_FORM_BODY_BYTES, NativeEventKind, NativeFile, validate_native_edit_key,
    validate_native_key,
};
use super::javascript::NativeScriptCommand;
use super::layout::{NativeLayoutSnapshot, NativePoint, NativeRect, NativeTextWidthCacheKey};
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
use super::resource_loader::{
    MAX_NATIVE_CSP_POLICIES, MAX_NATIVE_DOCUMENT_VIDEO_BYTES, MAX_NATIVE_DOCUMENT_VIDEO_FRAMES,
    MAX_NATIVE_MEDIA_BYTES, MAX_NATIVE_VIDEO_DECODED_BYTES, MAX_NATIVE_VIDEO_DIMENSION,
    MAX_NATIVE_VIDEO_FRAME_COUNT, NativeFetchReferrerPolicy, NativeMediaMetadata,
    NativeNavigationRequest, NativeRequestBody, NativeVideoFrame,
    javascript_mime_type_essence_match,
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
use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::sync::Arc;
use std::sync::Mutex;
use url::Url;

use super::font::{
    MAX_NATIVE_FONT_BYTES, MAX_NATIVE_FONT_FACES, MAX_NATIVE_FONT_TOTAL_BYTES, NativeFontBook,
    NativeFontFaceResource, NativeTextMetrics, NativeTextStyleOptions,
};

#[path = "dom/content_wire.rs"]
mod content_wire;
#[path = "dom/forms.rs"]
mod forms;
#[path = "dom/forms_api.rs"]
mod forms_api;
#[path = "dom/layout_style.rs"]
mod layout_style;
pub(crate) use layout_style::{
    reset_layout_text_metrics_diagnostic, take_layout_text_metrics_diagnostic,
};
#[path = "dom/parser.rs"]
mod parser;
#[path = "dom/script_mutations.rs"]
mod script_mutations;
#[path = "dom/script_snapshot.rs"]
mod script_snapshot;
use forms::*;
#[path = "dom/locators.rs"]
mod locators;
use locators::*;

// Real sites use longer `data-*` values for embedded state and client hints.
// Keep each attribute bounded while allowing those values through the parser
// and the content-process wire validation.
const MAX_ATTRIBUTE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_NATIVE_VIDEO_POSTERS: usize = 64;
const MAX_LOCATOR_BYTES: usize = crate::limits::MAX_TEXT_BYTES;
const MAX_FORM_CONTROLS: usize = 128;
const MAX_IMAGE_SRCSET_CANDIDATES: usize = 32;
const MAX_IMAGE_DENSITY_MILLI: u32 = 64_000;
pub(crate) const MAX_NATIVE_DOCUMENT_PRELOAD_IMAGE_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_NATIVE_DOCUMENT_PRELOAD_IMAGES: usize = 64;

#[cfg(test)]
fn is_html_literal_text_element(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript" | "plaintext"
    )
}

#[cfg(test)]
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
const DEFAULT_IMAGE_DENSITY_MILLI: u32 = 1_000;

fn parse_meta_referrer_policy(value: &str) -> Option<NativeFetchReferrerPolicy> {
    if value.is_empty() {
        return None;
    }
    let value = value.to_ascii_lowercase();
    let value = match value.as_str() {
        "never" => "no-referrer",
        "default" => "strict-origin-when-cross-origin",
        "always" => "unsafe-url",
        "origin-when-crossorigin" => "origin-when-cross-origin",
        value => value,
    };
    NativeFetchReferrerPolicy::parse(value).ok()
}

fn parse_element_referrer_policy(value: &str) -> Option<NativeFetchReferrerPolicy> {
    if value.is_empty() {
        return None;
    }
    let value = value.to_ascii_lowercase();
    NativeFetchReferrerPolicy::parse(&value).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeImageMapShape {
    Default,
    Rectangle,
    Circle,
    Polygon,
}
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeCheckableKind {
    Checkbox,
    Radio,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeCheckablePreActivation {
    node_id: NativeNodeId,
    kind: NativeCheckableKind,
    checked: bool,
    indeterminate: bool,
    radio_group_checked: Vec<(NativeNodeId, bool)>,
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
    indeterminate: bool,
    focused: bool,
    focus_anchor_node_index: Option<u32>,
    hovered: bool,
    user_interacted: bool,
    selected: bool,
    form_associated_disabled: Option<bool>,
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
            indeterminate: false,
            focused: false,
            focus_anchor_node_index: None,
            user_interacted: false,
            hovered: false,
            selected: attributes.contains_key("selected"),
            form_associated_disabled: None,
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
    DocumentFragment,
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
pub(crate) struct NativeDocumentWire {
    /// The content owner knows the parent already has the exact current state,
    /// so it can reuse it instead of transferring a full document snapshot.
    #[serde(default)]
    pub(crate) native_document_state_replayable: bool,
    pub(crate) nodes: Vec<NativeNodeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) computed_styles: Vec<NativeComputedStyle>,
    /// Interned computed styles keep repeated per-node CSS projections bounded
    /// without changing the decoded document representation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) computed_style_table: Vec<NativeComputedStyle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) computed_style_indices: Vec<u32>,
    #[serde(default)]
    pub(crate) quirks_mode: super::html_parser::HtmlParsedQuirksMode,
    #[serde(default)]
    pub(crate) document_referrer_policy: String,
    #[serde(default)]
    pub(crate) viewport: Viewport,
    #[serde(default)]
    pub(crate) font_resources: Vec<NativeFontFaceResourceWire>,
    #[serde(default)]
    pub(crate) blocked_inline_style_nodes: Vec<u32>,
    #[serde(default)]
    pub(crate) script_nodes: Vec<NativeScriptNodeIdentity>,
    #[serde(default)]
    pub(crate) started_script_nodes: Vec<u32>,
    #[serde(default)]
    pub(crate) embedded_frame_owner_removal_sequences: BTreeMap<u32, u64>,
    #[serde(default)]
    pub(crate) next_embedded_frame_owner_removal_sequence: u64,
    #[serde(default)]
    pub(crate) image_resources: Vec<NativeImageResourceWire>,
    #[serde(default)]
    pub(crate) background_image_sources: Vec<NativeBackgroundImageSourceWire>,
    #[serde(default)]
    pub(crate) background_image_resources: Vec<NativeImageResourceWire>,
    #[serde(default)]
    pub(crate) image_loads: Vec<NativeImageLoadWire>,
    #[serde(default)]
    pub(crate) video_poster_resources: Vec<NativeImageResourceWire>,
    #[serde(default)]
    pub(crate) video_poster_loads: Vec<NativeImageLoadWire>,
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
    /// Losslessly compressed RGBA data used when it is smaller than the raw
    /// base64 payload. Older peers continue to send `pixels_base64`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) pixels_zlib_base64: String,
    #[serde(default)]
    pub(crate) frames: Vec<NativeImageFrameWire>,
    #[serde(default)]
    pub(crate) loop_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeImageFrameWire {
    pub(crate) delay_ms: u32,
    pub(crate) pixels_base64: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) pixels_zlib_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeMediaResourceWire {
    pub(crate) node_index: u32,
    pub(crate) source: String,
    pub(crate) content_type: String,
    pub(crate) byte_length: usize,
    #[serde(default)]
    pub(crate) duration_millis: Option<u64>,
    #[serde(default)]
    pub(crate) video_frames: Vec<NativeVideoFrame>,
    #[serde(default)]
    pub(crate) current_time_millis: u64,
    #[serde(default)]
    pub(crate) is_playing: bool,
    #[serde(default = "default_media_playback_rate_millionths")]
    pub(crate) playback_rate_millionths: i64,
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

/// Caller-realm identity remapping for a subtree transferred to another
/// browsing-context document. The source may be a native index or a
/// script-created temporary index; the destination is always a fresh
/// temporary index owned by the caller's retained JavaScript wrappers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeNodeTransferIdentity {
    pub(crate) source_index: u32,
    pub(crate) destination_temporary_index: u32,
}

/// One native root included in a single atomic adoption transaction. A
/// virtual JavaScript `DocumentFragment` can contain several such roots even
/// though the fragment itself has no native arena slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeNodeSubtreeTransfer {
    pub(crate) source_index: u32,
    pub(crate) identities: Vec<NativeNodeTransferIdentity>,
}

impl NativeNodeSubtreeTransfer {
    /// Construct an empty transfer used for adapter-side validation probes.
    #[doc(hidden)]
    pub fn empty(source_index: u32) -> Self {
        Self {
            source_index,
            identities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeNodeWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) children: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) template_content: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) template_host: Option<u32>,
    pub(crate) kind: NativeNodeKindWire,
    #[serde(default, skip_serializing_if = "is_default_snapshot_value")]
    pub(crate) state: NativeElementStateWire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeNodeKindWire {
    Document,
    DocumentFragment,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) namespace_uri: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) attribute_namespaces: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) files: Vec<NativeFile>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) checked: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) indeterminate: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) focused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) focus_anchor_node_index: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) hovered: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) user_interacted: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) selected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) form_associated_disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) custom_validity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selection_start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selection_end: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selection_direction: Option<String>,
}

/// One arena-owned node with parent and child links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNode {
    id: NativeNodeId,
    parent: Option<NativeNodeId>,
    children: Vec<NativeNodeId>,
    template_content: Option<NativeNodeId>,
    template_host: Option<NativeNodeId>,
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
            | NativeNodeKind::DocumentFragment
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

    /// Return a raw-name attribute value with its namespace URI, if any.
    pub(crate) fn attribute_with_namespace_uri(&self, name: &str) -> Option<(&str, Option<&str>)> {
        self.attributes()?.iter().find_map(|(attribute, value)| {
            attribute.eq_ignore_ascii_case(name).then(|| {
                (
                    value.as_str(),
                    self.state
                        .attribute_namespaces
                        .get(attribute)
                        .map(String::as_str),
                )
            })
        })
    }

    /// Return a case-insensitive local-name match in one attribute namespace.
    pub fn attribute_in_namespace(&self, namespace_uri: Option<&str>, name: &str) -> Option<&str> {
        self.attributes()?.iter().find_map(|(attribute, value)| {
            let attribute_namespace = self
                .state
                .attribute_namespaces
                .get(attribute)
                .map(String::as_str);
            let local_name = if attribute_namespace.is_some() {
                attribute
                    .rsplit_once(':')
                    .map_or(attribute.as_str(), |(_, local_name)| local_name)
            } else {
                attribute.as_str()
            };
            (attribute_namespace == namespace_uri && local_name.eq_ignore_ascii_case(name))
                .then_some(value.as_str())
        })
    }

    /// Return a case-insensitive local-name match in any attribute namespace.
    pub fn attribute_in_any_namespace(&self, name: &str) -> Option<&str> {
        self.attribute_in_any_namespace_with_uri(name)
            .map(|(value, _)| value)
    }

    /// Return a case-insensitive local-name match and its namespace URI.
    pub(crate) fn attribute_in_any_namespace_with_uri(
        &self,
        name: &str,
    ) -> Option<(&str, Option<&str>)> {
        self.attributes()?.iter().find_map(|(attribute, value)| {
            let attribute_namespace = self
                .state
                .attribute_namespaces
                .get(attribute)
                .map(String::as_str);
            let local_name = if attribute_namespace.is_some() {
                attribute
                    .rsplit_once(':')
                    .map_or(attribute.as_str(), |(_, local_name)| local_name)
            } else {
                attribute.as_str()
            };
            local_name
                .eq_ignore_ascii_case(name)
                .then_some((value.as_str(), attribute_namespace))
        })
    }

    /// Return the lower-case element name, or `None` for document/text nodes.
    pub fn element_name(&self) -> Option<&str> {
        match &self.kind {
            NativeNodeKind::Element { name, .. } => Some(name),
            NativeNodeKind::Document
            | NativeNodeKind::DocumentFragment
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
    /// Privacy-safe identity for deduplicating repeated links to the same
    /// destination during revision-scoped semantic target selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation_target_fingerprint: Option<String>,
}

/// Bounded document data used to refresh the native JavaScript host view.
/// These remain snapshots rather than live DOM identities; committed document
/// state is still owned by Rust while target-local event callbacks execute in
/// the persistent JavaScript realm.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeScriptDocumentSnapshot {
    #[serde(default)]
    pub(crate) revision: u64,
    pub(crate) title: String,
    pub(crate) visible_text: String,
    pub elements: Vec<NativeScriptElementSnapshot>,
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
    #[serde(default)]
    pub(crate) template_content_index: Option<u32>,
    #[serde(default)]
    pub(crate) template_host_index: Option<u32>,
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
pub struct NativeScriptElementSnapshot {
    pub node_index: u32,
    pub(crate) parent_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub(crate) template_content_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) form_owner_index: Option<u32>,
    pub(crate) tag_name: String,
    #[serde(default)]
    pub(crate) namespace_uri: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(default)]
    pub(crate) attribute_namespaces: BTreeMap<String, String>,
    pub(crate) attributes: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub(crate) inner_html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub(crate) files: Vec<NativeFile>,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) checked: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub(crate) indeterminate: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) selected: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) disabled: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) hidden: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) focused: bool,
    #[serde(default)]
    pub(crate) focus_anchor_node_index: Option<u32>,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub(crate) hovered: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub(crate) user_interacted: bool,
    #[serde(skip_serializing_if = "is_default_snapshot_value")]
    pub(crate) validity: NativeValiditySnapshot,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) validation_message: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) custom_validity: String,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) will_validate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selection_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selection_end: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selection_direction: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default)]
    pub(crate) image_complete: bool,
    #[serde(skip_serializing_if = "is_default_snapshot_value")]
    #[serde(default)]
    pub(crate) image_natural_width: u32,
    #[serde(skip_serializing_if = "is_default_snapshot_value")]
    #[serde(default)]
    pub(crate) image_natural_height: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub(crate) image_current_src: String,
    #[serde(skip_serializing_if = "is_default_snapshot_value")]
    #[serde(default)]
    pub(crate) media_ready_state: u8,
    #[serde(skip_serializing_if = "is_default_snapshot_value")]
    #[serde(default)]
    pub(crate) media_network_state: u8,
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub(crate) media_current_src: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub(crate) media_duration_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub(crate) media_error: Option<u16>,
    #[serde(skip_serializing_if = "is_default_snapshot_value")]
    #[serde(default)]
    pub(crate) scroll_x: u32,
    #[serde(skip_serializing_if = "is_default_snapshot_value")]
    #[serde(default)]
    pub(crate) scroll_y: u32,
    #[serde(default = "default_inline_style_allowed")]
    #[serde(skip_serializing_if = "is_true")]
    pub(crate) inline_style_allowed: bool,
    /// The layout owner's computed style for this element.  Keeping this in
    /// the same snapshot as attributes and geometry lets one JavaScript turn
    /// observe a coherent style/layout revision rather than reconstructing
    /// CSS from inline attributes alone.
    #[serde(default)]
    pub(crate) computed_style: NativeComputedStyle,
}

fn is_default_snapshot_value<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

fn is_false(value: &bool) -> bool {
    !value
}

fn is_true(value: &bool) -> bool {
    *value
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
        referrer_policy: NativeFetchReferrerPolicy,
        parser_inserted: bool,
    },
    ModuleInline {
        source: String,
        timing: NativePageScriptTiming,
        node_index: u32,
        nonce: Option<String>,
        referrer_policy: NativeFetchReferrerPolicy,
        parser_inserted: bool,
    },
    ModuleExternal {
        href: String,
        timing: NativePageScriptTiming,
        node_index: u32,
        nonce: Option<String>,
        integrity: Option<String>,
        crossorigin: Option<String>,
        referrer_policy: NativeFetchReferrerPolicy,
        parser_inserted: bool,
    },
    ImportMap {
        source: Option<String>,
        node_index: u32,
        nonce: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativePageImportMapSource {
    pub(crate) source: Option<String>,
    pub(crate) node_index: u32,
    pub(crate) nonce: Option<String>,
}

impl NativePageScriptSource {
    fn into_dynamic(mut self) -> Self {
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
            Self::ImportMap { .. } => {}
        }
        self
    }

    pub(crate) fn node_index(&self) -> u32 {
        match self {
            Self::Inline { node_index, .. }
            | Self::External { node_index, .. }
            | Self::ModuleInline { node_index, .. }
            | Self::ModuleExternal { node_index, .. }
            | Self::ImportMap { node_index, .. } => *node_index,
        }
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
    quirks_mode: super::html_parser::HtmlParsedQuirksMode,
    document_referrer_policy: NativeFetchReferrerPolicy,
    nodes: Vec<NativeNode>,
    stylesheet: NativeStylesheet,
    font_resources: Vec<NativeFontFaceResource>,
    font_book: NativeFontBook,
    external_stylesheet_states: BTreeMap<u32, NativeExternalStylesheetState>,
    external_image_preload_states: BTreeMap<u32, String>,
    preloaded_image_resources: BTreeMap<String, NativeImage>,
    computed_styles: Option<Vec<NativeComputedStyle>>,
    layout_style_cache: NativeLayoutStyleCache,
    css_target_fragment: Option<String>,
    viewport: Viewport,
    diagnostics: Vec<NativeDiagnostic>,
    diagnostics_truncated: bool,
    script_node_ids: BTreeMap<u32, NativeNodeId>,
    started_script_nodes: BTreeSet<u32>,
    processed_csp_meta_nodes: BTreeSet<u32>,
    pending_csp_meta_policies: BTreeMap<u32, String>,
    embedded_frame_owner_removal_sequences: BTreeMap<u32, u64>,
    next_embedded_frame_owner_removal_sequence: u64,
    image_resources: BTreeMap<u32, NativeImageResource>,
    image_loads: BTreeMap<u32, String>,
    video_poster_resources: BTreeMap<u32, NativeImageResource>,
    video_poster_loads: BTreeMap<u32, String>,
    media_resources: BTreeMap<u32, NativeMediaResource>,
    media_loads: BTreeMap<u32, String>,
    media_errors: BTreeMap<u32, String>,
    background_image_sources: BTreeMap<u32, String>,
    background_image_resources: BTreeMap<u32, NativeImageResource>,
    canvas_resources: BTreeMap<u32, NativeCanvasResource>,
    inline_style_element_reports: BTreeMap<u32, (String, Option<String>)>,
    inline_style_attribute_reports: BTreeMap<u32, String>,
}

/// Per-document memo of cascade results for layout queries. It is derived
/// state: clones get an independent copy, while document equality ignores it.
type CachedNativeScriptSnapshot = (u64, u64, usize, Viewport, NativeScriptDocumentSnapshot);
type CachedNativeTextWidths = HashMap<NativeTextWidthCacheKey, HashMap<String, u32>>;
type CachedNativeScrollProjection = (
    u64,
    u64,
    Viewport,
    NativePoint,
    BTreeMap<u32, NativePoint>,
    NativeLayoutSnapshot,
    Vec<NativeScriptGeometrySnapshot>,
);

#[derive(Debug, Default)]
struct NativeLayoutStyleCache(
    Mutex<HashMap<NativeNodeId, NativeComputedStyle>>,
    Mutex<HashMap<NativeNodeId, Arc<BTreeMap<u64, String>>>>,
    Mutex<Option<NativeLayoutSnapshot>>,
    Mutex<u64>,
    Mutex<Option<CachedNativeScriptSnapshot>>,
    Mutex<Option<CachedNativeScrollProjection>>,
    Mutex<CachedNativeTextWidths>,
    Mutex<usize>,
);

impl Clone for NativeLayoutStyleCache {
    fn clone(&self) -> Self {
        let styles = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let custom_properties = self
            .1
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let layout = self
            .2
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let snapshot_generation = *self
            .3
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let text_widths = self
            .6
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let text_width_cache_hits = *self
            .7
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self(
            Mutex::new(styles),
            Mutex::new(custom_properties),
            Mutex::new(layout),
            Mutex::new(snapshot_generation),
            Mutex::new(None),
            Mutex::new(None),
            Mutex::new(text_widths),
            Mutex::new(text_width_cache_hits),
        )
    }
}

impl PartialEq for NativeLayoutStyleCache {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for NativeLayoutStyleCache {}

impl NativeLayoutStyleCache {
    fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.1
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.invalidate_layout_snapshot();
    }

    fn invalidate_layout_snapshot(&self) {
        *self
            .2
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        self.invalidate_script_snapshot();
    }

    fn text_widths(&self) -> CachedNativeTextWidths {
        self.6
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn cache_text_widths(&self, widths: CachedNativeTextWidths, hits: usize) {
        *self
            .6
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = widths;
        let mut total_hits = self
            .7
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *total_hits = total_hits.saturating_add(hits);
    }

    fn clear_text_widths(&self) {
        self.6
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        *self
            .7
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = 0;
    }

    #[cfg(test)]
    fn text_width_cache_hits(&self) -> usize {
        *self
            .7
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn invalidate_script_snapshot(&self) {
        let mut generation = self
            .3
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *generation = generation.wrapping_add(1);
        *self
            .4
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .5
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    fn script_snapshot_generation(&self) -> u64 {
        *self
            .3
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn script_snapshot(
        &self,
        revision: u64,
        max_text_bytes: usize,
        viewport: Viewport,
    ) -> Option<NativeScriptDocumentSnapshot> {
        let generation = *self
            .3
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.4
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .filter(
                |(cached_revision, cached_generation, cached_limit, cached_viewport, _)| {
                    *cached_revision == revision
                        && *cached_generation == generation
                        && *cached_limit == max_text_bytes
                        && *cached_viewport == viewport
                },
            )
            .map(|(_, _, _, _, snapshot)| snapshot.clone())
    }

    fn cache_script_snapshot(
        &self,
        revision: u64,
        max_text_bytes: usize,
        viewport: Viewport,
        snapshot: NativeScriptDocumentSnapshot,
    ) {
        let generation = *self
            .3
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *self
            .4
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some((revision, generation, max_text_bytes, viewport, snapshot));
    }

    fn layout(&self, revision: u64, viewport: Viewport) -> Option<NativeLayoutSnapshot> {
        self.2
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .filter(|layout| layout.revision == revision && layout.viewport == viewport)
            .cloned()
    }

    fn cache_layout(&self, layout: NativeLayoutSnapshot) {
        *self
            .2
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(layout);
    }

    fn scrolled_script_projection(
        &self,
        revision: u64,
        viewport: Viewport,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) -> Option<(NativeLayoutSnapshot, Vec<NativeScriptGeometrySnapshot>)> {
        let generation = *self
            .3
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.5
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .filter(
                |(
                    cached_revision,
                    cached_generation,
                    cached_viewport,
                    cached_scroll,
                    cached_nested,
                    _,
                    _,
                )| {
                    *cached_revision == revision
                        && *cached_generation == generation
                        && *cached_viewport == viewport
                        && *cached_scroll == scroll_offset
                        && cached_nested == nested_scroll_offsets
                },
            )
            .map(|(_, _, _, _, _, layout, geometry)| (layout.clone(), geometry.clone()))
    }

    fn cache_scrolled_script_projection(
        &self,
        revision: u64,
        viewport: Viewport,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
        layout: NativeLayoutSnapshot,
        geometry: Vec<NativeScriptGeometrySnapshot>,
    ) {
        const MAX_RETAINED_BYTES: usize = 16 * 1024 * 1024;
        let retained_bytes = layout
            .retained_bytes()
            .saturating_add(
                geometry
                    .capacity()
                    .saturating_mul(std::mem::size_of::<NativeScriptGeometrySnapshot>()),
            )
            .saturating_add(
                nested_scroll_offsets
                    .len()
                    .saturating_mul(std::mem::size_of::<(u32, NativePoint)>()),
            );
        if retained_bytes > MAX_RETAINED_BYTES {
            return;
        }
        let generation = *self
            .3
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *self
            .5
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((
            revision,
            generation,
            viewport,
            scroll_offset,
            nested_scroll_offsets.clone(),
            layout,
            geometry,
        ));
    }
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
    video_frames: Vec<NativeVideoFrame>,
    current_time_millis: u64,
    is_playing: bool,
    playback_rate_millionths: i64,
    playback_started_at: Option<std::time::Instant>,
}

fn default_media_playback_rate_millionths() -> i64 {
    1_000_000
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

fn decode_content_image_pixels(
    raw_base64: &str,
    zlib_base64: &str,
    expected_bytes: usize,
    label: &str,
) -> Result<Vec<u8>, NativeEngineError> {
    if raw_base64.is_empty() == zlib_base64.is_empty() {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: format!("content process returned ambiguous {label}"),
        });
    }
    let encoded = base64::engine::general_purpose::STANDARD
        .decode(if zlib_base64.is_empty() {
            raw_base64
        } else {
            zlib_base64
        })
        .map_err(|_| NativeEngineError::Parse {
            offset: 0,
            reason: format!("content process returned invalid {label}"),
        })?;
    let decoded = if zlib_base64.is_empty() {
        encoded
    } else {
        let mut decoder = ZlibDecoder::new(encoded.as_slice());
        let mut decoded = Vec::with_capacity(expected_bytes);
        decoder
            .by_ref()
            .take(expected_bytes.saturating_add(1) as u64)
            .read_to_end(&mut decoded)
            .map_err(|_| NativeEngineError::Parse {
                offset: 0,
                reason: format!("content process returned malformed compressed {label}"),
            })?;
        decoded
    };
    if decoded.len() != expected_bytes {
        return Err(NativeEngineError::Parse {
            offset: 0,
            reason: format!("content process returned {label} with the wrong dimensions"),
        });
    }
    Ok(decoded)
}

pub(crate) fn image_from_wire(
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
        .pixels_base64
        .len()
        .checked_add(resource.pixels_zlib_base64.len())
        .and_then(|total| {
            resource.frames.iter().try_fold(total, |total, frame| {
                total
                    .checked_add(frame.pixels_base64.len())
                    .and_then(|total| total.checked_add(frame.pixels_zlib_base64.len()))
            })
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
    let pixels = decode_content_image_pixels(
        &resource.pixels_base64,
        &resource.pixels_zlib_base64,
        expected_bytes,
        "image pixels",
    )?;
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
        let frame_pixels = decode_content_image_pixels(
            &frame.pixels_base64,
            &frame.pixels_zlib_base64,
            expected_bytes,
            "image frame pixels",
        )?;
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

fn move_node_indexed_entries<T>(
    source: &mut BTreeMap<u32, T>,
    destination: &mut BTreeMap<u32, T>,
    indexes: &BTreeMap<u32, u32>,
) {
    for (source_index, destination_index) in indexes {
        if let Some(value) = source.remove(source_index) {
            destination.insert(*destination_index, value);
        }
    }
}

#[path = "dom/document.rs"]
mod document;

mod actions;
mod resource_state;
#[cfg(test)]
mod tokenizer;
#[cfg(test)]
use tokenizer::*;

mod image_sources;
use image_sources::*;

mod html_tree_builder;
use html_tree_builder::*;

#[cfg(test)]
#[path = "dom/tests.rs"]
mod tests;
