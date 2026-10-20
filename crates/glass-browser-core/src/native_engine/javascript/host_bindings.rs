//! Native JavaScript host bindings for parsing, synchronous fetch, fonts, and crypto.

use super::*;
use crate::native_engine::html_parser::HtmlFragmentOptions;

pub(super) fn native_hmac_bytes(hash: &str, key: &[u8], data: &[u8]) -> Result<Vec<u8>, Error> {
    match hash.trim().to_ascii_uppercase().as_str() {
        "SHA-1" => {
            let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(key).map_err(|_| Error::Unknown)?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        "SHA-256" => {
            let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).map_err(|_| Error::Unknown)?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        "SHA-384" => {
            let mut mac = <Hmac<Sha384> as Mac>::new_from_slice(key).map_err(|_| Error::Unknown)?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        "SHA-512" => {
            let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(key).map_err(|_| Error::Unknown)?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        _ => Err(Error::Unknown),
    }
}

pub(super) fn native_ed25519_public_bytes(private_key: &[u8]) -> Result<Vec<u8>, Error> {
    let private_key: &[u8; 32] = private_key.try_into().map_err(|_| Error::Unknown)?;
    Ok(SigningKey::from_bytes(private_key)
        .verifying_key()
        .to_bytes()
        .to_vec())
}

pub(super) fn native_ed25519_validate_public_bytes(public_key: &[u8]) -> Result<(), Error> {
    let public_key: &[u8; 32] = public_key.try_into().map_err(|_| Error::Unknown)?;
    VerifyingKey::from_bytes(public_key)
        .map(|_| ())
        .map_err(|_| Error::Unknown)
}

pub(super) fn native_ed25519_sign(private_key: &[u8], data: &[u8]) -> Result<Vec<u8>, Error> {
    let private_key: &[u8; 32] = private_key.try_into().map_err(|_| Error::Unknown)?;
    Ok(SigningKey::from_bytes(private_key)
        .sign(data)
        .to_bytes()
        .to_vec())
}

pub(super) fn native_ed25519_verify(
    public_key: &[u8],
    signature: &[u8],
    data: &[u8],
) -> Result<bool, Error> {
    let public_key: &[u8; 32] = public_key.try_into().map_err(|_| Error::Unknown)?;
    let verifying_key = VerifyingKey::from_bytes(public_key).map_err(|_| Error::Unknown)?;
    let signature = Ed25519Signature::from_slice(signature).map_err(|_| Error::Unknown)?;
    Ok(verifying_key.verify(data, &signature).is_ok())
}

pub(super) fn native_hash_length(hash: &str) -> Option<usize> {
    match hash.trim().to_ascii_uppercase().as_str() {
        "SHA-1" => Some(20),
        "SHA-256" => Some(32),
        "SHA-384" => Some(48),
        "SHA-512" => Some(64),
        _ => None,
    }
}

pub(super) fn native_crypto_derive(
    algorithm: &str,
    hash: &str,
    key: &[u8],
    salt: &[u8],
    info: &[u8],
    iterations: usize,
    output_len: usize,
) -> Result<Vec<u8>, Error> {
    let hash_len = native_hash_length(hash).ok_or(Error::Unknown)?;
    if output_len > MAX_NATIVE_FORM_BODY_BYTES {
        return Err(Error::Unknown);
    }
    match algorithm.trim().to_ascii_uppercase().as_str() {
        "HKDF" => {
            if output_len > hash_len.saturating_mul(255)
                || info.len() > MAX_NATIVE_FORM_BODY_BYTES
                || salt.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            let salt = if salt.is_empty() {
                vec![0_u8; hash_len]
            } else {
                salt.to_vec()
            };
            let pseudorandom_key = native_hmac_bytes(hash, &salt, key)?;
            let block_count = output_len.div_ceil(hash_len);
            let work_per_block = info.len().saturating_add(hash_len).saturating_add(1);
            if block_count > 0
                && work_per_block > MAX_NATIVE_CRYPTO_DERIVE_WORK.saturating_div(block_count)
            {
                return Err(Error::Unknown);
            }
            let mut output = Vec::with_capacity(output_len);
            let mut previous = Vec::new();
            for block_index in 1..=block_count {
                let mut input = Vec::with_capacity(previous.len() + info.len() + 1);
                input.extend_from_slice(&previous);
                input.extend_from_slice(info);
                input.push(u8::try_from(block_index).map_err(|_| Error::Unknown)?);
                previous = native_hmac_bytes(hash, &pseudorandom_key, &input)?;
                output.extend_from_slice(&previous);
            }
            output.truncate(output_len);
            Ok(output)
        }
        "PBKDF2" => {
            if iterations == 0
                || iterations > MAX_NATIVE_CRYPTO_DERIVE_ITERATIONS
                || salt.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            let block_count = output_len.div_ceil(hash_len);
            let work_per_block = salt
                .len()
                .saturating_add(4)
                .saturating_add(iterations.saturating_mul(hash_len));
            if block_count > 0
                && work_per_block > MAX_NATIVE_CRYPTO_DERIVE_WORK.saturating_div(block_count)
            {
                return Err(Error::Unknown);
            }
            let mut output = Vec::with_capacity(output_len);
            for block_index in 1..=block_count {
                let block_index = u32::try_from(block_index).map_err(|_| Error::Unknown)?;
                let mut input = Vec::with_capacity(salt.len() + 4);
                input.extend_from_slice(salt);
                input.extend_from_slice(&block_index.to_be_bytes());
                let mut current = native_hmac_bytes(hash, key, &input)?;
                let mut accumulator = current.clone();
                for _ in 1..iterations {
                    current = native_hmac_bytes(hash, key, &current)?;
                    for (left, right) in accumulator.iter_mut().zip(current.iter()) {
                        *left ^= *right;
                    }
                }
                output.extend_from_slice(&accumulator);
            }
            output.truncate(output_len);
            Ok(output)
        }
        _ => Err(Error::Unknown),
    }
}

pub(super) fn native_aes_cbc_bytes(
    operation: &str,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, Error> {
    if iv.len() != 16 || data.len() > MAX_NATIVE_FORM_BODY_BYTES {
        return Err(Error::Unknown);
    }
    let decrypting = match operation.trim().to_ascii_lowercase().as_str() {
        "encrypt" => false,
        "decrypt" if !data.is_empty() && data.len().is_multiple_of(16) => true,
        _ => return Err(Error::Unknown),
    };
    macro_rules! run_cbc {
        ($cipher:ty) => {{
            let cipher =
                <$cipher as BlockKeyInit>::new_from_slice(key).map_err(|_| Error::Unknown)?;
            if decrypting {
                let mut previous = aes::Block::clone_from_slice(iv);
                let mut plaintext = Vec::with_capacity(data.len());
                for chunk in data.chunks_exact(16) {
                    let mut block = aes::Block::clone_from_slice(chunk);
                    cipher.decrypt_block(&mut block);
                    for (byte, previous_byte) in block.iter_mut().zip(previous.iter()) {
                        *byte ^= *previous_byte;
                    }
                    plaintext.extend_from_slice(&block);
                    previous.copy_from_slice(chunk);
                }
                let padding = usize::from(*plaintext.last().ok_or(Error::Unknown)?);
                if !(1..=16).contains(&padding)
                    || padding > plaintext.len()
                    || plaintext[plaintext.len() - padding..]
                        .iter()
                        .any(|byte| usize::from(*byte) != padding)
                {
                    return Err(Error::Unknown);
                }
                plaintext.truncate(plaintext.len() - padding);
                Ok(plaintext)
            } else {
                let padding = 16 - data.len() % 16;
                if data.len().saturating_add(padding) > MAX_NATIVE_FORM_BODY_BYTES {
                    return Err(Error::Unknown);
                }
                let mut padded = data.to_vec();
                padded.extend(std::iter::repeat_n(padding as u8, padding));
                let mut previous = aes::Block::clone_from_slice(iv);
                let mut ciphertext = Vec::with_capacity(padded.len());
                for chunk in padded.chunks_exact(16) {
                    let mut block = aes::Block::clone_from_slice(chunk);
                    for (byte, previous_byte) in block.iter_mut().zip(previous.iter()) {
                        *byte ^= *previous_byte;
                    }
                    cipher.encrypt_block(&mut block);
                    ciphertext.extend_from_slice(&block);
                    previous.copy_from_slice(&block);
                }
                Ok(ciphertext)
            }
        }};
    }
    match key.len() {
        16 => run_cbc!(Aes128),
        24 => run_cbc!(Aes192),
        32 => run_cbc!(Aes256),
        _ => Err(Error::Unknown),
    }
}

pub(super) fn native_aes_ctr_increment(counter: &mut aes::Block, length: usize) {
    let byte_count = length.div_ceil(8);
    let start = 16 - byte_count;
    let partial_bits = length % 8;
    let mut carry = true;
    for index in (start..16).rev() {
        if !carry {
            break;
        }
        let mask = if index == start && partial_bits != 0 {
            (1_u8 << partial_bits) - 1
        } else {
            u8::MAX
        };
        let value = counter[index] & mask;
        counter[index] = (counter[index] & !mask) | value.wrapping_add(1) & mask;
        carry = value == mask;
    }
}

pub(super) fn native_aes_ctr_bytes(
    key: &[u8],
    counter: &[u8],
    counter_length: usize,
    data: &[u8],
) -> Result<Vec<u8>, Error> {
    if counter.len() != 16
        || !(1..=128).contains(&counter_length)
        || data.len() > MAX_NATIVE_FORM_BODY_BYTES
    {
        return Err(Error::Unknown);
    }
    macro_rules! run_ctr {
        ($cipher:ty) => {{
            let cipher =
                <$cipher as BlockKeyInit>::new_from_slice(key).map_err(|_| Error::Unknown)?;
            let mut counter = aes::Block::clone_from_slice(counter);
            let mut output = Vec::with_capacity(data.len());
            for chunk in data.chunks(16) {
                let mut keystream = counter.clone();
                cipher.encrypt_block(&mut keystream);
                output.extend(
                    chunk
                        .iter()
                        .zip(keystream.iter())
                        .map(|(byte, stream_byte)| byte ^ stream_byte),
                );
                native_aes_ctr_increment(&mut counter, counter_length);
            }
            Ok(output)
        }};
    }
    match key.len() {
        16 => run_ctr!(Aes128),
        24 => run_ctr!(Aes192),
        32 => run_ctr!(Aes256),
        _ => Err(Error::Unknown),
    }
}

pub(super) fn install_native_inline_script_policy<'js>(
    ctx: rquickjs::Ctx<'js>,
    policy: Arc<Mutex<NativeInlineScriptPolicy>>,
    processed_meta_nodes: Arc<Mutex<BTreeSet<u32>>>,
    document_url: &str,
) -> Result<(), NativeEngineError> {
    let allows = Function::new(ctx.clone(), {
        let policy = Arc::clone(&policy);
        move |source: String, nonce: String| -> bool {
            let nonce = (!nonce.is_empty()).then_some(nonce.as_str());
            policy
                .lock()
                .map(|policy| policy.allows(&source, nonce))
                .unwrap_or(false)
        }
    })
    .map_err(|_| NativeEngineError::Worker {
        operation: "install inline script policy".into(),
        reason: "native inline script policy could not be installed".into(),
    })?;
    let report_only = Function::new(ctx.clone(), {
        let policy = Arc::clone(&policy);
        let document_url = document_url.to_owned();
        move |source: String, nonce: String| -> String {
            let nonce = (!nonce.is_empty()).then_some(nonce.as_str());
            policy
                .lock()
                .ok()
                .and_then(|policy| {
                    serde_json::to_string(&policy.report_only_inline_script_violations(
                        &document_url,
                        &source,
                        nonce,
                    ))
                    .ok()
                })
                .unwrap_or_else(|| "[]".into())
        }
    })
    .map_err(|_| NativeEngineError::Worker {
        operation: "install report-only inline script policy".into(),
        reason: "native report-only inline script policy could not be installed".into(),
    })?;
    let append_meta_policy = Function::new(ctx.clone(), {
        let policy = Arc::clone(&policy);
        let processed_meta_nodes = Arc::clone(&processed_meta_nodes);
        move |node_index: u32, source: String| -> bool {
            if source.is_empty() {
                return true;
            }
            let Ok(mut processed_meta_nodes) = processed_meta_nodes.lock() else {
                return false;
            };
            if processed_meta_nodes.contains(&node_index) {
                return true;
            }
            let Ok(mut policy) = policy.lock() else {
                return false;
            };
            if policy.append_meta_policy(&source).is_err() {
                return false;
            }
            processed_meta_nodes.insert(node_index);
            true
        }
    })
    .map_err(|_| NativeEngineError::Worker {
        operation: "install runtime CSP meta policy".into(),
        reason: "native runtime CSP meta policy callback could not be installed".into(),
    })?;
    let allows_attribute = Function::new(ctx.clone(), move |source: String| -> bool {
        policy
            .lock()
            .map(|policy| policy.allows_attribute(&source))
            .unwrap_or(false)
    })
    .map_err(|_| NativeEngineError::Worker {
        operation: "install inline event-handler policy".into(),
        reason: "native inline event-handler policy could not be installed".into(),
    })?;
    ctx.globals()
        .set("__glassAllowsInlineScript", allows)
        .map_err(|_| NativeEngineError::Worker {
            operation: "publish inline script policy".into(),
            reason: "native inline script policy could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassReportOnlyInlineScript", report_only)
        .map_err(|_| NativeEngineError::Worker {
            operation: "publish report-only inline script policy".into(),
            reason: "native report-only inline script policy could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassAppendMetaCspPolicy", append_meta_policy)
        .map_err(|_| NativeEngineError::Worker {
            operation: "publish runtime CSP meta policy".into(),
            reason: "native runtime CSP meta policy callback could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassAllowsInlineEventHandler", allows_attribute)
        .map_err(|_| NativeEngineError::Worker {
            operation: "publish inline event-handler policy".into(),
            reason: "native inline event-handler policy could not be published".into(),
        })
}

/// Publish the Rust URL parser as the single canonicalization boundary for
/// JavaScript URL construction and relative resolution. The JavaScript
/// objects still own their Web IDL surface and mutable state, but every
/// initial URL value is normalized by the same WHATWG-oriented parser already
/// used by the native resource and policy owners.
pub(super) fn install_native_url_source<'js>(
    ctx: rquickjs::Ctx<'js>,
) -> Result<(), NativeEngineError> {
    let canonical = Function::new(
        ctx.clone(),
        |input: String, base: Option<String>| -> std::result::Result<String, Error> {
            if input.len() > MAX_NATIVE_SCRIPT_BYTES {
                return Err(Error::Unknown);
            }
            let base = match base {
                Some(base) if !base.is_empty() => {
                    if base.len() > MAX_NATIVE_SCRIPT_BYTES {
                        return Err(Error::Unknown);
                    }
                    Some(Url::parse(&base).map_err(|_| Error::Unknown)?)
                }
                _ => None,
            };
            Url::options()
                .base_url(base.as_ref())
                .parse(&input)
                .map(|url| url.to_string())
                .map_err(|_| Error::Unknown)
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native URL source".into(),
        reason: "native URL source could not be installed".into(),
    })?;
    ctx.globals()
        .set("__glassNativeCanonicalUrl", canonical)
        .map_err(|_| NativeEngineError::Worker {
            operation: "publish native URL source".into(),
            reason: "native URL source could not be published".into(),
        })
}

pub(super) fn install_native_html_fragment_parser<'js>(ctx: Ctx<'js>) -> Result<(), Error> {
    let parse_fragment = Function::new(
        ctx.clone(),
        |source: String,
         context_namespace: String,
         context_name: String,
         context_attributes_json: String,
         context_element_depth: u32|
         -> String {
            let error_response = |kind: &str| serde_json::json!({ "ok": false, "error": kind });
            let response = if source.len() > crate::limits::MAX_TEXT_BYTES {
                error_response("byteLimit")
            } else if context_attributes_json.len() > MAX_NATIVE_SCRIPT_BYTES {
                error_response("invalidContext")
            } else if context_element_depth as usize > MAX_NATIVE_DOM_DEPTH {
                error_response("depthLimit")
            } else {
                match serde_json::from_str::<Vec<HtmlParsedAttribute>>(&context_attributes_json) {
                    Err(_) => error_response("invalidContext"),
                    Ok(context_attributes) => match parse_html5_fragment(
                        &source,
                        HtmlFragmentOptions {
                            context_namespace: &context_namespace,
                            context_name: &context_name,
                            context_attributes: &context_attributes,
                            scripting_enabled: true,
                            context_element_depth: context_element_depth as usize,
                            max_nodes: MAX_NATIVE_NODES.saturating_mul(2),
                            max_dom_depth: MAX_NATIVE_DOM_DEPTH,
                        },
                    ) {
                        Ok(fragment) => serde_json::json!({
                            "ok": true,
                            "fragment": fragment,
                        }),
                        Err(HtmlTreeSinkFailure::InvalidTree(_)) => {
                            error_response("invalidContext")
                        }
                        Err(HtmlTreeSinkFailure::NodeLimitExceeded { .. }) => {
                            error_response("nodeLimit")
                        }
                        Err(HtmlTreeSinkFailure::DomDepthExceeded { .. }) => {
                            error_response("depthLimit")
                        }
                    },
                }
            };
            serde_json::to_string(&response)
                .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization"}"#.into())
        },
    )?;
    ctx.globals()
        .set("__glassParseHtmlFragmentHost", parse_fragment)?;
    ctx.eval::<(), _>(
        r#"Object.defineProperty(globalThis, "__glassParseHtmlFragmentHost", {
          value: globalThis.__glassParseHtmlFragmentHost,
          writable: false,
          configurable: false,
          enumerable: false,
        });"#,
    )
}

pub(super) fn install_native_html_document_parser<'js>(ctx: Ctx<'js>) -> Result<(), Error> {
    const MAX_NATIVE_HTML_DOCUMENT_RESULT_BYTES: usize =
        MAX_NATIVE_SCRIPT_BYTES.saturating_mul(128);
    const MAX_NATIVE_HTML_DOCUMENT_SOURCE_BYTES: usize =
        crate::limits::MAX_TEXT_BYTES.saturating_mul(3);

    let parse_document = Function::new(ctx.clone(), |source: String| -> String {
        let error_response = |kind: &str| serde_json::json!({ "error": kind });
        let response = if source.len() > MAX_NATIVE_HTML_DOCUMENT_SOURCE_BYTES {
            error_response("byteLimit")
        } else {
            let parsed = parse_html5_document(
                &source,
                MAX_NATIVE_NODES.saturating_mul(2),
                MAX_NATIVE_DOM_DEPTH,
                false,
            );
            match parsed.sink_failure {
                Some(HtmlTreeSinkFailure::InvalidTree(_)) => error_response("invalidTree"),
                Some(HtmlTreeSinkFailure::NodeLimitExceeded { .. }) => error_response("nodeLimit"),
                Some(HtmlTreeSinkFailure::DomDepthExceeded { .. }) => error_response("depthLimit"),
                None => match serde_json::to_string(&parsed) {
                    Ok(serialized) if serialized.len() <= MAX_NATIVE_HTML_DOCUMENT_RESULT_BYTES => {
                        return serialized;
                    }
                    Ok(_) => error_response("resultLimit"),
                    Err(_) => error_response("serialization"),
                },
            }
        };
        serde_json::to_string(&response).unwrap_or_else(|_| r#"{"error":"serialization"}"#.into())
    })?;
    ctx.globals()
        .set("__glassParseHtmlDocumentHost", parse_document)?;
    ctx.eval::<(), _>(
        r#"Object.defineProperty(globalThis, "__glassParseHtmlDocumentHost", {
          value: globalThis.__glassParseHtmlDocumentHost,
          writable: false,
          configurable: false,
          enumerable: false,
        });"#,
    )
}

pub(super) fn install_native_html_document_decoder<'js>(ctx: Ctx<'js>) -> Result<(), Error> {
    let decode_document = Function::new(
        ctx.clone(),
        |bytes: TypedArray<'_, u8>,
         response_content_type: Option<String>,
         override_mime_type: Option<String>| {
            let bytes: &[u8] = bytes.as_ref();
            if bytes.len() > crate::limits::MAX_TEXT_BYTES {
                return None;
            }
            let decoded = super::super::html_encoding::decode_xhr_html_document(
                bytes,
                response_content_type.as_deref(),
                override_mime_type.as_deref(),
            );
            (decoded.len() <= crate::limits::MAX_TEXT_BYTES.saturating_mul(3)).then_some(decoded)
        },
    )?;
    ctx.globals()
        .set("__glassDecodeHtmlDocumentHost", decode_document)?;
    ctx.eval::<(), _>(
        r#"Object.defineProperty(globalThis, "__glassDecodeHtmlDocumentHost", {
          value: globalThis.__glassDecodeHtmlDocumentHost,
          writable: false,
          configurable: false,
          enumerable: false,
        });"#,
    )
}

#[derive(Debug, Deserialize)]
struct NativeSyncXhrRequest {
    #[serde(rename = "documentUrl")]
    document_url: String,
    href: String,
    method: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(rename = "bodyBase64", default)]
    body_base64: Option<String>,
    #[serde(rename = "contentType", default)]
    content_type: Option<String>,
    #[serde(default)]
    credentials: bool,
    #[serde(rename = "timeoutMs", default)]
    timeout_ms: Option<u32>,
    #[serde(rename = "referrerPolicy", default)]
    referrer_policy: Option<String>,
    #[serde(rename = "cookieWrites", default)]
    cookie_writes: Vec<String>,
}

pub(crate) type NativeSyncXhrParentBroker =
    Arc<dyn Fn(&str, &[String]) -> Result<serde_json::Value, NativeEngineError> + Send + Sync>;

static NATIVE_SYNC_XHR_PARENT_BROKER: OnceLock<NativeSyncXhrParentBroker> = OnceLock::new();

pub(crate) fn install_native_sync_xhr_parent_broker(
    broker: NativeSyncXhrParentBroker,
) -> Result<(), NativeEngineError> {
    NATIVE_SYNC_XHR_PARENT_BROKER
        .set(broker)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install parent synchronous XHR broker".into(),
            reason: "a parent broker is already installed for this content process".into(),
        })
}

pub(super) fn sync_xhr_error_payload(error: &NativeEngineError) -> serde_json::Value {
    let reason = error.to_string();
    let timeout = reason.to_ascii_lowercase().contains("timeout");
    serde_json::json!({
        "error": true,
        "errorMessage": reason,
        "timeout": timeout,
    })
}

/// Execute one synchronous XHR through the owning loader when used by a
/// standalone native realm. Content workers install a parent broker and do
/// not use this local network path.
fn run_native_sync_xhr(
    mut loader: NativeResourceLoader,
    request: NativeSyncXhrRequest,
) -> (NativeResourceLoader, serde_json::Value) {
    for cookie_write in &request.cookie_writes {
        if let Err(error) = loader.set_document_cookie(&request.document_url, cookie_write) {
            return (loader, sync_xhr_error_payload(&error));
        }
    }
    let method = match NativeFetchMethod::from_fetch_method(&request.method) {
        Ok(method) => method,
        Err(error) => return (loader, sync_xhr_error_payload(&error)),
    };
    let body = match request.body_base64 {
        Some(encoded) => match base64::engine::general_purpose::STANDARD.decode(encoded) {
            Ok(bytes) => Some(NativeRequestBody::Bytes(bytes)),
            Err(_) => {
                let error =
                    NativeEngineError::invalid("synchronous XHR body", "must be valid base64");
                return (loader, sync_xhr_error_payload(&error));
            }
        },
        None => None,
    };
    let timeout = request
        .timeout_ms
        .filter(|value| *value > 0)
        .map(|value| Duration::from_millis(u64::from(value)));
    let fetch_request = NativeFetchRequest {
        document_url: &request.document_url,
        href: &request.href,
        referrer_url: None,
        referrer_policy: request.referrer_policy,
        method,
        body,
        content_type: request.content_type,
        request_headers: request.headers,
        credentials: request.credentials,
        credentials_mode: Some(NativeFetchCredentialsMode::for_xhr(request.credentials)),
        cors_mode: NativeCorsMode::Cors,
        redirect_mode: NativeFetchRedirectMode::Follow,
        cache_mode: NativeFetchCacheMode::Default,
        timeout,
        max_response_bytes: None,
    };
    let result = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(loader.fetch_request_with_headers_async(fetch_request)),
        Err(_) => Err(NativeEngineError::Worker {
            operation: "synchronous XHR runtime".into(),
            reason: "a native synchronous XHR runtime could not be created".into(),
        }),
    };
    match result {
        Ok(response) => (
            loader,
            serde_json::json!({
                "error": false,
                "url": response.url,
                "status": response.status,
                "statusText": response.status_text,
                "contentType": response.content_type,
                "headers": response.headers,
                "bodyBase64": base64::engine::general_purpose::STANDARD.encode(response.body),
                "redirected": response.redirected,
            }),
        ),
        Err(error) => (loader, sync_xhr_error_payload(&error)),
    }
}

pub(super) fn install_native_sync_xhr_source<'js>(
    ctx: rquickjs::Ctx<'js>,
    loader_slot: Arc<Mutex<Option<NativeResourceLoader>>>,
    loader_used: Arc<AtomicBool>,
    cookie_state: Arc<Mutex<String>>,
    cookie_updates: Arc<Mutex<Vec<String>>>,
    parent_brokered_cookie_updates: Arc<Mutex<VecDeque<String>>>,
) -> Result<(), NativeEngineError> {
    let source = Function::new(
        ctx.clone(),
        move |encoded: String| -> std::result::Result<String, Error> {
            if encoded.len() > MAX_NATIVE_SCRIPT_BYTES {
                return Err(Error::Unknown);
            }
            let request: NativeSyncXhrRequest =
                serde_json::from_str(&encoded).map_err(|_| Error::Unknown)?;
            if let Some(parent_broker) = NATIVE_SYNC_XHR_PARENT_BROKER.get().cloned() {
                let cookie_writes =
                    cookie_updates
                        .lock()
                        .map_err(|_| Error::Unknown)
                        .map(|mut updates| {
                            let mut writes = std::mem::take(&mut *updates);
                            writes.extend(request.cookie_writes.iter().cloned());
                            writes
                        })?;
                let marker_count = {
                    let brokered = parent_brokered_cookie_updates
                        .lock()
                        .map_err(|_| Error::Unknown)?;
                    brokered.len().saturating_add(request.cookie_writes.len())
                };
                if marker_count > MAX_NATIVE_EFFECTS
                    || request.cookie_writes.len() > MAX_NATIVE_EFFECTS
                {
                    return Err(Error::Unknown);
                }
                let js_cookie_writes = request.cookie_writes.clone();
                let broker_result = thread::spawn(move || parent_broker(&encoded, &cookie_writes))
                    .join()
                    .map_err(|_| Error::Unknown)?;
                let (payload, parent_applied_writes) = match broker_result {
                    Ok(payload) => (payload, true),
                    Err(error) => (sync_xhr_error_payload(&error), false),
                };
                if parent_applied_writes {
                    parent_brokered_cookie_updates
                        .lock()
                        .map_err(|_| Error::Unknown)?
                        .extend(js_cookie_writes);
                }
                if let Some(document_cookie) = payload
                    .get("documentCookie")
                    .and_then(serde_json::Value::as_str)
                    && let Ok(mut current) = cookie_state.lock()
                {
                    *current = document_cookie.to_owned();
                }
                return serde_json::to_string(&payload).map_err(|_| Error::Unknown);
            }
            let loader = loader_slot
                .lock()
                .map_err(|_| Error::Unknown)?
                .take()
                .ok_or(Error::Unknown)?;
            let result = thread::spawn(move || run_native_sync_xhr(loader, request))
                .join()
                .map_err(|_| Error::Unknown)?;
            let (loader, payload) = result;
            loader_slot
                .lock()
                .map_err(|_| Error::Unknown)?
                .replace(loader);
            loader_used.store(true, Ordering::Release);
            serde_json::to_string(&payload).map_err(|_| Error::Unknown)
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native synchronous XHR source".into(),
        reason: "native synchronous XHR source could not be installed".into(),
    })?;
    ctx.globals()
        .set("__glassSyncXhr", source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "publish native synchronous XHR source".into(),
            reason: "native synchronous XHR source could not be published".into(),
        })
}

pub(super) fn install_native_local_font_source<'js>(
    ctx: rquickjs::Ctx<'js>,
) -> Result<(), NativeEngineError> {
    let source = Function::new(
        ctx.clone(),
        |family: String,
         weight: String,
         style: String,
         stretch: String|
         -> std::result::Result<Option<String>, Error> {
            let family = family.trim();
            if family.is_empty() || family.len() > MAX_NATIVE_SCRIPT_BYTES {
                return Err(Error::Unknown);
            }
            let weight = parse_font_weight_range(&weight)
                .ok_or(Error::Unknown)?
                .nominal();
            let weight = FontWeightValue::from_numeric(weight).ok_or(Error::Unknown)?;
            let style = match style.trim().to_ascii_lowercase().as_str() {
                "normal" => FontStyleValue::Normal,
                "italic" => FontStyleValue::Italic,
                _ => return Err(Error::Unknown),
            };
            let stretch = parse_font_stretch_range(&stretch)
                .ok_or(Error::Unknown)?
                .nominal();
            let bytes = NativeFontBook::system_local_font_bytes(family, weight, style, stretch);
            if bytes
                .as_ref()
                .is_some_and(|bytes| bytes.len() > MAX_NATIVE_FONT_BYTES)
            {
                return Err(Error::Unknown);
            }
            Ok(bytes.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)))
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native local font source".into(),
        reason: "native local font source could not be installed".into(),
    })?;
    ctx.globals()
        .set("__glassNativeLocalFont", source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "publish native local font source".into(),
            reason: "native local font source could not be published".into(),
        })
}

pub(super) fn install_native_crypto_sources<'js>(
    ctx: rquickjs::Ctx<'js>,
) -> Result<(), NativeEngineError> {
    let random_source = Function::new(
        ctx.clone(),
        |count: usize| -> std::result::Result<Vec<u8>, Error> {
            if count > MAX_NATIVE_PAGE_CRYPTO_VALUES_BYTES {
                return Err(Error::Unknown);
            }
            let mut bytes = vec![0_u8; count];
            getrandom::fill(&mut bytes).map_err(|_| Error::Unknown)?;
            Ok(bytes)
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native random source".into(),
        reason: "native random source could not be installed".into(),
    })?;
    let array_buffer_is_attached = Function::new(ctx.clone(), |value: Value<'_>| {
        ArrayBuffer::from_value(value).is_some()
    })
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native ArrayBuffer state source".into(),
        reason: "native ArrayBuffer state source could not be installed".into(),
    })?;
    let array_buffer_detach = Function::new(ctx.clone(), |value: Value<'_>| {
        let Some(mut buffer) = ArrayBuffer::from_value(value) else {
            return false;
        };
        buffer.detach();
        true
    })
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native ArrayBuffer detach source".into(),
        reason: "native ArrayBuffer detach source could not be installed".into(),
    })?;
    let digest_source = Function::new(
        ctx.clone(),
        |algorithm: String, encoded: String| -> std::result::Result<String, Error> {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| Error::Unknown)?;
            if bytes.len() > MAX_NATIVE_FORM_BODY_BYTES {
                return Err(Error::Unknown);
            }
            let algorithm = algorithm.trim().to_ascii_uppercase();
            let digest = match algorithm.as_str() {
                "SHA-1" => Sha1::digest(&bytes).to_vec(),
                "SHA-256" => Sha256::digest(&bytes).to_vec(),
                "SHA-384" => Sha384::digest(&bytes).to_vec(),
                "SHA-512" => Sha512::digest(&bytes).to_vec(),
                _ => return Err(Error::Unknown),
            };
            Ok(base64::engine::general_purpose::STANDARD.encode(digest))
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native crypto digest".into(),
        reason: "native crypto digest could not be installed".into(),
    })?;
    let hmac_source = Function::new(
        ctx.clone(),
        |algorithm: String, key_encoded: String, data_encoded: String| {
            let key = base64::engine::general_purpose::STANDARD
                .decode(key_encoded)
                .map_err(|_| Error::Unknown)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_encoded)
                .map_err(|_| Error::Unknown)?;
            if key.is_empty()
                || key.len() > MAX_NATIVE_FORM_BODY_BYTES
                || data.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            let digest = native_hmac_bytes(&algorithm, &key, &data)?;
            Ok(base64::engine::general_purpose::STANDARD.encode(digest))
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native crypto HMAC".into(),
        reason: "native crypto HMAC could not be installed".into(),
    })?;
    let ed25519_source = Function::new(
        ctx.clone(),
        |operation: String,
         key_encoded: String,
         data_encoded: String,
         signature_encoded: String|
         -> std::result::Result<String, Error> {
            let key = base64::engine::general_purpose::STANDARD
                .decode(key_encoded)
                .map_err(|_| Error::Unknown)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_encoded)
                .map_err(|_| Error::Unknown)?;
            let signature = base64::engine::general_purpose::STANDARD
                .decode(signature_encoded)
                .map_err(|_| Error::Unknown)?;
            if key.len() != 32
                || data.len() > MAX_NATIVE_FORM_BODY_BYTES
                || signature.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            match operation.trim().to_ascii_lowercase().as_str() {
                "public" => Ok(base64::engine::general_purpose::STANDARD
                    .encode(native_ed25519_public_bytes(&key)?)),
                "validate" if data.is_empty() && signature.is_empty() => {
                    native_ed25519_validate_public_bytes(&key)?;
                    Ok("1".into())
                }
                "sign" if signature.is_empty() => Ok(base64::engine::general_purpose::STANDARD
                    .encode(native_ed25519_sign(&key, &data)?)),
                "verify" if signature.len() == 64 => {
                    Ok(if native_ed25519_verify(&key, &signature, &data)? {
                        "1".into()
                    } else {
                        "0".into()
                    })
                }
                _ => Err(Error::Unknown),
            }
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native Ed25519 source".into(),
        reason: "native Ed25519 source could not be installed".into(),
    })?;
    let aes_gcm_source = Function::new(
        ctx.clone(),
        |operation: String,
         key_encoded: String,
         iv_encoded: String,
         aad_encoded: String,
         tag_length: usize,
         data_encoded: String|
         -> std::result::Result<String, Error> {
            let key = base64::engine::general_purpose::STANDARD
                .decode(key_encoded)
                .map_err(|_| Error::Unknown)?;
            let iv = base64::engine::general_purpose::STANDARD
                .decode(iv_encoded)
                .map_err(|_| Error::Unknown)?;
            let aad = base64::engine::general_purpose::STANDARD
                .decode(aad_encoded)
                .map_err(|_| Error::Unknown)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_encoded)
                .map_err(|_| Error::Unknown)?;
            if !matches!(key.len(), 16 | 24 | 32)
                || iv.len() != 12
                || tag_length != 128
                || aad.len() > MAX_NATIVE_FORM_BODY_BYTES
                || data.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            let operation = operation.trim().to_ascii_lowercase();
            let decrypting = match operation.as_str() {
                "encrypt" => false,
                "decrypt" if data.len() >= 16 => true,
                _ => return Err(Error::Unknown),
            };
            if !decrypting && data.len().saturating_add(16) > MAX_NATIVE_FORM_BODY_BYTES {
                return Err(Error::Unknown);
            }
            macro_rules! run_aes_gcm {
                ($cipher:ty) => {{
                    let cipher = <$cipher>::new_from_slice(&key).map_err(|_| Error::Unknown)?;
                    let nonce = Nonce::<U12>::from_slice(&iv);
                    let payload = Payload {
                        msg: data.as_slice(),
                        aad: aad.as_slice(),
                    };
                    if decrypting {
                        cipher.decrypt(nonce, payload).map_err(|_| Error::Unknown)?
                    } else {
                        cipher.encrypt(nonce, payload).map_err(|_| Error::Unknown)?
                    }
                }};
            }
            let output = match key.len() {
                16 => run_aes_gcm!(AesGcm::<Aes128, U12>),
                24 => run_aes_gcm!(AesGcm::<Aes192, U12>),
                32 => run_aes_gcm!(AesGcm::<Aes256, U12>),
                _ => return Err(Error::Unknown),
            };
            Ok(base64::engine::general_purpose::STANDARD.encode(output))
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native AES-GCM source".into(),
        reason: "native AES-GCM source could not be installed".into(),
    })?;
    let aes_cbc_source = Function::new(
        ctx.clone(),
        |operation: String,
         key_encoded: String,
         iv_encoded: String,
         data_encoded: String|
         -> std::result::Result<String, Error> {
            let key = base64::engine::general_purpose::STANDARD
                .decode(key_encoded)
                .map_err(|_| Error::Unknown)?;
            let iv = base64::engine::general_purpose::STANDARD
                .decode(iv_encoded)
                .map_err(|_| Error::Unknown)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_encoded)
                .map_err(|_| Error::Unknown)?;
            if !matches!(key.len(), 16 | 24 | 32)
                || iv.len() != 16
                || data.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            let output = native_aes_cbc_bytes(&operation, &key, &iv, &data)?;
            Ok(base64::engine::general_purpose::STANDARD.encode(output))
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native AES-CBC source".into(),
        reason: "native AES-CBC source could not be installed".into(),
    })?;
    let aes_ctr_source = Function::new(
        ctx.clone(),
        |key_encoded: String,
         counter_encoded: String,
         counter_length: usize,
         data_encoded: String|
         -> std::result::Result<String, Error> {
            let key = base64::engine::general_purpose::STANDARD
                .decode(key_encoded)
                .map_err(|_| Error::Unknown)?;
            let counter = base64::engine::general_purpose::STANDARD
                .decode(counter_encoded)
                .map_err(|_| Error::Unknown)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_encoded)
                .map_err(|_| Error::Unknown)?;
            if !matches!(key.len(), 16 | 24 | 32)
                || counter.len() != 16
                || data.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            let output = native_aes_ctr_bytes(&key, &counter, counter_length, &data)?;
            Ok(base64::engine::general_purpose::STANDARD.encode(output))
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native AES-CTR source".into(),
        reason: "native AES-CTR source could not be installed".into(),
    })?;
    let derive_source = Function::new(
        ctx.clone(),
        |algorithm: String,
         hash: String,
         key_encoded: String,
         salt_encoded: String,
         info_encoded: String,
         iterations: usize,
         output_bits: usize|
         -> std::result::Result<String, Error> {
            if !output_bits.is_multiple_of(8) {
                return Err(Error::Unknown);
            }
            let key = base64::engine::general_purpose::STANDARD
                .decode(key_encoded)
                .map_err(|_| Error::Unknown)?;
            let salt = base64::engine::general_purpose::STANDARD
                .decode(salt_encoded)
                .map_err(|_| Error::Unknown)?;
            let info = base64::engine::general_purpose::STANDARD
                .decode(info_encoded)
                .map_err(|_| Error::Unknown)?;
            if key.len() > MAX_NATIVE_FORM_BODY_BYTES
                || salt.len() > MAX_NATIVE_FORM_BODY_BYTES
                || info.len() > MAX_NATIVE_FORM_BODY_BYTES
            {
                return Err(Error::Unknown);
            }
            let output = native_crypto_derive(
                &algorithm,
                &hash,
                &key,
                &salt,
                &info,
                iterations,
                output_bits / 8,
            )?;
            Ok(base64::engine::general_purpose::STANDARD.encode(output))
        },
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "install native crypto derivation".into(),
        reason: "native crypto derivation could not be installed".into(),
    })?;
    ctx.globals()
        .set("__glassNativeRandomBytes", random_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native random source".into(),
            reason: "native random source could not be published".into(),
        })?;
    ctx.globals()
        .set(
            "__glassNativeArrayBufferIsAttached",
            array_buffer_is_attached,
        )
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native ArrayBuffer state source".into(),
            reason: "native ArrayBuffer state source could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeDetachArrayBuffer", array_buffer_detach)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native ArrayBuffer detach source".into(),
            reason: "native ArrayBuffer detach source could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeCryptoDigest", digest_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native crypto digest".into(),
            reason: "native crypto digest could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeCryptoHmac", hmac_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native crypto HMAC".into(),
            reason: "native crypto HMAC could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeCryptoEd25519", ed25519_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native Ed25519 source".into(),
            reason: "native Ed25519 source could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeCryptoAesGcm", aes_gcm_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native AES-GCM source".into(),
            reason: "native AES-GCM source could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeCryptoAesCbc", aes_cbc_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native AES-CBC source".into(),
            reason: "native AES-CBC source could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeCryptoAesCtr", aes_ctr_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native AES-CTR source".into(),
            reason: "native AES-CTR source could not be published".into(),
        })?;
    ctx.globals()
        .set("__glassNativeCryptoDerive", derive_source)
        .map_err(|_| NativeEngineError::Worker {
            operation: "install native crypto derivation".into(),
            reason: "native crypto derivation could not be published".into(),
        })
}
