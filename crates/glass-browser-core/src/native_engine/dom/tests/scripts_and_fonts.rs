use super::*;

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
fn javascript_dynamic_inline_script_insertion_runs_once() {
    let mut document = NativeDocument::parse(
        "<main id='root'><span id='anchor'></span></main>",
        &NativeEngineLimits::default(),
    )
    .expect("dynamic script document must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("dynamic-script-insertion")
        .expect("native JavaScript runtime must construct");
    let evaluation = runtime
        .evaluate(
            r##"(() => {
                    globalThis.__dynamicScriptRuns = 0;
                    const root = document.querySelector("#root");
                    const first = document.createElement("script");
                    first.id = "first-script";
                    first.textContent = "globalThis.__dynamicScriptRuns += 1";
                    root.appendChild(first);
                    const afterAppend = globalThis.__dynamicScriptRuns;
                    first.remove();
                    root.appendChild(first);
                    const afterReinsert = globalThis.__dynamicScriptRuns;
                    const second = document.createElement("script");
                    second.id = "second-script";
                    second.textContent = "globalThis.__dynamicScriptRuns += 10";
                    const anchor = document.querySelector("#anchor");
                    root.insertBefore(second, anchor);
                    return [
                        afterAppend,
                        afterReinsert,
                        globalThis.__dynamicScriptRuns,
                        first.parentElement === root,
                        second.parentElement === root,
                        anchor.previousSibling === second,
                    ];
                })()"##,
            &document,
            "fixture://dynamic-script-insertion.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("connected dynamic inline script insertion must return normally");
    assert_eq!(
        evaluation.value,
        serde_json::json!([1, 1, 11, true, true, true])
    );
    assert_eq!(
        evaluation
            .commands
            .iter()
            .filter(|command| matches!(command, NativeScriptCommand::StartScript { .. }))
            .count(),
        2,
        "each newly inserted inline script gets one start marker"
    );

    document
        .apply_script_commands(&evaluation.commands)
        .expect("dynamic script DOM commands must commit");
    let root = document.find_element_by_id("root").unwrap();
    let child_ids = document
        .node(root)
        .unwrap()
        .children()
        .iter()
        .map(|child| {
            document
                .node(*child)
                .unwrap()
                .attribute("id")
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(child_ids, ["second-script", "anchor", "first-script"]);
}

#[test]
fn javascript_dynamic_inline_scripts_match_mime_type_essences() {
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("dynamic script MIME document must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("dynamic-script-mime")
        .expect("native JavaScript runtime must construct");
    let script_types_json = serde_json::to_string(JAVASCRIPT_MIME_TYPE_ESSENCES)
        .expect("JavaScript MIME essence list must serialize");
    let source = r##"(() => {
                    globalThis.__dynamicScriptMimeRuns = 0;
                    const root = document.querySelector("#root");
                    const types = __JAVASCRIPT_MIME_TYPES__;
                    for (const [index, type] of types.entries()) {
                        const script = document.createElement("script");
                        script.id = `accepted-${index}`;
                        script.setAttribute("type", type);
                        script.textContent = "globalThis.__dynamicScriptMimeRuns += 1";
                        root.appendChild(script);
                    }
                    const caseVariant = document.createElement("script");
                    caseVariant.type = "TEXT/JAVASCRIPT";
                    caseVariant.textContent = "globalThis.__dynamicScriptMimeRuns += 1";
                    root.appendChild(caseVariant);
                    const parameterized = document.createElement("script");
                    parameterized.type = "text/javascript; charset=utf-8";
                    parameterized.textContent = "globalThis.__dynamicScriptMimeRuns += 100";
                    root.appendChild(parameterized);
                    const unicodeCase = document.createElement("script");
                    unicodeCase.type = "text/javaſcript";
                    unicodeCase.textContent = "globalThis.__dynamicScriptMimeRuns += 100";
                    root.appendChild(unicodeCase);
                    const leadingWhitespace = document.createElement("script");
                    leadingWhitespace.type = " text/javascript";
                    leadingWhitespace.textContent = "globalThis.__dynamicScriptMimeRuns += 100";
                    root.appendChild(leadingWhitespace);
                    const trailingWhitespace = document.createElement("script");
                    trailingWhitespace.type = "text/javascript ";
                    trailingWhitespace.textContent = "globalThis.__dynamicScriptMimeRuns += 100";
                    root.appendChild(trailingWhitespace);
                    const data = document.createElement("script");
                    data.type = "application/json";
                    data.textContent = "globalThis.__dynamicScriptMimeRuns += 100";
                    root.appendChild(data);
                    return globalThis.__dynamicScriptMimeRuns;
                })()"##
        .replace("__JAVASCRIPT_MIME_TYPES__", &script_types_json);
    let evaluation = runtime
        .evaluate(
            &source,
            &document,
            "fixture://dynamic-script-mime.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("dynamic inline script MIME policy must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(JAVASCRIPT_MIME_TYPE_ESSENCES.len() + 1)
    );
    assert_eq!(
        evaluation
            .commands
            .iter()
            .filter(|command| matches!(command, NativeScriptCommand::StartScript { .. }))
            .count(),
        JAVASCRIPT_MIME_TYPE_ESSENCES.len() + 1,
        "all JavaScript MIME essences and a case variant start"
    );
    document
        .apply_script_commands(&evaluation.commands)
        .expect("dynamic MIME script DOM commands must commit");
}

#[test]
fn dynamic_import_map_sources_follow_attachment_order_and_start_once() {
    let mut document =
        NativeDocument::parse("<main id='root'></main>", &NativeEngineLimits::default())
            .expect("dynamic import-map document must parse");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("dynamic-import-map-order")
        .expect("native JavaScript runtime must construct");
    let prepared = runtime
        .evaluate(
            r#"(() => {
                    const module = document.createElement('script');
                    module.type = 'module';
                    module.referrerPolicy = 'origin';
                    module.textContent = "import 'pkg';";
                    const map = document.createElement('script');
                    map.type = 'importmap';
                    map.textContent = '{"imports":{"pkg":"/mapped.js"}}';
                    globalThis.dynamicMap = map;
                    globalThis.dynamicModule = module;
                    return true;
                })()"#,
            &document,
            "https://example.test/",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("detached dynamic scripts must be created");
    document
        .apply_script_commands(&prepared.commands)
        .expect("detached dynamic script nodes must commit");
    assert!(
        document
            .take_newly_attached_page_script_sources(&prepared.commands, 8, 4096)
            .is_empty()
    );

    let attached = runtime
            .evaluate(
                "document.querySelector('#root').appendChild(dynamicMap); document.querySelector('#root').appendChild(dynamicModule); true",
                &document,
                "https://example.test/",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("dynamic scripts must attach");
    document
        .apply_script_commands(&attached.commands)
        .expect("dynamic script attachment must commit");
    let sources = document.take_newly_attached_page_script_sources(&attached.commands, 8, 4096);
    assert!(
        sources[0].node_index() > sources[1].node_index(),
        "the module was created before the map but attached after it"
    );
    assert!(matches!(
        sources.as_slice(),
        [
            NativePageScriptSource::ImportMap {
                source: Some(map_source),
                ..
            },
            NativePageScriptSource::ModuleInline {
                source,
                referrer_policy: NativeFetchReferrerPolicy::Origin,
                ..
            },
        ] if map_source.contains("\"pkg\":\"/mapped.js\"")
            && source.contains("import 'pkg'")
    ));
    assert!(
        document
            .take_newly_attached_page_script_sources(&attached.commands, 8, 4096)
            .is_empty()
    );
}

#[test]
fn parsed_inline_scripts_match_mime_type_essences_without_parameters() {
    let mime_types = JAVASCRIPT_MIME_TYPE_ESSENCES;
    let mut markup = String::from(
        "<html><head><script>window.__parsedMime0 = true</script><script type=''>window.__parsedMime1 = true</script>",
    );
    for (index, mime_type) in mime_types.iter().enumerate() {
        markup.push_str(&format!(
            "<script type=\"{mime_type}\">window.__parsedMime{index}_accepted = true</script>"
        ));
    }
    markup.push_str(
            "<script type='TEXT/JAVASCRIPT'>window.__parsedMimeCase = true</script><script type='text/javascript; charset=utf-8'>window.__parsedMimeParameterized = true</script><script type='text/javaſcript'>window.__parsedMimeUnicode = true</script><script type=' text/javascript'>window.__parsedMimeLeadingSpace = true</script><script type='text/javascript '>window.__parsedMimeTrailingSpace = true</script>",
        );
    markup.push_str(
            "<script type='application/json'>window.__parsedMimeJson = true</script></head><body></body></html>",
        );

    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default())
        .expect("script MIME document must parse");
    let sources = document.page_script_sources(32, 4096);
    assert_eq!(sources.len(), mime_types.len() + 3);
    assert!(
        sources
            .iter()
            .all(|source| matches!(source, NativePageScriptSource::Inline { .. }))
    );
    assert!(sources.iter().any(|source| matches!(
        source,
        NativePageScriptSource::Inline { source, .. }
            if source.contains("__parsedMime15_accepted")
    )));
    assert!(!sources.iter().any(|source| matches!(
        source,
        NativePageScriptSource::Inline { source, .. }
            if source.contains("__parsedMimeParameterized")
                || source.contains("__parsedMimeUnicode")
                || source.contains("__parsedMimeLeadingSpace")
                || source.contains("__parsedMimeTrailingSpace")
                || source.contains("__parsedMimeJson")
    )));
}

#[test]
fn content_wire_round_trips_user_interacted_state() {
    let limits = NativeEngineLimits::default();
    let mut document =
        NativeDocument::parse("<form><input id='required' required></form>", &limits).unwrap();
    let required = document.resolve_target("id=required").unwrap();
    document.apply_click(required).unwrap();
    let wire = document.to_content_wire();
    let encoded = serde_json::to_vec(&wire).unwrap();
    let decoded: NativeDocumentWire = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, wire);
    assert!(
        serde_json::to_value(NativeComputedStyle::default())
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty(),
        "default computed styles should omit all default fields"
    );
    let restored =
        NativeDocument::from_content_wire(decoded, &limits, document.generation()).unwrap();
    assert_eq!(restored.css_user_validity(required), Some(false));
}

#[test]
fn content_wire_omits_default_node_state_and_empty_children() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse("<main><p>hello <span>world</span></p></main>", &limits)
        .expect("static content wire fixture must parse");
    let wire = document.to_content_wire();
    let encoded = serde_json::to_value(&wire).expect("content wire must serialize");
    let nodes = encoded["nodes"]
        .as_array()
        .expect("content wire nodes must be an array");
    assert!(
        nodes.iter().any(|node| node.get("state").is_none()),
        "default per-node state should be omitted"
    );
    assert!(
        nodes.iter().any(|node| node.get("children").is_none()),
        "empty leaf child lists should be omitted"
    );

    let decoded: NativeDocumentWire =
        serde_json::from_value(encoded).expect("omitted defaults must deserialize");
    assert_eq!(decoded, wire);
    let restored = NativeDocument::from_content_wire(decoded, &limits, document.generation())
        .expect("omitted defaults must preserve the document");
    assert_eq!(restored.visible_text(1024), document.visible_text(1024));
    assert_eq!(
        restored.resolve_target("css=span").unwrap().index(),
        document.resolve_target("css=span").unwrap().index()
    );
}

#[test]
fn content_wire_round_trips_background_images_resolved_from_custom_properties() {
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse(
        "<style>body { --hero: url('https://example.test/hero.png'); background-image: var(--hero); }</style><p>hero</p>",
        &limits,
    )
    .unwrap();

    let wire = document.to_content_wire();
    let restored = NativeDocument::from_content_wire(wire, &limits, document.generation()).unwrap();
    let body = restored.resolve_target("css=body").unwrap();
    let source_id = restored.computed_style_for_layout(body).background_image();
    assert!(source_id.is_some());
    assert!(
        restored
            .background_image_sources
            .contains_key(&source_id.unwrap())
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
            family_key: super::super::super::css::font_family_hash("Embedded Sans"),
            weight: super::super::super::css::NativeFontWeightRange { min: 300, max: 700 },
            style: FontStyleValue::Normal,
            stretch: super::super::super::css::NativeFontStretchRange::default(),
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
        super::super::super::css::FontWeightValue::Numeric(500)
    );
    assert_eq!(
        wire.font_resources[0].weight_range,
        Some(super::super::super::css::NativeFontWeightRange { min: 300, max: 700 })
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
        NativeDocument::from_content_wire(wire.clone(), &limits, document.generation()).unwrap();
    assert_eq!(restored.font_resources.len(), 1);
    assert_eq!(restored.font_resources[0].family, "Embedded Sans");
    assert_eq!(
        restored.font_resources[0].weight,
        super::super::super::css::NativeFontWeightRange { min: 300, max: 700 }
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
        super::super::super::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST
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
        super::super::super::css::NativeFontWeightRange::singleton(
            super::super::super::css::FontWeightValue::Numeric(500)
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
