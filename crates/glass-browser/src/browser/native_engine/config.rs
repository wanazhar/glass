use super::browsing_context::NATIVE_CONTEXT_ID;
use super::error::NativeEngineError;
use std::path::PathBuf;
use url::Url;

/// Maximum source document size accepted by the native resource loader.
pub const MAX_NATIVE_DOCUMENT_BYTES: usize = 256 * 1024;
/// Maximum DOM nodes accepted by the initial arena.
pub const MAX_NATIVE_NODES: usize = 4_096;
/// Maximum open element depth accepted by the initial tree builder.
pub const MAX_NATIVE_DOM_DEPTH: usize = 128;
/// Maximum history entries retained by one native context.
pub const MAX_NATIVE_HISTORY_ENTRIES: usize = 64;
/// Maximum queued deterministic tasks.
pub const MAX_NATIVE_SCHEDULER_TASKS: usize = 256;
/// Maximum registered local fixtures.
pub const MAX_NATIVE_FIXTURES: usize = 32;
/// Maximum viewport dimension accepted before layout exists.
pub const MAX_NATIVE_VIEWPORT_DIMENSION: u32 = 16_384;
/// Maximum persisted `window.name` text retained by one native browsing
/// context. Names are browser-visible state but are not allowed to become an
/// unbounded IPC or target-routing payload.
pub(crate) const MAX_NATIVE_WINDOW_NAME_BYTES: usize = 256;

/// Viewport inputs reserved for future layout and rendering phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    /// Device scale factor in thousandths; `1000` means `1.0`.
    pub device_scale_factor_milli: u32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            device_scale_factor_milli: 1000,
        }
    }
}

impl Viewport {
    pub fn validate(&self) -> Result<(), NativeEngineError> {
        if self.width == 0 || self.width > MAX_NATIVE_VIEWPORT_DIMENSION {
            return Err(NativeEngineError::invalid(
                "viewport width",
                format!("must be between 1 and {MAX_NATIVE_VIEWPORT_DIMENSION}"),
            ));
        }
        if self.height == 0 || self.height > MAX_NATIVE_VIEWPORT_DIMENSION {
            return Err(NativeEngineError::invalid(
                "viewport height",
                format!("must be between 1 and {MAX_NATIVE_VIEWPORT_DIMENSION}"),
            ));
        }
        if self.device_scale_factor_milli == 0 || self.device_scale_factor_milli > 4000 {
            return Err(NativeEngineError::invalid(
                "viewport device scale factor",
                "must be between 1 and 4000 thousandths",
            ));
        }
        Ok(())
    }
}

/// Hard resource and state limits for one native engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEngineLimits {
    pub max_document_bytes: usize,
    pub max_nodes: usize,
    pub max_dom_depth: usize,
    pub max_text_bytes: usize,
    pub max_history_entries: usize,
    pub max_scheduler_tasks: usize,
    pub max_fixtures: usize,
}

impl Default for NativeEngineLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: MAX_NATIVE_DOCUMENT_BYTES,
            max_nodes: MAX_NATIVE_NODES,
            max_dom_depth: MAX_NATIVE_DOM_DEPTH,
            max_text_bytes: crate::browser_backend::MAX_TEXT_BYTES,
            max_history_entries: MAX_NATIVE_HISTORY_ENTRIES,
            max_scheduler_tasks: MAX_NATIVE_SCHEDULER_TASKS,
            max_fixtures: MAX_NATIVE_FIXTURES,
        }
    }
}

impl NativeEngineLimits {
    pub fn validate(&self) -> Result<(), NativeEngineError> {
        validate_positive_bounded(
            "max document bytes",
            self.max_document_bytes,
            MAX_NATIVE_DOCUMENT_BYTES,
        )?;
        validate_positive_bounded("max nodes", self.max_nodes, MAX_NATIVE_NODES)?;
        validate_positive_bounded("max DOM depth", self.max_dom_depth, MAX_NATIVE_DOM_DEPTH)?;
        validate_positive_bounded(
            "max text bytes",
            self.max_text_bytes,
            crate::browser_backend::MAX_TEXT_BYTES,
        )?;
        validate_positive_bounded(
            "max history entries",
            self.max_history_entries,
            MAX_NATIVE_HISTORY_ENTRIES,
        )?;
        validate_positive_bounded(
            "max scheduler tasks",
            self.max_scheduler_tasks,
            MAX_NATIVE_SCHEDULER_TASKS,
        )?;
        validate_positive_bounded("max fixtures", self.max_fixtures, MAX_NATIVE_FIXTURES)?;
        Ok(())
    }
}

fn validate_positive_bounded(
    field: &str,
    value: usize,
    maximum: usize,
) -> Result<(), NativeEngineError> {
    if value == 0 || value > maximum {
        return Err(NativeEngineError::invalid(
            field,
            format!("must be between 1 and {maximum}"),
        ));
    }
    Ok(())
}

/// A checked-in or caller-supplied local HTML fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFixture {
    pub url: String,
    pub html: String,
}

impl NativeFixture {
    pub fn new(url: impl Into<String>, html: impl Into<String>) -> Result<Self, NativeEngineError> {
        let url = canonical_fixture_url(&url.into())?;
        Ok(Self {
            url,
            html: html.into(),
        })
    }
}

/// Startup configuration for one native engine instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEngineConfig {
    pub context_id: String,
    pub opener_context_id: Option<String>,
    pub opener_window_name: String,
    pub window_name: String,
    pub initial_url: String,
    pub viewport: Viewport,
    pub limits: NativeEngineLimits,
    pub fixtures: Vec<NativeFixture>,
    pub storage_path: Option<PathBuf>,
}

impl Default for NativeEngineConfig {
    fn default() -> Self {
        Self {
            context_id: NATIVE_CONTEXT_ID.into(),
            opener_context_id: None,
            opener_window_name: String::new(),
            window_name: String::new(),
            initial_url: "about:blank".into(),
            viewport: Viewport::default(),
            limits: NativeEngineLimits::default(),
            fixtures: Vec::new(),
            storage_path: None,
        }
    }
}

impl NativeEngineConfig {
    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = context_id.into();
        self
    }

    pub fn with_opener_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.opener_context_id = Some(context_id.into());
        self
    }

    pub fn with_opener_window_name(mut self, name: impl Into<String>) -> Self {
        self.opener_window_name = name.into();
        self
    }

    pub fn with_window_name(mut self, name: impl Into<String>) -> Self {
        self.window_name = name.into();
        self
    }

    pub fn with_initial_url(mut self, url: impl Into<String>) -> Self {
        self.initial_url = url.into();
        self
    }

    pub fn with_viewport(mut self, viewport: Viewport) -> Self {
        self.viewport = viewport;
        self
    }

    pub fn with_limits(mut self, limits: NativeEngineLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_storage_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.storage_path = Some(path.into());
        self
    }

    pub fn with_fixture(
        mut self,
        url: impl Into<String>,
        html: impl Into<String>,
    ) -> Result<Self, NativeEngineError> {
        self.fixtures.push(NativeFixture::new(url, html)?);
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), NativeEngineError> {
        self.limits.validate()?;
        self.viewport.validate()?;
        validate_context_id(&self.context_id)?;
        if let Some(opener_context_id) = self.opener_context_id.as_deref() {
            validate_context_id(opener_context_id)?;
        }
        validate_window_name(&self.opener_window_name)?;
        validate_window_name(&self.window_name)?;
        validate_url_text("initial URL", &self.initial_url)?;
        if self
            .storage_path
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(NativeEngineError::invalid(
                "storage path",
                "must not be empty",
            ));
        }
        if self
            .storage_path
            .as_ref()
            .is_some_and(|path| path.to_str().is_none())
        {
            return Err(NativeEngineError::invalid(
                "storage path",
                "must be valid UTF-8 for content-process transfer",
            ));
        }
        if self
            .storage_path
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(NativeEngineError::invalid(
                "storage path",
                "must be absolute for sandboxed content-process transfer",
            ));
        }
        if !is_supported_url_shape(&self.initial_url) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native navigation accepts about:blank, data:text/html, fixture://, or HTTP(S) URLs".into(),
            });
        }
        if self.fixtures.len() > self.limits.max_fixtures {
            return Err(NativeEngineError::limit(
                "registered fixtures",
                self.limits.max_fixtures,
                self.fixtures.len(),
            ));
        }
        let mut urls = std::collections::BTreeSet::new();
        for fixture in &self.fixtures {
            let canonical = canonical_fixture_url(&fixture.url)?;
            if !urls.insert(canonical) {
                return Err(NativeEngineError::invalid(
                    "fixtures",
                    "fixture URLs must be unique",
                ));
            }
            if fixture.html.len() > self.limits.max_document_bytes {
                return Err(NativeEngineError::limit(
                    "fixture document",
                    self.limits.max_document_bytes,
                    fixture.html.len(),
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_window_name(value: &str) -> Result<(), NativeEngineError> {
    if value.len() > MAX_NATIVE_WINDOW_NAME_BYTES {
        return Err(NativeEngineError::limit(
            "window name",
            MAX_NATIVE_WINDOW_NAME_BYTES,
            value.len(),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(NativeEngineError::invalid(
            "window name",
            "must not contain control characters",
        ));
    }
    Ok(())
}

pub(crate) fn validate_context_id(value: &str) -> Result<(), NativeEngineError> {
    if value.is_empty() {
        return Err(NativeEngineError::invalid(
            "context id",
            "must not be empty",
        ));
    }
    if value.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "context id",
            crate::browser_backend::MAX_BACKEND_ID_BYTES,
            value.len(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_url_text(field: &str, value: &str) -> Result<(), NativeEngineError> {
    if value.is_empty() {
        return Err(NativeEngineError::invalid(field, "must not be empty"));
    }
    if value.len() > crate::browser_backend::MAX_TEXT_BYTES {
        return Err(NativeEngineError::limit(
            format!("{field} bytes"),
            crate::browser_backend::MAX_TEXT_BYTES,
            value.len(),
        ));
    }
    if !value.is_char_boundary(value.len()) {
        return Err(NativeEngineError::invalid(field, "must be valid UTF-8"));
    }
    Ok(())
}

pub(crate) fn is_supported_url_shape(value: &str) -> bool {
    let resource_url = without_fragment(value);
    resource_url == "about:blank"
        || resource_url.starts_with("data:")
        || resource_url.starts_with("fixture:")
        || is_network_url(resource_url)
}

pub(crate) fn is_network_url(value: &str) -> bool {
    Url::parse(without_fragment(value)).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

/// Return the resource portion of a URL, excluding raw fragment metadata.
pub(crate) fn without_fragment(value: &str) -> &str {
    value
        .split_once('#')
        .map_or(value, |(resource, _)| resource)
}

/// Decode one bounded URL fragment into UTF-8 without applying form semantics.
///
/// Unescaped bytes, including `+`, remain unchanged. A malformed escape or a
/// byte sequence that is not UTF-8 is an unresolved fragment target rather
/// than a partially decoded identifier.
pub(crate) fn decode_percent_encoded_fragment(value: &str) -> Option<String> {
    let source = value.as_bytes();
    let mut decoded = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index] != b'%' {
            decoded.push(source[index]);
            index += 1;
            continue;
        }
        if index.saturating_add(2) >= source.len() {
            return None;
        }
        let high = hex_value(source[index + 1])?;
        let low = hex_value(source[index + 2])?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(decoded).ok()
}

/// The bounded decoded text-fragment terms used by the native matcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextFragmentTerms {
    pub(crate) prefix: Option<String>,
    pub(crate) start: String,
    pub(crate) end: Option<String>,
    pub(crate) suffix: Option<String>,
}

/// Decode the bounded text-fragment grammar without applying form semantics or
/// treating encoded commas as separators.
pub(crate) fn decode_text_fragment_terms(value: &str) -> Option<TextFragmentTerms> {
    let payload = value.strip_prefix(":~:text=")?;
    let raw_terms = payload.split(',').collect::<Vec<_>>();
    if raw_terms.is_empty() || raw_terms.len() > 4 || raw_terms.iter().any(|term| term.is_empty()) {
        return None;
    }
    let prefix_raw = raw_terms
        .first()
        .filter(|term| term.ends_with('-'))
        .map(|term| &term[..term.len().saturating_sub(1)]);
    let suffix_raw = raw_terms
        .last()
        .filter(|term| term.starts_with('-'))
        .map(|term| &term[1..]);
    if prefix_raw.is_some_and(str::is_empty) || suffix_raw.is_some_and(str::is_empty) {
        return None;
    }
    let core_start = usize::from(prefix_raw.is_some());
    let core_end = raw_terms
        .len()
        .saturating_sub(usize::from(suffix_raw.is_some()));
    let core = raw_terms.get(core_start..core_end)?;
    if !(1..=2).contains(&core.len())
        || core
            .iter()
            .any(|term| term.starts_with('-') || term.ends_with('-'))
    {
        return None;
    }
    let decode_term = |term: &str| {
        let decoded = decode_percent_encoded_fragment(term)?;
        (!decoded.is_empty()).then_some(decoded)
    };
    let prefix = prefix_raw.and_then(decode_term);
    if prefix_raw.is_some() && prefix.is_none() {
        return None;
    }
    let suffix = suffix_raw.and_then(decode_term);
    if suffix_raw.is_some() && suffix.is_none() {
        return None;
    }
    let start = decode_term(core[0])?;
    let end = core.get(1).copied().and_then(decode_term);
    if core.len() == 2 && end.is_none() {
        return None;
    }
    Some(TextFragmentTerms {
        prefix,
        start,
        end,
        suffix,
    })
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn canonical_fixture_url(value: &str) -> Result<String, NativeEngineError> {
    validate_url_text("fixture URL", value)?;
    let parsed = Url::parse(value).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "fixture URL is not valid URL syntax".into(),
    })?;
    if parsed.scheme() != "fixture" || parsed.host_str().is_none() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "fixture URLs require the fixture:// scheme and a host".into(),
        });
    }
    if parsed.fragment().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "registered fixture URLs must not contain a fragment".into(),
        });
    }
    Ok(parsed.to_string())
}

/// Resolve a non-absolute link reference against a registered fixture base.
///
/// Relative navigation is deliberately narrower than URL parsing in general:
/// it is available only while the current document has a fixture origin, and
/// the resolved reference must remain on that exact fixture host and port.
pub(crate) fn resolve_fixture_relative_url(
    base_url: &str,
    reference: &str,
) -> Result<String, NativeEngineError> {
    validate_url_text("link href", reference)?;
    let base =
        Url::parse(without_fragment(base_url)).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "relative link resolution requires a registered fixture base".into(),
        })?;
    if base.scheme() != "fixture" || base.host_str().is_none() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "relative link resolution requires a registered fixture base".into(),
        });
    }
    let resolved = base
        .join(reference)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "relative link reference is malformed".into(),
        })?;
    if resolved.scheme() != "fixture"
        || resolved.host_str() != base.host_str()
        || resolved.port() != base.port()
        || !resolved.username().is_empty()
        || resolved.password().is_some()
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "relative link must remain on the current fixture host".into(),
        });
    }
    let resolved = resolved.to_string();
    validate_url_text("link target URL", &resolved)?;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::{
        NativeEngineError, TextFragmentTerms, decode_percent_encoded_fragment,
        decode_text_fragment_terms, resolve_fixture_relative_url,
    };

    #[test]
    fn fragment_percent_decoding_is_utf8_and_not_form_encoded() {
        assert_eq!(
            decode_percent_encoded_fragment("pricing%20plan"),
            Some("pricing plan".into())
        );
        assert_eq!(
            decode_percent_encoded_fragment("caf%C3%A9"),
            Some("café".into())
        );
        assert_eq!(decode_percent_encoded_fragment("a+b"), Some("a+b".into()));
        assert_eq!(decode_percent_encoded_fragment("bad%ZZ"), None);
        assert_eq!(decode_percent_encoded_fragment("bad%C3"), None);
    }

    #[test]
    fn text_fragment_terms_decode_per_term_and_reject_extended_syntax() {
        assert_eq!(
            decode_text_fragment_terms(":~:text=target%20phrase"),
            Some(TextFragmentTerms {
                prefix: None,
                start: "target phrase".into(),
                end: None,
                suffix: None,
            })
        );
        assert_eq!(
            decode_text_fragment_terms(":~:text=target%2C%20phrase,end"),
            Some(TextFragmentTerms {
                prefix: None,
                start: "target, phrase".into(),
                end: Some("end".into()),
                suffix: None,
            })
        );
        assert_eq!(
            decode_text_fragment_terms(":~:text=a%2Bb"),
            Some(TextFragmentTerms {
                prefix: None,
                start: "a+b".into(),
                end: None,
                suffix: None,
            })
        );
        assert_eq!(
            decode_text_fragment_terms(":~:text=prefix%20-,target%20phrase,anchor,-%20suffix"),
            Some(TextFragmentTerms {
                prefix: Some("prefix ".into()),
                start: "target phrase".into(),
                end: Some("anchor".into()),
                suffix: Some(" suffix".into()),
            })
        );
        for value in [
            ":~:text=prefix-,target",
            ":~:text=target,-suffix",
            ":~:text=prefix-,target,-suffix",
            ":~:text=prefix-,target,end",
            ":~:text=target,end,-suffix",
        ] {
            assert!(decode_text_fragment_terms(value).is_some(), "value={value}");
        }
        for value in [
            ":~:text=",
            ":~:text=target,",
            ":~:text=target,end,extra",
            ":~:text=prefix-,target,end,extra",
            ":~:text=prefix-,target,-suffix,extra",
            ":~:text=prefix-,target,end,-",
            ":~:text=prefix-,target%ZZ",
            ":~:text=bad%ZZ",
        ] {
            assert_eq!(decode_text_fragment_terms(value), None, "value={value}");
        }
    }

    #[test]
    fn fixture_relative_resolution_normalizes_and_keeps_the_current_host() {
        assert_eq!(
            resolve_fixture_relative_url(
                "fixture://site.test/docs/index#old",
                "./child/../next?mode=fast#target",
            )
            .unwrap(),
            "fixture://site.test/docs/next?mode=fast#target"
        );
    }

    #[test]
    fn fixture_relative_resolution_rejects_opaque_or_cross_host_references() {
        for (base, reference) in [
            ("data:text/html,%3Cp%3Eopaque%3C%2Fp%3E", "next"),
            ("fixture://site.test/docs/index", "//other.test/next"),
            ("fixture://site.test/docs/index", "//["),
        ] {
            let error = resolve_fixture_relative_url(base, reference).unwrap_err();
            assert!(matches!(error, NativeEngineError::UnsupportedUrl { .. }));
            assert!(!error.to_string().contains(reference));
        }
    }
}
