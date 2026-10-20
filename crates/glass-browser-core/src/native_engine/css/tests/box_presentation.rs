use super::*;

#[test]
fn stylesheet_cascade_prefers_specificity_then_inline_style() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "button { display: none; } #shown { display: block; }".into(),
    ])
    .unwrap();
    let node = node("<button id='shown' style='display:none'>Shown</button>");
    assert!(stylesheet.computed_for(&node).hidden());
}
#[test]
fn visibility_parser_accepts_custom_property_aliases_and_fallbacks() {
    let mode = parse_custom_property_name("--mode").unwrap();
    assert_eq!(
        parse_visibility("var(--mode)"),
        Some(VisibilityValue::CustomProperty(mode))
    );
    assert_eq!(
        parse_visibility("var(--mode, hidden)"),
        Some(VisibilityValue::CustomPropertyFallback(mode, true))
    );
    assert_eq!(
        parse_visibility("var(--mode, visible)"),
        Some(VisibilityValue::CustomPropertyFallback(mode, false))
    );
    assert_eq!(
        parse_declarations("visibility: var(--mode)").visibility,
        Some(LocalCascadeDeclaration::Value(
            VisibilityValue::CustomProperty(mode)
        ))
    );
    assert_eq!(parse_visibility("var(--mode, var(--other))"), None);
    assert_eq!(
        parse_visibility_declaration("revert-layer"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
}

#[test]
fn display_visibility_revert_layer_rolls_back_named_and_inline_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { display:none; visibility:hidden; } #repeat { display:block; visibility:hidden; } #contents { display:contents; visibility:visible; } #fallback { display:revert-layer; visibility:revert-layer; } #inline { display:block; visibility:hidden; } } @layer theme { #named { display:block; visibility:visible; } #named { display:revert-layer; visibility:revert-layer; } #repeat { display:revert-layer; visibility:revert-layer; } #contents { display:block; visibility:hidden; } #contents { display:revert-layer; visibility:revert-layer; } } @layer top { #repeat { display:revert-layer; visibility:revert-layer; } } #named { display:revert-layer; visibility:revert-layer; } #repeat { display:revert-layer; visibility:revert-layer; } #contents { display:revert-layer; visibility:revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<button id='named'>Named</button>");
    let repeat = node("<button id='repeat'>Repeat</button>");
    let contents = node("<button id='contents'>Contents</button>");
    let fallback = node("<button id='fallback'>Fallback</button>");
    let inline = node(
        "<button id='inline' style='display:ReVeRt-LaYeR;visibility:revert-layer'>Inline</button>",
    );

    assert_eq!(
        stylesheet.computed_for(&named).display(),
        DisplayValue::None
    );
    assert!(stylesheet.computed_for(&named).hidden());
    assert_eq!(
        stylesheet.computed_for(&repeat).display(),
        DisplayValue::Block
    );
    assert!(stylesheet.computed_for(&repeat).hidden());
    assert_eq!(
        stylesheet.computed_for(&contents).display(),
        DisplayValue::Contents
    );
    assert!(!stylesheet.computed_for(&contents).hidden());
    assert_eq!(
        stylesheet.computed_for(&fallback).display(),
        DisplayValue::Auto
    );
    assert!(!stylesheet.computed_for(&fallback).hidden());
    assert_eq!(
        stylesheet.computed_for(&inline).display(),
        DisplayValue::Block
    );
    assert!(stylesheet.computed_for(&inline).hidden());

    let declarations = parse_declarations(
        "display: block; display: unsupported; visibility: hidden; visibility: unsupported",
    );
    assert_eq!(
        declarations.display,
        Some(LocalCascadeDeclaration::Value(
            DisplayDeclarationValue::Value(DisplayValue::Block)
        ))
    );
    assert_eq!(
        declarations.visibility,
        Some(LocalCascadeDeclaration::Value(VisibilityValue::Hidden))
    );
    assert_eq!(
        parse_display_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_visibility_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
            vec![
                "#valid { display: ReVeRt-LaYeR; visibility: revert-layer; } #invalid { display: block inline; visibility: hidden visible; }"
                    .into(),
            ],
            &mut diagnostics,
        )
        .unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == "display"
            })
            .count(),
        1
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == "visibility"
            })
            .count(),
        1
    );
}
#[test]
fn visibility_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent { --visibility: hidden; --alias: var(--visibility); visibility: var(--alias); }
            #child { visibility: var(--alias); }
            #fallback { visibility: var(--missing, hidden); }
            #invalid { --bad: unsupported; visibility: var(--bad, visible); }
            #cycle { --cycle: var(--cycle); visibility: var(--cycle, hidden); }
            #visible { --visible: visible; visibility: var(--visible); }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='visible'>Visible</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let hidden = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .hidden()
    };

    assert!(hidden("parent"));
    assert!(hidden("child"));
    assert!(hidden("fallback"));
    assert!(!hidden("invalid"));
    assert!(hidden("cycle"));
    assert!(!hidden("visible"));
}

#[test]
fn display_parser_accepts_custom_property_aliases_and_fallbacks() {
    let display = parse_custom_property_name("--display").unwrap();
    assert_eq!(
        parse_display_property("none"),
        Some(DisplayDeclarationValue::Value(DisplayValue::None))
    );
    assert_eq!(
        parse_display_declaration("var(--display)"),
        Some(LocalCascadeDeclaration::Value(
            DisplayDeclarationValue::CustomProperty(display)
        ))
    );
    assert_eq!(
        parse_display_declaration("var(--display, block)"),
        Some(LocalCascadeDeclaration::Value(
            DisplayDeclarationValue::CustomPropertyFallback(display, DisplayValue::Block)
        ))
    );
    assert_eq!(
        parse_display_declaration("var(--display, var(--other))"),
        None
    );
}

#[test]
fn display_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --display: none; --alias: var(--display); --cycle: var(--cycle); display: var(--display); }
            #child { display: var(--alias); }
            #fallback { display: var(--missing, block); }
            #invalid { --bad: unsupported; display: var(--bad, none); }
            #cycle { display: var(--cycle, contents); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .display()
    };

    assert_eq!(style("parent"), DisplayValue::None);
    assert_eq!(style("child"), DisplayValue::None);
    assert_eq!(style("fallback"), DisplayValue::Block);
    assert_eq!(style("invalid"), DisplayValue::None);
    assert_eq!(style("cycle"), DisplayValue::Contents);
}

#[test]
fn position_parser_accepts_custom_property_aliases_and_fallbacks() {
    let position = parse_custom_property_name("--position").unwrap();
    assert_eq!(
        parse_position_property("absolute"),
        Some(PositionDeclarationValue::Value(
            NativePositionValue::Absolute
        ))
    );
    assert_eq!(
        parse_position_declaration("var(--position)"),
        Some(LocalCascadeDeclaration::Value(
            PositionDeclarationValue::CustomProperty(position)
        ))
    );
    assert_eq!(
        parse_position_declaration("var(--position, sticky)"),
        Some(LocalCascadeDeclaration::Value(
            PositionDeclarationValue::CustomPropertyFallback(position, NativePositionValue::Sticky)
        ))
    );
    assert_eq!(
        parse_position_declaration("var(--position, var(--other))"),
        None
    );
    assert_eq!(
        parse_position_declaration("initial"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_position_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn position_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --position: absolute; --alias: var(--position); --cycle: var(--cycle); position: var(--position); }
            #child { position: var(--alias); }
            #fallback { position: var(--missing, fixed); }
            #invalid { --bad: unsupported; position: var(--bad, relative); }
            #cycle { position: var(--cycle, sticky); }
            #reset { --reset: initial; position: var(--reset, fixed); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='reset'>Reset</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .position()
    };

    assert_eq!(style("parent"), NativePositionValue::Absolute);
    assert_eq!(style("child"), NativePositionValue::Absolute);
    assert_eq!(style("fallback"), NativePositionValue::Fixed);
    assert_eq!(style("invalid"), NativePositionValue::Relative);
    assert_eq!(style("cycle"), NativePositionValue::Sticky);
    assert_eq!(style("reset"), NativePositionValue::Static);
}

#[test]
fn z_index_parser_accepts_custom_property_aliases_and_fallbacks() {
    let z_index = parse_custom_property_name("--z-index").unwrap();
    assert_eq!(
        parse_z_index_property("12"),
        Some(ZIndexDeclarationValue::Value(NativeZIndexValue::Integer(
            12
        )))
    );
    assert_eq!(
        parse_z_index_declaration("var(--z-index)"),
        Some(LocalCascadeDeclaration::Value(
            ZIndexDeclarationValue::CustomProperty(z_index)
        ))
    );
    assert_eq!(
        parse_z_index_declaration("var(--z-index, -4)"),
        Some(LocalCascadeDeclaration::Value(
            ZIndexDeclarationValue::CustomPropertyFallback(z_index, NativeZIndexValue::Integer(-4))
        ))
    );
    assert_eq!(
        parse_z_index_declaration("var(--z-index, var(--other))"),
        None
    );
    assert_eq!(
        parse_z_index_declaration("initial"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_z_index_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn z_index_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --z-index: 10; --alias: var(--z-index); --cycle: var(--cycle); z-index: var(--z-index); }
            #child { z-index: var(--alias); }
            #fallback { z-index: var(--missing, -4); }
            #invalid { --bad: unsupported; z-index: var(--bad, 3); }
            #cycle { z-index: var(--cycle, 7); }
            #reset { --reset: initial; z-index: var(--reset, 9); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='reset'>Reset</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .z_index()
    };

    assert_eq!(style("parent"), NativeZIndexValue::Integer(10));
    assert_eq!(style("child"), NativeZIndexValue::Integer(10));
    assert_eq!(style("fallback"), NativeZIndexValue::Integer(-4));
    assert_eq!(style("invalid"), NativeZIndexValue::Integer(3));
    assert_eq!(style("cycle"), NativeZIndexValue::Integer(7));
    assert_eq!(style("reset"), NativeZIndexValue::Auto);
}

#[test]
fn position_offset_parser_accepts_custom_property_aliases_and_fallbacks() {
    let offset = parse_custom_property_name("--offset").unwrap();
    assert_eq!(
        parse_position_offset_property("12px"),
        Some(PositionOffsetDeclarationValue::Value(
            NativePositionOffset::Length(12)
        ))
    );
    assert_eq!(
        parse_position_offset_declaration("var(--offset)"),
        Some(LocalCascadeDeclaration::Value(
            PositionOffsetDeclarationValue::CustomProperty(offset)
        ))
    );
    assert_eq!(
        parse_position_offset_declaration("var(--offset, -4px)"),
        Some(LocalCascadeDeclaration::Value(
            PositionOffsetDeclarationValue::CustomPropertyFallback(
                offset,
                NativePositionOffset::Length(-4)
            )
        ))
    );
    assert_eq!(
        parse_position_offset_declaration("var(--offset, var(--other))"),
        None
    );
    assert_eq!(
        parse_position_offset_declaration("initial"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_position_offset_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn position_offsets_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --offset: 12px; --alias: var(--offset); --cycle: var(--cycle); top: var(--offset); right: var(--alias); bottom: var(--missing, -4px); left: var(--cycle, 8px); }
            #child { top: var(--alias); }
            #invalid { --bad: unsupported; top: var(--bad, 3px); }
            #reset { --reset: initial; top: var(--reset, 9px); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='invalid'>Invalid</span>
              <span id='reset'>Reset</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        let style = document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap());
        (style.top(), style.right(), style.bottom(), style.left())
    };

    assert_eq!(
        style("parent"),
        (
            NativePositionOffset::Length(12),
            NativePositionOffset::Length(12),
            NativePositionOffset::Length(-4),
            NativePositionOffset::Length(8)
        )
    );
    assert_eq!(style("child").0, NativePositionOffset::Length(12));
    assert_eq!(style("invalid").0, NativePositionOffset::Length(3));
    assert_eq!(style("reset").0, NativePositionOffset::Auto);
}
#[test]
fn local_presentation_important_parser_tracks_markers_and_invalid_preservation() {
    let declarations = parse_declarations(
        "display: none !IMPORTANT; visibility: hidden !important; opacity: 50% !important",
    );
    assert_eq!(
        declarations.display,
        Some(LocalCascadeDeclaration::Value(
            DisplayDeclarationValue::Value(DisplayValue::None)
        ))
    );
    assert_eq!(
        declarations.visibility,
        Some(LocalCascadeDeclaration::Value(VisibilityValue::Hidden))
    );
    assert_eq!(
        declarations.opacity,
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::Value(128)
        ))
    );
    assert!(declarations.local_importance.display);
    assert!(declarations.local_importance.visibility);
    assert!(declarations.local_importance.opacity);

    let preserved = parse_declarations(
        "display: none !important; display: invalid !important; visibility: hidden !important; visibility: invalid !important; opacity: 50% !important; opacity: 2 !important",
    );
    assert_eq!(
        preserved.display,
        Some(LocalCascadeDeclaration::Value(
            DisplayDeclarationValue::Value(DisplayValue::None)
        ))
    );
    assert_eq!(
        preserved.visibility,
        Some(LocalCascadeDeclaration::Value(VisibilityValue::Hidden))
    );
    assert_eq!(
        preserved.opacity,
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::Value(128)
        ))
    );
    assert!(preserved.local_importance.display);
    assert!(preserved.local_importance.visibility);
    assert!(preserved.local_importance.opacity);

    let normal = parse_declarations(
        "display: none !important; display: block; visibility: hidden !important; visibility: visible; opacity: 50% !important; opacity: 100%",
    );
    assert_eq!(
        normal.display,
        Some(LocalCascadeDeclaration::Value(
            DisplayDeclarationValue::Value(DisplayValue::Block)
        ))
    );
    assert_eq!(
        normal.visibility,
        Some(LocalCascadeDeclaration::Value(VisibilityValue::Other))
    );
    assert_eq!(
        normal.opacity,
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::Value(u8::MAX)
        ))
    );
    assert!(!normal.local_importance.display);
    assert!(!normal.local_importance.visibility);
    assert!(!normal.local_importance.opacity);
}

#[test]
fn stylesheet_cascade_resolves_local_presentation_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #important { display:none !important; visibility:hidden !important; opacity:25% !important; } #rollback { display:revert-layer !important; visibility:revert-layer !important; opacity:revert-layer !important; } #invalid { display:none !important; visibility:hidden !important; opacity:25% !important; } #normal { display:none; visibility:hidden; opacity:25%; } } @layer theme { #important { display:block !important; visibility:visible !important; opacity:75% !important; } #rollback { display:block !important; visibility:visible !important; opacity:75% !important; } #invalid { display:unsupported !important; visibility:unsupported !important; opacity:2 !important; } #normal { display:block; visibility:visible; opacity:75%; } } @layer top { #rollback { display:none !important; visibility:hidden !important; opacity:25% !important; } } #important { display:block; visibility:visible; opacity:100%; } #rollback { display:block; visibility:visible; opacity:100%; } #invalid { display:block; visibility:visible; opacity:100%; } #normal { display:block; visibility:visible; opacity:100%; } #unlayered { display:none !important; visibility:hidden !important; opacity:25% !important; } #unlayered { display:block !important; visibility:visible !important; opacity:75% !important; } #inline-rollback { display:none !important; visibility:hidden !important; opacity:25% !important; } #inline-rollback { display:block; visibility:visible; opacity:100%; }"
                .into(),
        ])
        .unwrap();
    let important = node("<div id='important'>Important</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let normal = node("<div id='normal'>Normal</div>");
    let unlayered = node("<div id='unlayered'>Unlayered</div>");
    let inline_rollback = node(
        "<div id='inline-rollback' style='display:ReVeRt-LaYeR !important;visibility:revert-layer !important;opacity:revert-layer !important'>Inline rollback</div>",
    );

    let style_for = |target: &NativeNode| stylesheet.computed_for(target);
    let important_style = style_for(&important);
    assert_eq!(important_style.display(), DisplayValue::None);
    assert!(important_style.hidden());
    assert_eq!(important_style.opacity(), 64);

    let rollback_style = style_for(&rollback);
    assert_eq!(rollback_style.display(), DisplayValue::Block);
    assert!(!rollback_style.hidden());
    assert_eq!(rollback_style.opacity(), 191);

    let invalid_style = style_for(&invalid);
    assert_eq!(invalid_style.display(), DisplayValue::None);
    assert!(invalid_style.hidden());
    assert_eq!(invalid_style.opacity(), 64);

    let normal_style = style_for(&normal);
    assert_eq!(normal_style.display(), DisplayValue::Block);
    assert!(!normal_style.hidden());
    assert_eq!(normal_style.opacity(), u8::MAX);

    let unlayered_style = style_for(&unlayered);
    assert_eq!(unlayered_style.display(), DisplayValue::Block);
    assert!(!unlayered_style.hidden());
    assert_eq!(unlayered_style.opacity(), 191);

    let inline_rollback_style = style_for(&inline_rollback);
    assert_eq!(inline_rollback_style.display(), DisplayValue::Block);
    assert!(!inline_rollback_style.hidden());
    assert_eq!(inline_rollback_style.opacity(), u8::MAX);
}
