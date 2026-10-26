use super::*;

const NESTED_STYLES: &str = "<!doctype html><style>html{font-size:20px;color:#123456} body{font-size:1.5rem} main{color:#345678} section{font-size:2em} p{margin:1rem}</style><main><section><p>nested</p><p>second</p></section></main>";

#[test]
fn layout_style_cache_reuses_cascade_and_matches_fresh_document() {
    let limits = NativeEngineLimits::default();
    let cached = NativeDocument::parse(NESTED_STYLES, &limits).unwrap();
    let fresh = NativeDocument::parse(NESTED_STYLES, &limits).unwrap();
    let body = cached.find_element(cached.root(), "body").unwrap();
    let main = cached.find_element(cached.root(), "main").unwrap();
    let section = cached.find_element(cached.root(), "section").unwrap();
    let paragraphs: Vec<_> = cached
        .nodes
        .iter()
        .filter(|node| node.element_name() == Some("p"))
        .map(NativeNode::id)
        .collect();

    // Query out of tree order: each ancestor gets memoized on the first
    // descendant walk and subsequent requests reuse the exact computed value.
    let queried = [paragraphs[1], body, section, main, paragraphs[0]];
    let styles = queried.map(|id| cached.computed_style_for_layout(id));
    let cached_count = cached.layout_style_cache.0.lock().unwrap().len();
    assert!(cached_count >= 6);
    for (id, style) in queried.into_iter().zip(styles) {
        assert_eq!(style, fresh.computed_style_for_layout(id));
    }
    assert_eq!(
        cached.layout_style_cache.0.lock().unwrap().len(),
        cached_count
    );
}

#[test]
fn cached_parent_style_fast_path_matches_fallback_on_mixed_large_document() {
    use std::time::Instant;

    let mut source = String::from(
        "<!doctype html><style>html{font-size:18px;color:#123456;line-height:1.4} body{--accent:#345678;font-family:Arial} main{font-size:1.25em} article.entry{margin:2px;padding:1px} article > a{color:var(--accent)} .detail{font-size:1.1em;white-space:normal} .muted{color:#777}</style><main><section>",
    );
    for index in 0..1_000 {
        source.push_str(&format!(
            "<article class=entry><a href='/item/{index}'>item {index}</a><p><span class=detail>details {index}</span><span class='detail muted'>more {index}</span></p></article>"
        ));
    }
    source.push_str("</section></main>");

    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(&source, &limits).unwrap();
    let target = document
        .nodes
        .iter()
        .rfind(|node| node.element_name() == Some("span"))
        .unwrap()
        .id();

    // First capture the legacy full-chain result, then invalidate derived
    // state and prime only the target's ancestors. The second lookup must use
    // its cached parent and produce exactly the same complete computed style.
    let fallback = document.computed_style_for_layout(target);
    document.invalidate_layout_styles();
    let mut ancestors = Vec::new();
    let mut current = document.node(target).and_then(NativeNode::parent);
    while let Some(id) = current {
        ancestors.push(id);
        current = document.node(id).and_then(NativeNode::parent);
    }
    for ancestor in ancestors.into_iter().rev() {
        document.computed_style_for_layout(ancestor);
    }
    let fast = document.computed_style_for_layout(target);
    assert_eq!(fast, fallback);

    document.invalidate_layout_styles();
    let started = Instant::now();
    let mut fast_styles = Vec::with_capacity(document.nodes.len());
    for node in &document.nodes {
        fast_styles.push(document.computed_style_for_layout(node.id()));
    }
    let fast_elapsed = started.elapsed();
    assert!(document.layout_style_cache.0.lock().unwrap().len() >= 4_000);

    let fallback_document = NativeDocument::parse(&source, &limits).unwrap();
    let fallback_started = Instant::now();
    let fallback_styles = fallback_document
        .nodes
        .iter()
        .map(|node| fallback_document.computed_style_for_layout_without_parent_cache(node.id()))
        .collect::<Vec<_>>();
    let fallback_elapsed = fallback_started.elapsed();
    assert_eq!(fast_styles, fallback_styles);
    eprintln!(
        "large mixed native DOM preorder styles: {} nodes, cached-parent={} ms, full-chain={} ms",
        document.nodes.len(),
        fast_elapsed.as_millis(),
        fallback_elapsed.as_millis()
    );
}

#[test]
fn clear_file_input_invalidates_cached_validity_styles() {
    let limits = NativeEngineLimits::default();
    let source = "<style>input:valid{color:#123456} input:invalid{color:#abcdef}</style><input id=upload type=file required>";
    let mut cached = NativeDocument::parse(source, &limits).unwrap();
    let upload = cached.find_element_by_id("upload").unwrap();
    {
        let node = cached.node_mut(upload).unwrap();
        node.state.value = Some(r"C:\fakepath\a.txt".into());
        node.state.files.push(NativeFile {
            name: "a.txt".into(),
            media_type: "text/plain".into(),
            last_modified: 0,
            bytes: vec![1],
        });
    }
    let valid_style = cached.computed_style_for_layout(upload);
    assert_eq!(
        valid_style.color(),
        Some(NativeColor {
            red: 0x12,
            green: 0x34,
            blue: 0x56,
            alpha: u8::MAX,
        })
    );

    cached
        .apply_script_commands(&[NativeScriptCommand::ClearFileInput {
            node_index: upload.index(),
        }])
        .unwrap();
    let cleared_style = cached.computed_style_for_layout(upload);
    let fresh = NativeDocument::parse(source, &limits).unwrap();
    let fresh_upload = fresh.find_element_by_id("upload").unwrap();
    assert_eq!(cleared_style, fresh.computed_style_for_layout(fresh_upload));
    assert_eq!(
        cleared_style.color(),
        Some(NativeColor {
            red: 0xab,
            green: 0xcd,
            blue: 0xef,
            alpha: u8::MAX,
        })
    );
}

#[test]
fn viewport_invalidation_clears_cache_and_clones_are_isolated() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(NESTED_STYLES, &limits).unwrap();
    let p = document.find_element(document.root(), "p").unwrap();
    let initial = document.computed_style_for_layout(p);
    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());
    assert!(!document.layout_style_cache.1.lock().unwrap().is_empty());

    let clone = document.clone();
    assert_eq!(clone.computed_style_for_layout(p), initial);
    assert!(!clone.layout_style_cache.1.lock().unwrap().is_empty());
    document
        .set_viewport(Viewport {
            width: 640,
            height: 480,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(document.layout_style_cache.0.lock().unwrap().is_empty());
    assert!(document.layout_style_cache.1.lock().unwrap().is_empty());
    assert!(!clone.layout_style_cache.0.lock().unwrap().is_empty());
    assert!(!clone.layout_style_cache.1.lock().unwrap().is_empty());

    let updated = document.computed_style_for_layout(p);
    let fresh = NativeDocument::parse(NESTED_STYLES, &limits).unwrap();
    assert_eq!(updated, fresh.computed_style_for_layout(p));
}

#[test]
fn layout_snapshot_cache_matches_fresh_layout_and_invalidates_style_and_viewport_changes() {
    let limits = NativeEngineLimits::default();
    let source =
        "<style>.active{width:240px} p{height:32px}</style><main><p id=target>text</p></main>";
    let mut document = NativeDocument::parse(source, &limits).unwrap();
    let target = document.find_element_by_id("target").unwrap();
    let viewport = Viewport::default();

    let initial = document.layout(viewport).unwrap();
    assert_eq!(document.layout(viewport).unwrap(), initial);
    assert!(document.layout_style_cache.2.lock().unwrap().is_some());
    let clone = document.clone();
    assert_eq!(clone.layout(viewport).unwrap(), initial);

    document
        .apply_script_attribute(target.index(), "class", "active", None, &BTreeMap::new())
        .unwrap();
    assert!(document.layout_style_cache.2.lock().unwrap().is_none());
    assert!(clone.layout_style_cache.2.lock().unwrap().is_some());
    let styled = document.layout(viewport).unwrap();
    let fresh_styled = NativeDocument::parse(
        "<style>.active{width:240px} p{height:32px}</style><main><p id=target class=active>text</p></main>",
        &limits,
    )
    .unwrap();
    assert_eq!(styled, fresh_styled.layout(viewport).unwrap());
    assert_ne!(initial, styled);

    let next_viewport = Viewport {
        width: 640,
        height: 480,
        device_scale_factor_milli: 1000,
    };
    document.set_viewport(next_viewport).unwrap();
    assert!(document.layout_style_cache.2.lock().unwrap().is_none());
    let resized = document.layout(next_viewport).unwrap();
    let mut fresh_resized = fresh_styled;
    fresh_resized.set_viewport(next_viewport).unwrap();
    assert_eq!(resized, fresh_resized.layout(next_viewport).unwrap());
}

#[test]
fn setting_an_attribute_to_its_existing_value_keeps_valid_layout_caches() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<style>.active{width:240px}</style><main><p id=target class=active>text</p></main>",
        &limits,
    )
    .unwrap();
    let target = document.find_element_by_id("target").unwrap();
    let viewport = Viewport::default();
    let initial = document.layout(viewport).unwrap();
    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());
    assert!(document.layout_style_cache.2.lock().unwrap().is_some());

    document
        .apply_script_attribute(target.index(), "class", "active", None, &BTreeMap::new())
        .unwrap();

    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());
    assert!(document.layout_style_cache.2.lock().unwrap().is_some());
    assert_eq!(document.layout(viewport).unwrap(), initial);
}

#[test]
fn script_snapshot_cache_reuses_and_invalidates_complete_projection() {
    let limits = NativeEngineLimits::default();
    let source = "<input id='field' value='initial'>";
    let mut document = NativeDocument::parse(source, &limits).unwrap();
    let viewport = Viewport::default();
    let first = document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport);
    assert!(document.layout_style_cache.4.lock().unwrap().is_some());
    assert_eq!(
        serde_json::to_value(
            document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport)
        )
        .unwrap(),
        serde_json::to_value(first).unwrap()
    );

    let field = document.find_element_by_id("field").unwrap();
    document
        .apply_script_attribute(field.index(), "data-state", "ready", None, &BTreeMap::new())
        .unwrap();
    document.set_revision(document.revision() + 1);
    assert!(document.layout_style_cache.4.lock().unwrap().is_none());
    let refreshed = document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport);
    assert_eq!(
        refreshed
            .elements
            .iter()
            .find(|element| element.node_index == field.index())
            .and_then(|element| element.attributes.get("data-state"))
            .map(String::as_str),
        Some("ready")
    );

    let mut fresh = NativeDocument::parse(
        "<input id='field' value='initial' data-state='ready'>",
        &limits,
    )
    .unwrap();
    fresh.set_revision(document.revision());
    assert_eq!(
        serde_json::to_value(refreshed).unwrap(),
        serde_json::to_value(
            fresh.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport)
        )
        .unwrap()
    );
}

#[test]
fn scrolled_script_projection_cache_matches_fresh_layouts_and_invalidates() {
    let limits = NativeEngineLimits::default();
    let source = "<style>html,body{margin:0}body{height:2400px}#scroller{width:120px;height:80px;overflow:auto}#inner{width:300px;height:500px}</style><main><div id='scroller'><div id='inner'>nested scroll content</div></div></main>";
    let viewport = Viewport::default();
    let mut cached = NativeDocument::parse(source, &limits).unwrap();
    let base_layout = cached.layout(viewport).unwrap();
    let root_max = base_layout.max_scroll_offset();
    assert!(root_max.y > 0);
    let root_offset = NativePoint {
        x: root_max.x.min(5),
        y: root_max.y.min(13),
    };
    let scroller = cached.find_element_by_id("scroller").unwrap();
    let metrics = base_layout.scroll_container_for(scroller).unwrap();
    let nested_scroll = BTreeMap::from([(
        scroller.index(),
        NativePoint {
            x: metrics
                .scroll_width
                .saturating_sub(metrics.client_width)
                .min(17),
            y: metrics
                .scroll_height
                .saturating_sub(metrics.client_height)
                .min(19),
        },
    )]);
    assert!(
        nested_scroll
            .values()
            .any(|point| point.x > 0 || point.y > 0)
    );

    let first = cached
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            root_offset,
            &nested_scroll,
        )
        .unwrap();
    assert!(cached.layout_style_cache.5.lock().unwrap().is_some());
    let repeated = cached
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            root_offset,
            &nested_scroll,
        )
        .unwrap();
    let mut fresh = NativeDocument::parse(source, &limits).unwrap();
    let fresh_scroller = fresh.find_element_by_id("scroller").unwrap();
    let fresh_metrics = fresh
        .layout(viewport)
        .unwrap()
        .scroll_container_for(fresh_scroller)
        .unwrap();
    let fresh_nested_scroll = BTreeMap::from([(
        fresh_scroller.index(),
        NativePoint {
            x: fresh_metrics
                .scroll_width
                .saturating_sub(fresh_metrics.client_width)
                .min(17),
            y: fresh_metrics
                .scroll_height
                .saturating_sub(fresh_metrics.client_height)
                .min(19),
        },
    )]);
    let fresh_snapshot = fresh
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            root_offset,
            &fresh_nested_scroll,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&repeated).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&fresh_snapshot).unwrap()
    );

    let changed_root_offset = NativePoint {
        x: root_max.x.min(2),
        y: root_max.y.min(7),
    };
    let changed_nested_scroll = BTreeMap::from([(scroller.index(), NativePoint { x: 1, y: 1 })]);
    let changed = cached
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            changed_root_offset,
            &changed_nested_scroll,
        )
        .unwrap();
    let fresh_changed = fresh
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            changed_root_offset,
            &BTreeMap::from([(fresh_scroller.index(), NativePoint { x: 1, y: 1 })]),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&changed).unwrap(),
        serde_json::to_value(&fresh_changed).unwrap()
    );

    let resized_viewport = Viewport {
        width: 640,
        height: 480,
        device_scale_factor_milli: viewport.device_scale_factor_milli,
    };
    let resized = cached
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            resized_viewport,
            NativePoint { x: 0, y: 0 },
            &BTreeMap::new(),
        )
        .unwrap();
    let fresh_resized = fresh
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            resized_viewport,
            NativePoint { x: 0, y: 0 },
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&resized).unwrap(),
        serde_json::to_value(&fresh_resized).unwrap()
    );

    let inner = cached.find_element_by_id("inner").unwrap();
    let fresh_inner = fresh.find_element_by_id("inner").unwrap();
    cached
        .apply_script_attribute(
            inner.index(),
            "style",
            "width:320px;height:600px",
            None,
            &BTreeMap::new(),
        )
        .unwrap();
    fresh
        .apply_script_attribute(
            fresh_inner.index(),
            "style",
            "width:320px;height:600px",
            None,
            &BTreeMap::new(),
        )
        .unwrap();
    assert!(cached.layout_style_cache.5.lock().unwrap().is_none());
    let mutated = cached
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            changed_root_offset,
            &changed_nested_scroll,
        )
        .unwrap();
    let fresh_mutated = fresh
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            changed_root_offset,
            &BTreeMap::from([(fresh_scroller.index(), NativePoint { x: 1, y: 1 })]),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&mutated).unwrap(),
        serde_json::to_value(&fresh_mutated).unwrap()
    );

    let clone = cached.clone();
    assert!(clone.layout_style_cache.5.lock().unwrap().is_none());
    let cloned_snapshot = clone
        .script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            changed_root_offset,
            &changed_nested_scroll,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&mutated).unwrap(),
        serde_json::to_value(&cloned_snapshot).unwrap()
    );

    let invalid_offset = NativePoint {
        x: root_max.x,
        y: root_max.y.saturating_add(1),
    };
    assert!(
        cached
            .script_snapshot_with_layout(
                crate::limits::MAX_TEXT_BYTES,
                viewport,
                invalid_offset,
                &changed_nested_scroll,
            )
            .is_err()
    );
}

#[test]
fn script_snapshot_cache_invalidates_when_image_properties_arrive() {
    let limits = NativeEngineLimits::default();
    let viewport = Viewport::default();
    let mut document = NativeDocument::parse("<img id='image' src='/image.png'>", &limits).unwrap();
    let image_id = document.find_element_by_id("image").unwrap();
    let before = document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport);
    let before_image = before
        .elements
        .iter()
        .find(|element| element.node_index == image_id.index())
        .unwrap();
    assert!(!before_image.image_complete);

    let source = document.image_current_src(image_id, viewport);
    document
        .mark_image_load(image_id.index(), source.clone(), viewport)
        .unwrap();
    document
        .set_image_resource(
            image_id.index(),
            source,
            NativeImage::new(24, 40, vec![0; 24 * 40 * 4]),
        )
        .unwrap();
    let after = document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport);
    let after_image = after
        .elements
        .iter()
        .find(|element| element.node_index == image_id.index())
        .unwrap();
    assert!(after_image.image_complete);
    assert_eq!(after_image.image_natural_width, 24);
    assert_eq!(after_image.image_natural_height, 40);
}

#[test]
fn script_snapshot_cache_skips_animated_images_and_oversized_file_payloads() {
    let limits = NativeEngineLimits::default();
    let viewport = Viewport::default();
    let mut image_document =
        NativeDocument::parse("<img id='image' src='/image.png'>", &limits).unwrap();
    let image_id = image_document.find_element_by_id("image").unwrap();
    let source = image_document.image_current_src(image_id, viewport);
    image_document
        .mark_image_load(image_id.index(), source.clone(), viewport)
        .unwrap();
    let animation = NativeImage::with_frames(
        1,
        1,
        vec![
            NativeImageFrame {
                delay_ms: 10,
                pixels: vec![0; 4],
            },
            NativeImageFrame {
                delay_ms: 10,
                pixels: vec![1; 4],
            },
        ],
        Some(0),
    )
    .unwrap();
    image_document
        .set_image_resource(image_id.index(), source, animation)
        .unwrap();
    image_document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport);
    assert!(
        image_document
            .layout_style_cache
            .4
            .lock()
            .unwrap()
            .is_none()
    );

    let mut file_document =
        NativeDocument::parse("<input id='upload' type='file'>", &limits).unwrap();
    let upload_id = file_document.find_element_by_id("upload").unwrap();
    file_document.node_mut(upload_id).unwrap().state.files = vec![NativeFile {
        name: "large.bin".into(),
        media_type: "application/octet-stream".into(),
        last_modified: 0,
        bytes: vec![0; 9 * 1024 * 1024],
    }];
    file_document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport);
    assert!(file_document.layout_style_cache.4.lock().unwrap().is_none());
}

#[test]
fn decoded_image_arrival_invalidates_cached_intrinsic_layout() {
    let limits = NativeEngineLimits::default();
    let viewport = Viewport::default();
    let mut document = NativeDocument::parse("<img id='image' src='/image.png'>", &limits).unwrap();
    let image_id = document.find_element_by_id("image").unwrap();
    let before = document.layout(viewport).unwrap();
    assert_eq!(
        before.box_for(image_id),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 20,
        })
    );

    let source = document.image_current_src(image_id, viewport);
    document
        .mark_image_load(image_id.index(), source.clone(), viewport)
        .unwrap();
    document
        .set_image_resource(
            image_id.index(),
            source,
            NativeImage::new(24, 40, vec![0; 24 * 40 * 4]),
        )
        .unwrap();

    assert!(document.layout_style_cache.2.lock().unwrap().is_none());
    let after = document.layout(viewport).unwrap();
    assert_eq!(
        after.box_for(image_id),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 40,
        })
    );
}

#[test]
fn decoded_video_poster_arrival_invalidates_cached_intrinsic_layout() {
    let limits = NativeEngineLimits::default();
    let viewport = Viewport::default();
    let mut document =
        NativeDocument::parse("<video id='video' poster='/poster.png'></video>", &limits).unwrap();
    let video_id = document.find_element_by_id("video").unwrap();
    let before = document.layout(viewport).unwrap();
    assert_eq!(
        before
            .box_for(video_id)
            .map(|rect| (rect.width, rect.height)),
        Some((8, 20))
    );

    let source = document.selected_video_poster_source(video_id).unwrap();
    document
        .mark_video_poster_load(video_id.index(), source.clone())
        .unwrap();
    document
        .set_video_poster_resource(
            video_id.index(),
            source,
            NativeImage::new(24, 40, vec![0; 24 * 40 * 4]),
        )
        .unwrap();

    assert!(document.layout_style_cache.2.lock().unwrap().is_none());
    let after = document.layout(viewport).unwrap();
    assert_eq!(
        after
            .box_for(video_id)
            .map(|rect| (rect.width, rect.height)),
        Some((24, 40))
    );

    // Starting a new load clears the old decoded poster. Its old intrinsic
    // dimensions must not survive in the same-revision layout cache.
    document
        .mark_video_poster_load(video_id.index(), "/poster.png".to_owned())
        .unwrap();
    assert!(document.layout_style_cache.2.lock().unwrap().is_none());
    assert_eq!(
        document
            .layout(viewport)
            .unwrap()
            .box_for(video_id)
            .map(|rect| (rect.width, rect.height)),
        Some((8, 20))
    );

    document
        .set_video_poster_resource(
            video_id.index(),
            "/poster.png".to_owned(),
            NativeImage::new(24, 40, vec![0; 24 * 40 * 4]),
        )
        .unwrap();
    assert_eq!(
        document
            .layout(viewport)
            .unwrap()
            .box_for(video_id)
            .map(|rect| (rect.width, rect.height)),
        Some((24, 40))
    );
    document.clear_video_poster_loads();
    assert!(document.layout_style_cache.2.lock().unwrap().is_none());
    assert_eq!(
        document
            .layout(viewport)
            .unwrap()
            .box_for(video_id)
            .map(|rect| (rect.width, rect.height)),
        Some((8, 20))
    );
}

#[test]
fn script_style_mutation_invalidates_cached_cascade() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(NESTED_STYLES, &limits).unwrap();
    let p = document.find_element(document.root(), "p").unwrap();
    let before = document.computed_style_for_layout(p);
    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());

    document
        .apply_script_attribute(p.index, "style", "color: #abcdef", None, &BTreeMap::new())
        .unwrap();
    assert!(document.layout_style_cache.0.lock().unwrap().is_empty());
    let after = document.computed_style_for_layout(p);
    assert_ne!(before, after);
}

#[test]
fn script_class_attribute_mutations_match_a_fresh_cascade() {
    let limits = NativeEngineLimits::default();
    let source = "<style>.active{color:#abcdef}</style><button id=target>go</button>";
    let mut document = NativeDocument::parse(source, &limits).unwrap();
    let target = document.find_element_by_id("target").unwrap();
    let base = document.computed_style_for_layout(target);
    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());

    document
        .apply_script_attribute(target.index(), "class", "active", None, &BTreeMap::new())
        .unwrap();
    assert!(document.layout_style_cache.0.lock().unwrap().is_empty());
    let active = document.computed_style_for_layout(target);
    let active_fresh = NativeDocument::parse(
        "<style>.active{color:#abcdef}</style><button id=target class=active>go</button>",
        &limits,
    )
    .unwrap();
    assert_eq!(
        active,
        active_fresh.computed_style_for_layout(active_fresh.find_element_by_id("target").unwrap())
    );
    assert_ne!(base, active);

    document
        .remove_script_attribute(target.index(), "class", None, &BTreeMap::new())
        .unwrap();
    let reset = document.computed_style_for_layout(target);
    let base_fresh = NativeDocument::parse(source, &limits).unwrap();
    assert_eq!(
        reset,
        base_fresh.computed_style_for_layout(base_fresh.find_element_by_id("target").unwrap())
    );
    assert_eq!(base, reset);
}

#[test]
fn layout_style_cache_reuses_and_invalidates_inherited_custom_properties() {
    let limits = NativeEngineLimits::default();
    let source = "<style>main{--accent:#123456} main.active{--accent:#abcdef} p{color:var(--accent)}</style><main id=root><section><p id=target>text</p></section></main>";
    let mut cached = NativeDocument::parse(source, &limits).unwrap();
    let mut fresh = NativeDocument::parse(source, &limits).unwrap();
    let root = cached.find_element_by_id("root").unwrap();
    let target = cached.find_element_by_id("target").unwrap();
    let fresh_root = fresh.find_element_by_id("root").unwrap();
    let fresh_target = fresh.find_element_by_id("target").unwrap();

    let initial = cached.computed_style_for_layout(target);
    assert_eq!(
        initial.color(),
        Some(NativeColor {
            red: 0x12,
            green: 0x34,
            blue: 0x56,
            alpha: u8::MAX,
        })
    );
    assert_eq!(initial, fresh.computed_style_for_layout(fresh_target));
    let cached_custom_property_count = cached.layout_style_cache.1.lock().unwrap().len();
    assert!(cached_custom_property_count >= 5);

    cached
        .apply_script_attribute(root.index(), "class", "active", None, &BTreeMap::new())
        .unwrap();
    fresh
        .apply_script_attribute(
            fresh_root.index(),
            "class",
            "active",
            None,
            &BTreeMap::new(),
        )
        .unwrap();
    assert!(cached.layout_style_cache.0.lock().unwrap().is_empty());
    assert!(cached.layout_style_cache.1.lock().unwrap().is_empty());
    assert_eq!(
        cached.computed_style_for_layout(target),
        fresh.computed_style_for_layout(fresh_target)
    );
    assert_ne!(cached.computed_style_for_layout(target), initial);
    assert_eq!(
        cached.computed_style_for_layout(target).color(),
        Some(NativeColor {
            red: 0xab,
            green: 0xcd,
            blue: 0xef,
            alpha: u8::MAX,
        })
    );
}

#[test]
fn background_image_candidates_match_full_computed_style_scan() {
    let source = r#"<style>
        .hero { background-image: url('/hero.png') }
        .hero.special { background-image: none }
        #custom { background-image: url('/custom.png') }
        article { background: url('/article.png') center / cover }
        .inherited { background-image: inherit }
    </style>
    <div class="hero">hero</div>
    <div class="hero special">overridden</div>
    <div id="custom">custom</div>
    <article>shorthand</article>
    <div class="inherited" style="background-image: url('/inline.png')">inline</div>
    <div class="unrelated">no background</div>"#;
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse(source, &limits).unwrap();
    let optimized = document.external_background_image_links(usize::MAX);
    let full_scan: Vec<_> = document
        .nodes
        .iter()
        .filter(|node| node.element_name().is_some())
        .filter_map(|node| {
            document
                .background_image_source_for_node(node.id())
                .filter(|source| {
                    !source.is_empty()
                        && !source
                            .get(..5)
                            .is_some_and(|s| s.eq_ignore_ascii_case("data:"))
                })
                .map(|source| (node.id().index(), source.to_owned()))
        })
        .collect();
    assert_eq!(optimized, full_scan);
    assert!(optimized.iter().any(|(_, url)| url.ends_with("/hero.png")));
    assert!(
        optimized
            .iter()
            .any(|(_, url)| url.ends_with("/custom.png"))
    );
    assert!(
        optimized
            .iter()
            .any(|(_, url)| url.ends_with("/article.png"))
    );
    assert!(
        optimized
            .iter()
            .any(|(_, url)| url.ends_with("/inline.png"))
    );
    assert!(!optimized.iter().any(|(_, url)| url.ends_with("/old.png")));
    let limited = document.external_background_image_links(3);
    assert_eq!(limited.as_slice(), &optimized[..3]);
    assert!(document.external_background_image_links(0).is_empty());
}

#[test]
fn dynamic_selector_state_changes_invalidate_cached_cascade() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<style>#target:hover,#target:focus,#target:target{display:none}</style><button id=target>go</button><button id=other>other</button>",
        &limits,
    )
    .unwrap();
    let target = document.find_element(document.root(), "button").unwrap();
    assert!(!document.computed_style_for_layout(target).hidden());

    document.set_css_target_fragment(Some("target".to_owned()));
    assert!(document.computed_style_for_layout(target).hidden());

    document.set_css_target_fragment(None);
    assert!(!document.computed_style_for_layout(target).hidden());
    document.apply_hover(target).unwrap();
    assert!(document.computed_style_for_layout(target).hidden());

    let other = document.find_element_by_id("other").unwrap();
    document.apply_hover(other).unwrap();
    assert!(!document.computed_style_for_layout(target).hidden());
    document.focus_element(target);
    assert!(document.computed_style_for_layout(target).hidden());
}

#[test]
fn script_text_content_invalidates_has_selector_cache() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<style>.card:has(.selected){display:none}</style><div class='card' id='card'><span class='selected'>selected</span></div>",
        &limits,
    )
    .unwrap();
    let card = document.find_element_by_id("card").unwrap();

    assert!(document.computed_style_for_layout(card).hidden());
    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());

    document
        .apply_script_commands(&[NativeScriptCommand::SetTextContent {
            node_index: card.index(),
            value: "plain text".into(),
        }])
        .unwrap();

    let updated = document.computed_style_for_layout(card);
    let fresh = NativeDocument::parse(
        "<style>.card:has(.selected){display:none}</style><div class='card' id='card'>plain text</div>",
        &limits,
    )
    .unwrap();
    let fresh_card = fresh.find_element_by_id("card").unwrap();
    assert_eq!(updated, fresh.computed_style_for_layout(fresh_card));
    assert!(!updated.hidden());
}

#[test]
fn script_remove_node_invalidates_structural_selector_cache() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(
        "<style>#list > p:first-child{display:none}</style><div id='list'><p id='first'>first</p><p id='second'>second</p></div>",
        &limits,
    )
    .unwrap();
    let first = document.find_element_by_id("first").unwrap();
    let second = document.find_element_by_id("second").unwrap();

    assert!(!document.computed_style_for_layout(second).hidden());
    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());

    document
        .apply_script_commands(&[NativeScriptCommand::RemoveNode {
            node_index: first.index(),
        }])
        .unwrap();

    let updated = document.computed_style_for_layout(second);
    let fresh = NativeDocument::parse(
        "<style>#list > p:first-child{display:none}</style><div id='list'><p id='second'>second</p></div>",
        &limits,
    )
    .unwrap();
    let fresh_second = fresh.find_element_by_id("second").unwrap();
    assert_eq!(updated, fresh.computed_style_for_layout(fresh_second));
    assert!(updated.hidden());
}
