use super::error::NativeEngineError;
use url::Url;

/// Maximum source document size accepted by the native Phase 1 loader.
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
    pub initial_url: String,
    pub viewport: Viewport,
    pub limits: NativeEngineLimits,
    pub fixtures: Vec<NativeFixture>,
}

impl Default for NativeEngineConfig {
    fn default() -> Self {
        Self {
            initial_url: "about:blank".into(),
            viewport: Viewport::default(),
            limits: NativeEngineLimits::default(),
            fixtures: Vec::new(),
        }
    }
}

impl NativeEngineConfig {
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
        validate_url_text("initial URL", &self.initial_url)?;
        if !is_supported_url_shape(&self.initial_url) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "Phase 1 accepts only about:blank, data:text/html, or fixture:// URLs"
                    .into(),
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
}

/// Return the resource portion of a URL, excluding raw fragment metadata.
pub(crate) fn without_fragment(value: &str) -> &str {
    value
        .split_once('#')
        .map_or(value, |(resource, _)| resource)
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
    use super::{NativeEngineError, resolve_fixture_relative_url};

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
