//! Bounded diagnostics for intentionally unsupported native-engine input.

/// Maximum number of diagnostics retained for one native document.
pub const MAX_NATIVE_DIAGNOSTICS: usize = 256;
/// Maximum UTF-8 byte length of one sanitized diagnostic detail token.
pub const MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES: usize = 64;

/// Stable category for one native-engine diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeDiagnosticCode {
    UnsupportedCssSelector,
    UnsupportedCssProperty,
    UnsupportedCssValue,
    MalformedCss,
}

/// Bounded source identity for a CSS diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeDiagnosticSource {
    Stylesheet { index: usize },
    InlineStyle { node_index: u32 },
}

/// One sanitized, read-only native-engine diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDiagnostic {
    pub code: NativeDiagnosticCode,
    pub source: NativeDiagnosticSource,
    pub offset: usize,
    pub detail: String,
}

impl NativeDiagnostic {
    pub(crate) fn new(
        code: NativeDiagnosticCode,
        source: NativeDiagnosticSource,
        offset: usize,
        detail: &str,
    ) -> Self {
        Self {
            code,
            source,
            offset,
            detail: sanitize_detail(detail),
        }
    }
}

/// Internal bounded collector shared by stylesheet and inline-style parsing.
#[derive(Debug, Default)]
pub(crate) struct NativeDiagnosticSink {
    diagnostics: Vec<NativeDiagnostic>,
    truncated: bool,
}

impl NativeDiagnosticSink {
    pub(crate) fn push(
        &mut self,
        code: NativeDiagnosticCode,
        source: NativeDiagnosticSource,
        offset: usize,
        detail: &str,
    ) {
        if self.diagnostics.len() >= MAX_NATIVE_DIAGNOSTICS {
            self.truncated = true;
            return;
        }
        self.diagnostics
            .push(NativeDiagnostic::new(code, source, offset, detail));
    }

    pub(crate) fn finish(self) -> (Vec<NativeDiagnostic>, bool) {
        (self.diagnostics, self.truncated)
    }
}

fn sanitize_detail(detail: &str) -> String {
    let mut sanitized = String::new();
    for character in detail.chars() {
        let character = if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        {
            character.to_ascii_lowercase()
        } else {
            '-'
        };
        if sanitized.len().saturating_add(character.len_utf8()) > MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES
        {
            break;
        }
        sanitized.push(character);
    }
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "unknown".into()
    } else {
        sanitized.into()
    }
}
