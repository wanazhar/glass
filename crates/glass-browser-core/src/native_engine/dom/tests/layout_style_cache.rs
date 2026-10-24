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
fn viewport_invalidation_clears_cache_and_clones_are_isolated() {
    let limits = NativeEngineLimits::default();
    let mut document = NativeDocument::parse(NESTED_STYLES, &limits).unwrap();
    let p = document.find_element(document.root(), "p").unwrap();
    let initial = document.computed_style_for_layout(p);
    assert!(!document.layout_style_cache.0.lock().unwrap().is_empty());

    let clone = document.clone();
    assert_eq!(clone.computed_style_for_layout(p), initial);
    document
        .set_viewport(Viewport {
            width: 640,
            height: 480,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(document.layout_style_cache.0.lock().unwrap().is_empty());
    assert!(!clone.layout_style_cache.0.lock().unwrap().is_empty());

    let updated = document.computed_style_for_layout(p);
    let fresh = NativeDocument::parse(NESTED_STYLES, &limits).unwrap();
    assert_eq!(updated, fresh.computed_style_for_layout(p));
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
