use super::*;

#[test]
fn stylesheet_link_referrer_policy_uses_element_or_document_default() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse_with_generation_and_referrer_policy(
        "<link id='default' rel='stylesheet' href='/default.css'>\
             <link id='origin' rel='stylesheet' href='/origin.css' referrerpolicy='ORIGIN'>\
             <link id='invalid' rel='stylesheet' href='/invalid.css' referrerpolicy='invalid'>",
        &limits,
        1,
        NativeFetchReferrerPolicy::UnsafeUrl,
    )
    .unwrap();
    let policy_for = |id| {
        document.stylesheet_link_referrer_policy_for_node_index(
            document.find_element_by_id(id).unwrap().index,
        )
    };
    assert_eq!(policy_for("default"), NativeFetchReferrerPolicy::UnsafeUrl);
    assert_eq!(policy_for("origin"), NativeFetchReferrerPolicy::Origin);
    assert_eq!(policy_for("invalid"), NativeFetchReferrerPolicy::UnsafeUrl);

    let live_meta_default = NativeDocument::parse_with_generation_and_referrer_policy(
        "<meta name='referrer' content='no-referrer'>\
             <link id='default' rel='stylesheet' href='/default.css'>",
        &limits,
        1,
        NativeFetchReferrerPolicy::UnsafeUrl,
    )
    .unwrap();
    let default_link = live_meta_default.find_element_by_id("default").unwrap();
    assert_eq!(
        live_meta_default.stylesheet_link_referrer_policy_for_node_index(default_link.index),
        NativeFetchReferrerPolicy::NoReferrer
    );
}

#[test]
fn external_image_preload_discovery_requires_supported_link_attributes() {
    let document = NativeDocument::parse(
        "<head>\
             <link rel='stylesheet PRELOAD' as='IMAGE' href='/supported.png'>\
             <link rel='preload' as='image' href='/media.png' media='screen'>\
             <link rel='preload' as='image' href='/type.png' type='image/png'>\
             <link rel='preload' as='image' href='/cors.png' crossorigin='anonymous'>\
             <link rel='preload' as='image' href='/integrity.png' integrity='sha256-x'>\
             <link rel='preload' as='image' href='/responsive.png' imagesrcset='/2x.png 2x'>\
             <link rel='preload' as='image' href='/sizes.png' imagesizes='100vw'>\
             <link rel='preload' as='style' href='/style.css'>\
             <link rel='preload' as='image' href=''>\
             </head>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    assert_eq!(
        document
            .external_image_preload_links()
            .into_iter()
            .map(|(_, href)| href)
            .collect::<Vec<_>>(),
        vec!["/supported.png".to_owned()]
    );
}

#[test]
fn document_preload_image_cache_enforces_and_releases_decoded_byte_budget() {
    let mut document =
        NativeDocument::parse("<p>page</p>", &NativeEngineLimits::default()).unwrap();
    let image = || NativeImage::new(256, 256, vec![0; 256 * 256 * 4]);

    for index in 0..8 {
        assert!(
            document
                .cache_preloaded_image_resource(
                    format!("https://example.test/{index}.png"),
                    image(),
                )
                .unwrap()
        );
    }
    assert!(
        !document
            .cache_preloaded_image_resource("https://example.test/overflow.png".into(), image())
            .unwrap()
    );
    assert!(
        document
            .take_preloaded_image_resource("https://example.test/0.png")
            .is_some()
    );
    assert!(
        document
            .cache_preloaded_image_resource("https://example.test/overflow.png".into(), image())
            .unwrap()
    );
}

#[test]
fn module_script_sources_capture_element_or_live_document_referrer_policy() {
    let document = NativeDocument::parse_with_generation_and_referrer_policy(
            "<script id='default' type='module'>import '/default.js';</script>\
             <script id='origin' type='module' referrerpolicy='ORIGIN'>import '/origin.js';</script>\
             <script id='invalid' type='module' referrerpolicy='invalid'>import '/invalid.js';</script>\
             <script id='external' type='module' src='/entry.js' referrerpolicy='no-referrer'></script>",
            &NativeEngineLimits::default(),
            1,
            NativeFetchReferrerPolicy::UnsafeUrl,
        )
        .unwrap();

    let sources = document.page_script_sources(8, 4096);
    let policies = sources
        .iter()
        .map(|source| match source {
            NativePageScriptSource::ModuleInline {
                source,
                referrer_policy,
                ..
            } => (source.clone(), *referrer_policy),
            NativePageScriptSource::ModuleExternal {
                href,
                referrer_policy,
                ..
            } => (href.clone(), *referrer_policy),
            _ => panic!("expected only module script sources"),
        })
        .collect::<Vec<_>>();

    assert_eq!(
        policies,
        [
            (
                "import '/default.js';".into(),
                NativeFetchReferrerPolicy::UnsafeUrl
            ),
            (
                "import '/origin.js';".into(),
                NativeFetchReferrerPolicy::Origin
            ),
            (
                "import '/invalid.js';".into(),
                NativeFetchReferrerPolicy::UnsafeUrl
            ),
            ("/entry.js".into(), NativeFetchReferrerPolicy::NoReferrer),
        ]
    );
}
