use super::*;

#[test]
fn declarations_parse_only_supported_presentation_properties() {
    let declarations = parse_declarations(
        "color: red; display: none !important; grid-template-columns: repeat(2, 1fr); grid-template-rows: 40px auto; visibility: visible; opacity: 50%; white-space: pre-line; text-align: center; text-align-last: end; text-justify: inter-word; justify-content: space-between; align-items: flex-end; align-self: center; align-content: stretch; flex-direction: row-reverse; direction: RTL; flex-wrap: wrap-reverse; order: -12; flex-grow: 2; flex-shrink: 3; flex-basis: 40px; text-decoration: underline; text-decoration-style: dotted; text-decoration-thickness: 2px; text-underline-offset: -2px; text-indent: 12px; word-spacing: 12px; letter-spacing: 12px; gap: 12px 14px; row-gap: 13px; column-gap: 15px; font-weight: bold; font-style: italic; word-break: break-all; text-overflow: ellipsis; vertical-align: bottom; width: 240px; height: 30px; min-width: 12px; max-width: 400px; min-height: 14px; max-height: 500px; line-height: 28px; border: 2px solid #102030; border-radius: 1px 2px 3px 4px; padding: 4px; margin: 3px; box-sizing: border-box; overflow: hidden",
    );
    assert_eq!(
        declarations.display,
        Some(LocalCascadeDeclaration::Value(
            DisplayDeclarationValue::Value(DisplayValue::None)
        ))
    );
    let expected_grid_columns = {
        let mut tracks = NativeGridTrackList::default();
        tracks.push(NativeGridTrack::Fr(1));
        tracks.push(NativeGridTrack::Fr(1));
        tracks
    };
    let expected_grid_rows = {
        let mut tracks = NativeGridTrackList::default();
        tracks.push(NativeGridTrack::Length(40));
        tracks.push(NativeGridTrack::Auto);
        tracks
    };
    assert_eq!(
        declarations.grid_template_columns,
        Some(LocalCascadeDeclaration::Value(
            GridTrackListDeclaration::Value(expected_grid_columns)
        ))
    );
    assert_eq!(
        declarations.grid_template_rows,
        Some(LocalCascadeDeclaration::Value(
            GridTrackListDeclaration::Value(expected_grid_rows)
        ))
    );
    assert_eq!(
        declarations.visibility,
        Some(LocalCascadeDeclaration::Value(VisibilityValue::Other))
    );
    assert_eq!(
        declarations.opacity,
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::Value(128)
        ))
    );
    assert_eq!(
        declarations.white_space,
        Some(InheritedTextDeclaration::Value(
            WhiteSpaceDeclarationValue::Value(WhiteSpaceValue::PreLine)
        ))
    );
    assert_eq!(
        declarations.text_align,
        Some(InheritedTextDeclaration::Value(
            TextAlignDeclarationValue::Value(TextAlignValue::Center)
        ))
    );
    assert_eq!(
        declarations.text_align_last,
        Some(InheritedTextDeclaration::Value(
            TextAlignLastDeclarationValue::Value(TextAlignLastValue::End)
        ))
    );
    assert_eq!(
        declarations.text_justify,
        Some(InheritedTextDeclaration::Value(
            TextJustifyDeclarationValue::Value(TextJustifyValue::InterWord)
        ))
    );
    assert_eq!(
        declarations.justify_content,
        Some(JustifyContentDeclaration::Value(
            JustifyContentValue::SpaceBetween
        ))
    );
    assert_eq!(
        declarations.align_items,
        Some(AlignItemsDeclaration::Value(AlignItemsValue::FlexEnd))
    );
    assert_eq!(
        declarations.align_self,
        Some(AlignSelfDeclaration::Value(AlignSelfValue::Center))
    );
    assert_eq!(
        declarations.align_content,
        Some(AlignContentDeclaration::Value(AlignContentValue::Stretch))
    );
    assert_eq!(
        declarations.flex_direction,
        Some(FlexDirectionDeclaration::Value(
            FlexDirectionValue::RowReverse
        ))
    );
    assert_eq!(
        declarations.direction,
        Some(InheritedTextDeclaration::Value(
            DirectionDeclarationValue::Value(DirectionValue::Rtl)
        ))
    );
    assert_eq!(
        declarations.flex_wrap,
        Some(FlexWrapDeclaration::Value(FlexWrapValue::WrapReverse))
    );
    assert_eq!(
        declarations.order,
        Some(FlexItemOrderDeclaration::Value(NativeOrderValue(-12)))
    );
    assert_eq!(declarations.flex_grow, Some(FlexGrowDeclaration::Value(2)));
    assert_eq!(
        declarations.flex_shrink,
        Some(FlexShrinkDeclaration::Value(3))
    );
    assert_eq!(
        declarations.flex_basis,
        Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(40)))
    );
    assert_eq!(
        declarations.text_decoration,
        Some(NativeTextDecorationDeclaration::Value(
            TextDecorationValue::new(true, false, false)
        ))
    );
    assert_eq!(
        declarations.text_decoration_style,
        Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Dotted
        ))
    );
    assert_eq!(
        declarations.text_decoration_thickness,
        Some(NativeTextDecorationThicknessDeclaration::Value(2))
    );
    assert_eq!(
        declarations.text_underline_offset,
        Some(NativeTextUnderlineOffsetDeclaration::Value(-2))
    );
    assert_eq!(
        declarations.text_indent,
        Some(LocalCascadeDeclaration::Value(
            TextIndentDeclarationValue::Value(12)
        ))
    );
    assert_eq!(
        declarations.word_spacing,
        Some(InheritedTextDeclaration::Value(
            WordSpacingDeclarationValue::Value(12)
        ))
    );
    assert_eq!(
        declarations.letter_spacing,
        Some(InheritedTextDeclaration::Value(
            LetterSpacingDeclarationValue::Value(12)
        ))
    );
    assert_eq!(
        declarations.gap,
        Some(GapShorthandDeclaration::Value(NativeGapValue {
            row: 12,
            column: 14,
        }))
    );
    assert_eq!(
        declarations.row_gap,
        Some(GapComponentDeclaration::Value(13))
    );
    assert_eq!(
        declarations.column_gap,
        Some(GapComponentDeclaration::Value(15))
    );
    assert_eq!(
        declarations.font_weight,
        Some(InheritedTextDeclaration::Value(
            FontWeightDeclarationValue::Absolute(FontWeightValue::Bold),
        ))
    );
    assert_eq!(
        declarations.font_style,
        Some(InheritedTextDeclaration::Value(
            FontStyleDeclarationValue::Italic
        ))
    );
    assert_eq!(
        declarations.word_break,
        Some(InheritedTextDeclaration::Value(
            WordBreakDeclarationValue::Value(WordBreakValue::BreakAll)
        ))
    );
    assert_eq!(
        declarations.text_overflow,
        Some(LocalCascadeDeclaration::Value(
            TextOverflowDeclarationValue::Value(TextOverflowValue::Ellipsis)
        ))
    );
    assert_eq!(
        declarations.vertical_align,
        Some(InheritedTextDeclaration::Value(
            VerticalAlignDeclarationValue::Value(VerticalAlignValue::Bottom)
        ))
    );
    assert_eq!(
        declarations.width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(240)
        ))
    );
    assert_eq!(
        declarations.height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(30)
        ))
    );
    assert_eq!(
        declarations.min_width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(12)
        ))
    );
    assert_eq!(
        declarations.max_width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(400)
        ))
    );
    assert_eq!(
        declarations.min_height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(14)
        ))
    );
    assert_eq!(
        declarations.max_height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(500)
        ))
    );
    assert_eq!(
        declarations.line_height,
        Some(InheritedTextDeclaration::Value(
            LineHeightDeclarationValue::Value(28)
        ))
    );
    assert_eq!(
        declarations.color,
        Some(LocalCascadeDeclaration::Value(NativeColorValue::Color(
            NativeColor::RED
        )))
    );
    let parsed_color = NativeColor {
        red: 16,
        green: 32,
        blue: 48,
        alpha: 255,
    };
    assert_eq!(
        declarations.border,
        [Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Complete(border_side(2, parsed_color)),
        )); 4]
    );
    assert_eq!(
        declarations.border_radius,
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Radius(NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 3,
                bottom_left: 4,
            }),
        ))
    );
    assert_eq!(
        declarations.padding,
        [Some(LocalCascadeDeclaration::Value(
            PaddingDeclarationValue::Value(4)
        )); 4]
    );
    assert_eq!(
        declarations.margin,
        [Some(LocalCascadeDeclaration::Value(
            MarginDeclarationValue::Value(NativeMarginValue::Length(3))
        )); 4]
    );
    assert_eq!(
        declarations.box_sizing,
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::BorderBox)
        ))
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
            OverflowDeclarationValue::Value(OverflowValue::Hidden)
        ))
    );
    assert_eq!(
        declarations.overflow_y,
        Some(LocalCascadeDeclaration::Value(
            OverflowDeclarationValue::Value(OverflowValue::Hidden)
        ))
    );
    assert_eq!(parse_white_space("normal"), Some(WhiteSpaceValue::Normal));
    assert_eq!(
        parse_white_space("pre-line"),
        Some(WhiteSpaceValue::PreLine)
    );
    assert_eq!(parse_white_space("pre"), Some(WhiteSpaceValue::Pre));
    assert_eq!(
        parse_white_space("pre-wrap"),
        Some(WhiteSpaceValue::PreWrap)
    );
    assert_eq!(parse_white_space("nowrap"), Some(WhiteSpaceValue::NoWrap));
    assert_eq!(parse_display("flex"), Some(DisplayValue::Flex));
    assert_eq!(parse_display("FLEX"), Some(DisplayValue::Flex));
    assert_eq!(parse_display("grid"), Some(DisplayValue::Grid));
    assert_eq!(parse_overflow("scroll"), Some(OverflowValue::Scroll));
    assert_eq!(parse_overflow("visible"), Some(OverflowValue::Other));
    assert_eq!(parse_overflow("auto"), Some(OverflowValue::Auto));
    assert_eq!(parse_overflow("clip"), Some(OverflowValue::Clip));
    assert_eq!(
        parse_border("1px dashed red"),
        Some(styled_border_side(
            1,
            NativeBorderStyle::Dashed,
            NativeColor::RED
        ))
    );
    assert_eq!(
        parse_border("2px DOTTED blue"),
        Some(styled_border_side(
            2,
            NativeBorderStyle::Dotted,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            }
        ))
    );
    let unsupported_sides = parse_declarations(
        "border-top: 1px wavy red; border-right: -1px solid blue; border-bottom: 20000px solid red; border-left: 1em solid green",
    );
    assert_eq!(unsupported_sides.border, [None; 4]);
    assert_eq!(
        parse_border("1px solid rgb(1, 2, 3)"),
        Some(border_side(
            1,
            NativeColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            },
        ))
    );
}
#[test]
fn grid_track_list_parser_accepts_custom_property_aliases_and_fallbacks() {
    let tracks = parse_custom_property_name("--tracks").unwrap();
    let mut fallback = NativeGridTrackList::default();
    fallback.push(NativeGridTrack::Length(12));
    fallback.push(NativeGridTrack::Fr(1));
    fallback.push(NativeGridTrack::Fr(1));

    assert_eq!(
        parse_grid_track_list_declaration("var(--tracks)"),
        Some(LocalCascadeDeclaration::Value(
            GridTrackListDeclaration::CustomProperty(tracks)
        ))
    );
    assert_eq!(
        parse_grid_track_list_declaration("VAR(--tracks, 12px repeat(2, 1fr))"),
        Some(LocalCascadeDeclaration::Value(
            GridTrackListDeclaration::CustomPropertyFallback(tracks, fallback)
        ))
    );
    assert!(parse_grid_track_list_declaration("var(--tracks, var(--other))").is_none());
    assert!(parse_grid_track_list_declaration("var(--tracks, inherit)").is_none());
}

#[test]
fn grid_track_list_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent {
                --columns: 120px repeat(2, 1fr);
                --rows: 40px auto;
                grid-template-columns: var(--columns);
                grid-template-rows: var(--rows);
            }
            #alias {
                --columns-alias: var(--columns);
                --rows-alias: var(--rows);
                grid-template-columns: var(--columns-alias);
                grid-template-rows: var(--rows-alias);
            }
            #fallback {
                grid-template-columns: var(--missing, 80px 2fr);
                grid-template-rows: var(--missing-rows, repeat(2, 24px));
            }
            #invalid {
                --bad: nonsense;
                grid-template-columns: var(--bad, 10px auto);
                grid-template-rows: var(--bad-rows);
            }
            #cycle {
                --cycle: var(--cycle);
                grid-template-columns: var(--cycle, 12px);
                grid-template-rows: var(--cycle-rows, none);
            }
            #ordered {
                grid-template-columns: 10px;
                grid-template-columns: var(--missing, 20px 30px);
                grid-template-rows: 10px;
                grid-template-rows: var(--missing-rows, 30px);
            }
            </style>
            <div id='parent'>
                <div id='alias'>Alias</div>
            </div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='ordered'>Ordered</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();

    let mut expected_parent_columns = NativeGridTrackList::default();
    expected_parent_columns.push(NativeGridTrack::Length(120));
    expected_parent_columns.push(NativeGridTrack::Fr(1));
    expected_parent_columns.push(NativeGridTrack::Fr(1));
    let mut expected_parent_rows = NativeGridTrackList::default();
    expected_parent_rows.push(NativeGridTrack::Length(40));
    expected_parent_rows.push(NativeGridTrack::Auto);
    let mut expected_fallback_columns = NativeGridTrackList::default();
    expected_fallback_columns.push(NativeGridTrack::Length(80));
    expected_fallback_columns.push(NativeGridTrack::Fr(2));
    let mut expected_fallback_rows = NativeGridTrackList::default();
    expected_fallback_rows.push(NativeGridTrack::Length(24));
    expected_fallback_rows.push(NativeGridTrack::Length(24));
    let mut expected_invalid_columns = NativeGridTrackList::default();
    expected_invalid_columns.push(NativeGridTrack::Length(10));
    expected_invalid_columns.push(NativeGridTrack::Auto);
    let mut expected_cycle_columns = NativeGridTrackList::default();
    expected_cycle_columns.push(NativeGridTrack::Length(12));
    let mut expected_ordered_columns = NativeGridTrackList::default();
    expected_ordered_columns.push(NativeGridTrack::Length(20));
    expected_ordered_columns.push(NativeGridTrack::Length(30));
    let mut expected_ordered_rows = NativeGridTrackList::default();
    expected_ordered_rows.push(NativeGridTrack::Length(30));

    for (target, columns, rows) in [
        ("id=parent", expected_parent_columns, expected_parent_rows),
        ("id=alias", expected_parent_columns, expected_parent_rows),
        (
            "id=fallback",
            expected_fallback_columns,
            expected_fallback_rows,
        ),
        (
            "id=invalid",
            expected_invalid_columns,
            NativeGridTrackList::default(),
        ),
        (
            "id=cycle",
            expected_cycle_columns,
            NativeGridTrackList::default(),
        ),
        (
            "id=ordered",
            expected_ordered_columns,
            expected_ordered_rows,
        ),
    ] {
        let node = document.resolve_target(target).unwrap();
        let style = document.computed_style_for_layout(node);
        assert_eq!(style.grid_template_columns(), columns, "target={target}");
        assert_eq!(style.grid_template_rows(), rows, "target={target}");
    }
}

#[test]
fn functional_alpha_color_parser_accepts_bounded_rgba_values() {
    assert_eq!(
        parse_color("RGBA( 16, 32, 48, 0.5 )"),
        Some(NativeColor {
            red: 16,
            green: 32,
            blue: 48,
            alpha: 128,
        })
    );
    assert_eq!(
        parse_color("rgba(255,0,1,50%)"),
        Some(NativeColor {
            red: 255,
            green: 0,
            blue: 1,
            alpha: 128,
        })
    );
    assert_eq!(
        parse_color("rgba(0, 0, 0, 0)"),
        Some(NativeColor {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        })
    );
    assert_eq!(parse_color("rgba(256, 0, 0, 1)"), None);
    assert_eq!(parse_color("rgba(0, -1, 0, 1)"), None);
    assert_eq!(parse_color("rgba(0, 0, 0, 1.001)"), None);
    assert_eq!(parse_color("rgba(0, 0, 0, 101%)"), None);
    assert_eq!(parse_color("rgba(0, 0, 0, 1, 0)"), None);
    assert_eq!(parse_color("rgba(0%, 0, 0, 1)"), None);
    assert_eq!(parse_color("rgb(0 0 0 / 0.5)"), None);
}

#[test]
fn local_paint_color_parser_accepts_css_wide_keywords_and_preserves_valid_values() {
    assert_eq!(
        parse_local_color_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_local_color_declaration("rgba(1, 2, 3, 0.5)"),
        Some(LocalCascadeDeclaration::Value(NativeColorValue::Color(
            NativeColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 128,
            }
        )))
    );
    for value in [
        "revert-layer red",
        "red revert-layer",
        "linear-gradient(red, blue)",
        "color(display-p3 1 0 0)",
    ] {
        assert_eq!(parse_local_color_declaration(value), None, "value={value}");
    }
    assert_eq!(
        parse_local_color_declaration("CuRrEnTcOlOr"),
        Some(LocalCascadeDeclaration::Value(
            NativeColorValue::CurrentColor
        ))
    );
    for (value, expected) in [
        ("InHeRiT", NativeColorValue::Inherit),
        ("UnSeT", NativeColorValue::Unset),
        ("InItIaL", NativeColorValue::Initial),
        ("ReVeRt", NativeColorValue::Revert),
    ] {
        assert_eq!(
            parse_local_color_declaration(value),
            Some(LocalCascadeDeclaration::Value(expected)),
            "value={value}"
        );
    }

    assert_eq!(
        parse_local_color_declaration("var(--accent)"),
        Some(LocalCascadeDeclaration::Value(
            NativeColorValue::CustomProperty(custom_property_hash("--accent"))
        ))
    );
    assert_eq!(
        parse_local_color_declaration("VAR(--accent, red)"),
        Some(LocalCascadeDeclaration::Value(
            NativeColorValue::CustomPropertyFallback(
                custom_property_hash("--accent"),
                NativeColor::RED
            )
        ))
    );
    assert_eq!(
        parse_local_color_declaration("var(--accent, var(--other))"),
        None
    );
    assert_eq!(parse_local_color_declaration("var(accent)"), None);

    let document = NativeDocument::parse(
            "<style>#parent { color:green; } #current { color:currentColor; } #inherit { color:inherit; } #unset { color:unset; } #revert { color:revert; } #initial { color:initial; } #root { color:currentColor; }</style><div id='parent'><span id='current'>Current</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='initial'>Initial</span></div><div id='root'>Root</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let current = document.resolve_target("id=current").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let unset = document.resolve_target("id=unset").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let root = document.resolve_target("id=root").unwrap();
    let omitted_document = NativeDocument::parse(
        "<div id='omitted'>Omitted</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let omitted = omitted_document.resolve_target("id=omitted").unwrap();
    assert_eq!(
        document.computed_style_for_layout(parent).color(),
        Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        })
    );
    assert_eq!(
        document.computed_style_for_layout(current).color(),
        Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        })
    );
    for node_id in [inherit, unset, revert] {
        assert_eq!(
            document.computed_style_for_layout(node_id).color(),
            Some(NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: u8::MAX,
            })
        );
    }
    assert_eq!(
        document.computed_style_for_layout(initial).color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(
        document.computed_style_for_layout(root).color(),
        Some(NativeColor::BLACK)
    );
    assert_eq!(
        NativeStylesheet::default()
            .computed_for(omitted_document.node(omitted).unwrap())
            .color(),
        None
    );

    let declarations = parse_declarations(
        "background-color: red; background-color: currentColor; color: blue; color: inherit",
    );
    assert_eq!(
        declarations.background_color,
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::CurrentColor
        ))
    );
    assert_eq!(
        declarations.color,
        Some(LocalCascadeDeclaration::Value(NativeColorValue::Inherit))
    );
    assert_eq!(
        parse_declarations("background-color: revert-layer").background_color,
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_background_color_declaration("CuRrEnTcOlOr"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::CurrentColor
        ))
    );
    assert_eq!(
        parse_declarations("color: REVERT-LAYER").color,
        Some(LocalCascadeDeclaration::RevertLayer)
    );
}

#[test]
fn background_shorthand_parser_accepts_custom_property_aliases_and_fallbacks() {
    let background = parse_custom_property_name("--background").unwrap();
    let fallback = parse_background_shorthand(
        "red url('https://example.test/background.png') no-repeat right bottom / cover",
    )
    .unwrap();
    assert_eq!(
        parse_background_shorthand_declaration("var(--background)"),
        Some(NativeBackgroundShorthandDeclaration::CustomProperty(
            background
        ))
    );
    assert_eq!(
        parse_background_shorthand_declaration(
            "VAR(--background, red url('https://example.test/background.png') no-repeat right bottom / cover)"
        ),
        Some(NativeBackgroundShorthandDeclaration::CustomPropertyFallback(background, fallback))
    );
    assert_eq!(
        parse_background_shorthand_declaration("var(--background, var(--other))"),
        None
    );
    assert_eq!(
        parse_background_shorthand_declaration("var(--background, inherit)"),
        None
    );
}

#[test]
fn background_image_parser_accepts_custom_property_aliases_and_fallbacks() {
    let image = parse_custom_property_name("--image").unwrap();
    let source = background_image_source_id("https://example.test/image.png");
    assert_eq!(
        parse_background_image_declaration("var(--image)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundImageValue::CustomProperty(image)
        ))
    );
    assert_eq!(
        parse_background_image_declaration("VAR(--image, url('https://example.test/image.png'))"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundImageValue::CustomPropertyFallback(image, Some(source))
        ))
    );
    assert_eq!(
        parse_background_image_declaration("var(--image, none)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundImageValue::CustomPropertyFallback(image, None)
        ))
    );
    assert_eq!(
        parse_background_image_declaration("var(--image, var(--other))"),
        None
    );
    assert_eq!(
        parse_background_image_declaration("var(--image, inherit)"),
        None
    );
}

#[test]
fn background_size_parser_accepts_custom_property_aliases_and_fallbacks() {
    let size = parse_custom_property_name("--size").unwrap();
    let fallback = NativeBackgroundSize::Explicit {
        width: NativeBackgroundSizeComponent::Percentage(500),
        height: NativeBackgroundSizeComponent::Length(24),
    };
    assert_eq!(
        parse_background_size_declaration("var(--size)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundSizeDeclaration::CustomProperty(size)
        ))
    );
    assert_eq!(
        parse_background_size_declaration("VAR(--size, 50% 24px)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundSizeDeclaration::CustomPropertyFallback(size, fallback)
        ))
    );
    assert_eq!(
        parse_background_size_declaration("var(--size, cover)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundSizeDeclaration::CustomPropertyFallback(
                size,
                NativeBackgroundSize::Cover
            )
        ))
    );
    assert_eq!(
        parse_background_size_declaration("var(--size, var(--other))"),
        None
    );
    assert_eq!(
        parse_background_size_declaration("var(--size, inherit)"),
        None
    );
}

#[test]
fn background_position_parser_accepts_custom_property_aliases_and_fallbacks() {
    let position = parse_custom_property_name("--position").unwrap();
    let fallback = NativeBackgroundPosition {
        x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
        y: NativeBackgroundPositionComponent::Percentage(0),
    };
    assert_eq!(
        parse_background_position_declaration("var(--position)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundPositionDeclaration::CustomProperty(position)
        ))
    );
    assert_eq!(
        parse_background_position_declaration("VAR(--position, right top)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundPositionDeclaration::CustomPropertyFallback(position, fallback)
        ))
    );
    assert_eq!(
        parse_background_position_declaration("var(--position, var(--other))"),
        None
    );
    assert_eq!(
        parse_background_position_declaration("var(--position, inherit)"),
        None
    );
}

#[test]
fn background_repeat_parser_accepts_custom_property_aliases_and_fallbacks() {
    let repeat = parse_custom_property_name("--repeat").unwrap();
    assert_eq!(
        parse_background_repeat_declaration("var(--repeat)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundRepeat::CustomProperty(repeat)
        ))
    );
    assert_eq!(
        parse_background_repeat_declaration("VAR(--repeat, repeat-x)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundRepeat::CustomPropertyFallback(
                repeat,
                NativeBackgroundRepeatFallback::RepeatX
            )
        ))
    );
    assert_eq!(
        parse_background_repeat_declaration("var(--repeat, no-repeat repeat)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundRepeat::CustomPropertyFallback(
                repeat,
                NativeBackgroundRepeatFallback::RepeatY
            )
        ))
    );
    assert_eq!(
        parse_background_repeat_declaration("var(--repeat, var(--other))"),
        None
    );
    assert_eq!(
        parse_background_repeat_declaration("var(--repeat, inherit)"),
        None
    );
}

#[test]
fn inherited_color_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            "<style>#parent { --accent: rgb(1, 2, 3); --alias: var(--accent); --cycle: var(--cycle); color: var(--accent); } #child { color: var(--alias); } #fallback { color: var(--missing, red); } #invalid { --bad: nonsense; color: var(--bad, white); } #cycle { color: var(--cycle, white); }</style><div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let cycle = document.resolve_target("id=cycle").unwrap();
    let accent = NativeColor {
        red: 1,
        green: 2,
        blue: 3,
        alpha: u8::MAX,
    };

    assert_eq!(
        document.computed_style_for_layout(parent).color(),
        Some(accent)
    );
    assert_eq!(
        document.computed_style_for_layout(child).color(),
        Some(accent)
    );
    assert_eq!(
        document.computed_style_for_layout(fallback).color(),
        Some(NativeColor::RED)
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).color(),
        Some(NativeColor::WHITE)
    );
    assert_eq!(
        document.computed_style_for_layout(cycle).color(),
        Some(NativeColor::WHITE)
    );
}

#[test]
fn background_image_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --image: url('https://example.test/parent.png'); background-image: var(--image); }
            #child { --alias: var(--image); background-image: var(--alias); }
            #fallback { background-image: var(--missing, url('https://example.test/fallback.png')); }
            #invalid { --bad: nonsense; background-image: var(--bad, url('https://example.test/invalid.png')); }
            #cycle { --cycle: var(--cycle); background-image: var(--cycle, none); }
            #none { --none: none; background-image: var(--none, url('https://example.test/ignored.png')); }
            #ordered { background-image: url('https://example.test/first.png'); background-image: var(--missing, url('https://example.test/last.png')); }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span><span id='none'>None</span><span id='ordered'>Ordered</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let cycle = document.resolve_target("id=cycle").unwrap();
    let none = document.resolve_target("id=none").unwrap();
    let ordered = document.resolve_target("id=ordered").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .background_image(),
        Some(background_image_source_id(
            "https://example.test/parent.png"
        ))
    );
    assert_eq!(
        document.computed_style_for_layout(child).background_image(),
        Some(background_image_source_id(
            "https://example.test/parent.png"
        ))
    );
    assert_eq!(
        document
            .computed_style_for_layout(fallback)
            .background_image(),
        Some(background_image_source_id(
            "https://example.test/fallback.png"
        ))
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .background_image(),
        Some(background_image_source_id(
            "https://example.test/invalid.png"
        ))
    );
    assert_eq!(
        document.computed_style_for_layout(cycle).background_image(),
        None
    );
    assert_eq!(
        document.computed_style_for_layout(none).background_image(),
        None
    );
    assert_eq!(
        document
            .computed_style_for_layout(ordered)
            .background_image(),
        Some(background_image_source_id("https://example.test/last.png"))
    );
}

#[test]
fn background_repeat_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --repeat: no-repeat; background-repeat: var(--repeat); }
            #child { --alias: var(--repeat); background-repeat: var(--alias); }
            #fallback { background-repeat: var(--missing, repeat-x); }
            #invalid { --bad: nonsense; background-repeat: var(--bad, no-repeat repeat); }
            #cycle { --cycle: var(--cycle); background-repeat: var(--cycle, repeat-y); }
            #explicit { --explicit: repeat-x; background-repeat: var(--explicit, no-repeat); }
            #ordered { background-repeat: repeat; background-repeat: var(--missing, no-repeat); }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span><span id='explicit'>Explicit</span><span id='ordered'>Ordered</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let cycle = document.resolve_target("id=cycle").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let ordered = document.resolve_target("id=ordered").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .background_repeat(),
        NativeBackgroundRepeat::NoRepeat
    );
    assert_eq!(
        document
            .computed_style_for_layout(child)
            .background_repeat(),
        NativeBackgroundRepeat::NoRepeat
    );
    assert_eq!(
        document
            .computed_style_for_layout(fallback)
            .background_repeat(),
        NativeBackgroundRepeat::RepeatX
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .background_repeat(),
        NativeBackgroundRepeat::RepeatY
    );
    assert_eq!(
        document
            .computed_style_for_layout(cycle)
            .background_repeat(),
        NativeBackgroundRepeat::RepeatY
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .background_repeat(),
        NativeBackgroundRepeat::RepeatX
    );
    assert_eq!(
        document
            .computed_style_for_layout(ordered)
            .background_repeat(),
        NativeBackgroundRepeat::NoRepeat
    );
}

#[test]
fn background_position_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --position: right bottom; background-position: var(--position); }
            #child { --alias: var(--position); background-position: var(--alias); }
            #fallback { background-position: var(--missing, left top); }
            #invalid { --bad: nonsense; background-position: var(--bad, center 75%); }
            #cycle { --cycle: var(--cycle); background-position: var(--cycle, right center); }
            #explicit { --explicit: 10px 20px; background-position: var(--explicit, left top); }
            #ordered { background-position: left top; background-position: var(--missing, right bottom); }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span><span id='explicit'>Explicit</span><span id='ordered'>Ordered</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let cycle = document.resolve_target("id=cycle").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let ordered = document.resolve_target("id=ordered").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
            y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(child)
            .background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
            y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(fallback)
            .background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(0),
            y: NativeBackgroundPositionComponent::Percentage(0),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE / 2),
            y: NativeBackgroundPositionComponent::Percentage(750),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(cycle)
            .background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
            y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE / 2),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Length(10),
            y: NativeBackgroundPositionComponent::Length(20),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(ordered)
            .background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
            y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
        }
    );
}

#[test]
fn background_size_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --size: cover; background-size: var(--size); }
            #child { --alias: var(--size); background-size: var(--alias); }
            #fallback { background-size: var(--missing, 50% 24px); }
            #invalid { --bad: nonsense; background-size: var(--bad, contain); }
            #cycle { --cycle: var(--cycle); background-size: var(--cycle, 12px 18px); }
            #explicit { --explicit: 25% auto; background-size: var(--explicit, cover); }
            #ordered { background-size: contain; background-size: var(--missing, 10px 20px); }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span><span id='explicit'>Explicit</span><span id='ordered'>Ordered</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let cycle = document.resolve_target("id=cycle").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let ordered = document.resolve_target("id=ordered").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).background_size(),
        NativeBackgroundSize::Cover
    );
    assert_eq!(
        document.computed_style_for_layout(child).background_size(),
        NativeBackgroundSize::Cover
    );
    assert_eq!(
        document
            .computed_style_for_layout(fallback)
            .background_size(),
        NativeBackgroundSize::Explicit {
            width: NativeBackgroundSizeComponent::Percentage(500),
            height: NativeBackgroundSizeComponent::Length(24),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .background_size(),
        NativeBackgroundSize::Contain
    );
    assert_eq!(
        document.computed_style_for_layout(cycle).background_size(),
        NativeBackgroundSize::Explicit {
            width: NativeBackgroundSizeComponent::Length(12),
            height: NativeBackgroundSizeComponent::Length(18),
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .background_size(),
        NativeBackgroundSize::Explicit {
            width: NativeBackgroundSizeComponent::Percentage(250),
            height: NativeBackgroundSizeComponent::Auto,
        }
    );
    assert_eq!(
        document
            .computed_style_for_layout(ordered)
            .background_size(),
        NativeBackgroundSize::Explicit {
            width: NativeBackgroundSizeComponent::Length(10),
            height: NativeBackgroundSizeComponent::Length(20),
        }
    );
}

#[test]
fn background_shorthand_custom_properties_resolve_all_components() {
    let parent = node("<div id='parent'>Parent</div>");
    let child = node("<div id='child'>Child</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let cycle = node("<div id='cycle'>Cycle</div>");
    let ordered = node("<div id='ordered'>Ordered</div>");
    let parent_css = "#parent { --background: rgb(1, 2, 3) url('https://example.test/parent.png') no-repeat right bottom / cover; background: var(--background); }";
    let child_css = "#child { --background: rgb(1, 2, 3) url('https://example.test/parent.png') no-repeat right bottom / cover; --alias: var(--background); background: var(--alias); }";
    let fallback_css = "#fallback { background: var(--missing, red url('https://example.test/fallback.png') repeat-x left top / 50% 24px); }";
    let invalid_css =
        "#invalid { --bad: nonsense; background: var(--bad, red no-repeat center / contain); }";
    let cycle_css = "#cycle { --cycle: var(--cycle); background: var(--cycle, red repeat-y right center / 12px 18px); }";
    let ordered_css =
        "#ordered { background: red; background: var(--missing, red no-repeat center / cover); }";
    let style_for = |target: &NativeNode, source: &str| {
        NativeStylesheet::from_sources(vec![source.to_owned()])
            .expect("shorthand fixture stylesheet")
            .computed_for(target)
    };
    let accent = NativeColor {
        red: 1,
        green: 2,
        blue: 3,
        alpha: u8::MAX,
    };

    for (target, source) in [(&parent, parent_css), (&child, child_css)] {
        let style = style_for(target, source);
        assert_eq!(
            style.background_color(),
            Some(accent),
            "background source: {source}"
        );
        assert_eq!(
            style.background_image(),
            Some(background_image_source_id(
                "https://example.test/parent.png"
            ))
        );
        assert_eq!(style.background_repeat(), NativeBackgroundRepeat::NoRepeat);
        assert_eq!(
            style.background_position(),
            NativeBackgroundPosition {
                x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
                y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
            }
        );
        assert_eq!(style.background_size(), NativeBackgroundSize::Cover);
    }

    let fallback_style = style_for(&fallback, fallback_css);
    assert_eq!(fallback_style.background_color(), Some(NativeColor::RED));
    assert_eq!(
        fallback_style.background_image(),
        Some(background_image_source_id(
            "https://example.test/fallback.png"
        ))
    );
    assert_eq!(
        fallback_style.background_repeat(),
        NativeBackgroundRepeat::RepeatX
    );
    assert_eq!(
        fallback_style.background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(0),
            y: NativeBackgroundPositionComponent::Percentage(0),
        }
    );
    assert_eq!(
        fallback_style.background_size(),
        NativeBackgroundSize::Explicit {
            width: NativeBackgroundSizeComponent::Percentage(500),
            height: NativeBackgroundSizeComponent::Length(24),
        }
    );
    let invalid_style = style_for(&invalid, invalid_css);
    assert_eq!(invalid_style.background_color(), Some(NativeColor::RED));
    assert_eq!(invalid_style.background_image(), None);
    assert_eq!(
        invalid_style.background_repeat(),
        NativeBackgroundRepeat::NoRepeat
    );
    assert_eq!(
        invalid_style.background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE / 2),
            y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE / 2),
        }
    );
    assert_eq!(
        invalid_style.background_size(),
        NativeBackgroundSize::Contain
    );
    let cycle_style = style_for(&cycle, cycle_css);
    assert_eq!(cycle_style.background_color(), Some(NativeColor::RED));
    assert_eq!(cycle_style.background_image(), None);
    assert_eq!(
        cycle_style.background_repeat(),
        NativeBackgroundRepeat::RepeatY
    );
    assert_eq!(
        cycle_style.background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
            y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE / 2),
        }
    );
    assert_eq!(
        cycle_style.background_size(),
        NativeBackgroundSize::Explicit {
            width: NativeBackgroundSizeComponent::Length(12),
            height: NativeBackgroundSizeComponent::Length(18),
        }
    );
    let ordered_style = style_for(&ordered, ordered_css);
    assert_eq!(ordered_style.background_color(), Some(NativeColor::RED));
    assert_eq!(
        ordered_style.background_repeat(),
        NativeBackgroundRepeat::NoRepeat
    );
    assert_eq!(ordered_style.background_size(), NativeBackgroundSize::Cover);
    let inherited_document = NativeDocument::parse(
            r#"<style>#parent { --background: rgb(1, 2, 3) url('https://example.test/inherited.png') no-repeat right bottom / cover; } #child { --alias: var(--background); background: var(--alias); }</style><div id='parent'><span id='child'>Child</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let inherited_child = inherited_document.resolve_target("id=child").unwrap();
    let inherited_style = inherited_document.computed_style_for_layout(inherited_child);
    assert_eq!(inherited_style.background_color(), Some(accent));
    assert_eq!(
        inherited_style.background_image(),
        Some(background_image_source_id(
            "https://example.test/inherited.png"
        ))
    );
    assert_eq!(
        inherited_style.background_repeat(),
        NativeBackgroundRepeat::NoRepeat
    );
    assert_eq!(
        inherited_style.background_position(),
        NativeBackgroundPosition {
            x: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
            y: NativeBackgroundPositionComponent::Percentage(NATIVE_BACKGROUND_PERCENT_SCALE),
        }
    );
    assert_eq!(
        inherited_style.background_size(),
        NativeBackgroundSize::Cover
    );
}

#[test]
fn border_radius_parser_expands_bounded_physical_shorthand() {
    assert_eq!(
        parse_border_radius("5px"),
        Some(NativeBorderRadius {
            top_left: 5,
            top_right: 5,
            bottom_right: 5,
            bottom_left: 5,
        })
    );
    assert_eq!(
        parse_border_radius("1px 2px"),
        Some(NativeBorderRadius {
            top_left: 1,
            top_right: 2,
            bottom_right: 1,
            bottom_left: 2,
        })
    );
    assert_eq!(
        parse_border_radius("1px 2px 3px"),
        Some(NativeBorderRadius {
            top_left: 1,
            top_right: 2,
            bottom_right: 3,
            bottom_left: 2,
        })
    );
    assert_eq!(parse_border_radius("1px/2px"), None);
    assert_eq!(parse_border_radius("50%"), None);
    assert_eq!(parse_border_radius("-1px"), None);
    assert_eq!(parse_border_radius("1px 2px 3px 4px 5px"), None);
    assert_eq!(parse_border_radius("20000px"), None);
    assert_eq!(
        parse_border_radius_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_border_radius_declaration("1px 2px"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Radius(NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 1,
                bottom_left: 2,
            }),
        ))
    );
    assert_eq!(parse_border_radius("inherit"), None);
    assert_eq!(
        parse_border_radius_declaration("InHeRiT"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Inherit
        ))
    );
    assert_eq!(
        parse_border_radius_declaration("UNSET"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Unset
        ))
    );
    assert_eq!(
        parse_border_radius_declaration("initial"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Initial
        ))
    );
    assert_eq!(
        parse_border_radius_declaration("revert"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Revert
        ))
    );
    for value in [
        "inherit 1px",
        "1px inherit",
        "initial 2px",
        "1px revert",
        "revert-layer 1px",
    ] {
        assert_eq!(
            parse_border_radius_declaration(value),
            None,
            "value={value}"
        );
    }
    assert_eq!(parse_border_radius_declaration("1px revert-layer"), None);
}

#[test]
fn border_radius_corner_parser_preserves_bounded_values_and_order() {
    assert_eq!(
        parse_border_radius_corner_declaration("9px"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Corner(9),
        ))
    );
    for (value, expected) in [
        (
            "InHeRiT",
            LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Inherit),
        ),
        (
            "UNSET",
            LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Unset),
        ),
        (
            "initial",
            LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Initial),
        ),
        (
            "ReVeRt",
            LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Revert),
        ),
        ("revert-layer", LocalCascadeDeclaration::RevertLayer),
    ] {
        assert_eq!(
            parse_border_radius_corner_declaration(value),
            Some(expected)
        );
    }
    for value in [
        "1px 2px",
        "50%",
        "-1px",
        "1.5px",
        "20000px",
        "inherit 1px",
        "1px revert-layer",
    ] {
        assert_eq!(
            parse_border_radius_corner_declaration(value),
            None,
            "value={value}"
        );
    }

    let declarations = parse_declarations(
        "border-radius: 1px 2px 3px 4px; border-top-left-radius: 9px; border-bottom-left-radius: 10px;",
    );
    assert_eq!(declarations.border_radius_order, 0);
    assert_eq!(
        declarations.border_radius_corners,
        [
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Corner(9)
            )),
            None,
            None,
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Corner(10)
            )),
        ]
    );
    assert_eq!(declarations.border_radius_corner_orders, [1, 0, 0, 2]);
}

#[test]
fn border_radius_parser_accepts_custom_property_aliases_and_fallbacks() {
    let radius = parse_custom_property_name("--radius").unwrap();
    assert_eq!(
        parse_border_radius_declaration("var(--radius)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::CustomProperty(radius)
        ))
    );
    assert_eq!(
        parse_border_radius_declaration("var(--radius, 1px 2px)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::CustomPropertyFallback(
                radius,
                NativeBorderRadius {
                    top_left: 1,
                    top_right: 2,
                    bottom_right: 1,
                    bottom_left: 2,
                },
            )
        ))
    );
    assert_eq!(
        parse_border_radius_corner_declaration("var(--radius, 3px)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::CustomPropertyCornerFallback(radius, 3)
        ))
    );
    assert_eq!(
        parse_border_radius_declaration("var(--radius, var(--other))"),
        None
    );
    assert_eq!(
        parse_border_radius_corner_declaration("var(--radius, var(--other))"),
        None
    );
    assert_eq!(
        parse_border_radius_declaration("revert-layer"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
}

#[test]
fn border_radius_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --radius: 1px 2px 3px 4px; --alias: var(--radius); border-radius: var(--alias); }
            #child { border-radius: var(--alias); }
            #fallback { border-radius: var(--missing, 5px 6px); }
            #invalid { --bad: unsupported; border-radius: var(--bad, 7px); }
            #cycle { --cycle: var(--cycle); border-radius: var(--cycle, 8px); }
            #reset { --reset: initial; border-radius: var(--reset, 9px); }
            #longhand { --corner: 10px; border-top-left-radius: var(--corner); }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='longhand'>Longhand</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let radius = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .border_radius()
    };

    assert_eq!(
        radius("parent"),
        NativeBorderRadius {
            top_left: 1,
            top_right: 2,
            bottom_right: 3,
            bottom_left: 4,
        }
    );
    assert_eq!(radius("child"), radius("parent"));
    assert_eq!(
        radius("fallback"),
        NativeBorderRadius {
            top_left: 5,
            top_right: 6,
            bottom_right: 5,
            bottom_left: 6,
        }
    );
    assert_eq!(
        radius("invalid"),
        NativeBorderRadius {
            top_left: 7,
            top_right: 7,
            bottom_right: 7,
            bottom_left: 7,
        }
    );
    assert_eq!(
        radius("cycle"),
        NativeBorderRadius {
            top_left: 8,
            top_right: 8,
            bottom_right: 8,
            bottom_left: 8,
        }
    );
    assert_eq!(radius("reset"), NativeBorderRadius::default());
    assert_eq!(
        radius("longhand"),
        NativeBorderRadius {
            top_left: 10,
            ..NativeBorderRadius::default()
        }
    );
}

#[test]
fn stylesheet_cascade_composes_border_radius_shorthand_and_corners() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#all { border-radius: 1px 2px 3px 4px; border-top-left-radius: 9px; border-bottom-right-radius: 8px; } #reverse { border-top-left-radius: 9px; border-radius: 1px 2px 3px 4px; } #invalid { border-radius: 5px 6px 7px 8px; border-top-right-radius: 50%; } #inherit { border-radius: 1px; border-top-left-radius: inherit; }".into(),
        ])
        .unwrap();
    let inherited_radius = NativeBorderRadius {
        top_left: 11,
        top_right: 12,
        bottom_right: 13,
        bottom_left: 14,
    };
    let computed = |id: &str| {
        let element = node(&format!("<div id='{id}'>Radius</div>"));
        stylesheet
            .computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    border_radius: inherited_radius,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
            .border_radius()
    };
    assert_eq!(
        computed("all"),
        NativeBorderRadius {
            top_left: 9,
            top_right: 2,
            bottom_right: 8,
            bottom_left: 4,
        }
    );
    assert_eq!(
        computed("reverse"),
        NativeBorderRadius {
            top_left: 1,
            top_right: 2,
            bottom_right: 3,
            bottom_left: 4,
        }
    );
    assert_eq!(
        computed("invalid"),
        NativeBorderRadius {
            top_left: 5,
            top_right: 6,
            bottom_right: 7,
            bottom_left: 8,
        }
    );
    assert_eq!(
        computed("inherit"),
        NativeBorderRadius {
            top_left: 11,
            top_right: 1,
            bottom_right: 1,
            bottom_left: 1,
        }
    );
}

#[test]
fn border_radius_corner_revert_layer_rolls_back_only_selected_corner() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #card { border-radius: 1px 2px 3px 4px; } } @layer theme { #card { border-radius: 9px; border-top-left-radius: revert-layer; border-bottom-right-radius: 7px; } }".into(),
        ])
        .unwrap();
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='card'>Radius</div>"))
            .border_radius(),
        NativeBorderRadius {
            top_left: 1,
            top_right: 9,
            bottom_right: 7,
            bottom_left: 9,
        }
    );
}

#[test]
fn logical_border_radius_parser_preserves_corner_values_and_order() {
    let declarations = parse_declarations(
        "border-start-start-radius: 9px; border-start-end-radius: inherit; border-end-start-radius: revert-layer; border-end-end-radius: 10px;",
    );
    assert_eq!(
        declarations.logical_border_radius.corners,
        [
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Corner(9)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Inherit
            )),
            Some(LocalCascadeDeclaration::RevertLayer),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Corner(10)
            )),
        ]
    );
    assert_eq!(
        declarations.logical_border_radius.corner_orders,
        [0, 1, 2, 3]
    );
}

#[test]
fn stylesheet_cascade_maps_logical_border_radius_for_ltr_and_rtl() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#ltr { direction: ltr; border-start-start-radius: 1px; border-start-end-radius: 2px; border-end-start-radius: 4px; border-end-end-radius: 3px; } #rtl { direction: rtl; border-start-start-radius: 1px; border-start-end-radius: 2px; border-end-start-radius: 4px; border-end-end-radius: 3px; } #precedence { border-top-left-radius: 9px; border-start-start-radius: 7px; } #reverse { border-start-start-radius: 7px; border-top-left-radius: 9px; } #invalid { border-radius: 5px; border-start-start-radius: 50%; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        let element = node(&format!("<div id='{id}'>Radius</div>"));
        stylesheet
            .computed_for_with_matcher(&element, NativeInheritedStyle::default(), |selector| {
                selector.matches(&element)
            })
            .border_radius()
    };
    assert_eq!(
        computed("ltr"),
        NativeBorderRadius {
            top_left: 1,
            top_right: 2,
            bottom_right: 3,
            bottom_left: 4,
        }
    );
    assert_eq!(
        computed("rtl"),
        NativeBorderRadius {
            top_left: 2,
            top_right: 1,
            bottom_right: 4,
            bottom_left: 3,
        }
    );
    assert_eq!(computed("precedence").top_left, 7);
    assert_eq!(computed("reverse").top_left, 9);
    assert_eq!(
        computed("invalid"),
        NativeBorderRadius {
            top_left: 5,
            top_right: 5,
            bottom_right: 5,
            bottom_left: 5,
        }
    );
}

#[test]
fn logical_border_radius_revert_layer_rolls_back_only_mapped_corner() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #card { direction: rtl; border-radius: 1px 2px 3px 4px; } } @layer theme { #card { border-start-start-radius: 9px; border-start-start-radius: revert-layer; border-start-end-radius: 8px; } }".into(),
        ])
        .unwrap();
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='card'>Radius</div>"))
            .border_radius(),
        NativeBorderRadius {
            top_left: 8,
            top_right: 2,
            bottom_right: 3,
            bottom_left: 4,
        }
    );
}

#[test]
fn border_radius_important_parser_tracks_terminal_case_insensitive_priority() {
    let declarations = parse_declarations(
        "border-radius: 1px !IMPORTANT; border-top-left-radius: 2px; border-top-right-radius: 3px !important; border-start-start-radius: 4px !important; border-end-end-radius: 50% !important;",
    );
    assert_eq!(
        declarations.border_radius,
        Some(LocalCascadeDeclaration::Value(
            NativeBorderRadiusValue::Radius(NativeBorderRadius {
                top_left: 1,
                top_right: 1,
                bottom_right: 1,
                bottom_left: 1,
            }),
        ))
    );
    assert!(declarations.border_radius_important);
    assert_eq!(
        declarations.border_radius_corner_important,
        [false, true, false, false]
    );
    assert_eq!(
        declarations.logical_border_radius.important,
        [true, false, false, false]
    );
    assert_eq!(strip_important_suffix(" 4px !ImPoRtAnT "), ("4px", true));
    assert_eq!(
        strip_important_suffix("4px ! important"),
        ("4px ! important", false)
    );
}

#[test]
fn stylesheet_cascade_resolves_radius_important_priority_and_revert_layer() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { border-radius: 1px !important; } #rollback { border-radius: 1px !important; border-top-left-radius: revert-layer !important; } #normal { border-radius: 1px; } #mixed { border-radius: 1px !important; } #logical { direction: rtl; border-top-right-radius: 2px !important; border-start-start-radius: 7px !important; } #logical-reverse { direction: rtl; border-start-start-radius: 7px !important; border-top-right-radius: 2px !important; } } @layer theme { #layered { border-radius: 2px !important; } #rollback { border-radius: 2px !important; } #normal { border-radius: 2px; } } #layered { border-radius: 3px !important; } #normal { border-radius: 3px; } #mixed { border-radius: 9px; } #inline { border-radius: 3px !important; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        stylesheet
            .computed_for(&node(&format!("<div id='{id}'>Radius</div>")))
            .border_radius()
    };
    let uniform = |radius| NativeBorderRadius {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    };

    assert_eq!(computed("layered"), uniform(1));
    assert_eq!(computed("normal"), uniform(3));
    assert_eq!(computed("mixed"), uniform(1));
    assert_eq!(
        computed("rollback"),
        NativeBorderRadius {
            top_left: 2,
            top_right: 1,
            bottom_right: 1,
            bottom_left: 1,
        }
    );
    assert_eq!(computed("logical").top_right, 7);
    assert_eq!(computed("logical-reverse").top_right, 2);

    let inline = node("<div id='inline' style='border-radius: 4px !important'>Inline</div>");
    assert_eq!(stylesheet.computed_for(&inline).border_radius(), uniform(4));
}

#[test]
fn stylesheet_cascade_resolves_border_radius_css_wide_values() {
    let inherited_radius = NativeBorderRadius {
        top_left: 7,
        top_right: 6,
        bottom_right: 5,
        bottom_left: 4,
    };
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border-radius: inherit; } #unset { border-radius: unset; } #initial { border-radius: initial; } #revert { border-radius: revert; } #omitted {} #invalid { border-radius: inherit; border-radius: invalid; } #mixed { border-radius: inherit 1px; }"
                .into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        let element = node(&format!("<div id='{id}'>Radius</div>"));
        stylesheet
            .computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    border_radius: inherited_radius,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
            .border_radius()
    };

    assert_eq!(computed("inherit"), inherited_radius);
    assert_eq!(computed("invalid"), inherited_radius);
    for id in ["unset", "initial", "revert", "omitted", "mixed"] {
        assert_eq!(computed(id), NativeBorderRadius::default(), "id={id}");
    }
}

#[test]
fn box_edges_parser_expands_bounded_physical_values() {
    assert_eq!(parse_box_edges("4px"), Some([4; 4]));
    assert_eq!(parse_box_edges("1px 2px"), Some([1, 2, 1, 2]));
    assert_eq!(parse_box_edges("1px 2px 3px"), Some([1, 2, 3, 2]));
    assert_eq!(parse_box_edges("1px 2px 3px 4px"), Some([1, 2, 3, 4]));
    assert_eq!(parse_box_edges("1px 2px 3px 4px 5px"), None);
    assert_eq!(parse_box_edges("50%"), None);
    assert_eq!(parse_box_edges("-1px"), None);

    let declarations = parse_declarations(
        "padding: 1px 2px 3px 4px; padding-left: 5px; margin: 6px 7px; margin-bottom: 8px",
    );
    assert_eq!(
        declarations.padding,
        [
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(1)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(2)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(3)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(5)
            )),
        ]
    );
    assert_eq!(
        declarations.margin,
        [
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(6))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(7))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(8))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(7))
            )),
        ]
    );
    let declarations = parse_declarations("margin: 6px 7px; margin-bottom: 8px");
    assert_eq!(
        declarations.margin,
        [
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(6))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(7))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(8))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(7))
            )),
        ]
    );
    let declarations =
        parse_declarations("padding: 4px; padding-left: 50%; margin: 2px; margin-top: -1px");
    assert_eq!(
        declarations.padding,
        [Some(LocalCascadeDeclaration::Value(
            PaddingDeclarationValue::Value(4)
        )); 4]
    );
    assert_eq!(
        declarations.margin,
        [Some(LocalCascadeDeclaration::Value(
            MarginDeclarationValue::Value(NativeMarginValue::Length(2))
        )); 4]
    );
}

#[test]
fn margin_parser_preserves_auto_edges_and_rejects_unbounded_values() {
    assert_eq!(parse_margin_value("auto"), Some(NativeMarginValue::Auto));
    assert_eq!(parse_margin_value("AUTO"), Some(NativeMarginValue::Auto));
    assert_eq!(
        parse_margin_value("4px"),
        Some(NativeMarginValue::Length(4))
    );
    assert_eq!(parse_margin_value("-1px"), None);
    assert_eq!(parse_margin_value("50%"), None);
    assert_eq!(
        parse_margin_edges("auto 2px 3px 4px"),
        Some([
            NativeMarginValue::Auto,
            NativeMarginValue::Length(2),
            NativeMarginValue::Length(3),
            NativeMarginValue::Length(4),
        ])
    );
    assert_eq!(parse_margin_edges("auto -1px"), None);
    assert_eq!(parse_margin_edges("auto 2px 3px 4px 5px"), None);

    let declarations = parse_declarations(
        "margin: auto 2px; margin-left: 4px; margin-bottom: auto; margin-right: -1px",
    );
    assert_eq!(
        declarations.margin,
        [
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Auto)
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(2))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Auto)
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(4))
            )),
        ]
    );
}
