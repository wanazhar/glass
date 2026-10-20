use super::*;

#[test]
fn native_css_direction_reaches_real_font_raster_coordinates() {
    let limits = NativeEngineLimits::default();
    let viewport = Viewport {
        width: 320,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let ltr = NativeDocument::parse(
        "<style>#text { font-family: sans-serif; direction: ltr; }</style><div id='text'>ABC</div>",
        &limits,
    )
    .unwrap();
    let rtl = NativeDocument::parse(
        "<style>#text { font-family: sans-serif; direction: rtl; }</style><div id='text'>ABC</div>",
        &limits,
    )
    .unwrap();
    let glyph_run = |document: &NativeDocument| {
        document
            .display_list(viewport)
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::GlyphRun { run, .. } => Some(run),
                NativeDisplayCommand::TextRun { .. }
                | NativeDisplayCommand::BeginOpacityGroup { .. }
                | NativeDisplayCommand::Clear { .. }
                | NativeDisplayCommand::EndOpacityGroup { .. }
                | NativeDisplayCommand::FillRect { .. }
                | NativeDisplayCommand::SetNestedScrollOffset { .. }
                | NativeDisplayCommand::Image { .. }
                | NativeDisplayCommand::SvgImage { .. }
                | NativeDisplayCommand::SvgStroke { .. }
                | NativeDisplayCommand::SvgPolygonFill { .. }
                | NativeDisplayCommand::SvgPolyline { .. }
                | NativeDisplayCommand::SvgPathFill { .. }
                | NativeDisplayCommand::SvgPathStroke { .. }
                | NativeDisplayCommand::BorderRect { .. } => None,
            })
    };
    let Some(ltr) = glyph_run(&ltr) else {
        return;
    };
    let Some(rtl) = glyph_run(&rtl) else {
        return;
    };
    assert_eq!(ltr.width, rtl.width);
    assert_eq!(ltr.glyphs.len(), rtl.glyphs.len());
    assert!(
        rtl.glyphs
            .windows(2)
            .all(|glyphs| glyphs[0].x >= glyphs[1].x)
    );
}

#[test]
fn native_font_weight_inherits_and_changes_only_fixed_cell_raster() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:16px; font-weight:400; } #bold { display:block; width:16px; font-weight:700; } #parent { display:block; width:48px; font-weight:bold; } #clear { font-weight:normal; } #numeric { font-weight:700; } #invalid { font-weight:1001; }</style><div id='normal'>A</div><div id='bold'>A</div><div id='parent'>A<span id='clear'>B</span><span id='numeric'>C</span><span id='invalid'>D</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let bold = document.resolve_target("id=bold").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let numeric = document.resolve_target("id=numeric").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(normal).unwrap().width, 16);
    assert_eq!(layout.box_for(bold).unwrap().width, 16);
    assert_eq!(layout.box_for(normal).unwrap().height, 20);
    assert_eq!(layout.box_for(bold).unwrap().height, 20);
    assert_eq!(layout.box_for(normal).unwrap().y, 0);
    assert_eq!(layout.box_for(bold).unwrap().y, 20);
    assert_eq!(layout.box_for(parent).unwrap().y, 40);

    let list = document.display_list(viewport).unwrap();
    let weight_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                bold,
                ..
            } if *command_node == node_id => Some(*bold),
            _ => None,
        })
    };
    assert_eq!(weight_for(normal), Some(false));
    assert_eq!(weight_for(bold), Some(true));
    assert_eq!(weight_for(parent), Some(true));
    assert_eq!(weight_for(clear), Some(false));
    assert_eq!(weight_for(numeric), Some(true));
    assert_eq!(weight_for(invalid), Some(true));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(1, 21), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 21), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(6, 21), Some([255, 255, 255, 255]));
    assert_eq!(layout.max_scroll_offset().x, 0);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(normal));
    assert_eq!(layout.hit_test(1, 21).unwrap(), Some(bold));
}

#[test]
fn native_font_style_inherits_and_shears_only_fixed_cell_raster() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:16px; font-style:normal; } #italic { display:block; width:16px; font-style:italic; } #parent { display:block; width:48px; font-style:italic; } #clear { font-style:normal; } #invalid { font-style:oblique; }</style><div id='normal'>A</div><div id='italic'>A</div><div id='parent'>A<span id='clear'>B</span><span id='invalid'>C</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let italic = document.resolve_target("id=italic").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    let layout = document.layout(viewport).unwrap();
    let normal_box = layout.box_for(normal).unwrap();
    let italic_box = layout.box_for(italic).unwrap();
    assert_eq!(normal_box.width, italic_box.width);
    assert_eq!(normal_box.height, italic_box.height);
    assert_eq!(normal_box.x, italic_box.x);
    assert_eq!(layout.max_scroll_offset().x, 0);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(normal));
    assert_eq!(layout.hit_test(1, 21).unwrap(), Some(italic));

    let list = document.display_list(viewport).unwrap();
    let style_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                bold,
                italic,
                ..
            } if *command_node == node_id => Some((*bold, *italic)),
            _ => None,
        })
    };
    assert_eq!(style_for(normal), Some((false, false)));
    assert_eq!(style_for(italic), Some((false, true)));
    assert_eq!(style_for(parent), Some((false, true)));
    assert_eq!(style_for(clear), Some((false, false)));
    assert_eq!(style_for(invalid), Some((false, true)));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 20), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(3, 20), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 26), Some([0, 0, 0, 255]));
}
