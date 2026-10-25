use super::super::javascript::{
    NativeCookieChange, NativeIndexedDbState, NativeWebStorageState, load_cookie_profile,
    migrate_cookie_profile_from_web_storage, save_web_storage_profile,
};
use super::{
    JAVASCRIPT_MIME_TYPE_ESSENCES, MAX_NATIVE_CACHE_ENTRIES,
    MAX_NATIVE_CSP_SOURCE_EXPRESSION_BYTES, MAX_NATIVE_MEDIA_BYTES, NativeCookieProfileEntry,
    NativeCorsMode, NativeCspDirectives, NativeCspPolicy, NativeEngineConfig, NativeEngineError,
    NativeFetchCredentialsMode, NativeFetchMethod, NativeFetchReferrerPolicy, NativeInlineCspKind,
    NativeModuleResourceType, NativeNavigationMethod, NativeNavigationPolicyKind,
    NativeNetworkState, NativeObjectUrlResource, NativeRequestBody, NativeResource,
    NativeResourceLoader, NativeSubresourceKind, NativeTextCacheEntry, cache_control_max_age,
    cache_control_requires_revalidation, cached_resource_content_type_text_allowed,
    content_security_policy, cors_origin_header, cors_preflight_response_allowed,
    cors_response_allowed, csp_report_deliveries_for_declaration,
    csp_script_sources_allow_for_rooted_file, csp_sources_allow, csp_sources_allow_for_redirect,
    csp_sources_allow_for_rooted_file, data_font_bytes, data_media_metadata, decode_html_body,
    document_cache_fresh_until, document_cache_storage_allowed, fetch_credentials_for_url,
    fetch_referrer_for_target, fetch_referrer_source, javascript_mime_essence_allowed,
    javascript_mime_type_essence_match, media_metadata_from_bytes, mixed_content_allowed,
    module_content_type_text_allowed, referrer_for_navigation, referrer_for_navigation_with_policy,
    resolve_subresource_url, subresource_integrity_matches, supported_media_type_text,
};
use base64::Engine as _;
use reqwest::header::{
    ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_SECURITY_POLICY, HeaderMap, HeaderName,
    HeaderValue, PRAGMA, VARY,
};
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

#[test]
fn xhr_credentials_modes_match_same_origin_and_include_contracts() {
    let initiator = Url::parse("https://page.example.test/document").unwrap();
    let same_origin_target = Url::parse("https://page.example.test/xhr").unwrap();
    let cross_origin_target = Url::parse("https://api.example.test/xhr").unwrap();

    let default_mode = NativeFetchCredentialsMode::for_xhr(false);
    assert_eq!(default_mode, NativeFetchCredentialsMode::SameOrigin);
    assert!(fetch_credentials_for_url(
        Some(default_mode),
        false,
        &initiator,
        &same_origin_target,
    ));
    assert!(!fetch_credentials_for_url(
        Some(default_mode),
        false,
        &initiator,
        &cross_origin_target,
    ));

    let explicit_mode = NativeFetchCredentialsMode::for_xhr(true);
    assert_eq!(explicit_mode, NativeFetchCredentialsMode::Include);
    assert!(fetch_credentials_for_url(
        Some(explicit_mode),
        true,
        &initiator,
        &cross_origin_target,
    ));
}

#[test]
fn javascript_mime_type_attributes_and_response_headers_treat_parameters_differently() {
    assert_eq!(JAVASCRIPT_MIME_TYPE_ESSENCES.len(), 16);
    for essence in JAVASCRIPT_MIME_TYPE_ESSENCES {
        assert!(javascript_mime_type_essence_match(essence), "{essence}");
        assert!(javascript_mime_essence_allowed(essence), "{essence}");

        let parameterized = format!("{essence}; charset=utf-8");
        assert!(
            javascript_mime_essence_allowed(&parameterized),
            "external Content-Type should ignore parameters for {essence}"
        );
        assert!(
            !javascript_mime_type_essence_match(&parameterized),
            "script type attributes must not accept parameters for {essence}"
        );
    }
    assert!(!javascript_mime_essence_allowed("application/json"));
    assert!(!javascript_mime_type_essence_match("application/json"));
}

#[test]
fn json_module_mime_type_uses_the_mime_sniffing_json_group() {
    for content_type in [
        "application/json",
        "application/json; charset=utf-8",
        "text/json",
        "application/problem+json",
        "text/vnd.api+json; charset=utf-8",
    ] {
        assert!(
            module_content_type_text_allowed(NativeModuleResourceType::Json, Some(content_type)),
            "{content_type}"
        );
    }
    for content_type in ["text/plain", "text/javascript", "application/octet-stream"] {
        assert!(
            !module_content_type_text_allowed(NativeModuleResourceType::Json, Some(content_type)),
            "{content_type}"
        );
    }
    assert!(!module_content_type_text_allowed(
        NativeModuleResourceType::Json,
        None
    ));
    assert!(!module_content_type_text_allowed(
        NativeModuleResourceType::JavaScript,
        None
    ));
    assert!(!module_content_type_text_allowed(
        NativeModuleResourceType::Unsupported,
        Some("application/json")
    ));
    assert!(cached_resource_content_type_text_allowed(None, None));
    assert!(!cached_resource_content_type_text_allowed(
        None,
        Some("application/json")
    ));
}

#[test]
fn fetch_method_accepts_bounded_custom_http_tokens_without_widening_navigation() {
    for (input, expected) in [("report", "REPORT"), ("X-Glass-Method", "X-GLASS-METHOD")] {
        let method = NativeFetchMethod::from_fetch_method(input).unwrap();
        assert_eq!(method.as_str(), expected);
    }
    for input in ["", "bad method", "CONNECT", "TRACE", "TRACK"] {
        assert!(
            NativeFetchMethod::from_fetch_method(input).is_err(),
            "{input:?}"
        );
    }
    assert!(NativeFetchMethod::from_fetch_method(&"A".repeat(65)).is_err());
    assert!(NativeNavigationMethod::from_fetch_method("REPORT").is_err());
}

#[test]
fn binary_navigation_body_uses_compact_base64_wire_encoding() {
    let body = NativeRequestBody::Bytes((0_u8..=255).collect());
    let encoded = serde_json::to_value(&body).unwrap();
    assert_eq!(encoded["kind"], "bytes");
    let encoded_value = encoded["value"].as_str().unwrap();
    assert_eq!(encoded_value.len(), 344);
    assert!(encoded_value.len() < 256 * 2);
    let decoded: NativeRequestBody = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, body);
}

#[test]
fn decodes_bounded_declared_html_charsets() {
    assert_eq!(
        decode_html_body(b"caf\xe9", Some("windows-1252"), 32).unwrap(),
        "café"
    );
    assert_eq!(
        decode_html_body(&[0xff, 0xfe, b'c', 0, b'a', 0, b'f', 0], None, 32).unwrap(),
        "caf"
    );
    assert!(decode_html_body(b"\xff", Some("us-ascii"), 32).is_err());
    assert!(decode_html_body(b"text", Some("x-unknown"), 32).is_err());
}

#[test]
fn rooted_file_loader_reads_documents_and_denies_escape() {
    let root =
        std::env::temp_dir().join(format!("glass-native-file-loader-{}", std::process::id()));
    let outside = std::env::temp_dir().join(format!(
        "glass-native-file-loader-outside-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside);
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let page_path = root.join("index.html");
    let media_path = root.join("clip.wav");
    let outside_path = outside.join("outside.html");
    fs::write(&page_path, "<p>rooted</p>").unwrap();
    fs::write(&media_path, b"RIFFxxxxWAVE").unwrap();
    fs::write(&outside_path, "<p>outside</p>").unwrap();
    let page_url = Url::from_file_path(&page_path).unwrap().to_string();
    let outside_url = Url::from_file_path(&outside_path).unwrap().to_string();
    let config = NativeEngineConfig::default().with_allowed_file_root(root.clone());
    let loader = NativeResourceLoader::new(&config).unwrap();

    assert_eq!(loader.load(&page_url).unwrap().body, "<p>rooted</p>");
    assert!(matches!(
        loader.load(&outside_url),
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));
    let metadata = loader
        .load_local_file_media(&page_url, "clip.wav")
        .unwrap()
        .unwrap();
    assert_eq!(metadata.content_type, "audio/wav");
    assert_eq!(metadata.byte_length, 12);

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn media_metadata_accepts_supported_types_and_bounds_payloads() {
    let mut wav = vec![0_u8; 44 + 8000];
    wav[0..4].copy_from_slice(b"RIFF");
    wav[4..8].copy_from_slice(&(8036_u32).to_le_bytes());
    wav[8..12].copy_from_slice(b"WAVE");
    wav[12..16].copy_from_slice(b"fmt ");
    wav[16..20].copy_from_slice(&(16_u32).to_le_bytes());
    wav[20..22].copy_from_slice(&(1_u16).to_le_bytes());
    wav[22..24].copy_from_slice(&(1_u16).to_le_bytes());
    wav[24..28].copy_from_slice(&(8000_u32).to_le_bytes());
    wav[28..32].copy_from_slice(&(8000_u32).to_le_bytes());
    wav[32..34].copy_from_slice(&(1_u16).to_le_bytes());
    wav[34..36].copy_from_slice(&(8_u16).to_le_bytes());
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&(8000_u32).to_le_bytes());
    wav[44..].fill(128);

    assert_eq!(
        supported_media_type_text("audio/mp3; codecs=1"),
        Some("audio/mpeg")
    );
    let metadata = media_metadata_from_bytes(Some("audio/wav"), &wav)
        .unwrap()
        .unwrap();
    assert_eq!(metadata.content_type, "audio/wav");
    assert_eq!(metadata.byte_length, wav.len());
    assert_eq!(metadata.duration_millis, Some(1000));
    assert_eq!(
        media_metadata_from_bytes(None, b"not a media resource").unwrap(),
        None
    );
    assert_eq!(
        media_metadata_from_bytes(Some("application/octet-stream"), &wav).unwrap(),
        None
    );
    assert!(media_metadata_from_bytes(None, &vec![0_u8; MAX_NATIVE_MEDIA_BYTES + 1]).is_err());
}

#[test]
fn native_webm_vp8_decodes_bounded_real_rgba_frames() {
    let bytes = include_bytes!("../../../tests/fixtures/native-media/two-color-frames-vp8.webm");
    let metadata = media_metadata_from_bytes(Some("video/webm"), bytes)
        .unwrap()
        .unwrap();
    assert_eq!(metadata.content_type, "video/webm");
    assert_eq!(metadata.video_frames.len(), 2);
    assert!(metadata.duration_millis.unwrap() >= 900);
    for frame in &metadata.video_frames {
        assert_eq!((frame.width, frame.height), (16, 16));
        assert_eq!(frame.pixels.len(), 16 * 16 * 4);
        assert_eq!(frame.pixels[3], 255);
    }
    assert!(metadata.video_frames[0].pixels[0] > 200);
    assert!(metadata.video_frames[0].pixels[2] < 80);
    assert!(metadata.video_frames[1].pixels[0] < 80);
    assert!(metadata.video_frames[1].pixels[2] > 200);
}

#[test]
fn data_media_decodes_bounded_binary_payloads() {
    let url = Url::parse(
        "data:audio/wav;base64,UklGRiUAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQEAAACA",
    )
    .unwrap();
    let metadata = data_media_metadata(&url).unwrap().unwrap();
    assert_eq!(metadata.content_type, "audio/wav");
    assert_eq!(metadata.byte_length, 45);
    assert_eq!(metadata.duration_millis, Some(0));

    let sniffed = Url::parse("data:;base64,T2dnUw==").unwrap();
    assert_eq!(
        data_media_metadata(&sniffed).unwrap().unwrap().content_type,
        "application/ogg"
    );
    assert!(
        data_media_metadata(&Url::parse("data:application/octet-stream,%00").unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn data_font_decoding_is_bounded_and_csp_aware() {
    let payload = base64::engine::general_purpose::STANDARD.encode([0_u8, 1, 2, 3]);
    let url = Url::parse(&format!("data:font/ttf;base64,{payload}")).unwrap();
    assert_eq!(data_font_bytes(&url).unwrap(), Some(vec![0, 1, 2, 3]));
    assert!(
        data_font_bytes(&Url::parse("data:application/octet-stream,AAAA").unwrap())
            .unwrap()
            .is_none()
    );

    let config = NativeEngineConfig::default();
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    assert_eq!(
        loader
            .load_font("https://app.test/index.html", url.as_str(), None)
            .unwrap(),
        Some(vec![0, 1, 2, 3])
    );
    loader
        .set_document_content_security_policy_from_pairs(
            "https://app.test/index.html",
            &[("Content-Security-Policy".into(), "font-src 'none'".into())],
        )
        .unwrap();
    assert!(
        loader
            .load_font("https://app.test/index.html", url.as_str(), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn rooted_file_and_blob_font_sources_are_bounded() {
    let root = std::env::temp_dir().join(format!(
        "glass-rooted-font-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("index.html"),
        "<!doctype html><title>Fonts</title>",
    )
    .unwrap();
    let font_bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    std::fs::write(root.join("face.ttf"), font_bytes).unwrap();
    let page_url = Url::from_file_path(root.join("index.html"))
        .unwrap()
        .to_string();
    let config = NativeEngineConfig::default().with_allowed_file_root(root.clone());
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    let file_bytes = loader
        .load_font(&page_url, "face.ttf", None)
        .unwrap()
        .unwrap();
    assert_eq!(file_bytes, font_bytes);
    assert!(
        loader
            .load_font("https://app.test/index.html", "file:///etc/hosts", None)
            .unwrap()
            .is_none()
    );

    let object_url = NativeObjectUrlResource {
        content_type: Some("font/ttf".into()),
        body: vec![1, 2, 3, 4],
    };
    assert_eq!(
        loader
            .load_font(
                "fixture://app.test/index.html",
                "blob:https://app.test/native-font",
                Some(&object_url),
            )
            .unwrap(),
        Some(vec![1, 2, 3, 4])
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn network_font_response_preserves_url_headers_and_body() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: font/ttf\r\n",
            "Access-Control-Allow-Origin: *\r\n",
            "Content-Length: 4\r\n",
            "Connection: close\r\n\r\n",
            "font"
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let config = NativeEngineConfig::default();
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    let source = format!("http://{address}/font.ttf");
    let response = loader
        .load_font_response_async("http://app.test/index.html", &source, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.url, source);
    assert_eq!(response.status, 200);
    assert_eq!(response.status_text, "OK");
    assert_eq!(response.content_type.as_deref(), Some("font/ttf"));
    assert!(
        response
            .headers
            .iter()
            .any(|(name, value)| name == "content-type" && value == "font/ttf")
    );
    assert_eq!(response.body, b"font");
    assert!(!response.redirected);
    server.await.unwrap();
}

#[tokio::test]
async fn network_font_cache_reuses_fresh_response_without_refetching() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 2048];
        let read = stream.read(&mut request).await.unwrap();
        assert!(read > 0);
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: font/ttf\r\n",
            "Access-Control-Allow-Origin: *\r\n",
            "Cache-Control: public, max-age=60\r\n",
            "Content-Length: 4\r\n",
            "Connection: close\r\n\r\n",
            "font"
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let config = NativeEngineConfig::default();
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    let source = format!("http://{address}/font.ttf");
    let first = loader
        .load_font_response_async("http://app.test/index.html", &source, None)
        .await
        .unwrap();
    let second = loader
        .load_font_response_async("http://app.test/index.html", &source, None)
        .await
        .unwrap();
    assert_eq!(
        first.as_ref().map(|response| response.body.as_slice()),
        Some(&b"font"[..])
    );
    assert_eq!(second, first);
    assert_eq!(
        first
            .as_ref()
            .and_then(|response| response.content_type.as_deref()),
        Some("font/ttf")
    );
    assert_eq!(loader.network.font_cache.len(), 1);
    assert_eq!(
        loader
            .load_font_async("http://app.test/index.html", &source, None)
            .await
            .unwrap(),
        Some(b"font".to_vec())
    );
    server.await.unwrap();
}

#[tokio::test]
async fn network_font_cache_revalidates_with_etag_and_304() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut first_stream, _) = listener.accept().await.unwrap();
        let mut first_request = [0_u8; 2048];
        let first_read = first_stream.read(&mut first_request).await.unwrap();
        assert!(first_read > 0);
        let first_response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: font/ttf\r\n",
            "Access-Control-Allow-Origin: *\r\n",
            "Cache-Control: no-cache\r\n",
            "ETag: \"font-v1\"\r\n",
            "Content-Length: 4\r\n",
            "Connection: close\r\n\r\n",
            "font"
        );
        first_stream
            .write_all(first_response.as_bytes())
            .await
            .unwrap();

        let (mut second_stream, _) = listener.accept().await.unwrap();
        let mut second_request = [0_u8; 2048];
        let second_read = second_stream.read(&mut second_request).await.unwrap();
        let second_request = String::from_utf8_lossy(&second_request[..second_read]);
        assert!(
            second_request
                .to_ascii_lowercase()
                .contains("if-none-match: \"font-v1\"")
        );
        let second_response = concat!(
            "HTTP/1.1 304 Not Modified\r\n",
            "Cache-Control: public, max-age=60\r\n",
            "ETag: \"font-v1\"\r\n",
            "Connection: close\r\n\r\n"
        );
        second_stream
            .write_all(second_response.as_bytes())
            .await
            .unwrap();
    });
    let config = NativeEngineConfig::default();
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    let source = format!("http://{address}/font.ttf");
    let first = loader
        .load_font_async("http://app.test/index.html", &source, None)
        .await
        .unwrap();
    let second = loader
        .load_font_async("http://app.test/index.html", &source, None)
        .await
        .unwrap();
    assert_eq!(first, Some(b"font".to_vec()));
    assert_eq!(second, first);
    let response = loader
        .load_font_response_async("http://app.test/index.html", &source, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.url, source);
    assert_eq!(response.content_type.as_deref(), Some("font/ttf"));
    assert_eq!(response.body, b"font");
    assert_eq!(loader.network.font_cache.len(), 1);
    server.await.unwrap();
}

#[test]
fn subresource_integrity_uses_valid_strongest_hash_metadata() {
    let body = b"globalThis.integrity = true;";
    let sha256 = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body));
    let sha384 = base64::engine::general_purpose::STANDARD.encode(Sha384::digest(body));
    let sha512 = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(body));
    let wrong_sha256 = base64::engine::general_purpose::STANDARD.encode([0_u8; 32]);
    let wrong_sha512 = base64::engine::general_purpose::STANDARD.encode([0_u8; 64]);

    assert!(subresource_integrity_matches(None, body));
    assert!(subresource_integrity_matches(
        Some(&format!("sha256-{sha256}")),
        body
    ));
    assert!(subresource_integrity_matches(
        Some(&format!("sha384-{sha384}?future-option sha256-{sha256}")),
        body
    ));
    assert!(subresource_integrity_matches(
        Some(&format!("sha512-{sha512} sha384-wrong")),
        body
    ));
    assert!(!subresource_integrity_matches(
        Some(&format!("sha512-{wrong_sha512} sha256-{sha256}")),
        body
    ));
    assert!(subresource_integrity_matches(Some("sha999-unknown"), body));
    assert!(!subresource_integrity_matches(
        Some(&format!("sha256-{wrong_sha256}")),
        body
    ));
}

#[tokio::test]
async fn content_process_loader_cannot_read_or_change_cookie_state() {
    let mut loader = NativeResourceLoader::for_content_process(1024, None, &[]).unwrap();
    let url = Url::parse("https://app.test/page").unwrap();

    let direct_request_error = loader.load_async(url.as_str()).await.unwrap_err();
    assert!(direct_request_error.to_string().contains("parent broker"));

    assert_eq!(
        loader.network.cookie_header_for_request(
            &url,
            Some(&url),
            false,
            NativeNavigationMethod::Get,
        ),
        None
    );
    assert!(
        loader
            .network
            .store_cookie(&url, "secret=child; HttpOnly; Path=/")
            .is_empty()
    );
    loader
        .set_document_cookie(url.as_str(), "visible=child; Path=/")
        .unwrap();

    assert_eq!(loader.document_cookie(url.as_str()).unwrap(), "");
    assert!(loader.cookie_profile().is_empty());
    assert!(loader.take_cookie_changes().is_empty());
    let mut parent_loader = NativeResourceLoader::new(&NativeEngineConfig::default()).unwrap();
    parent_loader
        .set_document_cookie(url.as_str(), "parent=owned; Path=/")
        .unwrap();
    let parent_profile = parent_loader.cookie_profile();
    assert_eq!(parent_profile.len(), 1);
    assert!(loader.set_cookie_profiles(&parent_profile).is_err());
    assert!(
        loader
            .apply_cookie_changes(&[NativeCookieChange {
                name: "secret".into(),
                domain: "app.test".into(),
                path: "/".into(),
                cookie: None,
            }])
            .is_err()
    );
    assert!(loader.merge_fetch_task_state(parent_loader).is_err());
    assert!(loader.cookie_profile().is_empty());
}

#[test]
fn cookie_state_obeys_scope_security_expiration_and_bounds() {
    let mut state = NativeNetworkState::default();
    let page = Url::parse("http://example.test/account/login").unwrap();
    state.store_cookie(&page, "session=alpha; Path=/account");
    let secure_page = Url::parse("https://example.test/account/login").unwrap();
    state.store_cookie(&secure_page, "secure=secret; Secure; Path=/");
    state.store_cookie(&page, "hidden=value; HttpOnly; Path=/account");
    assert_eq!(
        state.cookie_header_for_request(
            &Url::parse("http://example.test/account/home").unwrap(),
            None,
            false,
            NativeNavigationMethod::Get,
        ),
        Some("hidden=value; session=alpha".into())
    );
    assert_eq!(
        state.cookie_header_for_request(
            &Url::parse("http://example.test/public").unwrap(),
            None,
            false,
            NativeNavigationMethod::Get,
        ),
        None
    );
    assert_eq!(
        state.cookie_header_for_request(
            &Url::parse("https://example.test/account/home").unwrap(),
            None,
            false,
            NativeNavigationMethod::Get,
        ),
        Some("hidden=value; session=alpha; secure=secret".into())
    );
    assert_eq!(
        state.document_cookie_header(&Url::parse("https://example.test/account/home").unwrap()),
        Some("session=alpha; secure=secret".into())
    );

    state.store_cookie(&page, "session=gone; Path=/account; Max-Age=0");
    assert_eq!(
        state.cookie_header_for_request(
            &Url::parse("http://example.test/account/home").unwrap(),
            None,
            false,
            NativeNavigationMethod::Get,
        ),
        Some("hidden=value".into())
    );

    for index in 0..(MAX_NATIVE_CACHE_ENTRIES + 1) {
        state.store_document_cache(
            format!("http://example.test/{index}"),
            super::NativeDocumentCacheEntry {
                resource: NativeResource {
                    url: format!("http://example.test/{index}"),
                    origin: super::NativeOrigin::from_url(
                        &Url::parse("http://example.test/").unwrap(),
                    )
                    .unwrap(),
                    body: index.to_string(),
                },
                fresh_until: None,
                etag: None,
                last_modified: None,
            },
        );
    }
    assert_eq!(state.cache.len(), MAX_NATIVE_CACHE_ENTRIES);
}

#[test]
fn cookie_same_site_policy_preserves_metadata_and_filters_cross_site_requests() {
    let mut state = NativeNetworkState::default();
    let app = Url::parse("https://app.example.test/account/page").unwrap();
    let api = Url::parse("https://api.example.test/data").unwrap();
    let evil = Url::parse("https://evil.test/data").unwrap();

    state.store_cookie(
        &app,
        "strict=1; Domain=example.test; Path=/; SameSite=Strict; Priority=High",
    );
    state.store_cookie(
        &app,
        "lax=1; Domain=example.test; Path=/; SameSite=Lax; Priority=Medium",
    );
    state.store_cookie(
        &app,
        "none=1; Domain=example.test; Path=/; SameSite=None; Secure; Priority=Low",
    );
    state.store_cookie(&evil, "strict=1; Path=/; SameSite=Strict");
    state.store_cookie(&evil, "lax=1; Path=/; SameSite=Lax");
    state.store_cookie(&evil, "default=1; Path=/");
    state.store_cookie(&evil, "none=1; Path=/; SameSite=None; Secure");

    assert_eq!(
        state.cookie_header_for_request(&api, Some(&app), false, NativeNavigationMethod::Get),
        Some("lax=1; none=1; strict=1".into())
    );
    assert_eq!(
        state.cookie_header_for_request(&evil, Some(&app), false, NativeNavigationMethod::Get,),
        Some("none=1".into())
    );
    assert_eq!(
        state.cookie_header_for_request(&evil, Some(&app), true, NativeNavigationMethod::Get,),
        Some("default=1; lax=1; none=1".into())
    );
    assert_eq!(
        state.cookie_header_for_request(&evil, Some(&app), true, NativeNavigationMethod::Post,),
        Some("none=1".into())
    );

    let profiles = state.cookie_profile();
    let strict = profiles
        .iter()
        .find(|cookie| cookie.name == "strict" && cookie.domain == "example.test")
        .unwrap();
    assert_eq!(strict.same_site.as_deref(), Some("Strict"));
    assert_eq!(strict.priority.as_deref(), Some("High"));

    assert!(
        state
            .store_cookie(&evil, "invalid=1; Path=/; SameSite=None")
            .is_empty()
    );
    assert!(
        NativeNetworkState::from_profile(vec![NativeCookieProfileEntry {
            name: "invalid".into(),
            value: "1".into(),
            domain: "evil.test".into(),
            path: "/".into(),
            host_only: true,
            secure: false,
            http_only: false,
            same_site: Some("None".into()),
            priority: None,
            expires_at_unix_seconds: None,
        }])
        .is_err()
    );
}

#[test]
fn cookie_profile_merges_stale_key_changes() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-cookie-merge.json",
        std::process::id()
    ));
    let cookie_path = PathBuf::from(format!("{}.cookies", profile_path.display()));
    let lock_path = profile_path.with_extension("lock");
    let cookie_lock_path = cookie_path.with_extension("lock");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&cookie_path);
    let _ = fs::remove_file(&lock_path);
    let _ = fs::remove_file(&cookie_lock_path);
    let config = NativeEngineConfig::default().with_storage_path(profile_path.clone());
    let page = "https://example.test/account/page";
    let mut first = NativeResourceLoader::new(&config).unwrap();
    let mut second = NativeResourceLoader::new(&config).unwrap();
    first
        .set_document_cookie(page, "first=one; Path=/")
        .unwrap();
    second
        .set_document_cookie(page, "second=two; Path=/")
        .unwrap();

    let first_state = first.cookie_profile();
    let first_changes = first.take_cookie_changes();
    save_web_storage_profile(
        Some(&profile_path),
        &NativeWebStorageState::default(),
        &[],
        &first_state,
        &first_changes,
        &NativeIndexedDbState::default(),
        &[],
    )
    .unwrap();
    let second_state = second.cookie_profile();
    let second_changes = second.take_cookie_changes();
    save_web_storage_profile(
        Some(&profile_path),
        &NativeWebStorageState::default(),
        &[],
        &second_state,
        &second_changes,
        &NativeIndexedDbState::default(),
        &[],
    )
    .unwrap();

    let reopened = NativeResourceLoader::new(&config).unwrap();
    assert_eq!(
        reopened.document_cookie(page).unwrap(),
        "first=one; second=two"
    );
    let web_profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&profile_path).unwrap()).unwrap();
    let cookie_profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&cookie_path).unwrap()).unwrap();
    assert_eq!(web_profile["cookies"], serde_json::json!([]));
    assert_eq!(cookie_profile["cookies"].as_array().unwrap().len(), 2);

    let content_loader =
        NativeResourceLoader::for_content_process(256 * 1024, Some(&profile_path), &[]).unwrap();
    assert!(
        content_loader
            .cookies_for_document(page)
            .unwrap()
            .is_empty()
    );

    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(cookie_path);
    let _ = fs::remove_file(lock_path);
    let _ = fs::remove_file(cookie_lock_path);
}

#[test]
fn legacy_cookie_profile_migrates_out_of_content_readable_storage_file() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-cookie-migrate.json",
        std::process::id()
    ));
    let cookie_path = PathBuf::from(format!("{}.cookies", profile_path.display()));
    let lock_path = profile_path.with_extension("lock");
    let cookie_lock_path = cookie_path.with_extension("lock");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&cookie_path);
    let _ = fs::remove_file(&lock_path);
    let _ = fs::remove_file(&cookie_lock_path);
    let legacy_cookie = serde_json::json!({
        "name": "legacy_secret",
        "value": "must-stay-parent-owned",
        "domain": "example.test",
        "path": "/",
        "host_only": true,
        "secure": false,
        "http_only": true,
        "same_site": "Lax",
        "priority": null,
        "expires_at_unix_seconds": null,
    });
    fs::write(
        &profile_path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": 7,
            "local": {},
            "cookies": [legacy_cookie],
        }))
        .unwrap(),
    )
    .unwrap();

    migrate_cookie_profile_from_web_storage(Some(&profile_path)).unwrap();

    let web_profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&profile_path).unwrap()).unwrap();
    assert_eq!(web_profile["revision"], 7);
    assert_eq!(web_profile["cookies"], serde_json::json!([]));
    assert_eq!(
        load_cookie_profile(Some(&profile_path))
            .unwrap()
            .first()
            .map(|cookie| cookie.value.as_str()),
        Some("must-stay-parent-owned")
    );

    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(cookie_path);
    let _ = fs::remove_file(lock_path);
    let _ = fs::remove_file(cookie_lock_path);
}

#[test]
fn cache_policy_rejects_private_or_stale_variants() {
    let mut headers = HeaderMap::new();
    assert!(document_cache_storage_allowed(&headers));
    assert!(!cache_control_requires_revalidation(&headers));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    assert!(document_cache_storage_allowed(&headers));
    assert!(!cache_control_requires_revalidation(&headers));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    assert!(!document_cache_storage_allowed(&headers));
    headers.remove(CACHE_CONTROL);
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    assert!(document_cache_storage_allowed(&headers));
    assert!(cache_control_requires_revalidation(&headers));
    headers.remove(PRAGMA);
    headers.insert(VARY, HeaderValue::from_static("Accept-Encoding, Cookie"));
    assert!(!document_cache_storage_allowed(&headers));
}

#[test]
fn document_cache_tracks_freshness_and_revalidation_metadata() {
    let now = Instant::now();
    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    assert_eq!(cache_control_max_age(&headers), Some(60));
    assert!(document_cache_storage_allowed(&headers));
    assert!(document_cache_fresh_until(&headers, now).is_some_and(|deadline| deadline > now));

    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    assert!(document_cache_storage_allowed(&headers));
    assert_eq!(document_cache_fresh_until(&headers, now), Some(now));

    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    assert!(!document_cache_storage_allowed(&headers));
}

#[test]
fn script_cache_preserves_module_response_referrer_policy_across_revalidation() {
    let now = Instant::now();
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    response_headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("origin"),
    );
    let entry = NativeTextCacheEntry::from_response(
        "https://module.test/entry.js".into(),
        "export {};".into(),
        &response_headers,
        now,
    )
    .expect("cacheable module response metadata must be retained");
    assert_eq!(
        entry.referrer_policy,
        Some(NativeFetchReferrerPolicy::Origin)
    );

    let absent_header = HeaderMap::new();
    let unchanged = entry
        .clone()
        .refresh_from_not_modified(&absent_header, now)
        .unwrap();
    assert_eq!(
        unchanged.referrer_policy,
        Some(NativeFetchReferrerPolicy::Origin)
    );

    let mut replacement_headers = HeaderMap::new();
    replacement_headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    let replaced = entry
        .clone()
        .refresh_from_not_modified(&replacement_headers, now)
        .unwrap();
    assert_eq!(
        replaced.referrer_policy,
        Some(NativeFetchReferrerPolicy::NoReferrer)
    );

    replacement_headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("not-a-policy"),
    );
    let ignored = entry
        .refresh_from_not_modified(&replacement_headers, now)
        .unwrap();
    assert_eq!(ignored.referrer_policy, None);
}

#[test]
fn referrer_policy_is_full_same_origin_origin_only_cross_origin_and_none_on_downgrade() {
    assert_eq!(
        referrer_for_navigation(
            "http://source.test/path/page?query=1#fragment",
            "http://source.test/next",
        )
        .unwrap(),
        Some("http://source.test/path/page?query=1".into())
    );
    assert_eq!(
        referrer_for_navigation("http://source.test/path/page", "http://target.test/next").unwrap(),
        Some("http://source.test/".into())
    );
    assert_eq!(
        referrer_for_navigation("https://source.test/path", "http://target.test/next").unwrap(),
        None
    );
    assert_eq!(
        referrer_for_navigation("data:text/html,<p>local</p>", "http://target.test/next").unwrap(),
        None
    );
}

#[test]
fn navigation_referrer_policy_is_applied_before_cross_origin_requests() {
    let source = "https://source.test/path/page?token=secret#fragment";
    let target = "https://target.test/next";
    assert_eq!(
        referrer_for_navigation_with_policy(source, target, NativeFetchReferrerPolicy::NoReferrer,)
            .unwrap(),
        None
    );
    assert_eq!(
        referrer_for_navigation_with_policy(source, target, NativeFetchReferrerPolicy::Origin,)
            .unwrap(),
        Some("https://source.test/".into())
    );
    assert_eq!(
        referrer_for_navigation_with_policy(source, target, NativeFetchReferrerPolicy::UnsafeUrl,)
            .unwrap(),
        Some("https://source.test/path/page?token=secret".into())
    );
}

#[test]
fn fetch_referrer_policy_accepts_all_standard_tokens_and_empty_default() {
    let cases = [
        ("", NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin),
        ("no-referrer", NativeFetchReferrerPolicy::NoReferrer),
        (
            "no-referrer-when-downgrade",
            NativeFetchReferrerPolicy::NoReferrerWhenDowngrade,
        ),
        ("same-origin", NativeFetchReferrerPolicy::SameOrigin),
        ("origin", NativeFetchReferrerPolicy::Origin),
        ("strict-origin", NativeFetchReferrerPolicy::StrictOrigin),
        (
            "origin-when-cross-origin",
            NativeFetchReferrerPolicy::OriginWhenCrossOrigin,
        ),
        (
            "strict-origin-when-cross-origin",
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
        ),
        ("unsafe-url", NativeFetchReferrerPolicy::UnsafeUrl),
    ];

    for (token, expected) in cases {
        assert_eq!(NativeFetchReferrerPolicy::parse(token).unwrap(), expected);
    }
    assert!(NativeFetchReferrerPolicy::parse("invalid").is_err());
}

#[test]
fn fetch_referrer_policies_compute_each_target_from_the_original_source() {
    let source = Url::parse("https://source.test/account?token=hidden#fragment").unwrap();
    let same_origin = Url::parse("https://source.test/next").unwrap();
    let cross_origin = Url::parse("https://target.test/next").unwrap();
    let downgrade = Url::parse("http://target.test/next").unwrap();
    let full = "https://source.test/account?token=hidden";
    let origin = "https://source.test/";

    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &same_origin,
            NativeFetchReferrerPolicy::NoReferrer
        ),
        None
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &cross_origin,
            NativeFetchReferrerPolicy::NoReferrerWhenDowngrade,
        ),
        Some(full.into())
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &downgrade,
            NativeFetchReferrerPolicy::NoReferrerWhenDowngrade,
        ),
        None
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &same_origin,
            NativeFetchReferrerPolicy::SameOrigin
        ),
        Some(full.into())
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &cross_origin,
            NativeFetchReferrerPolicy::SameOrigin
        ),
        None
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &cross_origin,
            NativeFetchReferrerPolicy::Origin
        ),
        Some(origin.into())
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &downgrade,
            NativeFetchReferrerPolicy::StrictOrigin
        ),
        None
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &cross_origin,
            NativeFetchReferrerPolicy::OriginWhenCrossOrigin,
        ),
        Some(origin.into())
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &downgrade,
            NativeFetchReferrerPolicy::OriginWhenCrossOrigin,
        ),
        Some(origin.into())
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &same_origin,
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
        ),
        Some(full.into())
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &downgrade,
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
        ),
        None
    );
    assert_eq!(
        fetch_referrer_for_target(
            Some(&source),
            &downgrade,
            NativeFetchReferrerPolicy::UnsafeUrl
        ),
        Some(full.into())
    );
}

#[test]
fn fetch_referrer_source_removes_credentials_and_fragment_and_enforces_owner_origin() {
    let owner = Url::parse("https://source.test/page").unwrap();
    let referrer = fetch_referrer_source(
        Some("https://user:secret@source.test/path?query=1#fragment"),
        &owner,
    )
    .unwrap()
    .unwrap();
    assert_eq!(referrer.as_str(), "https://source.test/path?query=1");
    assert!(fetch_referrer_source(Some(""), &owner).unwrap().is_none());
    assert!(fetch_referrer_source(Some("https://other.test/path"), &owner).is_err());
}

#[test]
fn csp_source_lists_cover_resource_families_and_default_fallback() {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src https://scripts.test; img-src *; connect-src 'self'",
        ),
    );
    let policy = content_security_policy(&headers);
    let document = Url::parse("https://app.test/index.html").unwrap();
    let script = Url::parse("https://scripts.test/app.js").unwrap();
    let image = Url::parse("https://images.test/logo.png").unwrap();
    let socket = Url::parse("https://api.test/data").unwrap();

    assert!(policy.allows(NativeSubresourceKind::Script, &document, &script));
    assert!(policy.allows(NativeSubresourceKind::Image, &document, &image));
    assert!(!policy.allows(NativeSubresourceKind::Style, &document, &document));
    assert!(policy.allows(NativeSubresourceKind::Connect, &document, &document));
    assert!(!policy.allows(NativeSubresourceKind::Connect, &document, &socket));
    assert!(!policy.allows(NativeSubresourceKind::Font, &document, &document));
}

#[test]
fn csp_source_expressions_match_scheme_host_port_path_and_upgrade_rules() {
    let document = Url::parse("http://app.test/index.html").unwrap();
    let wildcard = vec!["*.Example.test".to_owned()];
    assert!(csp_sources_allow(
        Some(&wildcard),
        &document,
        &Url::parse("https://cdn.example.test/assets/app.js").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&wildcard),
        &document,
        &Url::parse("https://example.test/assets/app.js").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&wildcard),
        &document,
        &Url::parse("https://badexample.test/assets/app.js").unwrap()
    ));

    let path = vec!["https://cdn.example.test/assets/".to_owned()];
    assert!(csp_sources_allow(
        Some(&path),
        &document,
        &Url::parse("https://cdn.example.test/assets/app.js?cache=1#fragment").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&path),
        &document,
        &Url::parse("https://cdn.example.test/asset/app.js").unwrap()
    ));
    let exact_path = vec!["https://cdn.example.test/assets/app%2Ejs".to_owned()];
    assert!(csp_sources_allow(
        Some(&exact_path),
        &document,
        &Url::parse("https://cdn.example.test/assets/app.js").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&exact_path),
        &document,
        &Url::parse("https://cdn.example.test/assets/app.js/extra").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&exact_path),
        &document,
        &Url::parse("https://cdn.example.test/redirected.js").unwrap()
    ));
    assert!(csp_sources_allow_for_redirect(
        Some(&exact_path),
        &document,
        &Url::parse("https://cdn.example.test/redirected.js").unwrap()
    ));
    assert!(!csp_sources_allow_for_redirect(
        Some(&exact_path),
        &document,
        &Url::parse("https://other.example.test/redirected.js").unwrap()
    ));

    let default_port = vec!["https://api.test".to_owned()];
    assert!(csp_sources_allow(
        Some(&default_port),
        &document,
        &Url::parse("https://api.test/").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&default_port),
        &document,
        &Url::parse("https://api.test:8443/").unwrap()
    ));
    let explicit_port = vec!["https://api.test:8443".to_owned()];
    assert!(csp_sources_allow(
        Some(&explicit_port),
        &document,
        &Url::parse("https://api.test:8443/").unwrap()
    ));
    let any_port = vec!["https://api.test:*".to_owned()];
    assert!(csp_sources_allow(
        Some(&any_port),
        &document,
        &Url::parse("https://api.test:8443/").unwrap()
    ));

    let http_scheme = vec!["http:".to_owned()];
    assert!(csp_sources_allow(
        Some(&http_scheme),
        &document,
        &Url::parse("https://cdn.test/app.js").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&http_scheme),
        &document,
        &Url::parse("ftp://cdn.test/app.js").unwrap()
    ));
    let https_scheme = vec!["https:".to_owned()];
    assert!(!csp_sources_allow(
        Some(&https_scheme),
        &document,
        &Url::parse("http://cdn.test/app.js").unwrap()
    ));

    let self_source = vec!["'self'".to_owned()];
    assert!(csp_sources_allow(
        Some(&self_source),
        &document,
        &Url::parse("https://app.test/secure").unwrap()
    ));
    assert!(!csp_sources_allow(
        Some(&self_source),
        &document,
        &Url::parse("https://app.test:8443/secure").unwrap()
    ));

    let mixed_none = vec!["'none'".to_owned(), "https://cdn.test".to_owned()];
    assert!(csp_sources_allow(
        Some(&mixed_none),
        &document,
        &Url::parse("https://cdn.test/app.js").unwrap()
    ));
    let malformed = vec![
        "https://user:pass@example.test".to_owned(),
        "https://example.test:bad".to_owned(),
        "https://example.test/path?query".to_owned(),
        "https://example.test/path%ZZ".to_owned(),
        "https://example.test/path;comma".to_owned(),
        "https://example.test:70000".to_owned(),
    ];
    assert!(!csp_sources_allow(
        Some(&malformed),
        &document,
        &Url::parse("https://example.test/path").unwrap()
    ));
    let oversized = vec![format!(
        "https://{}.test",
        "a".repeat(MAX_NATIVE_CSP_SOURCE_EXPRESSION_BYTES)
    )];
    assert!(!csp_sources_allow(
        Some(&oversized),
        &document,
        &Url::parse("https://example.test/path").unwrap()
    ));
}

#[test]
fn csp_strict_dynamic_uses_parser_metadata_and_external_nonces() {
    let document = Url::parse("https://app.test/index.html").unwrap();
    let target = Url::parse("https://unlisted.test/app.js").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'strict-dynamic' 'nonce-Boot123' https:",
        ),
    );
    let policy = content_security_policy(&headers);

    assert!(policy.allows_script(&document, &target, true, Some("Boot123")));
    assert!(!policy.allows_script(&document, &target, true, Some("wrong")));
    assert!(!policy.allows_script(&document, &target, true, None));
    assert!(policy.allows_script(&document, &target, false, None));

    let image = Url::parse("https://unlisted.test/logo.png").unwrap();
    assert!(!policy.allows(NativeSubresourceKind::Image, &document, &image));
}

#[test]
fn rooted_file_csp_self_uses_the_admitting_configured_root() {
    let document = Url::parse("file:///trusted/site/index.html").unwrap();
    let same_root = Url::parse("file:///trusted/site/app.js").unwrap();
    let other_root = Url::parse("file:///trusted/other/app.js").unwrap();
    let self_source = vec!["'self'".to_owned()];

    assert!(csp_script_sources_allow_for_rooted_file(
        Some(&self_source),
        &document,
        &same_root,
        true,
        None,
        true,
    ));
    assert!(!csp_script_sources_allow_for_rooted_file(
        Some(&self_source),
        &document,
        &other_root,
        true,
        None,
        false,
    ));

    let explicit_file_scheme = vec!["file:".to_owned()];
    assert!(csp_script_sources_allow_for_rooted_file(
        Some(&explicit_file_scheme),
        &document,
        &other_root,
        true,
        None,
        false,
    ));
}

#[test]
fn rooted_file_style_csp_uses_fallback_nonce_and_root() {
    let document_url = Url::parse("file:///trusted/site/index.html").unwrap();
    let same_root_url = Url::parse("file:///trusted/site/style.css").unwrap();
    let other_root_url = Url::parse("file:///trusted/other/style.css").unwrap();
    let same_root = PathBuf::from("/trusted/site");
    let other_root = PathBuf::from("/trusted/other");

    let style_fallback = NativeCspDirectives {
        style_sources: Some(vec!["'self'".to_owned()]),
        default_sources: Some(vec!["'none'".to_owned()]),
        ..NativeCspDirectives::default()
    };
    assert!(style_fallback.allows_rooted_file_style(&document_url, &same_root_url, None, true,));
    assert!(!style_fallback.allows_rooted_file_style(&document_url, &other_root_url, None, false,));

    let element_override = NativeCspDirectives {
        style_sources: Some(vec!["'self'".to_owned()]),
        style_element_sources: Some(vec!["'none'".to_owned()]),
        ..NativeCspDirectives::default()
    };
    assert!(!element_override.allows_rooted_file_style(&document_url, &same_root_url, None, true,));

    let default_fallback = NativeCspDirectives {
        default_sources: Some(vec!["'self'".to_owned()]),
        ..NativeCspDirectives::default()
    };
    assert!(default_fallback.allows_rooted_file_style(&document_url, &same_root_url, None, true,));
    assert!(!default_fallback.allows_rooted_file_style(
        &document_url,
        &other_root_url,
        None,
        false,
    ));

    let nonce_policy = NativeCspDirectives {
        style_element_sources: Some(vec!["'nonce-Style123'".to_owned()]),
        ..NativeCspDirectives::default()
    };
    assert!(nonce_policy.allows_rooted_file_style(
        &document_url,
        &other_root_url,
        Some("Style123"),
        false,
    ));
    assert!(!nonce_policy.allows_rooted_file_style(
        &document_url,
        &other_root_url,
        Some("wrong"),
        false,
    ));

    let mut conjunctive = NativeCspPolicy {
        policies: vec![nonce_policy, style_fallback],
        rooted_file_self_root: Some(same_root.clone()),
        ..NativeCspPolicy::default()
    };
    assert!(conjunctive.allows_rooted_file_style(
        &document_url,
        &same_root_url,
        Some("Style123"),
        Some(&same_root),
    ));
    assert!(!conjunctive.allows_rooted_file_style(
        &document_url,
        &other_root_url,
        Some("Style123"),
        Some(&other_root),
    ));
    conjunctive.policies.clear();
    assert!(csp_sources_allow_for_rooted_file(
        Some(&["file:".to_owned()]),
        &document_url,
        &other_root_url,
        None,
        false,
    ));
}

#[test]
fn rooted_file_csp_denies_script_before_reading_bytes() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-rooted-file-csp-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let other = root.join("other");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    fs::create_dir_all(&other).unwrap();
    let document_path = site.join("index.html");
    let denied_script_path = other.join("invalid-utf8.js");
    fs::write(&document_path, "<!doctype html>").unwrap();
    fs::write(&denied_script_path, [0xff]).unwrap();
    let document_url = Url::from_file_path(&document_path).unwrap().to_string();
    let denied_script_url = Url::from_file_path(&denied_script_path)
        .unwrap()
        .to_string();
    let config = NativeEngineConfig::default()
        .with_allowed_file_root(site)
        .with_allowed_file_root(other);
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    loader
        .apply_meta_content_security_policies(&document_url, &["script-src 'self'".to_owned()])
        .unwrap();

    assert!(
        loader
            .load_local_file_script_with_policy(
                &document_url,
                &denied_script_url,
                1024,
                true,
                None,
                None,
            )
            .unwrap()
            .is_none()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rooted_file_style_csp_denies_css_before_reading_bytes() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-rooted-file-style-csp-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let other = root.join("other");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    fs::create_dir_all(&other).unwrap();
    let document_path = site.join("index.html");
    let denied_stylesheet_path = other.join("invalid-utf8.css");
    fs::write(&document_path, "<!doctype html>").unwrap();
    fs::write(&denied_stylesheet_path, [0xff]).unwrap();
    let document_url = Url::from_file_path(&document_path).unwrap().to_string();
    let denied_stylesheet_url = Url::from_file_path(&denied_stylesheet_path)
        .unwrap()
        .to_string();
    let config = NativeEngineConfig::default()
        .with_allowed_file_root(&site)
        .with_allowed_file_root(&other);
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    loader
        .apply_meta_content_security_policies(&document_url, &["style-src 'self'".to_owned()])
        .unwrap();

    assert!(
        loader
            .load_local_file_stylesheet(&document_url, &denied_stylesheet_url, None)
            .unwrap()
            .is_none()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rooted_file_image_font_media_csp_uses_directives_fallback_and_roots() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-rooted-file-image-font-media-csp-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let other = root.join("other");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();
    for directory in [&site, &other] {
        fs::write(
            directory.join("image.jpg"),
            include_bytes!("../../../tests/fixtures/remote-android-concept.jpg"),
        )
        .unwrap();
        fs::write(directory.join("font.ttf"), b"bounded font bytes").unwrap();
        fs::write(directory.join("media.wav"), b"bounded media bytes").unwrap();
    }

    let document_url = Url::from_file_path(site.join("index.html"))
        .unwrap()
        .to_string();
    let other_image_url = Url::from_file_path(other.join("image.jpg"))
        .unwrap()
        .to_string();
    let other_font_url = Url::from_file_path(other.join("font.ttf"))
        .unwrap()
        .to_string();
    let other_media_url = Url::from_file_path(other.join("media.wav"))
        .unwrap()
        .to_string();
    let config = NativeEngineConfig::default()
        .with_allowed_file_root(&site)
        .with_allowed_file_root(&other);
    let mut loader = NativeResourceLoader::new(&config).unwrap();
    loader
        .apply_meta_content_security_policies(
            &document_url,
            &["default-src 'none'; img-src 'self'; font-src 'self'; media-src 'self'".into()],
        )
        .unwrap();

    assert!(
        loader
            .load_local_file_image(&document_url, "image.jpg")
            .unwrap()
            .is_some()
    );
    assert!(
        loader
            .load_font(&document_url, "font.ttf", None)
            .unwrap()
            .is_some()
    );
    assert!(
        loader
            .load_local_file_media(&document_url, "media.wav")
            .unwrap()
            .is_some()
    );
    assert!(
        loader
            .load_local_file_image(&document_url, &other_image_url)
            .unwrap()
            .is_none()
    );
    assert!(
        loader
            .load_font(&document_url, &other_font_url, None)
            .unwrap()
            .is_none()
    );
    assert!(
        loader
            .load_local_file_media(&document_url, &other_media_url)
            .unwrap()
            .is_none()
    );

    loader
        .apply_meta_content_security_policies(&document_url, &["img-src 'none'".into()])
        .unwrap();
    assert!(
        loader
            .load_local_file_image(&document_url, "image.jpg")
            .unwrap()
            .is_none()
    );

    let mut fallback_loader = NativeResourceLoader::new(&config).unwrap();
    fallback_loader
        .apply_meta_content_security_policies(&document_url, &["default-src 'self'".into()])
        .unwrap();
    assert!(
        fallback_loader
            .load_local_file_image(&document_url, "image.jpg")
            .unwrap()
            .is_some()
    );
    assert!(
        fallback_loader
            .load_font(&document_url, "font.ttf", None)
            .unwrap()
            .is_some()
    );
    assert!(
        fallback_loader
            .load_local_file_media(&document_url, "media.wav")
            .unwrap()
            .is_some()
    );
    assert!(
        fallback_loader
            .load_local_file_image(&document_url, &other_image_url)
            .unwrap()
            .is_none()
    );
    assert!(
        fallback_loader
            .load_font(&document_url, &other_font_url, None)
            .unwrap()
            .is_none()
    );
    assert!(
        fallback_loader
            .load_local_file_media(&document_url, &other_media_url)
            .unwrap()
            .is_none()
    );

    let mut file_scheme_loader = NativeResourceLoader::new(&config).unwrap();
    file_scheme_loader
        .apply_meta_content_security_policies(
            &document_url,
            &["img-src file:; font-src file:; media-src file:".into()],
        )
        .unwrap();
    assert!(
        file_scheme_loader
            .load_local_file_image(&document_url, &other_image_url)
            .unwrap()
            .is_some()
    );
    assert!(
        file_scheme_loader
            .load_font(&document_url, &other_font_url, None)
            .unwrap()
            .is_some()
    );
    assert!(
        file_scheme_loader
            .load_local_file_media(&document_url, &other_media_url)
            .unwrap()
            .is_some()
    );

    let unconfigured_root = root.join("unconfigured");
    fs::create_dir_all(&unconfigured_root).unwrap();
    let unconfigured_image = unconfigured_root.join("image.jpg");
    fs::write(
        &unconfigured_image,
        include_bytes!("../../../tests/fixtures/remote-android-concept.jpg"),
    )
    .unwrap();
    let unconfigured_image_url = Url::from_file_path(unconfigured_image).unwrap().to_string();
    assert!(matches!(
        file_scheme_loader.load_local_file_image(&document_url, &unconfigured_image_url),
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rooted_file_frame_csp_uses_fallback_conjunction_and_configured_roots() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-rooted-file-frame-csp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let site = root.join("site");
    let other = root.join("other");
    let outside = root.join("outside");
    for directory in [&site, &other, &outside] {
        fs::create_dir_all(directory).unwrap();
        fs::write(directory.join("child.html"), "<title>child</title>").unwrap();
    }
    fs::write(site.join("index.html"), "<title>parent</title>").unwrap();
    let document_url = Url::from_file_path(site.join("index.html"))
        .unwrap()
        .to_string();
    let same_root_url = Url::from_file_path(site.join("child.html"))
        .unwrap()
        .to_string();
    let other_root_url = Url::from_file_path(other.join("child.html"))
        .unwrap()
        .to_string();
    let outside_root_url = Url::from_file_path(outside.join("child.html"))
        .unwrap()
        .to_string();
    let config = NativeEngineConfig::default()
        .with_allowed_file_root(&site)
        .with_allowed_file_root(&other);
    let mut loader = NativeResourceLoader::new(&config).unwrap();

    loader
        .apply_meta_content_security_policies(&document_url, &["frame-src 'self'".into()])
        .unwrap();
    assert!(
        loader
            .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
            .unwrap()
    );
    assert!(
        !loader
            .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
            .unwrap()
    );
    assert!(
        !loader
            .allows_rooted_file_frame_navigation(&document_url, &outside_root_url)
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(
            &document_url,
            &["child-src 'self'; default-src 'none'".into()],
        )
        .unwrap();
    assert!(
        loader
            .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
            .unwrap()
    );
    assert!(
        !loader
            .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(&document_url, &["default-src 'self'".into()])
        .unwrap();
    assert!(
        loader
            .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
            .unwrap()
    );
    assert!(
        !loader
            .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(
            &document_url,
            &["frame-src 'none'; child-src 'self'; default-src 'self'".into()],
        )
        .unwrap();
    assert!(
        !loader
            .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
            .unwrap()
    );
    assert!(
        loader
            .allows_rooted_file_frame_navigation(&document_url, "about:blank")
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(
            &document_url,
            &["frame-src 'self'".into(), "frame-src 'none'".into()],
        )
        .unwrap();
    assert!(
        !loader
            .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(&document_url, &["frame-src file:".into()])
        .unwrap();
    assert!(
        loader
            .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
            .unwrap()
    );
    assert!(
        !loader
            .allows_rooted_file_frame_navigation(&document_url, &outside_root_url)
            .unwrap()
    );
    assert!(
        loader
            .allows_rooted_file_frame_navigation(&document_url, "about:blank")
            .unwrap()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rooted_file_inline_style_csp_enforces_fallback_nonce_hash_and_conjunction() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-rooted-file-inline-style-csp-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    let document_path = site.join("index.html");
    fs::write(&document_path, "<!doctype html>").unwrap();
    let document_url = Url::from_file_path(&document_path).unwrap().to_string();
    let style_element = "#element { color: red; }";
    let style_attribute = "color: blue;";
    let style_element_hash =
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(style_element.as_bytes()));
    let style_attribute_hash = base64::engine::general_purpose::STANDARD
        .encode(Sha256::digest(style_attribute.as_bytes()));
    let config = NativeEngineConfig::default().with_allowed_file_root(&site);
    let mut loader = NativeResourceLoader::new(&config).unwrap();

    loader
            .apply_meta_content_security_policies(
                &document_url,
                &[format!(
                    "default-src 'none'; style-src 'self'; style-src-elem 'nonce-Style123' 'sha256-{style_element_hash}'; style-src-attr 'unsafe-hashes' 'sha256-{style_attribute_hash}'"
                )],
            )
            .unwrap();
    assert!(
        loader
            .allows_inline_style_element(
                &document_url,
                "#element { color: green; }",
                Some("Style123")
            )
            .unwrap()
    );
    assert!(
        loader
            .allows_inline_style_element(&document_url, style_element, None)
            .unwrap()
    );
    assert!(
        !loader
            .allows_inline_style_element(&document_url, "#element { color: red; } ", None)
            .unwrap()
    );
    assert!(
        loader
            .allows_inline_style_attribute(&document_url, style_attribute)
            .unwrap()
    );
    assert!(
        !loader
            .allows_inline_style_attribute(&document_url, "color: green;")
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(
            &document_url,
            &["style-src 'unsafe-inline'".to_owned()],
        )
        .unwrap();
    assert!(
        loader
            .allows_inline_style_element(&document_url, style_element, None)
            .unwrap()
    );
    assert!(
        loader
            .allows_inline_style_attribute(&document_url, style_attribute)
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(&document_url, &["default-src 'none'".to_owned()])
        .unwrap();
    assert!(
        !loader
            .allows_inline_style_element(&document_url, style_element, None)
            .unwrap()
    );
    assert!(
        !loader
            .allows_inline_style_attribute(&document_url, style_attribute)
            .unwrap()
    );

    loader
        .apply_meta_content_security_policies(
            &document_url,
            &[
                "style-src-elem 'unsafe-inline'; style-src-attr 'unsafe-inline'".to_owned(),
                "default-src 'none'".to_owned(),
            ],
        )
        .unwrap();
    assert!(
        !loader
            .allows_inline_style_element(&document_url, style_element, None)
            .unwrap()
    );
    assert!(
        !loader
            .allows_inline_style_attribute(&document_url, style_attribute)
            .unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn csp_strict_dynamic_inline_precedence_and_report_only_match_enforcement() {
    let source = "globalThis.boot = true;";
    let mut headers = HeaderMap::new();
    headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "script-src 'unsafe-inline' 'strict-dynamic' 'nonce-Boot123'; style-src 'unsafe-inline'",
            ),
        );
    headers.insert(
        HeaderName::from_static("content-security-policy-report-only"),
        HeaderValue::from_static("script-src 'strict-dynamic' https://listed.test"),
    );
    let policy = content_security_policy(&headers);
    assert!(!policy.allows_inline(NativeInlineCspKind::ScriptElement, source, None));
    assert!(policy.allows_inline(NativeInlineCspKind::ScriptElement, source, Some("Boot123")));
    assert!(policy.allows_inline(
        NativeInlineCspKind::StyleElement,
        "body { color: red; }",
        None
    ));

    let document = Url::parse("https://app.test/index.html").unwrap();
    let target = Url::parse("https://unlisted.test/app.js").unwrap();
    assert_eq!(
        policy
            .report_only_url_violations(
                NativeSubresourceKind::Script,
                &document,
                &target,
                false,
                None,
            )
            .len(),
        0
    );
    assert_eq!(
        policy
            .report_only_url_violations(
                NativeSubresourceKind::Script,
                &document,
                &target,
                true,
                None,
            )
            .len(),
        1
    );
}

#[test]
fn csp_header_policies_are_intersected_and_frame_groups_are_preserved() {
    let mut headers = HeaderMap::new();
    headers.append(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "script-src https://scripts-one.test; frame-src https://frame-one.test",
        ),
    );
    headers.append(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "script-src https://scripts-two.test; frame-src https://frame-two.test",
        ),
    );
    let policy = content_security_policy(&headers);
    let document = Url::parse("https://app.test/index.html").unwrap();
    let script_one = Url::parse("https://scripts-one.test/app.js").unwrap();
    let script_two = Url::parse("https://scripts-two.test/app.js").unwrap();
    let frame_one = Url::parse("https://frame-one.test/child").unwrap();
    let frame_two = Url::parse("https://frame-two.test/child").unwrap();
    assert!(!policy.allows(NativeSubresourceKind::Script, &document, &script_one));
    assert!(!policy.allows(NativeSubresourceKind::Script, &document, &script_two));
    assert_eq!(
        policy.frame_source_groups(),
        Some(vec![
            vec!["https://frame-one.test".into()],
            vec!["https://frame-two.test".into()],
        ])
    );
    assert!(
        !policy
            .frame_source_groups()
            .unwrap()
            .iter()
            .all(|sources| csp_sources_allow(Some(sources), &document, &frame_one))
    );
    assert!(
        !policy
            .frame_source_groups()
            .unwrap()
            .iter()
            .all(|sources| csp_sources_allow(Some(sources), &document, &frame_two))
    );
}

#[test]
fn service_worker_interception_uses_the_document_connect_policy_once() {
    let mut loader = NativeResourceLoader::for_content_process(
        NativeEngineConfig::default().limits.max_document_bytes,
        None,
        &[],
    )
    .unwrap();
    let document = Url::parse("http://app.test/page").unwrap();
    let target = Url::parse("http://app.test/api").unwrap();
    loader
        .set_document_content_security_policy_from_pairs(
            document.as_str(),
            &[(
                "Content-Security-Policy".into(),
                "connect-src 'none'".into(),
            )],
        )
        .unwrap();
    assert!(
        loader
            .enforce_service_worker_connect_policy(&document, &target)
            .is_err()
    );

    loader
        .set_document_content_security_policy_from_pairs(
            document.as_str(),
            &[
                (
                    "Content-Security-Policy".into(),
                    "connect-src 'self'".into(),
                ),
                (
                    "Content-Security-Policy-Report-Only".into(),
                    "connect-src 'none'".into(),
                ),
            ],
        )
        .unwrap();
    loader
        .enforce_service_worker_connect_policy(&document, &target)
        .unwrap();
    loader.report_service_worker_connect_policy(&document, &target);
    let reports = loader.take_csp_violations();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].effective_directive, "connect-src");
    assert_eq!(reports[0].blocked_uri, target.as_str());
}

#[test]
fn service_worker_font_interception_uses_the_document_font_policy() {
    let mut loader = NativeResourceLoader::for_content_process(
        NativeEngineConfig::default().limits.max_document_bytes,
        None,
        &[],
    )
    .unwrap();
    let document = Url::parse("http://app.test/page").unwrap();
    let target = Url::parse("http://app.test/font.ttf").unwrap();
    loader
        .set_document_content_security_policy_from_pairs(
            document.as_str(),
            &[("Content-Security-Policy".into(), "font-src 'none'".into())],
        )
        .unwrap();
    assert!(
        loader
            .enforce_service_worker_font_policy(&document, &target)
            .is_err()
    );

    loader
        .set_document_content_security_policy_from_pairs(
            document.as_str(),
            &[
                ("Content-Security-Policy".into(), "font-src 'self'".into()),
                (
                    "Content-Security-Policy-Report-Only".into(),
                    "font-src 'none'".into(),
                ),
            ],
        )
        .unwrap();
    loader
        .enforce_service_worker_font_policy(&document, &target)
        .unwrap();
    loader.report_service_worker_font_policy(&document, &target);
    let reports = loader.take_csp_violations();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].effective_directive, "font-src");
    assert_eq!(reports[0].blocked_uri, target.as_str());
}

#[tokio::test]
async fn network_blob_svg_image_decodes_when_img_src_allows_blob() {
    let document = Url::parse("http://app.test/page").unwrap();
    let mut loader = NativeResourceLoader::for_content_process(
        NativeEngineConfig::default().limits.max_document_bytes,
        None,
        &[],
    )
    .unwrap();
    loader
        .set_document_content_security_policy_from_pairs(
            document.as_str(),
            &[(
                "Content-Security-Policy".into(),
                "default-src 'none'; img-src blob:".into(),
            )],
        )
        .unwrap();
    let object_url = NativeObjectUrlResource {
        content_type: Some("image/svg+xml".into()),
        body: br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"/></svg>"#.to_vec(),
    };

    let image = loader
        .load_image_async_with_object_url_and_referrer_policy(
            document.as_str(),
            "blob:http://app.test/glass-native-1",
            Some(&object_url),
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
        )
        .await
        .unwrap()
        .expect("img-src blob should allow the Blob-backed SVG image");
    assert_eq!((image.width, image.height), (2, 2));
}

#[test]
fn csp_report_delivery_selects_endpoint_format_and_rejects_unsafe_targets() {
    let document = Url::parse("https://app.test/index.html").unwrap();
    let violation = NativeResourceLoader::csp_violation_record(
        &document,
        "https://api.test/data",
        "connect-src",
        "connect-src 'none'; report-uri /legacy; report-to modern",
        "",
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("content-security-policy-report-only"),
        HeaderValue::from_static("connect-src 'none'; report-uri /legacy; report-to modern"),
    );
    headers.insert(
        HeaderName::from_static("reporting-endpoints"),
        HeaderValue::from_static("modern=\"https://reports.test/csp\""),
    );
    let policy = content_security_policy(&headers);
    let declaration = &policy.report_only_policies[0];
    let deliveries = csp_report_deliveries_for_declaration(
        &policy.reporting_endpoints,
        declaration,
        &document,
        &violation,
    );
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].endpoint, "https://reports.test/csp");
    assert_eq!(deliveries[0].content_type, "application/reports+json");
    let payload: serde_json::Value = serde_json::from_slice(&deliveries[0].body).unwrap();
    assert_eq!(payload[0]["type"], "csp-violation");
    assert_eq!(payload[0]["body"]["blocked-uri"], "https://api.test/data");

    let mut legacy_headers = HeaderMap::new();
    legacy_headers.insert(
        HeaderName::from_static("content-security-policy-report-only"),
        HeaderValue::from_static("connect-src 'none'; report-uri /legacy"),
    );
    let legacy_policy = content_security_policy(&legacy_headers);
    let legacy_declaration = &legacy_policy.report_only_policies[0];
    let legacy_deliveries = csp_report_deliveries_for_declaration(
        &legacy_policy.reporting_endpoints,
        legacy_declaration,
        &document,
        &violation,
    );
    assert_eq!(legacy_deliveries.len(), 1);
    assert_eq!(legacy_deliveries[0].endpoint, "https://app.test/legacy");
    assert_eq!(legacy_deliveries[0].content_type, "application/csp-report");

    let mut unsafe_headers = HeaderMap::new();
    unsafe_headers.insert(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static(
                "connect-src 'none'; report-uri https://user:secret@reports.test/csp; report-to missing",
            ),
        );
    let unsafe_policy = content_security_policy(&unsafe_headers);
    let unsafe_declaration = &unsafe_policy.report_only_policies[0];
    assert!(
        csp_report_deliveries_for_declaration(
            &unsafe_policy.reporting_endpoints,
            unsafe_declaration,
            &document,
            &violation,
        )
        .is_empty()
    );

    let mut legacy_group_headers = HeaderMap::new();
    legacy_group_headers.insert(
        HeaderName::from_static("content-security-policy-report-only"),
        HeaderValue::from_static("connect-src 'none'; report-to modern"),
    );
    legacy_group_headers.insert(
            HeaderName::from_static("report-to"),
            HeaderValue::from_static(
                "[{\"group\":\"modern\",\"max_age\":60,\"endpoints\":[{\"url\":\"https://reports.test/legacy-group\"}]}]",
            ),
        );
    let legacy_group_policy = content_security_policy(&legacy_group_headers);
    let legacy_group_declaration = &legacy_group_policy.report_only_policies[0];
    let legacy_group_deliveries = csp_report_deliveries_for_declaration(
        &legacy_group_policy.reporting_endpoints,
        legacy_group_declaration,
        &document,
        &violation,
    );
    assert_eq!(legacy_group_deliveries.len(), 1);
    assert_eq!(
        legacy_group_deliveries[0].endpoint,
        "https://reports.test/legacy-group"
    );
}

#[test]
fn csp_meta_policies_replace_prior_document_meta_state() {
    let mut loader = NativeResourceLoader::for_content_process(
        NativeEngineConfig::default().limits.max_document_bytes,
        None,
        &[],
    )
    .unwrap();
    let document_url = "http://app.test/page";
    loader
        .set_document_content_security_policy_from_pairs(
            document_url,
            &[(
                "Content-Security-Policy".into(),
                "script-src 'unsafe-inline'".into(),
            )],
        )
        .unwrap();
    loader
        .apply_meta_content_security_policies(document_url, &["script-src 'nonce-first'".into()])
        .unwrap();
    let first = loader.inline_script_policy(document_url).unwrap();
    assert!(first.allows("globalThis.value = 1", Some("first")));
    assert!(!first.allows("globalThis.value = 1", None));

    loader
        .apply_meta_content_security_policies(document_url, &["script-src 'nonce-second'".into()])
        .unwrap();
    let second = loader.inline_script_policy(document_url).unwrap();
    assert!(!second.allows("globalThis.value = 1", Some("first")));
    assert!(second.allows("globalThis.value = 1", Some("second")));
}

#[test]
fn csp_meta_policies_append_without_relaxing_prior_state() {
    let mut loader = NativeResourceLoader::for_content_process(
        NativeEngineConfig::default().limits.max_document_bytes,
        None,
        &[],
    )
    .unwrap();
    let document_url = "http://app.test/page";
    loader
        .set_document_content_security_policy_from_pairs(
            document_url,
            &[(
                "Content-Security-Policy".into(),
                "script-src 'unsafe-inline'".into(),
            )],
        )
        .unwrap();
    loader
        .apply_meta_content_security_policies(document_url, &["script-src 'nonce-first'".into()])
        .unwrap();
    loader
        .append_meta_content_security_policies(document_url, &["script-src 'none'".into()])
        .unwrap();

    let policy = loader.inline_script_policy(document_url).unwrap();
    assert!(!policy.allows("globalThis.value = 1", Some("first")));
    assert!(!policy.allows("globalThis.value = 1", None));
}

#[test]
fn csp_inline_sources_preserve_nonce_hash_and_element_directive_semantics() {
    let script = "globalThis.cspInline = true;";
    let nonce_script = "globalThis.cspNonce = true;";
    let style_element = "body { color: red; }";
    let style_attribute = "color: blue";
    let script_attribute = "globalThis.cspAttribute = true;";
    let script_hash =
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(script.as_bytes()));
    let style_element_hash =
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(style_element.as_bytes()));
    let style_attribute_hash = base64::engine::general_purpose::STANDARD
        .encode(Sha256::digest(style_attribute.as_bytes()));
    let script_attribute_hash = base64::engine::general_purpose::STANDARD
        .encode(Sha256::digest(script_attribute.as_bytes()));
    let mut headers = HeaderMap::new();
    let policy_header = format!(
        "default-src 'none'; script-src 'nonce-Wrong'; script-src-elem 'nonce-AbC123' 'sha256-{script_hash}'; script-src-attr 'unsafe-hashes' 'sha256-{script_attribute_hash}'; style-src 'none'; style-src-elem 'nonce-Style123' 'sha256-{style_element_hash}'; style-src-attr 'unsafe-hashes' 'sha256-{style_attribute_hash}'"
    );
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&policy_header).unwrap(),
    );
    let policy = content_security_policy(&headers);

    assert!(policy.allows_inline(NativeInlineCspKind::ScriptElement, script, Some("AbC123"),));
    assert!(!policy.allows_inline(
        NativeInlineCspKind::ScriptElement,
        nonce_script,
        Some("abc123"),
    ));
    assert!(policy.allows_inline(NativeInlineCspKind::ScriptElement, script, None));
    assert!(policy.allows_inline(NativeInlineCspKind::ScriptAttribute, script_attribute, None,));
    assert!(!policy.allows_inline(
        NativeInlineCspKind::ScriptAttribute,
        "globalThis.cspAttribute = false;",
        None,
    ));
    assert!(policy.allows_inline(
        NativeInlineCspKind::StyleElement,
        style_element,
        Some("Style123"),
    ));
    assert!(policy.allows_inline(NativeInlineCspKind::StyleElement, style_element, None,));
    assert!(policy.allows_inline(NativeInlineCspKind::StyleAttribute, style_attribute, None,));
    assert!(!policy.allows_inline(NativeInlineCspKind::StyleAttribute, "color: green", None,));
}

#[test]
fn csp_frame_src_takes_precedence_over_child_src_and_default_src() {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; child-src https://child.test; frame-src https://frame.test",
        ),
    );
    let policy = content_security_policy(&headers);
    let document = Url::parse("https://app.test/index.html").unwrap();
    let frame = Url::parse("https://frame.test/child").unwrap();
    let child = Url::parse("https://child.test/child").unwrap();
    assert!(policy.allows(NativeSubresourceKind::Frame, &document, &frame));
    assert!(!policy.allows(NativeSubresourceKind::Frame, &document, &child));

    let mut child_only_headers = HeaderMap::new();
    child_only_headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; child-src https://child.test"),
    );
    let child_only_policy = content_security_policy(&child_only_headers);
    assert!(child_only_policy.allows(NativeSubresourceKind::Frame, &document, &child));
    assert!(!child_only_policy.allows(NativeSubresourceKind::Frame, &document, &frame));
}

#[test]
fn csp_worker_creation_uses_worker_src_fallback_chain() {
    let document = Url::parse("https://app.test/index.html").unwrap();
    let worker = Url::parse("https://app.test/worker.js").unwrap();
    let cross_origin_worker = Url::parse("https://worker.test/worker.js").unwrap();
    for (header, expected_directive) in [
        (
            "worker-src 'self'; child-src 'none'; script-src 'none'; default-src 'none'",
            "worker-src",
        ),
        (
            "child-src 'self'; script-src 'none'; default-src 'none'",
            "child-src",
        ),
        ("script-src 'self'; default-src 'none'", "script-src"),
        ("default-src 'self'", "default-src"),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(header).unwrap(),
        );
        let policy = content_security_policy(&headers);
        assert!(policy.allows(NativeSubresourceKind::Worker, &document, &worker));
        assert!(!policy.allows(
            NativeSubresourceKind::Worker,
            &document,
            &cross_origin_worker
        ));
        assert_eq!(
            policy.policies[0]
                .directive_for(NativeSubresourceKind::Worker)
                .map(|(name, _)| name),
            Some(expected_directive)
        );
    }
}

#[test]
fn csp_worker_imports_use_script_policy_and_report_only_does_not_block() {
    let document = Url::parse("https://app.test/worker.js").unwrap();
    let imported_script = Url::parse("https://app.test/imported.js").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("worker-src 'self'; script-src 'none'; default-src 'self'"),
    );
    let enforced = content_security_policy(&headers);
    assert!(enforced.allows(NativeSubresourceKind::Worker, &document, &imported_script));
    assert!(!enforced.allows(NativeSubresourceKind::Script, &document, &imported_script));
    assert_eq!(
        enforced.policies[0]
            .directive_for(NativeSubresourceKind::Script)
            .map(|(name, _)| name),
        Some("script-src")
    );

    let mut report_only_headers = HeaderMap::new();
    report_only_headers.insert(
        HeaderName::from_static("content-security-policy-report-only"),
        HeaderValue::from_static("script-src 'none'; worker-src 'self'"),
    );
    let report_only = content_security_policy(&report_only_headers);
    assert!(report_only.allows(NativeSubresourceKind::Script, &document, &imported_script));
    let violations = report_only.report_only_url_violations(
        NativeSubresourceKind::Script,
        &document,
        &imported_script,
        true,
        None,
    );
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].1, "script-src");
    assert!(
        report_only
            .report_only_url_violations(
                NativeSubresourceKind::Worker,
                &document,
                &imported_script,
                true,
                None,
            )
            .is_empty()
    );
}

#[test]
fn csp_form_action_is_explicit_and_intersected_across_headers() {
    let mut headers = HeaderMap::new();
    headers.append(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; form-action https://submit-one.test"),
    );
    headers.append(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("form-action https://submit-two.test"),
    );
    let policy = content_security_policy(&headers);
    let document = Url::parse("https://app.test/form").unwrap();
    let first = Url::parse("https://submit-one.test/receive").unwrap();
    let second = Url::parse("https://submit-two.test/receive").unwrap();
    assert!(!policy.allows_navigation(NativeNavigationPolicyKind::FormAction, &document, &first,));
    assert!(!policy.allows_navigation(NativeNavigationPolicyKind::FormAction, &document, &second,));

    let mut unrestricted_headers = HeaderMap::new();
    unrestricted_headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'"),
    );
    let unrestricted = content_security_policy(&unrestricted_headers);
    assert!(unrestricted.allows_navigation(
        NativeNavigationPolicyKind::FormAction,
        &document,
        &first,
    ));

    let mut navigation_headers = HeaderMap::new();
    navigation_headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src *; navigate-to https://destination.test"),
    );
    let navigation_policy = content_security_policy(&navigation_headers);
    let destination = Url::parse("https://destination.test/result").unwrap();
    assert!(navigation_policy.allows_navigation(
        NativeNavigationPolicyKind::NavigateTo,
        &document,
        &destination,
    ));
    assert!(!navigation_policy.allows_navigation(
        NativeNavigationPolicyKind::NavigateTo,
        &document,
        &first,
    ));
}

#[test]
fn subresource_resolution_and_mixed_content_fail_closed() {
    let document = Url::parse("https://app.test/path/index.html").unwrap();
    let same_origin = resolve_subresource_url(&document, "../styles/site.css")
        .unwrap()
        .unwrap();
    assert_eq!(same_origin.as_str(), "https://app.test/styles/site.css");
    let insecure = resolve_subresource_url(&document, "http://cdn.test/site.css")
        .unwrap()
        .unwrap();
    assert!(!mixed_content_allowed(&document, &insecure));
    assert!(
        resolve_subresource_url(&document, "data:text/css,body%7B%7D")
            .unwrap()
            .is_none()
    );
    assert!(resolve_subresource_url(&document, "https://user:pass@cdn.test/site.css").is_err());
}

#[test]
fn cors_response_requires_explicit_cross_origin_authorization() {
    let document = Url::parse("https://app.test/index.html").unwrap();
    let resource = Url::parse("https://api.test/data.json").unwrap();
    assert_eq!(
        cors_origin_header(&document, &resource, NativeCorsMode::Cors),
        Some("https://app.test".into())
    );
    assert_eq!(
        cors_origin_header(&document, &resource, NativeCorsMode::NoCors),
        None
    );

    let mut headers = HeaderMap::new();
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    assert!(cors_response_allowed(&headers, &document, &resource, false));
    assert!(!cors_response_allowed(&headers, &document, &resource, true));

    headers.insert(
        ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("https://app.test"),
    );
    assert!(!cors_response_allowed(&headers, &document, &resource, true));
    headers.insert(
        ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    assert!(cors_response_allowed(&headers, &document, &resource, true));
    assert!(cors_response_allowed(
        &HeaderMap::new(),
        &document,
        &document,
        true
    ));
}

#[test]
fn cors_preflight_requires_allowed_method_and_requested_header() {
    let document = Url::parse("https://app.test/index.html").unwrap();
    let resource = Url::parse("https://api.test/data.json").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("https://app.test"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    assert!(cors_preflight_response_allowed(
        &headers,
        &document,
        &resource,
        "POST",
        &["content-type".into()],
        false,
    ));
    headers.remove(ACCESS_CONTROL_ALLOW_HEADERS);
    assert!(!cors_preflight_response_allowed(
        &headers,
        &document,
        &resource,
        "POST",
        &["content-type".into()],
        false,
    ));
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET"),
    );
    assert!(!cors_preflight_response_allowed(
        &headers,
        &document,
        &resource,
        "POST",
        &["content-type".into()],
        false,
    ));
}
