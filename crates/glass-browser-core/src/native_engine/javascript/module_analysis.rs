use super::*;

/// Extract the bounded static import/export specifiers from a module source.
///
/// This is intentionally a lexical prefetch pass, not a replacement for the
/// JavaScript parser. QuickJS remains authoritative for module grammar and
/// evaluation; this pass only discovers URLs that the content process must
/// fetch before installing the in-memory module loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeStaticModuleRequest {
    pub(crate) specifier: String,
    pub(crate) module_type: NativeModuleResourceType,
    pub(crate) specifier_span: Option<(usize, usize)>,
}

/// Extract statically declared `importScripts()` string arguments. The host
/// preloads these dependencies before evaluating the worker root source, so a
/// normal top-level import remains synchronous from the worker's perspective.
/// Non-literal calls fail explicitly in the worker bootstrap instead of
/// receiving an implicit network capability.
pub(crate) fn static_worker_import_specifiers(
    source: &str,
) -> Result<Vec<String>, NativeEngineError> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut specifiers = Vec::new();
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() {
            break;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if !is_javascript_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
            index += 1;
        }
        if &bytes[start..index] != b"importScripts"
            || bytes.get(start.wrapping_sub(1)) == Some(&b'.')
        {
            continue;
        }
        let open = skip_javascript_space_and_comments(bytes, index);
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let mut cursor = skip_javascript_space_and_comments(bytes, open + 1);
        if bytes.get(cursor) == Some(&b')') {
            index = cursor + 1;
            continue;
        }
        loop {
            let Some((specifier, next)) = read_javascript_string(bytes, cursor) else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "native Worker importScripts requires string literal URLs".into(),
                });
            };
            if specifier.is_empty() {
                return Err(NativeEngineError::invalid(
                    "Worker importScripts URL",
                    "must not be empty",
                ));
            }
            specifiers.push(specifier);
            if specifiers.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "Worker importScripts dependencies",
                    MAX_NATIVE_MODULE_IMPORTS,
                    specifiers.len(),
                ));
            }
            cursor = skip_javascript_space_and_comments(bytes, next);
            match bytes.get(cursor) {
                Some(&b',') => {
                    cursor = skip_javascript_space_and_comments(bytes, cursor + 1);
                }
                Some(&b')') => {
                    index = cursor + 1;
                    break;
                }
                _ => {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "native Worker importScripts call has invalid arguments".into(),
                    });
                }
            }
        }
    }
    Ok(specifiers)
}

fn module_request_after_keyword(
    bytes: &[u8],
    keyword_end: usize,
    import_keyword: bool,
) -> Option<NativeStaticModuleRequest> {
    let start = skip_javascript_space_and_comments(bytes, keyword_end);
    if import_keyword && bytes.get(start) == Some(&b'(') {
        return None;
    }
    let (specifier, end, specifier_span) = if bytes
        .get(start)
        .is_some_and(|byte| matches!(byte, b'\'' | b'"'))
    {
        let (specifier, end) = read_javascript_string(bytes, start)?;
        (specifier, end, (start, end))
    } else {
        let mut index = start;
        loop {
            index = skip_javascript_space_and_comments(bytes, index);
            if index >= bytes.len() || bytes[index] == b';' {
                return None;
            }
            if matches!(bytes[index], b'\'' | b'"' | b'`') {
                index = skip_javascript_string(bytes, index);
                continue;
            }
            if is_javascript_identifier_start(bytes[index]) {
                let identifier_start = index;
                index += 1;
                while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
                    index += 1;
                }
                if &bytes[identifier_start..index] == b"from" {
                    let specifier_start = skip_javascript_space_and_comments(bytes, index);
                    let (specifier, end) = read_javascript_string(bytes, specifier_start)?;
                    break (specifier, end, (specifier_start, end));
                }
                continue;
            }
            index += 1;
        }
    };
    Some(NativeStaticModuleRequest {
        specifier,
        module_type: static_module_resource_type_after(bytes, end),
        specifier_span: Some(specifier_span),
    })
}

fn static_module_resource_type_after(bytes: &[u8], end: usize) -> NativeModuleResourceType {
    let start = skip_javascript_space_and_comments(bytes, end);
    let mut keyword_end = start;
    while keyword_end < bytes.len() && is_javascript_identifier_continue(bytes[keyword_end]) {
        keyword_end += 1;
    }
    if &bytes[start..keyword_end] == b"assert" {
        return NativeModuleResourceType::Unsupported;
    }
    if &bytes[start..keyword_end] != b"with" {
        return NativeModuleResourceType::JavaScript;
    }
    let object_start = skip_javascript_space_and_comments(bytes, keyword_end);
    if bytes.get(object_start) != Some(&b'{') {
        return NativeModuleResourceType::Unsupported;
    }
    let mut index = object_start + 1;
    let mut module_type = None;
    let mut unsupported_attribute = false;
    loop {
        index = skip_javascript_space_and_comments(bytes, index);
        if bytes.get(index) == Some(&b'}') {
            break;
        }
        let Some(&first) = bytes.get(index) else {
            return NativeModuleResourceType::Unsupported;
        };
        let (key, key_end) = if matches!(first, b'\'' | b'"') {
            let Some((key, key_end)) = read_javascript_string(bytes, index) else {
                return NativeModuleResourceType::Unsupported;
            };
            (key, key_end)
        } else if is_javascript_identifier_start(first) {
            let key_start = index;
            index += 1;
            while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
                index += 1;
            }
            (
                String::from_utf8_lossy(&bytes[key_start..index]).into_owned(),
                index,
            )
        } else {
            return NativeModuleResourceType::Unsupported;
        };
        index = skip_javascript_space_and_comments(bytes, key_end);
        if bytes.get(index) != Some(&b':') {
            return NativeModuleResourceType::Unsupported;
        }
        let value_start = skip_javascript_space_and_comments(bytes, index + 1);
        let Some((value, value_end)) = read_javascript_string(bytes, value_start) else {
            return NativeModuleResourceType::Unsupported;
        };
        if key == "type" {
            module_type = Some(match value.as_str() {
                "json" => NativeModuleResourceType::Json,
                _ => NativeModuleResourceType::Unsupported,
            });
        } else {
            unsupported_attribute = true;
        }
        index = skip_javascript_space_and_comments(bytes, value_end);
        match bytes.get(index) {
            Some(b',') => index += 1,
            Some(b'}') => break,
            _ => return NativeModuleResourceType::Unsupported,
        }
    }
    if unsupported_attribute {
        NativeModuleResourceType::Unsupported
    } else {
        module_type.unwrap_or(NativeModuleResourceType::JavaScript)
    }
}

fn skip_javascript_space_and_comments(bytes: &[u8], mut index: usize) -> usize {
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        return index;
    }
}

fn skip_javascript_string(bytes: &[u8], mut index: usize) -> usize {
    let Some(&quote) = bytes.get(index) else {
        return index;
    };
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = index.saturating_add(2);
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += 1;
        }
    }
    index
}

fn read_javascript_string(bytes: &[u8], mut index: usize) -> Option<(String, usize)> {
    let quote = *bytes.get(index)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    index += 1;
    let mut value = String::new();
    while index < bytes.len() {
        match bytes[index] {
            byte if byte == quote => return Some((value, index + 1)),
            b'\\' if index + 1 < bytes.len() => {
                value.push(bytes[index + 1] as char);
                index += 2;
            }
            byte => {
                value.push(byte as char);
                index += 1;
            }
        }
    }
    None
}

fn is_javascript_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_javascript_identifier_continue(byte: u8) -> bool {
    is_javascript_identifier_start(byte) || byte.is_ascii_digit()
}

pub(crate) fn static_module_requests(
    source: &str,
) -> Result<Vec<NativeStaticModuleRequest>, NativeEngineError> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut requests = Vec::new();
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() {
            break;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if !is_javascript_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
            index += 1;
        }
        let keyword = &bytes[start..index];
        if keyword != b"import" && keyword != b"export" {
            continue;
        }
        if let Some(request) = module_request_after_keyword(bytes, index, keyword == b"import") {
            if request.specifier.is_empty() {
                return Err(NativeEngineError::invalid(
                    "module import",
                    "module specifier must not be empty",
                ));
            }
            requests.push(request);
            if requests.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "module imports",
                    MAX_NATIVE_MODULE_IMPORTS,
                    requests.len(),
                ));
            }
        }
    }
    Ok(requests)
}

pub(super) fn rewrite_static_json_module_specifiers(
    source: &str,
) -> Result<String, NativeEngineError> {
    let requests = static_module_requests(source)?;
    let mut output = String::with_capacity(source.len());
    let mut copied_until = 0;
    let mut rewritten = false;
    for request in requests {
        if request.module_type != NativeModuleResourceType::Json {
            continue;
        }
        let Some((start, end)) = request.specifier_span else {
            continue;
        };
        if start < copied_until || end < start || end > source.len() {
            return Err(NativeEngineError::invalid(
                "static JSON module request",
                "specifier range is invalid",
            ));
        }
        output.push_str(&source[copied_until..start]);
        let internal_specifier =
            format!("{NATIVE_JSON_MODULE_REQUEST_PREFIX}{}", request.specifier);
        let internal_literal = serde_json::to_string(&internal_specifier).map_err(|_| {
            NativeEngineError::invalid(
                "static JSON module request",
                "specifier could not be encoded",
            )
        })?;
        output.push_str(&internal_literal);
        copied_until = end;
        rewritten = true;
    }
    if !rewritten {
        return Ok(source.to_owned());
    }
    output.push_str(&source[copied_until..]);
    if output.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "transformed JavaScript source",
            MAX_NATIVE_SCRIPT_BYTES,
            output.len(),
        ));
    }
    Ok(output)
}

pub(super) fn rewrite_runtime_dynamic_module_imports(
    source: &str,
    referrer_expression: &str,
) -> Result<String, NativeEngineError> {
    rewrite_runtime_dynamic_module_imports_with_count(source, referrer_expression)
        .map(|(rewritten, _)| rewritten)
}

pub(super) fn rewrite_runtime_dynamic_module_imports_with_policy(
    source: &str,
    referrer_expression: &str,
    referrer_policy_expression: &str,
) -> Result<String, NativeEngineError> {
    rewrite_runtime_dynamic_module_imports_with_count_and_policy(
        source,
        referrer_expression,
        referrer_policy_expression,
    )
    .map(|(rewritten, _)| rewritten)
}

pub(super) fn native_module_referrer_policy_expression(
    referrer_policy: Option<NativeFetchReferrerPolicy>,
) -> String {
    // The enum serializes only the fixed Referrer Policy tokens recognized by
    // the native Fetch implementation, so each value is safe as a JS literal.
    referrer_policy
        .map(|policy| format!("'{}'", policy.as_str()))
        .unwrap_or_else(|| "null".into())
}

pub(super) fn rewrite_runtime_dynamic_module_imports_with_count(
    source: &str,
    referrer_expression: &str,
) -> Result<(String, usize), NativeEngineError> {
    rewrite_runtime_dynamic_module_imports_with_count_and_policy(
        source,
        referrer_expression,
        "null",
    )
}

pub(super) fn rewrite_runtime_dynamic_module_imports_with_count_and_policy(
    source: &str,
    referrer_expression: &str,
    referrer_policy_expression: &str,
) -> Result<(String, usize), NativeEngineError> {
    let bytes = source.as_bytes();
    let sites = javascript_dynamic_import_call_sites(bytes);
    if sites.len() > MAX_NATIVE_MODULE_IMPORTS {
        return Err(NativeEngineError::limit(
            "native runtime dynamic imports per script",
            MAX_NATIVE_MODULE_IMPORTS,
            sites.len(),
        ));
    }
    let mut output = String::with_capacity(source.len());
    let mut copied_until = 0;
    let mut rewritten_import_calls = 0usize;
    for (start, open, close) in sites {
        let specifier_start = skip_javascript_space_and_comments(bytes, open + 1);
        if specifier_start == close {
            continue;
        }
        output.push_str(&source[copied_until..start]);
        output.push_str("globalThis.__glassDynamicImport(");
        output.push_str(referrer_expression);
        output.push(',');
        output.push_str(referrer_policy_expression);
        output.push(',');
        copied_until = open + 1;
        rewritten_import_calls = rewritten_import_calls.saturating_add(1);
    }
    output.push_str(&source[copied_until..]);
    if output.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "transformed JavaScript source",
            MAX_NATIVE_SCRIPT_BYTES,
            output.len(),
        ));
    }
    Ok((output, rewritten_import_calls))
}

pub(super) fn rewrite_dynamic_imports_as_rejected(
    source: &str,
) -> Result<String, NativeEngineError> {
    rewrite_dynamic_imports_as_rejected_with_count(source).map(|(rewritten, _)| rewritten)
}

pub(super) fn rewrite_dynamic_imports_as_rejected_with_count(
    source: &str,
) -> Result<(String, usize), NativeEngineError> {
    let bytes = source.as_bytes();
    let sites = javascript_dynamic_import_call_sites(bytes);
    if sites.len() > MAX_NATIVE_MODULE_IMPORTS {
        return Err(NativeEngineError::limit(
            "native service worker dynamic imports per script",
            MAX_NATIVE_MODULE_IMPORTS,
            sites.len(),
        ));
    }
    let mut output = String::with_capacity(source.len());
    let mut copied_until = 0;
    let mut rewritten_import_calls = 0usize;
    for (start, open, close) in sites {
        if skip_javascript_space_and_comments(bytes, open + 1) == close {
            continue;
        }
        output.push_str(&source[copied_until..start]);
        output.push_str("globalThis.__glassRejectDynamicImport(");
        copied_until = open + 1;
        rewritten_import_calls = rewritten_import_calls.saturating_add(1);
    }
    output.push_str(&source[copied_until..]);
    if output.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "transformed Service Worker JavaScript source",
            MAX_NATIVE_SCRIPT_BYTES,
            output.len(),
        ));
    }
    Ok((output, rewritten_import_calls))
}

pub(super) fn javascript_dynamic_import_call_sites(bytes: &[u8]) -> Vec<(usize, usize, usize)> {
    fn collect(bytes: &[u8], start: usize, end: usize, sites: &mut Vec<(usize, usize, usize)>) {
        let mut index = start;
        let mut previous_is_member_access = false;
        let mut previous_can_end_expression = false;
        while index < end && sites.len() <= MAX_NATIVE_MODULE_IMPORTS {
            index = skip_javascript_space_and_comments(bytes, index);
            if index >= end {
                break;
            }
            match bytes[index] {
                b'\'' | b'"' => {
                    index = skip_javascript_string(bytes, index).min(end);
                    previous_is_member_access = false;
                    previous_can_end_expression = true;
                }
                b'`' => {
                    index = collect_template_dynamic_imports(bytes, index, end, sites);
                    previous_is_member_access = false;
                    previous_can_end_expression = true;
                }
                b'/' if !previous_can_end_expression && bytes.get(index + 1) != Some(&b'=') => {
                    let regex_end = skip_javascript_regex(bytes, index, end);
                    if regex_end > index + 1 {
                        index = regex_end;
                        previous_is_member_access = false;
                        previous_can_end_expression = true;
                    } else {
                        index += 1;
                        previous_is_member_access = false;
                        previous_can_end_expression = false;
                    }
                }
                byte if is_javascript_identifier_start(byte) => {
                    let token_start = index;
                    index += 1;
                    while index < end && is_javascript_identifier_continue(bytes[index]) {
                        index += 1;
                    }
                    let token = &bytes[token_start..index];
                    if token == b"import" && !previous_is_member_access {
                        let open = skip_javascript_space_and_comments(bytes, index);
                        if bytes.get(open) == Some(&b'(')
                            && let Some(close) = matching_javascript_call_paren(bytes, open, end)
                        {
                            let after_call = skip_javascript_space_and_comments(bytes, close + 1);
                            let argument_start =
                                skip_javascript_space_and_comments(bytes, open + 1);
                            if argument_start < close && bytes.get(after_call) != Some(&b'{') {
                                sites.push((token_start, open, close));
                            }
                        }
                    }
                    previous_is_member_access = false;
                    previous_can_end_expression = !matches!(
                        token,
                        b"await"
                            | b"case"
                            | b"delete"
                            | b"do"
                            | b"else"
                            | b"in"
                            | b"instanceof"
                            | b"new"
                            | b"of"
                            | b"return"
                            | b"throw"
                            | b"typeof"
                            | b"void"
                            | b"yield"
                    );
                }
                b'.' => {
                    previous_is_member_access = true;
                    previous_can_end_expression = false;
                    index += 1;
                }
                b'?' if bytes.get(index + 1) == Some(&b'.') => {
                    previous_is_member_access = true;
                    previous_can_end_expression = false;
                    index += 2;
                }
                byte if byte.is_ascii_digit() => {
                    index += 1;
                    while index < end
                        && (bytes[index].is_ascii_alphanumeric()
                            || matches!(bytes[index], b'.' | b'_'))
                    {
                        index += 1;
                    }
                    previous_is_member_access = false;
                    previous_can_end_expression = true;
                }
                b')' | b']' | b'}' => {
                    index += 1;
                    previous_is_member_access = false;
                    previous_can_end_expression = true;
                }
                b'+' if bytes.get(index + 1) == Some(&b'+') => {
                    index += 2;
                    previous_is_member_access = false;
                    previous_can_end_expression = true;
                }
                b'-' if bytes.get(index + 1) == Some(&b'-') => {
                    index += 2;
                    previous_is_member_access = false;
                    previous_can_end_expression = true;
                }
                _ => {
                    previous_is_member_access = false;
                    previous_can_end_expression = false;
                    index += 1;
                }
            }
        }
    }

    let mut sites = Vec::new();
    collect(bytes, 0, bytes.len(), &mut sites);
    sites.sort_unstable_by_key(|(start, _, _)| *start);
    sites.truncate(MAX_NATIVE_MODULE_IMPORTS.saturating_add(1));
    sites
}

pub(super) fn matching_javascript_call_paren(
    bytes: &[u8],
    open: usize,
    end: usize,
) -> Option<usize> {
    let mut index = open;
    let mut depth = 0usize;
    let mut previous_can_end_expression = false;
    while index < end {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= end {
            break;
        }
        match bytes[index] {
            b'\'' | b'"' => {
                index = skip_javascript_string(bytes, index).min(end);
                previous_can_end_expression = true;
            }
            b'`' => {
                index = skip_javascript_template_literal(bytes, index, end);
                previous_can_end_expression = true;
            }
            b'/' if !previous_can_end_expression && bytes.get(index + 1) != Some(&b'=') => {
                let regex_end = skip_javascript_regex(bytes, index, end);
                if regex_end > index + 1 {
                    index = regex_end;
                    previous_can_end_expression = true;
                } else {
                    index += 1;
                    previous_can_end_expression = false;
                }
            }
            b'(' => {
                depth = depth.saturating_add(1);
                index += 1;
                previous_can_end_expression = false;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
                index += 1;
                previous_can_end_expression = true;
            }
            byte if is_javascript_identifier_start(byte) => {
                let start = index;
                index += 1;
                while index < end && is_javascript_identifier_continue(bytes[index]) {
                    index += 1;
                }
                previous_can_end_expression = !matches!(
                    &bytes[start..index],
                    b"await"
                        | b"case"
                        | b"delete"
                        | b"do"
                        | b"else"
                        | b"in"
                        | b"instanceof"
                        | b"new"
                        | b"of"
                        | b"return"
                        | b"throw"
                        | b"typeof"
                        | b"void"
                        | b"yield"
                );
            }
            byte if byte.is_ascii_digit() => {
                index += 1;
                while index < end
                    && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'.' | b'_'))
                {
                    index += 1;
                }
                previous_can_end_expression = true;
            }
            b']' | b'}' => {
                index += 1;
                previous_can_end_expression = true;
            }
            _ => {
                index += 1;
                previous_can_end_expression = false;
            }
        }
    }
    None
}

pub(super) fn skip_javascript_regex(bytes: &[u8], start: usize, end: usize) -> usize {
    let mut index = start + 1;
    let mut in_character_class = false;
    while index < end {
        match bytes[index] {
            b'\\' => index = index.saturating_add(2),
            b'[' => {
                in_character_class = true;
                index += 1;
            }
            b']' => {
                in_character_class = false;
                index += 1;
            }
            b'/' if !in_character_class => {
                index += 1;
                while index < end && is_javascript_identifier_continue(bytes[index]) {
                    index += 1;
                }
                return index;
            }
            b'\n' | b'\r' => return start,
            _ => index += 1,
        }
    }
    start
}

pub(super) fn skip_javascript_template_literal(bytes: &[u8], start: usize, end: usize) -> usize {
    let mut index = start + 1;
    while index < end {
        match bytes[index] {
            b'\\' => index = index.saturating_add(2),
            b'`' => return index + 1,
            b'$' if bytes.get(index + 1) == Some(&b'{') => {
                let Some(close) = javascript_template_expression_end(bytes, index + 2, end) else {
                    return end;
                };
                index = close + 1;
            }
            _ => index += 1,
        }
    }
    end
}

pub(super) fn collect_template_dynamic_imports(
    bytes: &[u8],
    start: usize,
    end: usize,
    sites: &mut Vec<(usize, usize, usize)>,
) -> usize {
    let mut index = start + 1;
    while index < end && sites.len() <= MAX_NATIVE_MODULE_IMPORTS {
        match bytes[index] {
            b'\\' => index = index.saturating_add(2),
            b'`' => return index + 1,
            b'$' if bytes.get(index + 1) == Some(&b'{') => {
                let expression_start = index + 2;
                let Some(close) = javascript_template_expression_end(bytes, expression_start, end)
                else {
                    return end;
                };
                javascript_dynamic_import_call_sites_in_range(
                    bytes,
                    expression_start,
                    close,
                    sites,
                );
                index = close + 1;
            }
            _ => index += 1,
        }
    }
    end
}

pub(super) fn javascript_template_expression_end(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Option<usize> {
    let mut index = start;
    let mut braces = 1usize;
    let mut previous_can_end_expression = false;
    while index < end {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= end {
            break;
        }
        match bytes[index] {
            b'\'' | b'"' => {
                index = skip_javascript_string(bytes, index).min(end);
                previous_can_end_expression = true;
            }
            b'`' => {
                index = skip_javascript_template_literal(bytes, index, end);
                previous_can_end_expression = true;
            }
            b'/' if !previous_can_end_expression && bytes.get(index + 1) != Some(&b'=') => {
                let regex_end = skip_javascript_regex(bytes, index, end);
                if regex_end > index + 1 {
                    index = regex_end;
                    previous_can_end_expression = true;
                } else {
                    index += 1;
                    previous_can_end_expression = false;
                }
            }
            b'{' => {
                braces = braces.saturating_add(1);
                index += 1;
                previous_can_end_expression = false;
            }
            b'}' => {
                braces = braces.saturating_sub(1);
                if braces == 0 {
                    return Some(index);
                }
                index += 1;
                previous_can_end_expression = true;
            }
            byte if is_javascript_identifier_start(byte) => {
                let start = index;
                index += 1;
                while index < end && is_javascript_identifier_continue(bytes[index]) {
                    index += 1;
                }
                previous_can_end_expression = !matches!(
                    &bytes[start..index],
                    b"await"
                        | b"case"
                        | b"delete"
                        | b"do"
                        | b"else"
                        | b"in"
                        | b"instanceof"
                        | b"new"
                        | b"of"
                        | b"return"
                        | b"throw"
                        | b"typeof"
                        | b"void"
                        | b"yield"
                );
            }
            byte if byte.is_ascii_digit() => {
                index += 1;
                while index < end
                    && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'.' | b'_'))
                {
                    index += 1;
                }
                previous_can_end_expression = true;
            }
            _ => {
                index += 1;
                previous_can_end_expression = false;
            }
        }
    }
    None
}

pub(super) fn javascript_dynamic_import_call_sites_in_range(
    bytes: &[u8],
    start: usize,
    end: usize,
    sites: &mut Vec<(usize, usize, usize)>,
) {
    let mut local = javascript_dynamic_import_call_sites(&bytes[start..end]);
    for (token, open, close) in &mut local {
        *token += start;
        *open += start;
        *close += start;
    }
    let remaining = MAX_NATIVE_MODULE_IMPORTS
        .saturating_add(1)
        .saturating_sub(sites.len());
    sites.extend(local.into_iter().take(remaining));
}
