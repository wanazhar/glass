use super::*;

#[test]
fn direction_revert_layer_rolls_back_named_and_inline_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { direction: ltr; } #repeated { direction: ltr; } #inline { direction: ltr; } #fallback { direction: revert-layer; } } @layer theme { #named { direction: rtl; } #repeated { direction: revert-layer; } #inline { direction: rtl; } } @layer top { #repeated { direction: revert-layer; } } #named { direction: revert-layer; } #repeated { direction: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let inline = node("<div id='inline' style='direction:ReVeRt-LaYeR'>Inline</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");

    assert_eq!(
        stylesheet.computed_for(&named).direction(),
        DirectionValue::Rtl
    );
    assert_eq!(
        stylesheet.computed_for(&repeated).direction(),
        DirectionValue::Ltr
    );
    assert_eq!(
        stylesheet.computed_for(&inline).direction(),
        DirectionValue::Rtl
    );
    assert_eq!(
        stylesheet.computed_for(&fallback).direction(),
        DirectionValue::Ltr
    );

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
        vec!["#target { direction: revert-layer; }".into()],
        &mut diagnostics,
    )
    .unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "direction"
    }));
}

#[test]
fn overflow_parser_accepts_custom_property_aliases_and_fallbacks() {
    let mode = parse_custom_property_name("--mode").unwrap();
    assert_eq!(
        parse_overflow_property("hidden"),
        Some(OverflowDeclarationValue::Value(OverflowValue::Hidden))
    );
    assert_eq!(
        parse_overflow_declaration("var(--mode)"),
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::CustomProperty(mode)
        ))
    );
    assert_eq!(
        parse_overflow_declaration("var(--mode, auto)"),
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::CustomPropertyFallback(mode, OverflowValue::Auto)
        ))
    );
    assert_eq!(
        parse_overflow_declaration("var(--mode, var(--other))"),
        None
    );
    assert_eq!(
        parse_overflow_declaration("initial"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_overflow_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn overflow_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --mode: hidden; --alias: var(--mode); --cycle: var(--cycle); overflow: var(--mode); }
            #child { overflow-x: var(--alias); overflow-y: var(--missing, auto); }
            #fallback { overflow: var(--missing, clip); }
            #invalid { --bad: 1px; overflow: var(--bad, scroll); }
            #cycle { overflow: var(--cycle, hidden); }
            #wide-initial { --wide: initial; overflow: var(--wide); }
            #wide-inherit { --wide: inherit; overflow: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        let style = document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap());
        (style.overflow_x(), style.overflow_y())
    };

    assert_eq!(
        style("parent"),
        (OverflowValue::Hidden, OverflowValue::Hidden)
    );
    assert_eq!(style("child"), (OverflowValue::Hidden, OverflowValue::Auto));
    assert_eq!(
        style("fallback"),
        (OverflowValue::Clip, OverflowValue::Clip)
    );
    assert_eq!(
        style("invalid"),
        (OverflowValue::Scroll, OverflowValue::Scroll)
    );
    assert_eq!(
        style("cycle"),
        (OverflowValue::Hidden, OverflowValue::Hidden)
    );
    assert_eq!(
        style("wide-initial"),
        (OverflowValue::Other, OverflowValue::Other)
    );
    assert_eq!(
        style("wide-inherit"),
        (OverflowValue::Hidden, OverflowValue::Hidden)
    );
}

#[test]
fn overflow_axis_longhands_cascade_independently_from_shorthand() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        ".card { overflow: hidden; } #card { overflow-x: clip; overflow-y: visible; }".into(),
    ])
    .unwrap();
    let node = node("<div id='card' class='card'>Card</div>");
    let style = stylesheet.computed_for(&node);

    assert!(style.overflow_clip_x());
    assert!(!style.overflow_clip_y());

    let declarations =
        parse_declarations("overflow-x: hidden; overflow: clip; overflow-y: visible;");
    assert_eq!(
        declarations.overflow_x,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Clip)
        ))
    );
    assert_eq!(
        declarations.overflow_y,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Other)
        ))
    );
}

#[test]
fn overflow_explicit_inherit_uses_parent_axes_and_preserves_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#both { overflow: inherit; } #x { overflow-x: inherit; } #y { overflow-y: inherit; } #omitted { overflow: visible; } #invalid { overflow: hidden; overflow: inherit hidden; } #important { overflow: hidden !important; } #important { overflow: inherit; } #important-inherit { overflow: visible; } #important-inherit { overflow: inherit !important; }".into(),
        ])
        .unwrap();
    let inherited = NativeInheritedStyle {
        overflow_x: OverflowValue::Clip,
        overflow_y: OverflowValue::Other,
        ..NativeInheritedStyle::default()
    };
    let style = |id| {
        let element = node(&format!("<div id='{id}'>Test</div>"));
        stylesheet
            .computed_for_with_matcher(&element, inherited, |selector| selector.matches(&element))
    };

    assert_eq!(
        (
            style("both").overflow_clip_x(),
            style("both").overflow_clip_y()
        ),
        (true, false)
    );
    assert_eq!(
        (style("x").overflow_clip_x(), style("x").overflow_clip_y()),
        (true, false)
    );
    assert_eq!(
        (style("y").overflow_clip_x(), style("y").overflow_clip_y()),
        (false, false)
    );
    assert_eq!(
        (
            style("omitted").overflow_clip_x(),
            style("omitted").overflow_clip_y()
        ),
        (false, false)
    );
    assert_eq!(
        (
            style("invalid").overflow_clip_x(),
            style("invalid").overflow_clip_y()
        ),
        (true, true)
    );
    assert_eq!(
        (
            style("important").overflow_clip_x(),
            style("important").overflow_clip_y()
        ),
        (true, true)
    );
    assert_eq!(
        (
            style("important-inherit").overflow_clip_x(),
            style("important-inherit").overflow_clip_y()
        ),
        (true, false)
    );

    let root = node("<div id='both'>Root</div>");
    let root_style =
        stylesheet.computed_for_with_matcher(&root, NativeInheritedStyle::default(), |selector| {
            selector.matches(&root)
        });
    assert_eq!(
        (root_style.overflow_clip_x(), root_style.overflow_clip_y()),
        (false, false)
    );
    assert_eq!(
        parse_overflow_declaration("InHeRiT"),
        Some(LocalCascadeDeclaration::Inherit)
    );
    assert_eq!(parse_overflow_declaration("inherit hidden"), None);
}

#[test]
fn overflow_css_wide_resets_use_visible_fallback_and_preserve_priority() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#initial { overflow: hidden; overflow: InItIaL; } #x-reset { overflow-x: hidden; overflow-x: INITIAL; overflow-y: clip; } #y-reset { overflow-x: clip; overflow-y: hidden; overflow-y: unset; } #revert { overflow: clip; overflow: ReVeRt; } #invalid { overflow: hidden; overflow: initial hidden; } #important { overflow: hidden !important; } #important { overflow: initial; } #important-reset { overflow: hidden; } #important-reset { overflow: initial !important; }".into(),
        ])
        .unwrap();
    let inherited = NativeInheritedStyle {
        overflow_x: OverflowValue::Clip,
        overflow_y: OverflowValue::Other,
        ..NativeInheritedStyle::default()
    };
    let style = |id| {
        let element = node(&format!("<div id='{id}'>Test</div>"));
        stylesheet
            .computed_for_with_matcher(&element, inherited, |selector| selector.matches(&element))
    };

    for id in ["initial", "revert"] {
        assert_eq!(
            (style(id).overflow_clip_x(), style(id).overflow_clip_y()),
            (false, false),
            "overflow reset for {id}"
        );
    }
    assert_eq!(
        (
            style("x-reset").overflow_clip_x(),
            style("x-reset").overflow_clip_y()
        ),
        (false, true)
    );
    assert_eq!(
        (
            style("y-reset").overflow_clip_x(),
            style("y-reset").overflow_clip_y()
        ),
        (true, false)
    );
    assert_eq!(
        (
            style("invalid").overflow_clip_x(),
            style("invalid").overflow_clip_y()
        ),
        (true, true)
    );
    assert_eq!(
        (
            style("important").overflow_clip_x(),
            style("important").overflow_clip_y()
        ),
        (true, true)
    );
    assert_eq!(
        (
            style("important-reset").overflow_clip_x(),
            style("important-reset").overflow_clip_y()
        ),
        (false, false)
    );

    for keyword in ["INITIAL", "UnSeT", "ReVeRt"] {
        assert_eq!(
            parse_overflow_declaration(keyword),
            Some(LocalCascadeDeclaration::Reset),
            "overflow {keyword}"
        );
    }
    assert_eq!(parse_overflow_declaration("initial hidden"), None);
    assert_eq!(
        parse_overflow_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
}

#[test]
fn overflow_scroll_keywords_are_supported_without_diagnostics() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#visible { overflow: ViSiBlE; } #auto { overflow-x: AUTO; overflow-y: hidden; } #scroll { overflow-x: clip; overflow-y: ScRoLl; }"
                .into(),
        ])
        .unwrap();
    let visible = node("<div id='visible'>Visible</div>");
    let auto = node("<div id='auto'>Auto</div>");
    let scroll = node("<div id='scroll'>Scroll</div>");
    let clips = |element: &NativeNode| {
        let style = stylesheet.computed_for(element);
        (style.overflow_clip_x(), style.overflow_clip_y())
    };

    assert_eq!(clips(&visible), (false, false));
    assert_eq!(clips(&auto), (true, true));
    assert_eq!(clips(&scroll), (true, true));
    for keyword in ["visible", "AUTO", "ScRoLl"] {
        assert_eq!(
            parse_overflow_declaration(keyword),
            Some(LocalCascadeDeclaration::Value(
                OverflowDeclarationValue::Value(match keyword {
                    "AUTO" => OverflowValue::Auto,
                    "ScRoLl" => OverflowValue::Scroll,
                    _ => OverflowValue::Other,
                })
            )),
            "overflow {keyword}"
        );
    }
    assert_eq!(parse_overflow_declaration("visible hidden"), None);

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
            vec![
                "#visible { overflow: visible; } #auto { overflow-x: auto; } #scroll { overflow-y: scroll; }".into(),
            ],
            &mut diagnostics,
        )
        .unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert!(!diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            NativeDiagnosticCode::UnsupportedCssProperty
                | NativeDiagnosticCode::UnsupportedCssValue
        ) && matches!(
            diagnostic.detail.as_str(),
            "overflow" | "overflow-x" | "overflow-y"
        )
    }));
}

#[test]
fn overflow_revert_layer_rolls_back_independent_axes_and_shorthand() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { overflow: hidden; } #axis { overflow-x: hidden; overflow-y: clip; } #repeat { overflow: hidden; } #fallback { overflow: revert-layer; } #inline { overflow: hidden; } #unsupported { overflow: visible; } } @layer theme { #named { overflow-x: clip; overflow-y: revert-layer; } #axis { overflow-x: revert-layer; overflow-y: hidden; } #repeat { overflow: revert-layer; } } @layer top { #named { overflow: revert-layer; } #repeat { overflow: revert-layer; } } #named { overflow: revert-layer; } #repeat { overflow: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let axis = node("<div id='axis'>Axis</div>");
    let repeat = node("<div id='repeat'>Repeat</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let inline = node("<div id='inline' style='overflow:ReVeRt-LaYeR'>Inline</div>");
    let unsupported = node("<div id='unsupported'>Unsupported</div>");

    let style = |node| stylesheet.computed_for(node);
    assert!(style(&named).overflow_clip_x());
    assert!(style(&named).overflow_clip_y());
    assert!(style(&axis).overflow_clip_x());
    assert!(style(&axis).overflow_clip_y());
    assert!(style(&repeat).overflow_clip_x());
    assert!(style(&repeat).overflow_clip_y());
    assert!(!style(&fallback).overflow_clip_x());
    assert!(!style(&fallback).overflow_clip_y());
    assert!(style(&inline).overflow_clip_x());
    assert!(style(&inline).overflow_clip_y());
    assert!(!style(&unsupported).overflow_clip_x());
    assert!(!style(&unsupported).overflow_clip_y());

    assert_eq!(
        parse_overflow_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_overflow_declaration("visible"),
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Other)
        ))
    );

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
        vec!["#target { overflow: ReVeRt-LaYeR; overflow-x: visible; overflow-y: clip; }".into()],
        &mut diagnostics,
    )
    .unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(diagnostic.detail.as_str(), "overflow" | "overflow-y")
    }));
    assert!(!diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            NativeDiagnosticCode::UnsupportedCssProperty
                | NativeDiagnosticCode::UnsupportedCssValue
        ) && matches!(
            diagnostic.detail.as_str(),
            "overflow" | "overflow-x" | "overflow-y"
        )
    }));
}

#[test]
fn overflow_important_parser_tracks_markers_and_invalid_preservation() {
    let declarations = parse_declarations(
        "overflow: hidden !IMPORTANT; overflow-x: clip; overflow-y: visible !important",
    );
    assert_eq!(
        declarations.overflow,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Hidden)
        ))
    );
    assert_eq!(
        declarations.overflow_x,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Clip)
        ))
    );
    assert_eq!(
        declarations.overflow_y,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Other)
        ))
    );
    assert_eq!(
        declarations.overflow_importance,
        NativeOverflowDeclarationImportance {
            shorthand: true,
            x: false,
            y: true,
        }
    );

    let preserved = parse_declarations(
        "overflow: hidden !important; overflow: bad !important; overflow-x: clip !important; overflow-x: bad !important; overflow-y: visible !important; overflow-y: bad !important",
    );
    assert_eq!(
        preserved.overflow,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Hidden)
        ))
    );
    assert_eq!(
        preserved.overflow_x,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Clip)
        ))
    );
    assert_eq!(
        preserved.overflow_y,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Other)
        ))
    );
    assert_eq!(
        preserved.overflow_importance,
        NativeOverflowDeclarationImportance {
            shorthand: true,
            x: true,
            y: true,
        }
    );
}

#[test]
fn stylesheet_cascade_resolves_overflow_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { overflow: hidden !important; }
                #axis { overflow-x: hidden !important; overflow-y: visible !important; }
                #rollback { overflow: hidden !important; overflow: revert-layer !important; }
                #invalid { overflow: hidden !important; overflow: bad !important; overflow-x: clip !important; overflow-x: bad !important; overflow-y: visible !important; overflow-y: bad !important; }
                #normal { overflow: clip; }
            }
            @layer theme {
                #important { overflow: visible !important; }
                #axis { overflow-x: visible !important; overflow-y: hidden !important; }
                #rollback { overflow: clip !important; }
                #normal { overflow: hidden; }
            }
            #important { overflow: clip; }
            #axis { overflow: clip; }
            #rollback { overflow: visible !important; }
            #normal { overflow: visible; }
            #inline { overflow: visible !important; }"#
                .into(),
        ])
        .unwrap();
    let important = node("<div id='important'>Important</div>");
    let axis = node("<div id='axis'>Axis</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let normal = node("<div id='normal'>Normal</div>");
    let inline = node("<div id='inline' style='overflow:hidden !important'>Inline</div>");

    let clips = |element: &NativeNode| {
        let style = stylesheet.computed_for(element);
        (style.overflow_clip_x(), style.overflow_clip_y())
    };
    assert_eq!(clips(&important), (true, true));
    assert_eq!(clips(&axis), (true, false));
    assert_eq!(clips(&rollback), (true, true));
    assert_eq!(clips(&invalid), (true, false));
    assert_eq!(clips(&normal), (false, false));
    assert_eq!(clips(&inline), (true, true));
}

#[test]
fn stylesheet_cascade_resolves_min_max_dimensions_with_inline_precedence() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "div { min-width: 8px; max-width: 64px; min-height: 10px; max-height: 80px; } #card { min-width: 16px; max-height: 40px; }"
                .into(),
        ])
        .unwrap();
    let node = node("<div id='card' style='max-width: 32px; min-height: 20px'>Card</div>");
    let style = stylesheet.computed_for(&node);

    assert_eq!(style.min_width(), Some(16));
    assert_eq!(style.max_width(), Some(32));
    assert_eq!(style.min_height(), Some(20));
    assert_eq!(style.max_height(), Some(40));
}

#[test]
fn pointer_events_parser_accepts_custom_property_aliases_and_fallbacks() {
    let mode = parse_custom_property_name("--mode").unwrap();
    assert_eq!(
        parse_pointer_events_property("auto"),
        Some(PointerEventsDeclarationValue::Value(
            NativePointerEventsValue::Auto
        ))
    );
    assert_eq!(
        parse_pointer_events_declaration("var(--mode)"),
        Some(InheritedTextDeclaration::Value(
            PointerEventsDeclarationValue::CustomProperty(mode)
        ))
    );
    assert_eq!(
        parse_pointer_events_declaration("var(--mode, none)"),
        Some(InheritedTextDeclaration::Value(
            PointerEventsDeclarationValue::CustomPropertyFallback(
                mode,
                NativePointerEventsValue::None
            )
        ))
    );
    assert_eq!(
        parse_pointer_events_declaration("var(--mode, var(--other))"),
        None
    );
    assert_eq!(
        parse_pointer_events_declaration("inherit"),
        Some(InheritedTextDeclaration::Inherit)
    );
}

#[test]
fn computed_style_inherits_fixed_line_height_and_preserves_child_precedence() {
    let document = NativeDocument::parse(
            "<style>#parent { line-height: 28px; } #explicit { line-height: 32px; }</style><div id='parent'><section id='child'>Child</section><section id='explicit'>Explicit</section><section id='invalid' style='line-height:0px'>Invalid</section></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).line_height(),
        Some(28)
    );
    assert_eq!(
        document.computed_style_for_layout(child).line_height(),
        Some(28)
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).line_height(),
        Some(32)
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).line_height(),
        Some(28)
    );
}

#[test]
fn inherited_line_height_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --line-height: 28px; --alias: var(--line-height); --cycle: var(--cycle); line-height: var(--line-height); }
            #child { line-height: var(--alias); }
            #fallback { line-height: var(--missing, 24px); }
            #invalid { --bad: 1em; line-height: var(--bad, 20px); }
            #cycle { line-height: var(--cycle, 16px); }
            #wide-initial { --wide: initial; line-height: var(--wide); }
            #wide-inherit { --wide: inherit; line-height: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .line_height()
    };

    assert_eq!(style("parent"), Some(28));
    assert_eq!(style("child"), Some(28));
    assert_eq!(style("fallback"), Some(24));
    assert_eq!(style("invalid"), Some(20));
    assert_eq!(style("cycle"), Some(16));
    assert_eq!(style("wide-initial"), None);
    assert_eq!(style("wide-inherit"), Some(28));
}

#[test]
fn inherited_pointer_events_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --pointer-events: none; --alias: var(--pointer-events); --cycle: var(--cycle); pointer-events: var(--pointer-events); }
            #child { pointer-events: var(--alias); }
            #fallback { pointer-events: var(--missing, auto); }
            #invalid { --bad: visible; pointer-events: var(--bad, auto); }
            #cycle { pointer-events: var(--cycle, auto); }
            #wide-initial { --wide: initial; pointer-events: var(--wide); }
            #wide-inherit { --wide: inherit; pointer-events: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .pointer_events()
    };

    assert_eq!(style("parent"), NativePointerEventsValue::None);
    assert_eq!(style("child"), NativePointerEventsValue::None);
    assert_eq!(style("fallback"), NativePointerEventsValue::Auto);
    assert_eq!(style("invalid"), NativePointerEventsValue::Auto);
    assert_eq!(style("cycle"), NativePointerEventsValue::Auto);
    assert_eq!(style("wide-initial"), NativePointerEventsValue::Auto);
    assert_eq!(style("wide-inherit"), NativePointerEventsValue::None);
}

#[test]
fn stylesheet_cascade_resolves_physical_border_sides_independently() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { border: 1px solid red; } #card { border-left: 3px solid blue; }".into(),
    ])
    .unwrap();
    let node = node("<div id='card' style='border-top: 2px solid green'>Card</div>");
    assert_eq!(
        stylesheet.computed_for(&node).border(),
        Some(NativeBorder {
            top: border_side(
                2,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                },
            ),
            right: border_side(1, NativeColor::RED),
            bottom: border_side(1, NativeColor::RED),
            left: border_side(
                3,
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
            ),
        })
    );
}

#[test]
fn dimension_parser_rejects_non_pixel_or_unbounded_values() {
    assert_eq!(parse_dimension("240px"), Some(240));
    assert_eq!(parse_dimension("240"), None);
    assert_eq!(parse_dimension("50%"), None);
    assert_eq!(parse_dimension("20000px"), None);
}

#[test]
fn line_height_parser_accepts_only_positive_bounded_pixels() {
    assert_eq!(parse_line_height("28px"), Some(28));
    assert_eq!(parse_line_height("0px"), None);
    assert_eq!(parse_line_height("normal"), None);
    assert_eq!(parse_line_height("1.5"), None);
    assert_eq!(parse_line_height("50%"), None);
    assert_eq!(parse_line_height("20000px"), None);
}

#[test]
fn line_height_declaration_parser_accepts_standalone_css_wide_keywords() {
    assert_eq!(
        parse_line_height_declaration("ReVeRt-LaYeR"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_line_height_declaration("28px"),
        Some(InheritedTextDeclaration::Value(
            LineHeightDeclarationValue::Value(28)
        ))
    );
    assert_eq!(
        parse_line_height_declaration(" REVERT-LAYER "),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_line_height_declaration("InHeRiT"),
        Some(InheritedTextDeclaration::Inherit)
    );
    assert_eq!(
        parse_line_height_declaration("INITIAL"),
        Some(InheritedTextDeclaration::Initial)
    );
    assert_eq!(
        parse_line_height_declaration("unset"),
        Some(InheritedTextDeclaration::Unset)
    );
    assert_eq!(
        parse_line_height_declaration("ReVeRt"),
        Some(InheritedTextDeclaration::Revert)
    );
    assert_eq!(parse_line_height_declaration("inherit 28px"), None);
    assert_eq!(parse_line_height_declaration("0px"), None);
    assert_eq!(parse_line_height_declaration("revert-layer 28px"), None);
    let line_height_name = parse_custom_property_name("--line-height").unwrap();
    assert_eq!(
        parse_line_height_property("var(--line-height)"),
        Some(LineHeightDeclarationValue::CustomProperty(line_height_name))
    );
    assert_eq!(
        parse_line_height_declaration("var(--line-height, 24px)"),
        Some(InheritedTextDeclaration::Value(
            LineHeightDeclarationValue::CustomPropertyFallback(line_height_name, 24)
        ))
    );
    assert_eq!(
        parse_line_height_property("var(--line-height, var(--other))"),
        None
    );
}

#[test]
fn text_indent_parser_accepts_only_bounded_non_negative_pixels() {
    assert_eq!(parse_dimension("16px"), Some(16));
    assert_eq!(parse_dimension("0px"), Some(0));
    assert_eq!(parse_dimension("-1px"), None);
    assert_eq!(parse_dimension("1.5px"), None);
    assert_eq!(parse_dimension("2em"), None);
    assert_eq!(parse_dimension("50%"), None);
    assert_eq!(parse_dimension("20000px"), None);
}

#[test]
fn text_indent_parser_accepts_custom_property_aliases_and_fallbacks() {
    let indent = parse_custom_property_name("--indent").unwrap();
    assert_eq!(
        parse_text_indent_property("var(--indent)"),
        Some(TextIndentDeclarationValue::CustomProperty(indent))
    );
    assert_eq!(
        parse_text_indent_declaration("var(--indent, 12px)"),
        Some(LocalCascadeDeclaration::Value(
            TextIndentDeclarationValue::CustomPropertyFallback(indent, 12)
        ))
    );
    assert_eq!(
        parse_text_indent_declaration("var(--indent, var(--other))"),
        None
    );
    assert_eq!(
        parse_text_indent_declaration("initial"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_text_indent_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn word_spacing_parser_accepts_only_bounded_non_negative_pixels() {
    assert_eq!(parse_dimension("16px"), Some(16));
    assert_eq!(parse_dimension("0px"), Some(0));
    assert_eq!(parse_dimension("-1px"), None);
    assert_eq!(parse_dimension("1.5px"), None);
    assert_eq!(parse_dimension("2em"), None);
    assert_eq!(parse_dimension("50%"), None);
    assert_eq!(parse_dimension("20000px"), None);
}

#[test]
fn letter_spacing_parser_accepts_only_bounded_non_negative_pixels() {
    assert_eq!(parse_dimension("16px"), Some(16));
    assert_eq!(parse_dimension("0px"), Some(0));
    assert_eq!(parse_dimension("-1px"), None);
    assert_eq!(parse_dimension("1.5px"), None);
    assert_eq!(parse_dimension("2em"), None);
    assert_eq!(parse_dimension("50%"), None);
    assert_eq!(parse_dimension("normal"), None);
    assert_eq!(parse_dimension("20000px"), None);
}

#[test]
fn gap_parser_accepts_css_wide_resets_and_bounded_non_negative_pixels() {
    assert_eq!(
        parse_declarations("gap: 16px").gap,
        Some(GapShorthandDeclaration::Value(NativeGapValue {
            row: 16,
            column: 16,
        }))
    );
    assert_eq!(
        parse_declarations("gap: 8px 16px").gap,
        Some(GapShorthandDeclaration::Value(NativeGapValue {
            row: 8,
            column: 16,
        }))
    );
    assert_eq!(
        parse_declarations("gap: 0px").gap,
        Some(GapShorthandDeclaration::Value(NativeGapValue {
            row: 0,
            column: 0,
        }))
    );
    assert_eq!(
        parse_declarations("gap: REVERT-LAYER").gap,
        Some(GapShorthandDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_declarations("gap: INITIAL").gap,
        Some(GapShorthandDeclaration::Initial)
    );
    assert_eq!(
        parse_declarations("gap: UnSeT").gap,
        Some(GapShorthandDeclaration::Unset)
    );
    assert_eq!(
        parse_declarations("gap: ReVeRt").gap,
        Some(GapShorthandDeclaration::Revert)
    );
    assert_eq!(
        parse_declarations("gap: InHeRiT").gap,
        Some(GapShorthandDeclaration::Inherit)
    );
    assert_eq!(parse_declarations("gap: inherit 1px").gap, None);
    assert_eq!(parse_declarations("gap: 1px inherit").gap, None);
    assert_eq!(parse_declarations("gap: -1px").gap, None);
    assert_eq!(parse_declarations("gap: 1px 2px 3px").gap, None);
    assert_eq!(parse_declarations("gap: revert-layer 1px").gap, None);
    assert_eq!(parse_declarations("gap: 1.5px").gap, None);
    assert_eq!(parse_declarations("gap: 2em").gap, None);
    assert_eq!(parse_declarations("gap: 50%").gap, None);
    assert_eq!(parse_declarations("gap: 20000px").gap, None);
}
#[test]
fn gap_parser_accepts_custom_property_aliases_and_fallbacks() {
    let gap = parse_custom_property_name("--gap").unwrap();
    let row = parse_custom_property_name("--row").unwrap();
    assert_eq!(
        parse_gap_declaration("var(--gap)"),
        Some(GapShorthandDeclaration::CustomProperty(gap))
    );
    assert_eq!(
        parse_gap_declaration("var(--gap, 8px 12px)"),
        Some(GapShorthandDeclaration::CustomPropertyFallback(
            gap,
            NativeGapValue { row: 8, column: 12 }
        ))
    );
    assert_eq!(
        parse_gap_component_declaration("var(--row, 4px)"),
        Some(GapComponentDeclaration::CustomPropertyFallback(row, 4))
    );
    let declarations =
        parse_declarations("gap: var(--gap); row-gap: var(--row, 4px); column-gap: var(--gap)");
    assert_eq!(
        declarations.gap,
        Some(GapShorthandDeclaration::CustomProperty(gap))
    );
    assert_eq!(
        declarations.row_gap,
        Some(GapComponentDeclaration::CustomPropertyFallback(row, 4))
    );
    assert_eq!(
        declarations.column_gap,
        Some(GapComponentDeclaration::CustomProperty(gap))
    );
    assert_eq!(parse_gap_declaration("var(--gap, var(--other))"), None);
    assert_eq!(parse_gap_component_declaration("var(--row, 1px 2px)"), None);
}

#[test]
fn row_gap_parser_accepts_css_wide_resets_and_bounded_non_negative_pixels() {
    assert_eq!(
        parse_declarations("row-gap: 16px").row_gap,
        Some(GapComponentDeclaration::Value(16))
    );
    assert_eq!(
        parse_declarations("row-gap: 0px").row_gap,
        Some(GapComponentDeclaration::Value(0))
    );
    assert_eq!(
        parse_declarations("row-gap: revert-LAYER").row_gap,
        Some(GapComponentDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_declarations("row-gap: INITIAL").row_gap,
        Some(GapComponentDeclaration::Initial)
    );
    assert_eq!(
        parse_declarations("row-gap: UnSeT").row_gap,
        Some(GapComponentDeclaration::Unset)
    );
    assert_eq!(
        parse_declarations("row-gap: ReVeRt").row_gap,
        Some(GapComponentDeclaration::Revert)
    );
    assert_eq!(
        parse_declarations("row-gap: InHeRiT").row_gap,
        Some(GapComponentDeclaration::Inherit)
    );
    assert_eq!(parse_declarations("row-gap: inherit 1px").row_gap, None);
    assert_eq!(parse_declarations("row-gap: -1px").row_gap, None);
    assert_eq!(parse_declarations("row-gap: 1px 2px").row_gap, None);
    assert_eq!(
        parse_declarations("row-gap: revert-layer 1px").row_gap,
        None
    );
    assert_eq!(parse_declarations("row-gap: 1.5px").row_gap, None);
    assert_eq!(parse_declarations("row-gap: 2em").row_gap, None);
    assert_eq!(parse_declarations("row-gap: 50%").row_gap, None);
    assert_eq!(parse_declarations("row-gap: 20000px").row_gap, None);
}

#[test]
fn column_gap_parser_accepts_css_wide_resets_and_bounded_non_negative_pixels() {
    assert_eq!(
        parse_declarations("column-gap: 16px").column_gap,
        Some(GapComponentDeclaration::Value(16))
    );
    assert_eq!(
        parse_declarations("column-gap: 0px").column_gap,
        Some(GapComponentDeclaration::Value(0))
    );
    assert_eq!(
        parse_declarations("column-gap: REVERT-layer").column_gap,
        Some(GapComponentDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_declarations("column-gap: INITIAL").column_gap,
        Some(GapComponentDeclaration::Initial)
    );
    assert_eq!(
        parse_declarations("column-gap: UnSeT").column_gap,
        Some(GapComponentDeclaration::Unset)
    );
    assert_eq!(
        parse_declarations("column-gap: ReVeRt").column_gap,
        Some(GapComponentDeclaration::Revert)
    );
    assert_eq!(
        parse_declarations("column-gap: InHeRiT").column_gap,
        Some(GapComponentDeclaration::Inherit)
    );
    assert_eq!(
        parse_declarations("column-gap: 1px inherit").column_gap,
        None
    );
    assert_eq!(parse_declarations("column-gap: -1px").column_gap, None);
    assert_eq!(parse_declarations("column-gap: 1px 2px").column_gap, None);
    assert_eq!(
        parse_declarations("column-gap: revert-layer 1px").column_gap,
        None
    );
    assert_eq!(parse_declarations("column-gap: 1.5px").column_gap, None);
    assert_eq!(parse_declarations("column-gap: 2em").column_gap, None);
    assert_eq!(parse_declarations("column-gap: 50%").column_gap, None);
    assert_eq!(parse_declarations("column-gap: 20000px").column_gap, None);
}
