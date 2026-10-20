use super::*;

#[tokio::test]
async fn native_css_locators_share_the_stylesheet_selector_grammar() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://css-locator")
        .with_fixture(
            "fixture://css-locator",
            "<style>.profile .name { display: block; }</style><form class='profile'><input class='name' id='first'><input class='other' id='second'></form>",
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .action(NativeAction::Type {
            target: "css=form.profile input.name".into(),
            text: "Ada".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('first').value")
            .await
            .unwrap(),
        serde_json::json!("Ada")
    );

    let error = engine
        .action(NativeAction::Click {
            target: "css=input".into(),
        })
        .unwrap_err();
    assert!(error.to_string().contains("matched 2 elements"));
    let error = engine
        .action(NativeAction::Click {
            target: "css=input:not-supported".into(),
        })
        .unwrap_err();
    assert!(error.to_string().contains("CSS action locator"));
    engine.close_async().await.unwrap();
}
#[test]
fn native_vertical_scroll_maps_layout_hit_testing_and_raster_output() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 80,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://scroll",
            "<!doctype html><div id='top' style='display:block;height:24px;background-color:red'>Top</div><button id='bottom' style='display:block;height:24px;background-color:blue'>Bottom</button>",
        )
        .unwrap()
        .with_initial_url("fixture://scroll");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let bottom = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Bottom")
        .unwrap();
    let initial = engine.layout().unwrap();
    assert_eq!(initial.scroll_offset, NativePoint { x: 0, y: 0 });
    assert!(initial.content_height > initial.viewport.height);
    assert_eq!(initial.viewport_rect_for(bottom.node_id), None);

    let scrolled = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(scrolled.accepted);
    assert_eq!(scrolled.revision, 2);
    let layout = engine.layout().unwrap();
    assert_eq!(layout.scroll_offset, layout.max_scroll_offset());
    assert_eq!(layout.viewport_rect_for(bottom.node_id).unwrap().y, 0);
    assert_eq!(engine.hit_test(9, 1).unwrap(), Some(bottom.node_id));
    assert_eq!(
        engine
            .effects_since(1)
            .unwrap()
            .effects
            .iter()
            .map(|effect| effect.kind)
            .collect::<Vec<_>>(),
        vec![NativeEventKind::Scroll]
    );

    let list = engine.display_list().unwrap();
    assert_eq!(list.scroll_offset, layout.scroll_offset);
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(9, 1), Some([0, 0, 255, 255]));

    let clicked = engine
        .action(NativeAction::Click {
            target: "id=bottom".into(),
        })
        .unwrap();
    assert!(clicked.accepted);
    assert_eq!(clicked.revision, 3);

    let no_op = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(!no_op.accepted);
    assert_eq!(no_op.revision, 3);
    assert_eq!(engine.scroll_offset(), layout.scroll_offset);

    let horizontal_edge = engine
        .action(NativeAction::Scroll {
            delta_x: 1,
            delta_y: 0,
        })
        .unwrap();
    assert!(!horizontal_edge.accepted);
    assert_eq!(engine.revision(), 3);
}
#[test]
fn native_horizontal_scroll_projects_wide_preformatted_content_and_history() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 24,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://wide",
            "<!doctype html><html><head></head><body><pre id='wide' style='white-space:pre'>A A</pre><div id='below' style='display:block;height:40px'>Below</div><pre style='display:none;white-space:pre'>Hidden hidden hidden</pre><script>Invisible invisible invisible</script></body></html>",
        )
        .unwrap()
        .with_fixture("fixture://narrow", "<p>Narrow</p>")
        .unwrap()
        .with_initial_url("fixture://wide");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial = engine.layout().unwrap();
    let wide_text = initial
        .text_runs
        .iter()
        .find(|run| run.text == "A A")
        .unwrap_or_else(|| {
            panic!(
                "wide preformatted text should have a layout run; snapshot={:?}; boxes={:#?}",
                engine.snapshot().unwrap().visible_text,
                initial.boxes
            )
        });
    let wide = wide_text.node_id;
    assert_eq!(initial.content_width, 32);
    assert_eq!(initial.max_scroll_offset(), NativePoint { x: 8, y: 36 });
    assert_eq!(initial.scroll_offset, NativePoint { x: 0, y: 0 });
    assert_eq!(wide_text.origin, NativePoint { x: 8, y: 8 });
    assert!(!wide_text.truncated);
    assert_eq!(initial.viewport_rect_for(wide).unwrap().width, 8);

    let unscrolled = engine.rasterize().unwrap();
    assert_eq!(unscrolled.pixel(9, 8), Some([0, 0, 0, 255]));

    let horizontal = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: 0,
        })
        .unwrap();
    assert!(horizontal.accepted);
    assert_eq!(horizontal.revision, 2);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 8, y: 0 });
    let horizontal_layout = engine.layout().unwrap();
    assert_eq!(
        horizontal_layout.max_scroll_offset(),
        NativePoint { x: 8, y: 36 }
    );
    assert_eq!(
        horizontal_layout.viewport_rect_for(wide),
        Some(NativeRect {
            x: 0,
            y: 8,
            width: 8,
            height: 20,
        })
    );
    assert_eq!(engine.hit_test(1, 8).unwrap(), Some(wide));
    assert_eq!(engine.display_list().unwrap().scroll_offset.x, 8);
    assert_eq!(
        engine.rasterize().unwrap().pixel(1, 8),
        Some([0, 0, 0, 255])
    );

    let diagonal = engine
        .action(NativeAction::Scroll {
            delta_x: -4,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(diagonal.accepted);
    assert_eq!(diagonal.revision, 3);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 4, y: 36 });

    let edge = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(edge.accepted);
    assert_eq!(edge.revision, 4);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 8, y: 36 });

    let no_op = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(!no_op.accepted);
    assert_eq!(no_op.revision, 4);

    let saved = engine.scroll_offset();
    engine.navigate("fixture://narrow").unwrap();
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    engine.go_back().unwrap().unwrap();
    assert_eq!(engine.scroll_offset(), saved);
}
#[test]
fn native_display_list_is_revisioned_deterministic_and_visibility_aware() {
    let document = NativeDocument::parse(
        "<style>#card { background-color: #102030; color: rgb(1, 2, 3); }</style><main><div id='card'>Hello <span>child</span></div><div id='hidden' style='display:none;background-color:red'>Hidden</div></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();

    let list = document.display_list(viewport).unwrap();
    assert_eq!(list.revision, document.revision());
    assert_eq!(list.viewport, viewport);
    assert!(matches!(
        list.commands.first(),
        Some(NativeDisplayCommand::Clear { color }) if *color == NativeColor::WHITE
    ));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == card
                    && *color == NativeColor { red: 16, green: 32, blue: 48, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { text, color, .. }
                if text == "Hello"
                    && *color == NativeColor { red: 1, green: 2, blue: 3, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { text, color, .. }
                if text == "child"
                    && *color == NativeColor { red: 1, green: 2, blue: 3, alpha: 255 }
        )
    }));
    assert!(!list.commands.iter().any(|command| match command {
        NativeDisplayCommand::FillRect { node_id, .. }
        | NativeDisplayCommand::SvgStroke { node_id, .. }
        | NativeDisplayCommand::SvgPolygonFill { node_id, .. }
        | NativeDisplayCommand::SvgPolyline { node_id, .. }
        | NativeDisplayCommand::SvgPathFill { node_id, .. }
        | NativeDisplayCommand::SvgPathStroke { node_id, .. }
        | NativeDisplayCommand::BorderRect { node_id, .. }
        | NativeDisplayCommand::Image { node_id, .. }
        | NativeDisplayCommand::SvgImage { node_id, .. }
        | NativeDisplayCommand::TextRun { node_id, .. }
        | NativeDisplayCommand::GlyphRun { node_id, .. } => *node_id == hidden,
        NativeDisplayCommand::BeginOpacityGroup { .. }
        | NativeDisplayCommand::Clear { .. }
        | NativeDisplayCommand::SetNestedScrollOffset { .. }
        | NativeDisplayCommand::EndOpacityGroup { .. } => false,
    }));
    assert_eq!(list, document.display_list(viewport).unwrap());
}
#[test]
fn native_display_visibility_revert_layer_preserves_hidden_subtree_owners() {
    let document = NativeDocument::parse(
        "<style>@layer base { #rolled { display:none;visibility:hidden;width:12px;height:8px;background-color:red; } #repeat { display:block;visibility:visible;width:12px;height:8px;background-color:red; } #contents { display:contents;visibility:visible; } #fallback { display:revert-layer;visibility:revert-layer;width:12px;height:8px;background-color:green; } #inline { display:block;visibility:hidden;width:12px;height:8px;background-color:yellow; } #child { display:block;width:12px;height:8px;background-color:blue; } } @layer theme { #rolled { display:block;visibility:visible; } #rolled { display:revert-layer;visibility:revert-layer; } #repeat { display:revert-layer;visibility:revert-layer; } #contents { display:block;visibility:hidden; } #contents { display:revert-layer;visibility:revert-layer; } } @layer top { #repeat { display:revert-layer;visibility:revert-layer; } } #rolled { display:revert-layer;visibility:revert-layer; } #repeat { display:revert-layer;visibility:revert-layer; } #contents { display:revert-layer;visibility:revert-layer; }</style><button id='rolled'>Rolled</button><button id='repeat'>Repeat</button><div id='contents'><button id='child'>Child</button></div><button id='fallback'>Fallback</button><button id='inline' style='display:ReVeRt-LaYeR;visibility:revert-layer'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let rolled = document.resolve_target("id=rolled").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let contents = document.resolve_target("id=contents").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(rolled), None);
    assert_eq!(layout.box_for(contents), None);
    assert_eq!(
        layout.box_for(repeat),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 12,
            height: 8,
        })
    );
    assert_eq!(layout.box_for(child).unwrap().y, 8);
    assert_eq!(layout.box_for(fallback).unwrap().y, 16);
    assert_eq!(layout.box_for(inline), None);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(repeat));
    assert_eq!(layout.hit_test(1, 9).unwrap(), Some(child));
    assert_eq!(layout.hit_test(1, 17).unwrap(), Some(fallback));

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .any(|node| node.node_id == rolled && node.hidden)
    );
    assert!(
        semantic_nodes
            .iter()
            .any(|node| node.node_id == repeat && !node.hidden)
    );
    assert!(
        semantic_nodes
            .iter()
            .any(|node| node.node_id == child && !node.hidden)
    );
    assert!(
        semantic_nodes
            .iter()
            .any(|node| node.node_id == fallback && !node.hidden)
    );
    assert!(
        semantic_nodes
            .iter()
            .any(|node| node.node_id == inline && node.hidden)
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == repeat && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == child && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == fallback && *color == NativeColor { red: 0, green: 128, blue: 0, alpha: 255 }
        )
    }));
    assert!(!list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, .. }
                | NativeDisplayCommand::BorderRect { node_id, .. }
                | NativeDisplayCommand::TextRun { node_id, .. }
                if *node_id == rolled || *node_id == inline
        )
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(diagnostic.detail.as_str(), "display" | "visibility")
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 9), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(1, 17), Some([0, 128, 0, 255]));
}
#[test]
fn native_gap_revert_layer_preserves_row_column_layout_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #row { display:flex;width:14px;height:16px;flex-wrap:wrap;gap:2px 3px;align-items:flex-start;align-content:flex-start; } #column { display:flex;width:12px;height:14px;flex-direction:column;flex-wrap:wrap;gap:2px 3px;align-items:flex-start;align-content:flex-start; } } @layer theme { #row { gap:4px 5px; } #column { gap:4px 5px; } } @layer top { #row { gap:revert-layer; row-gap:8px; } #column { gap:revert-layer; column-gap:6px; } } #row { column-gap:revert-layer; } #column { row-gap:revert-layer; }</style><div id='row'><div id='row-first' style='width:4px;height:4px;flex-shrink:0;background-color:red'>A</div><div id='row-second' style='width:4px;height:4px;flex-shrink:0;background-color:green'>B</div><div id='row-third' style='width:4px;height:4px;flex-shrink:0;background-color:blue'>C</div></div><div id='column'><div id='column-first' style='width:4px;height:4px;flex-shrink:0;background-color:red'>D</div><div id='column-second' style='width:4px;height:4px;flex-shrink:0;background-color:green'>E</div><div id='column-third' style='width:4px;height:4px;flex-shrink:0;background-color:blue'>F</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(diagnostic.detail.as_str(), "gap" | "row-gap" | "column-gap"))
    }));

    let row = document.resolve_target("id=row").unwrap();
    let row_first = document.resolve_target("id=row-first").unwrap();
    let row_second = document.resolve_target("id=row-second").unwrap();
    let row_third = document.resolve_target("id=row-third").unwrap();
    let column = document.resolve_target("id=column").unwrap();
    let column_first = document.resolve_target("id=column-first").unwrap();
    let column_second = document.resolve_target("id=column-second").unwrap();
    let column_third = document.resolve_target("id=column-third").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let row_rect = layout.box_for(row).unwrap();
    let row_first_rect = layout.box_for(row_first).unwrap();
    let row_second_rect = layout.box_for(row_second).unwrap();
    let row_third_rect = layout.box_for(row_third).unwrap();
    assert_eq!(row_rect.height, 16);
    assert_eq!(row_first_rect.x, row_rect.x);
    assert_eq!(row_first_rect.y, row_rect.y);
    assert_eq!(row_second_rect.x, row_rect.x + 9);
    assert_eq!(row_second_rect.y, row_rect.y);
    assert_eq!(row_third_rect.x, row_rect.x);
    assert_eq!(row_third_rect.y, row_rect.y + 12);
    assert_eq!(layout.content_height, 30);
    assert_eq!(layout.max_scroll_offset().y, 6);
    assert_eq!(
        layout.hit_test(
            i64::from(row_third_rect.x + 1),
            i64::from(row_third_rect.y + 1),
        ),
        Ok(Some(row_third))
    );

    let column_rect = layout.box_for(column).unwrap();
    let column_first_rect = layout.box_for(column_first).unwrap();
    let column_second_rect = layout.box_for(column_second).unwrap();
    let column_third_rect = layout.box_for(column_third).unwrap();
    assert_eq!(column_first_rect.x, column_rect.x);
    assert_eq!(column_first_rect.y, column_rect.y);
    assert_eq!(column_second_rect.x, column_rect.x);
    assert_eq!(column_second_rect.y, column_rect.y + 8);
    assert_eq!(column_third_rect.x, column_rect.x + 10);
    assert_eq!(column_third_rect.y, column_rect.y);
    assert_eq!(
        layout.hit_test(
            i64::from(column_third_rect.x + 1),
            i64::from(column_third_rect.y + 1),
        ),
        Ok(Some(column_third))
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == row_third && *rect == row_third_rect
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == column_third && *rect == column_third_rect
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(row_third_rect.x + 1, row_third_rect.y + 1),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        surface.pixel(column_third_rect.x + 1, column_third_rect.y + 1),
        Some([0, 0, 255, 255])
    );
}
#[test]
fn native_gap_css_wide_resets_reach_layout_and_raster() {
    let document = NativeDocument::parse(
        "<style>.flow { display:flex; width:24px; height:8px; align-items:flex-start; } .item { width:4px; height:4px; flex-shrink:0; background-color:green; } #shorthand { gap:4px 5px; gap:InItIaL; } #longhand { gap:4px 5px; row-gap:UnSeT; column-gap:ReVeRt; } #invalid { gap:3px 4px; gap:inherit 1px; } #important { gap:4px !important; } #important { gap:initial; }</style><div id='shorthand' class='flow'><div id='shorthand-first' class='item'>A</div><div id='shorthand-second' class='item'>B</div></div><div id='longhand' class='flow'><div id='longhand-first' class='item'>C</div><div id='longhand-second' class='item'>D</div></div><div id='invalid' class='flow'><div id='invalid-first' class='item'>E</div><div id='invalid-second' class='item'>F</div></div><div id='important' class='flow'><div id='important-first' class='item'>G</div><div id='important-second' class='item'>H</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let shorthand_first = document.resolve_target("id=shorthand-first").unwrap();
    let shorthand_second = document.resolve_target("id=shorthand-second").unwrap();
    let longhand_first = document.resolve_target("id=longhand-first").unwrap();
    let longhand_second = document.resolve_target("id=longhand-second").unwrap();
    let invalid_first = document.resolve_target("id=invalid-first").unwrap();
    let invalid_second = document.resolve_target("id=invalid-second").unwrap();
    let important_first = document.resolve_target("id=important-first").unwrap();
    let important_second = document.resolve_target("id=important-second").unwrap();

    let layout = document.layout(viewport).unwrap();
    let rect = |node_id| layout.box_for(node_id).expect("layout rectangle");
    let shorthand_first_rect = rect(shorthand_first);
    let shorthand_second_rect = rect(shorthand_second);
    assert_eq!(shorthand_second_rect.x, shorthand_first_rect.x + 4);
    let longhand_first_rect = rect(longhand_first);
    let longhand_second_rect = rect(longhand_second);
    assert_eq!(longhand_second_rect.x, longhand_first_rect.x + 4);
    let invalid_first_rect = rect(invalid_first);
    let invalid_second_rect = rect(invalid_second);
    assert_eq!(invalid_second_rect.x, invalid_first_rect.x + 8);
    let important_first_rect = rect(important_first);
    let important_second_rect = rect(important_second);
    assert_eq!(important_second_rect.x, important_first_rect.x + 8);
    assert_eq!(
        layout.hit_test(
            i64::from(invalid_second_rect.x + 1),
            i64::from(invalid_second_rect.y + 1),
        ),
        Ok(Some(invalid_second))
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == important_second && *rect == important_second_rect
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(invalid_second_rect.x + 1, invalid_second_rect.y + 1),
        Some([0, 128, 0, 255])
    );
    assert_eq!(
        surface.pixel(important_second_rect.x + 1, important_second_rect.y + 1),
        Some([0, 128, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "gap"
    }));
}
#[test]
fn native_inherited_alignment_css_wide_resets_reach_layout_and_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
        #parent { display:block; width:64px; height:100px; direction:rtl; text-align:start; text-align-last:auto; text-justify:inter-word; }
        .probe { display:block; width:64px; height:20px; }
        #inherit { direction:inherit; text-align:inherit; text-align-last:inherit; text-justify:inherit; background-color:#ff0000; }
        #unset { direction:unset; text-align:unset; text-align-last:unset; text-justify:unset; background-color:#00ff00; }
        #revert { direction:revert; text-align:revert; text-align-last:revert; text-justify:revert; background-color:#0000ff; }
        #initial { direction:initial; text-align:initial; text-align-last:initial; text-justify:initial; background-color:#ffff00; }
        #terminal { direction:rtl; direction:initial; text-align:start; text-align:initial; text-align-last:right; text-align-last:initial; text-justify:none; text-justify:initial; background-color:#ff00ff; }
        </style>
        <div id='parent'><div id='inherit' class='probe' role='button'>A</div><div id='unset' class='probe' role='button'>A</div><div id='revert' class='probe' role='button'>A</div><div id='initial' class='probe' role='button'>A</div><div id='terminal' class='probe' role='button'>A</div></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 112,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let unset = document.resolve_target("id=unset").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let terminal = document.resolve_target("id=terminal").unwrap();
    let targets = [inherit, unset, revert, initial, terminal];

    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "direction" | "text-align" | "text-align-last" | "text-justify"
            ))
    }));

    let layout = document.layout(viewport).unwrap();
    assert!(layout.box_for(parent).is_some());
    for target in targets {
        assert!(layout.box_for(target).is_some());
        assert!(layout.text_runs.iter().any(|run| run.node_id == target));
    }
    let text_x = |node_id| {
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
            .origin
            .x
    };
    assert_eq!(text_x(inherit), text_x(unset));
    assert_eq!(text_x(inherit), text_x(revert));
    assert!(text_x(inherit) > text_x(initial));
    assert_eq!(text_x(initial), text_x(terminal));

    let list = document.display_list(viewport).unwrap();
    for target in targets {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, .. } if *node_id == target
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::TextRun { node_id, .. } if *node_id == target
            )
        }));
    }

    let surface = list.rasterize().unwrap();
    for (target, color) in [
        (inherit, [255, 0, 0, 255]),
        (unset, [0, 255, 0, 255]),
        (revert, [0, 0, 255, 255]),
        (initial, [255, 255, 0, 255]),
        (terminal, [255, 0, 255, 255]),
    ] {
        let rect = layout.box_for(target).unwrap();
        assert_eq!(
            layout
                .hit_test(i64::from(rect.x + 1), i64::from(rect.y + 1))
                .unwrap(),
            Some(target)
        );
        assert_eq!(surface.pixel(rect.x + 1, rect.y + 1), Some(color));
    }
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(
        document
            .semantic_nodes()
            .iter()
            .filter(|semantic_node| targets.contains(&semantic_node.node_id))
            .count()
            >= targets.len()
    );
}
#[test]
fn native_order_inherit_reorders_visual_items_and_preserves_semantics() {
    let document = NativeDocument::parse(
        "<style>#outer{display:flex;width:20px;height:8px;gap:2px;align-items:flex-start;order:-3;}#first{order:0;width:4px;height:8px;flex-grow:0;flex-shrink:0;background-color:green;}#direct{order:InHeRiT;width:4px;height:8px;flex-grow:0;flex-shrink:0;background-color:red;}#omitted{width:4px;height:8px;flex-grow:0;flex-shrink:0;background-color:blue;}#invalid{display:none;order:2;order:inherit 1;}</style><div id='outer'><button id='first'>A</button><button id='direct'><span id='nested' style='display:block;height:4px'>B</span></button><button id='omitted'>C</button><button id='invalid'>D</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let outer = document.resolve_target("id=outer").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let direct = document.resolve_target("id=direct").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let outer_box = layout.box_for(outer).unwrap();
    let first_box = layout.box_for(first).unwrap();
    let direct_box = layout.box_for(direct).unwrap();
    let nested_box = layout.box_for(nested).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    assert_eq!(outer_box.width, 20);
    assert_eq!(direct_box.x, outer_box.x);
    assert_eq!(first_box.x, direct_box.x + direct_box.width + 2);
    assert_eq!(omitted_box.x, first_box.x + first_box.width + 2);
    assert_eq!(direct_box.width, 4);
    assert_eq!(nested_box.width, direct_box.width);

    let semantic_nodes = document.semantic_nodes();
    let first_position = semantic_nodes
        .iter()
        .position(|node| node.node_id == first)
        .unwrap();
    let direct_position = semantic_nodes
        .iter()
        .position(|node| node.node_id == direct)
        .unwrap();
    let omitted_position = semantic_nodes
        .iter()
        .position(|node| node.node_id == omitted)
        .unwrap();
    assert!(first_position < direct_position && direct_position < omitted_position);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == direct
                    && rect.width == direct_box.width
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(i64::from(direct_box.x + 1), i64::from(direct_box.y + 1),)
            .unwrap(),
        Some(nested)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(direct_box.x + 1, direct_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "order"
    }));
}
#[test]
fn native_gap_inherit_reaches_row_column_layout_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#outer{display:flex;width:28px;height:18px;gap:3px 4px;align-items:flex-start;}#inherit{display:flex;width:12px;height:14px;flex-wrap:wrap;gap:InHeRiT;align-items:flex-start;align-content:flex-start;}#omitted{display:flex;width:12px;height:14px;flex-wrap:wrap;align-items:flex-start;align-content:flex-start;}#first,#second,#third,#omitted-first,#omitted-second,#omitted-third{width:4px;height:4px;flex-shrink:0;}#first{background-color:green;}#second{background-color:red;}#third{background-color:blue;}#omitted-first,#omitted-second,#omitted-third{background-color:#804000;}#invalid{display:none;gap:inherit 1px;}</style><div id='outer'><div id='inherit'><button id='first'>A</button><button id='second'>B</button><button id='third'>C</button></div><div id='omitted'><button id='omitted-first'>D</button><button id='omitted-second'>E</button><button id='omitted-third'>F</button></div><div id='invalid'>Invalid</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let outer = document.resolve_target("id=outer").unwrap();
    let inherited = document.resolve_target("id=inherit").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let omitted_first = document.resolve_target("id=omitted-first").unwrap();
    let omitted_second = document.resolve_target("id=omitted-second").unwrap();
    let omitted_third = document.resolve_target("id=omitted-third").unwrap();
    let viewport = Viewport {
        width: 36,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let rect = |node_id| layout.box_for(node_id).expect("layout rectangle");
    let outer_rect = rect(outer);
    let inherited_rect = rect(inherited);
    let first_rect = rect(first);
    let second_rect = rect(second);
    let third_rect = rect(third);
    let omitted_rect = rect(omitted);
    assert_eq!(outer_rect.width, 28);
    assert_eq!(inherited_rect.x, outer_rect.x);
    assert_eq!(omitted_rect.x, inherited_rect.x + inherited_rect.width + 4);
    assert_eq!(first_rect.x, inherited_rect.x);
    assert_eq!(first_rect.y, inherited_rect.y);
    assert_eq!(second_rect.x, first_rect.x + first_rect.width + 4);
    assert_eq!(second_rect.y, first_rect.y);
    assert_eq!(third_rect.x, first_rect.x);
    assert_eq!(third_rect.y, first_rect.y + first_rect.height + 3);
    assert_eq!(rect(omitted_first).x, omitted_rect.x);
    assert_eq!(rect(omitted_second).x, omitted_rect.x + 4);
    assert_eq!(rect(omitted_third).x, omitted_rect.x + 8);
    assert_eq!(rect(omitted_third).y, omitted_rect.y);

    let semantic_nodes = document.semantic_nodes();
    let source_position = |node_id| {
        semantic_nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .unwrap()
    };
    assert!(source_position(first) < source_position(second));
    assert!(source_position(second) < source_position(third));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == third
                    && *rect == third_rect
                    && *color == NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    }
        )
    }));
    assert_eq!(
        layout.hit_test(i64::from(third_rect.x + 1), i64::from(third_rect.y + 1),),
        Ok(Some(third))
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(third_rect.x + 1, third_rect.y + 1),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "gap"
    }));
}
#[test]
fn native_gap_longhand_inherit_reaches_independent_axes_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#outer{display:flex;width:32px;height:18px;gap:3px 4px;align-items:flex-start;}#row{display:flex;width:14px;height:14px;flex-wrap:wrap;row-gap:inherit;column-gap:0;align-items:flex-start;align-content:flex-start;}#column{display:flex;width:14px;height:14px;flex-wrap:wrap;row-gap:0;column-gap:inherit;align-items:flex-start;align-content:flex-start;}#row-first,#row-second,#row-third,#column-first,#column-second,#column-third{width:5px;height:4px;flex-shrink:0;}#row-first{background-color:green;}#row-second{background-color:red;}#row-third{background-color:blue;}#column-first,#column-second,#column-third{background-color:#804000;}#invalid{display:none;row-gap:inherit 1px;column-gap:1px inherit;}</style><div id='outer'><div id='row'><button id='row-first'>A</button><button id='row-second'>B</button><button id='row-third'>C</button></div><div id='column'><button id='column-first'>D</button><button id='column-second'>E</button><button id='column-third'>F</button></div><div id='invalid'>Invalid</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let outer = document.resolve_target("id=outer").unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let row_first = document.resolve_target("id=row-first").unwrap();
    let row_second = document.resolve_target("id=row-second").unwrap();
    let row_third = document.resolve_target("id=row-third").unwrap();
    let column = document.resolve_target("id=column").unwrap();
    let column_first = document.resolve_target("id=column-first").unwrap();
    let column_second = document.resolve_target("id=column-second").unwrap();
    let column_third = document.resolve_target("id=column-third").unwrap();
    let viewport = Viewport {
        width: 40,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let rect = |node_id| layout.box_for(node_id).expect("layout rectangle");
    let outer_rect = rect(outer);
    let row_rect = rect(row);
    let row_first_rect = rect(row_first);
    let row_second_rect = rect(row_second);
    let row_third_rect = rect(row_third);
    let column_rect = rect(column);
    let column_first_rect = rect(column_first);
    let column_second_rect = rect(column_second);
    let column_third_rect = rect(column_third);

    assert_eq!(outer_rect.width, 32);
    assert_eq!(row_rect.x, outer_rect.x);
    assert_eq!(column_rect.x, row_rect.x + row_rect.width + 4);
    assert_eq!(row_first_rect.x, row_rect.x);
    assert_eq!(row_first_rect.y, row_rect.y);
    assert_eq!(row_second_rect.x, row_first_rect.x + row_first_rect.width);
    assert_eq!(row_second_rect.y, row_first_rect.y);
    assert_eq!(row_third_rect.x, row_first_rect.x);
    assert_eq!(
        row_third_rect.y,
        row_first_rect.y + row_first_rect.height + 3
    );
    assert_eq!(column_first_rect.x, column_rect.x);
    assert_eq!(column_first_rect.y, column_rect.y);
    assert_eq!(
        column_second_rect.x,
        column_first_rect.x + column_first_rect.width + 4
    );
    assert_eq!(column_second_rect.y, column_first_rect.y);
    assert_eq!(column_third_rect.x, column_first_rect.x);
    assert_eq!(
        column_third_rect.y,
        column_first_rect.y + column_first_rect.height
    );

    let semantic_nodes = document.semantic_nodes();
    let source_position = |node_id| {
        semantic_nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .unwrap()
    };
    assert!(source_position(row_first) < source_position(row_second));
    assert!(source_position(row_second) < source_position(row_third));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == row_third
                    && *rect == row_third_rect
                    && *color == NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    }
        )
    }));
    assert_eq!(
        layout.hit_test(
            i64::from(row_third_rect.x + 1),
            i64::from(row_third_rect.y + 1),
        ),
        Ok(Some(row_third))
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(row_third_rect.x + 1, row_third_rect.y + 1),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && (diagnostic.detail == "row-gap" || diagnostic.detail == "column-gap")
    }));
}
#[test]
fn native_css_relative_position_translates_subtree_without_reflow() {
    let document = NativeDocument::parse(
        "<div id='first' style='width:24px;height:8px;background:red;position:relative;left:4px;top:3px'><span>Hi</span></div><div id='second' style='width:8px;height:8px;background:blue'></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 4,
            y: 3,
            width: 24,
            height: 8,
        })
    );
    assert_eq!(layout.box_for(second).unwrap().y, 8);
    assert_eq!(layout.hit_test(23, 4).unwrap(), Some(first));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first
                    && *rect == NativeRect { x: 4, y: 3, width: 24, height: 8 }
                    && *color == NativeColor::RED
        )
    }));
}
#[test]
fn native_css_relative_position_uses_opposite_edge_when_primary_is_auto() {
    let document = NativeDocument::parse(
        "<div style='width:30px;height:30px;padding:10px;box-sizing:border-box'><div id='target' style='width:8px;height:8px;position:relative;right:3px;bottom:4px;background:blue'></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let target = document.resolve_target("id=target").unwrap();
    let layout = document
        .layout(Viewport {
            width: 40,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(
        layout.box_for(target),
        Some(NativeRect {
            x: 7,
            y: 6,
            width: 8,
            height: 8,
        })
    );
}
#[test]
fn native_css_fixed_position_stays_viewport_anchored_when_root_scrolls() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 20,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://fixed",
            "<div style='width:10px;height:5px;overflow:hidden'><button id='fixed' style='display:block;position:fixed;left:20px;top:3px;width:8px;height:4px;background:red'></button></div><div style='height:120px'></div>",
        )
        .unwrap()
        .with_initial_url("fixture://fixed");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let fixed = engine
        .preflight_target("id=fixed", NativePreflightAction::Click)
        .unwrap()
        .node
        .unwrap()
        .node_id;
    let initial = engine.layout().unwrap();
    assert_eq!(
        initial.box_for(fixed),
        Some(NativeRect {
            x: 20,
            y: 3,
            width: 8,
            height: 4,
        })
    );
    assert!(
        initial
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == fixed)
            .is_some_and(|layout_box| layout_box.fixed)
    );

    let scrolled = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 10,
        })
        .unwrap();
    assert!(scrolled.accepted);
    let layout = engine.layout().unwrap();
    assert_eq!(
        layout.box_for(fixed),
        Some(NativeRect {
            x: 20,
            y: 13,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(fixed),
        Some(NativeRect {
            x: 20,
            y: 3,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(engine.hit_test(21, 4).unwrap(), Some(fixed));

    let list = engine.display_list().unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == fixed
                    && *rect == NativeRect { x: 20, y: 13, width: 8, height: 4 }
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        engine.rasterize().unwrap().pixel(20, 3),
        Some([255, 0, 0, 255])
    );
}
#[test]
fn native_css_sticky_position_preserves_flow_and_sticks_to_viewport() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 20,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://sticky",
            "<div id='container' style='width:24px;height:100px'><div style='height:30px'></div><button id='sticky' style='display:block;position:sticky;top:2px;width:8px;height:4px;background:red'>S</button><button id='tail' style='display:block;width:0px;height:100px'></button></div>",
        )
        .unwrap()
        .with_initial_url("fixture://sticky");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let sticky = engine
        .preflight_target("id=sticky", NativePreflightAction::Click)
        .unwrap()
        .node
        .unwrap()
        .node_id;
    let tail = engine
        .preflight_target("id=tail", NativePreflightAction::Click)
        .unwrap()
        .node
        .unwrap()
        .node_id;

    let initial = engine.layout().unwrap();
    assert_eq!(
        initial.box_for(sticky),
        Some(NativeRect {
            x: 8,
            y: 38,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(initial.box_for(tail).unwrap().y, 42);
    assert!(
        initial
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == sticky)
            .is_some_and(|layout_box| layout_box.sticky)
    );
    assert!(
        initial
            .text_runs
            .iter()
            .find(|text_run| text_run.node_id == sticky)
            .is_some_and(|text_run| text_run.sticky)
    );

    let scrolled = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 40,
        })
        .unwrap();
    assert!(scrolled.accepted);
    let layout = engine.layout().unwrap();
    assert_eq!(
        layout.box_for(sticky),
        Some(NativeRect {
            x: 8,
            y: 42,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(layout.viewport_rect_for(sticky).unwrap().y, 2);
    assert_eq!(layout.box_for(tail).unwrap().y, 42);
    assert_eq!(engine.hit_test(9, 3).unwrap(), Some(sticky));

    let list = engine.display_list().unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == sticky
                    && *rect == NativeRect { x: 8, y: 42, width: 8, height: 4 }
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        engine.rasterize().unwrap().pixel(8, 2),
        Some([255, 0, 0, 255])
    );
}
#[test]
fn native_css_sticky_position_releases_at_containing_block_end() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 20,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://sticky-boundary",
            "<div id='container' style='width:24px;height:60px'><div style='height:20px'></div><button id='sticky' style='display:block;position:sticky;top:0;width:8px;height:10px;background:blue'></button><button style='display:block;height:100px'></button></div>",
        )
        .unwrap()
        .with_initial_url("fixture://sticky-boundary");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let sticky = engine
        .preflight_target("id=sticky", NativePreflightAction::Click)
        .unwrap()
        .node
        .unwrap()
        .node_id;

    engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 45,
        })
        .unwrap();
    let near_end = engine.layout().unwrap();
    assert_eq!(near_end.box_for(sticky).unwrap().y, 45);
    assert_eq!(near_end.viewport_rect_for(sticky).unwrap().y, 0);

    engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 25,
        })
        .unwrap();
    let after_end = engine.layout().unwrap();
    assert_eq!(after_end.box_for(sticky).unwrap().y, 58);
    assert_eq!(after_end.viewport_rect_for(sticky), None);
}
#[test]
fn native_css_z_index_cascades_stylesheet_important_values() {
    let document = NativeDocument::parse(
        "<style>#high { position:absolute;left:2px;top:2px;width:12px;height:8px;background:blue;z-index:3 } #high { z-index:2 !important } #low { position:absolute;left:2px;top:2px;width:12px;height:8px;background:red;z-index:1 }</style><button id='high'>high</button><button id='low'>low</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let high = document.resolve_target("id=high").unwrap();
    let low = document.resolve_target("id=low").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.hit_test(3, 3).unwrap(), Some(high));
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == high)
            .map(|layout_box| layout_box.z_index),
        Some(2)
    );
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == low)
            .map(|layout_box| layout_box.z_index),
        Some(1)
    );
    assert_eq!(
        document
            .display_list(viewport)
            .unwrap()
            .rasterize()
            .unwrap()
            .pixel(3, 3),
        Some([0, 0, 255, 255])
    );
}
#[test]
fn native_css_pointer_events_none_skips_visual_overlay_in_hit_testing() {
    let document = NativeDocument::parse(
        "<button id='under' style='display:block;position:absolute;left:2px;top:2px;width:12px;height:8px;background:red;z-index:1'>under</button><button id='overlay' style='display:block;position:absolute;left:2px;top:2px;width:12px;height:8px;background:blue;z-index:2;pointer-events:none'>overlay</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let under = document.resolve_target("id=under").unwrap();
    let overlay = document.resolve_target("id=overlay").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.hit_test(3, 3).unwrap(), Some(under));
    assert!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == under)
            .is_some_and(|layout_box| layout_box.pointer_events)
    );
    assert!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == overlay)
            .is_some_and(|layout_box| !layout_box.pointer_events)
    );
    assert_eq!(
        document
            .display_list(viewport)
            .unwrap()
            .rasterize()
            .unwrap()
            .pixel(3, 3),
        Some([0, 0, 255, 255])
    );
}
#[test]
fn native_css_pointer_events_inheritance_allows_explicit_descendant_target() {
    let document = NativeDocument::parse(
        "<button id='under' style='display:block;position:absolute;left:2px;top:2px;width:12px;height:8px;background:red;z-index:1'>under</button><div id='overlay' style='display:block;position:absolute;left:2px;top:2px;width:12px;height:8px;z-index:2;pointer-events:none'><button id='child' style='display:block;position:absolute;left:0;top:0;width:12px;height:8px;background:green;pointer-events:auto'>child</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overlay = document.resolve_target("id=overlay").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let layout = document
        .layout(Viewport {
            width: 24,
            height: 16,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(layout.hit_test(3, 3).unwrap(), Some(child));
    assert!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == overlay)
            .is_some_and(|layout_box| !layout_box.pointer_events)
    );
    assert!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == child)
            .is_some_and(|layout_box| layout_box.pointer_events)
    );
}
#[test]
fn native_css_diagnostics_identify_unsupported_input_without_raw_echo() {
    let document = NativeDocument::parse(
        "<!doctype html><style>button::before, main >, #ok { color: red; width: 10%; display: grid; opacity: 1.1; text-align: start; text-align: match-parent; text-align-last: match-parent; justify-content: safe center; order: 1025; flex: 1.5 1 8px; flex-flow: column wrap wrap; flex-grow: 1.5; flex-shrink: 1.5; flex-basis: 1.5px; align-items: baseline; align-self: baseline; align-content: safe center; place-content: stretch stretch stretch; flex-direction: column reverse; direction: vertical-rl; flex-wrap: wrap reverse; text-decoration: blink; text-decoration-line: blink; text-decoration-style: zigzag; text-decoration-skip-ink: all; text-decoration-thickness: 5px; text-underline-offset: 5px; text-decoration-color: linear-gradient(red, blue); text-transform: capitalize; font-weight: 1001; font-style: oblique; word-break: keep-all; text-overflow: fade; overflow: visible hidden; white-space: break-spaces; gap: 1px 2px 3px; row-gap: 4px 5px; column-gap: 5px 6px; custom-property: url(secret); broken; }</style><style>.unclosed { color: blue; </style><button id='ok' style='filter: blur(1px); padding: -1px'>OK</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    let diagnostics = document.diagnostics();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssSelector
            && diagnostic.detail == "pseudo-selector"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssSelector
            && diagnostic.detail == "selector-combinator"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            && diagnostic.detail == "custom-property"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            && diagnostic.detail == "filter"
            && matches!(
                diagnostic.source,
                NativeDiagnosticSource::InlineStyle { .. }
            )
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "width"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "opacity"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-align"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-align-last"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-grow"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "flex"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-flow"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-shrink"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-basis"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-line"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-style"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-ink"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-thickness"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-underline-offset"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-color"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-transform"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "font-weight"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "font-style"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "word-break"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-overflow"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "overflow"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "gap"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "justify-content"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "order"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-items"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-self"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-content"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "place-content"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-direction"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "direction"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-wrap"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "row-gap"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "column-gap"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "white-space"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "padding"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::MalformedCss
            && diagnostic.detail == "missing-colon"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::MalformedCss
            && diagnostic.detail == "unclosed-rule"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.source,
            NativeDiagnosticSource::Stylesheet { index: 0 }
        )
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.source,
            NativeDiagnosticSource::InlineStyle { .. }
        )
    }));
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.detail.len() <= MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES)
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.detail.contains("secret"))
    );
}
#[test]
fn native_css_diagnostics_are_bounded() {
    let mut source = String::from("<style>");
    for index in 0..MAX_NATIVE_DIAGNOSTICS.saturating_add(8) {
        source.push_str(&format!(".item{index} {{ unknown-{index}: value; }}"));
    }
    source.push_str("</style><div class='item0'>Item</div>");

    let document = NativeDocument::parse(&source, &NativeEngineLimits::default()).unwrap();
    assert_eq!(document.diagnostics().len(), MAX_NATIVE_DIAGNOSTICS);
    assert!(document.diagnostics_truncated());
}
#[test]
fn native_css_diagnostics_replace_atomically_with_navigation() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://diagnostic-bad",
            "<!doctype html><style>#card { width: 50%; }</style><div id='card'>Bad</div>",
        )
        .unwrap()
        .with_fixture(
            "fixture://diagnostic-good",
            "<!doctype html><div id='card'>Good</div>",
        )
        .unwrap()
        .with_initial_url("fixture://diagnostic-bad");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial = engine.diagnostics().unwrap();
    assert_eq!(initial.revision, 1);
    assert_eq!(initial.diagnostics.len(), 1);
    assert!(!initial.truncated);

    let failed = engine.navigate("fixture://missing");
    assert!(matches!(
        failed,
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));
    let after_failure = engine.diagnostics().unwrap();
    assert_eq!(after_failure, initial);

    engine.navigate("fixture://diagnostic-good").unwrap();
    let after_success = engine.diagnostics().unwrap();
    assert_eq!(after_success.revision, 2);
    assert!(after_success.diagnostics.is_empty());
    assert!(!after_success.truncated);
}
#[test]
fn native_custom_elements_upgrade_create_and_run_lifecycle_reactions() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_custom_elements_upgrade_create_and_run_lifecycle_reactions_inner());
    });
}

async fn native_custom_elements_upgrade_create_and_run_lifecycle_reactions_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native custom-element request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<!doctype html><html><body><x-probe id='parser' data-value='parser'></x-probe></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/custom-elements")),
    )
    .await
    .unwrap();
    let result = session
        .script(
            r##"await (async () => {
              const trace = [];
              const parserElement = document.querySelector('#parser');
              const detached = document.createElement('x-probe');
              const detachedInitiallyNative = detached instanceof HTMLElement;
              const pendingDefinition = customElements.whenDefined('x-probe');
              class ProbeElement extends HTMLElement {
                static get observedAttributes() { return ['data-value']; }
                constructor() {
                  super();
                  trace.push('construct:' + this.localName);
                }
                attributeChangedCallback(name, oldValue, newValue) {
                  trace.push('attribute:' + name + ':' + oldValue + ':' + newValue);
                }
                connectedCallback() { trace.push('connected:' + this.localName); }
                disconnectedCallback() { trace.push('disconnected:' + this.localName); }
                adoptedCallback() { trace.push('adopted:' + this.localName); }
              }
              customElements.define('x-probe', ProbeElement);
              const pendingConstructor = await pendingDefinition;
              const parserOrder = trace.slice(0, 3);
              const detachedRemainedUnupgraded = !(detached instanceof ProbeElement);
              customElements.upgrade(detached);
              const manualUpgradeWorked = detached instanceof ProbeElement;
              document.body.appendChild(detached);
              detached.setAttribute('data-value', 'detached');
              detached.remove();

              const created = document.createElement('x-probe');
              const createElementUpgraded = created instanceof ProbeElement;
              created.setAttribute('data-value', 'created');
              document.body.appendChild(created);
              created.remove();
              const imported = document.importNode(created, { customElementRegistry: customElements });
              const importNodeUsedRegistry = imported instanceof ProbeElement
                && imported.ownerDocument === document;
              let unsupportedImportRegistryError = null;
              try { document.importNode(created, { customElementRegistry: {} }); }
              catch (error) { unsupportedImportRegistryError = error.name; }

              const importTrace = [];
              class ImportProbe extends HTMLElement {
                static get observedAttributes() { return ['data-order']; }
                constructor() {
                  super();
                  importTrace.push('construct:' + this.getAttribute('data-order'));
                }
                attributeChangedCallback(_name, oldValue, newValue) {
                  importTrace.push('attribute:' + oldValue + ':' + newValue);
                }
                connectedCallback() {
                  importTrace.push('connected:' + this.getAttribute('data-order'));
                }
              }
              customElements.define('x-import-probe', ImportProbe);
              const importSource = document.createElement('x-import-probe');
              importSource.setAttribute('data-order', 'outer');
              const importSourceChild = document.createElement('x-import-probe');
              importSourceChild.setAttribute('data-order', 'inner');
              importSource.appendChild(importSourceChild);
              importTrace.length = 0;
              const importedTree = document.importNode(importSource, {
                customElementRegistry: customElements,
              });
              const importCloneTrace = importTrace.slice();
              const importTreeIdentity = importedTree instanceof ImportProbe
                && importedTree.firstElementChild instanceof ImportProbe
                && importedTree !== importSource
                && importedTree.firstElementChild !== importSourceChild
                && importedTree.ownerDocument === document;
              document.body.appendChild(importedTree);
              const importConnectedTrace = importTrace.slice(importCloneTrace.length);

              const constructed = new ProbeElement();
              const directConstructorWorked = constructed instanceof ProbeElement;
              document.body.appendChild(constructed);
              constructed.setAttribute('data-value', 'constructed');
              constructed.remove();

              const left = document.createElement('div');
              const right = document.createElement('div');
              document.body.append(left, right);
              const mover = document.createElement('x-probe');
              left.appendChild(mover);
              const beforeMove = trace.length;
              right.appendChild(mover);
              const moveOrder = trace.slice(beforeMove, beforeMove + 2);
              mover.remove();

              const mutationTrace = [];
              class MutationProbe extends HTMLElement {
                static get observedAttributes() { return ['data-value']; }
                constructor() {
                  super();
                  mutationTrace.push('construct:' + this.id);
                }
                attributeChangedCallback(name, oldValue, newValue, namespace) {
                  mutationTrace.push('attribute:' + this.id + ':' + oldValue + ':' + newValue + ':' + namespace);
                }
                connectedCallback() { mutationTrace.push('connected:' + this.id); }
                disconnectedCallback() { mutationTrace.push('disconnected:' + this.id); }
              }
              customElements.define('x-mutation-probe', MutationProbe);
              globalThis.__glassMutationProbeConstructor = MutationProbe;
              const mutationHost = document.createElement('div');
              mutationHost.id = 'mutation-host';
              document.body.appendChild(mutationHost);
              mutationHost.innerHTML = "<x-mutation-probe id='parent' data-value='parent-initial'><x-mutation-probe id='child' data-value='child-initial'></x-mutation-probe></x-mutation-probe>";
              const innerHtmlReactions = mutationTrace.splice(0);
              const mutationParent = mutationHost.firstElementChild;
              const mutationChild = mutationParent.firstElementChild;
              globalThis.__glassMutationParent = mutationParent;
              globalThis.__glassMutationChild = mutationChild;
              mutationParent.setAttributeNS(null, 'data-value', 'namespaced');
              mutationParent.removeAttributeNS(null, 'data-value');
              const namespacedAttributeReactions = mutationTrace.splice(0);

              const directTemplate = document.createElement('template');
              directTemplate.id = 'direct-template-host';
              directTemplate.innerHTML = "<x-mutation-probe id='direct-template-custom' data-value='direct-initial'><template><x-mutation-probe id='direct-nested-template-custom' data-value='direct-nested-initial'></x-mutation-probe></template></x-mutation-probe>";
              document.body.appendChild(directTemplate);
              const directTemplateCustom = directTemplate.content.querySelector('#direct-template-custom');
              const directNestedTemplateCustom = directTemplateCustom.querySelector('template').content.querySelector('#direct-nested-template-custom');

              const ordinaryTemplateHost = document.createElement('div');
              ordinaryTemplateHost.id = 'ordinary-template-host';
              document.body.appendChild(ordinaryTemplateHost);
              ordinaryTemplateHost.innerHTML = "<template><x-mutation-probe id='ordinary-template-custom' data-value='ordinary-initial'><em>nested</em></x-mutation-probe></template>";
              const ordinaryTemplateCustom = ordinaryTemplateHost.firstElementChild.content.querySelector('#ordinary-template-custom');
              globalThis.__glassDirectTemplateCustom = directTemplateCustom;
              globalThis.__glassDirectNestedTemplateCustom = directNestedTemplateCustom;
              globalThis.__glassOrdinaryTemplateCustom = ordinaryTemplateCustom;
              globalThis.__glassInertTemplateLifecycleTrace = mutationTrace;
              const inertTemplateLifecycle = mutationTrace.splice(0);
              globalThis.__glassMutationTrace = mutationTrace;

              const reportedErrors = [];
              const previousReportError = globalThis.reportError;
              globalThis.reportError = error => reportedErrors.push(String(error));
              class FailingConstructor extends HTMLElement {
                constructor() { super(); throw new Error('constructor failure'); }
              }
              customElements.define('x-failing-constructor', FailingConstructor);
              const failedConstruction = document.createElement('x-failing-constructor');
              const followUpElement = document.createElement('div');
              class FailingCallback extends HTMLElement {
                connectedCallback() { throw new Error('callback failure'); }
              }
              customElements.define('x-failing-callback', FailingCallback);
              const failedCallback = document.createElement('x-failing-callback');
              document.body.appendChild(failedCallback);
              failedCallback.remove();
              globalThis.reportError = previousReportError;
              let invalidNameError = null;
              try { customElements.define('invalid', class extends HTMLElement {}); }
              catch (error) { invalidNameError = error.name; }
              let duplicateNameError = null;
              try { customElements.define('x-probe', class extends HTMLElement {}); }
              catch (error) { duplicateNameError = error.name; }
              let mismatchedBuiltinPrototypeError = null;
              try { customElements.define('x-option', class extends HTMLElement {}, { extends: 'button' }); }
              catch (error) { mismatchedBuiltinPrototypeError = error.name; }
              let invalidWhenDefinedError = null;
              try { await customElements.whenDefined('invalid'); }
              catch (error) { invalidWhenDefinedError = error.name; }

              customElements.initialize(document);
              globalThis.__glassCustomElementTrace = trace;
              globalThis.__glassCustomElementConstructor = ProbeElement;
              return {
                registryIdentity: window.customElements === customElements
                  && document.customElementRegistry === customElements,
                pendingResolved: pendingConstructor === ProbeElement,
                lookup: customElements.get('x-probe') === ProbeElement
                  && customElements.getName(ProbeElement) === 'x-probe',
                parserUpgrade: parserElement instanceof ProbeElement,
                parserOrder,
                detachedInitiallyNative,
                detachedRemainedUnupgraded,
                manualUpgradeWorked,
                createElementUpgraded,
                importNodeUsedRegistry,
                unsupportedImportRegistryError,
                importCloneTrace,
                importTreeIdentity,
                importConnectedTrace,
                directConstructorWorked,
                moveOrder,
                innerHtmlReactions,
                namespacedAttributeReactions,
                inertTemplateLifecycle,
                exceptionRecovery: [
                  failedConstruction.__glassCustomElementState,
                  typeof followUpElement.nodeIndex === 'number',
                  failedCallback.__glassCustomElementState,
                  reportedErrors.length,
                ],
                invalidNameError,
                duplicateNameError,
                mismatchedBuiltinPrototypeError,
                invalidWhenDefinedError,
                lifecycleCounts: {
                  constructor: trace.filter(item => item.startsWith('construct:')).length,
                  attribute: trace.filter(item => item.startsWith('attribute:')).length,
                  connected: trace.filter(item => item.startsWith('connected:')).length,
                  disconnected: trace.filter(item => item.startsWith('disconnected:')).length,
                },
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!({
            "registryIdentity": true,
            "pendingResolved": true,
            "lookup": true,
            "parserUpgrade": true,
            "parserOrder": [
                "construct:x-probe",
                "attribute:data-value:null:parser",
                "connected:x-probe",
            ],
            "detachedInitiallyNative": true,
            "detachedRemainedUnupgraded": true,
            "manualUpgradeWorked": true,
            "createElementUpgraded": true,
            "importNodeUsedRegistry": true,
            "unsupportedImportRegistryError": "TypeError",
            "importCloneTrace": [
                "construct:outer",
                "construct:inner",
                "attribute:null:outer",
                "attribute:null:inner",
            ],
            "importTreeIdentity": true,
            "importConnectedTrace": ["connected:outer", "connected:inner"],
            "directConstructorWorked": true,
            "moveOrder": ["disconnected:x-probe", "connected:x-probe"],
            "innerHtmlReactions": [
                "construct:parent",
                "construct:child",
                "attribute:parent:null:parent-initial:null",
                "attribute:child:null:child-initial:null",
                "connected:parent",
                "connected:child",
            ],
            "namespacedAttributeReactions": [
              "attribute:parent:parent-initial:namespaced:null",
              "attribute:parent:namespaced:null:null",
            ],
            "inertTemplateLifecycle": [],
            "exceptionRecovery": ["failed", true, "custom", 2],
            "invalidNameError": "SyntaxError",
            "duplicateNameError": "NotSupportedError",
            "mismatchedBuiltinPrototypeError": "NotSupportedError",
            "invalidWhenDefinedError": "SyntaxError",
            "lifecycleCounts": {
                "constructor": 6,
                "attribute": 5,
                "connected": 6,
                "disconnected": 5,
            },
        })
    );

    let mutation_routes = session
        .script(
            r##"(() => {
              const trace = globalThis.__glassMutationTrace;
              const directTemplateCustom = globalThis.__glassDirectTemplateCustom;
              const directNestedTemplateCustom = globalThis.__glassDirectNestedTemplateCustom;
              const ordinaryTemplateCustom = globalThis.__glassOrdinaryTemplateCustom;
              const templateIdentities = [
                directTemplateCustom === document.getElementById('direct-template-host').content.querySelector('#direct-template-custom'),
                directNestedTemplateCustom === document.getElementById('direct-template-host').content.querySelector('#direct-template-custom').querySelector('template').content.querySelector('#direct-nested-template-custom'),
                ordinaryTemplateCustom === document.getElementById('ordinary-template-host').firstElementChild.content.querySelector('#ordinary-template-custom'),
              ];
              directTemplateCustom.setAttribute('data-value', 'direct-mutated');
              directNestedTemplateCustom.setAttribute('data-value', 'direct-nested-mutated');
              ordinaryTemplateCustom.setAttribute('data-value', 'ordinary-mutated');
              const templateValues = [
                directTemplateCustom.getAttribute('data-value'),
                directNestedTemplateCustom.getAttribute('data-value'),
                ordinaryTemplateCustom.getAttribute('data-value'),
              ];
              const templateLifecycle = globalThis.__glassInertTemplateLifecycleTrace.splice(0);
              const mutationHost = document.getElementById('mutation-host');
              const parentSameIdentity = document.getElementById('parent') === globalThis.__glassMutationParent;
              const childSameIdentity = document.getElementById('child') === globalThis.__glassMutationChild;
              const bootstrapReactions = trace.splice(0);
              mutationHost.textContent = 'cleared';
              const textContent = trace.splice(0);
              mutationHost.innerHTML = "<x-mutation-probe id='replacement'></x-mutation-probe>";
              const replacement = trace.splice(0);
              mutationHost.innerHTML = '';
              const innerHtmlRemoval = trace.splice(0);
              return { parentSameIdentity, childSameIdentity, templateIdentities, templateValues, templateLifecycle, bootstrapReactions, textContent, replacement, innerHtmlRemoval };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        mutation_routes.value,
        serde_json::json!({
            "parentSameIdentity": true,
            "childSameIdentity": true,
            "templateIdentities": [true, true, true],
            "templateValues": ["direct-mutated", "direct-nested-mutated", "ordinary-mutated"],
            "templateLifecycle": [],
            "bootstrapReactions": [],
            "textContent": ["disconnected:parent", "disconnected:child"],
            "replacement": ["construct:replacement", "connected:replacement"],
            "innerHtmlRemoval": ["disconnected:replacement"],
        })
    );

    let template_mutations_persisted = session
        .script(
            "(() => { const direct = globalThis.__glassDirectTemplateCustom; const directNested = globalThis.__glassDirectNestedTemplateCustom; const ordinary = globalThis.__glassOrdinaryTemplateCustom; return { directSame: direct === document.getElementById('direct-template-host').content.querySelector('#direct-template-custom'), directValue: direct.getAttribute('data-value'), directNestedSame: directNested === document.getElementById('direct-template-host').content.querySelector('#direct-template-custom').querySelector('template').content.querySelector('#direct-nested-template-custom'), directNestedValue: directNested.getAttribute('data-value'), ordinarySame: ordinary === document.getElementById('ordinary-template-host').firstElementChild.content.querySelector('#ordinary-template-custom'), ordinaryValue: ordinary.getAttribute('data-value'), stillInert: [!(direct instanceof globalThis.__glassMutationProbeConstructor), !(directNested instanceof globalThis.__glassMutationProbeConstructor), !(ordinary instanceof globalThis.__glassMutationProbeConstructor)], lifecycle: globalThis.__glassInertTemplateLifecycleTrace.slice() }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        template_mutations_persisted.value,
        serde_json::json!({
            "directSame": true,
            "directValue": "direct-mutated",
            "directNestedSame": true,
            "directNestedValue": "direct-nested-mutated",
            "ordinarySame": true,
            "ordinaryValue": "ordinary-mutated",
            "stillInert": [true, true, true],
            "lifecycle": [],
        })
    );

    let refreshed = session
        .script(
            "(() => { const parser = document.querySelector('#parser'); return { registryStable: window.customElements === document.customElementRegistry, defined: customElements.get('x-probe') === globalThis.__glassCustomElementConstructor, parserStillCustom: parser instanceof globalThis.__glassCustomElementConstructor, parserState: parser.__glassCustomElementState || null, parserPrototypeRestored: Object.getPrototypeOf(parser) === globalThis.__glassCustomElementConstructor.prototype, parserHasDefinition: Boolean(parser.__glassCustomElementDefinition), parserCount: document.querySelectorAll('x-probe').length }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        refreshed.value,
        serde_json::json!({
            "registryStable": true,
            "defined": true,
            "parserStillCustom": true,
            "parserState": "custom",
            "parserPrototypeRestored": true,
            "parserHasDefinition": true,
            "parserCount": 1,
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_customized_builtins_preserve_interface_is_and_lifecycle() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_customized_builtins_preserve_interface_is_and_lifecycle_inner());
    });
}

async fn native_customized_builtins_preserve_interface_is_and_lifecycle_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native customized built-in request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<!doctype html><html><body><button id='parser' is='x-plastic-button' data-value='parser' data-label='parser'>P</button><div id='wrong-local' is='x-plastic-button'></div><button id='attribute-only'></button></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/customized-builtins")),
    )
    .await
    .unwrap();
    let result = session
        .script(
            r##"await (async () => {
              const trace = [];
              class PlasticButton extends HTMLButtonElement {
                static get observedAttributes() { return ['data-value']; }
                constructor() {
                  super();
                  trace.push('construct:' + this.localName);
                }
                attributeChangedCallback(name, oldValue, newValue) {
                  trace.push('attribute:' + name + ':' + oldValue + ':' + newValue);
                }
                connectedCallback() {
                  trace.push('connected:' + this.getAttribute('data-label'));
                }
              }
              customElements.define('x-plastic-button', PlasticButton, { extends: 'button' });

              const parser = document.getElementById('parser');
              const objectCreated = document.createElement('button', { is: 'x-plastic-button' });
              objectCreated.setAttribute('data-label', 'object');
              const stringCreated = document.createElement('button', 'x-plastic-button');
              stringCreated.setAttribute('data-label', 'string');
              const namespaceCreated = document.createElementNS(
                'http://www.w3.org/1999/xhtml', 'button', { is: 'x-plastic-button' },
              );
              namespaceCreated.setAttribute('data-label', 'namespace');
              const directCreated = new PlasticButton();
              directCreated.setAttribute('data-label', 'direct');
              await Promise.resolve();
              const imported = document.importNode(objectCreated, true);
              const cloned = objectCreated.cloneNode(true);
              document.body.append(
                objectCreated, stringCreated, namespaceCreated,
                directCreated, imported, cloned,
              );

              let clickCount = 0;
              objectCreated.addEventListener('click', () => { clickCount += 1; });
              objectCreated.click();

              const wrongLocal = document.getElementById('wrong-local');
              const attributeOnly = document.getElementById('attribute-only');
              attributeOnly.setAttribute('is', 'x-plastic-button');
              document.body.append(attributeOnly);
              const captureDefineError = (name, constructor, extendsName) => {
                try { customElements.define(name, constructor, { extends: extendsName }); }
                catch (error) { return error.name; }
                return null;
              };
              const wrongBaseError = captureDefineError(
                'x-wrong-button-base', class extends HTMLElement {}, 'button',
              );
              const unknownBaseError = captureDefineError(
                'x-unknown-button-base', class extends HTMLElement {}, 'made-up-element',
              );
              const customBaseError = captureDefineError(
                'x-custom-button-base', class extends HTMLElement {}, 'x-other-element',
              );
              let conflictingCreationOptionsError = null;
              try {
                document.createElement('button', {
                  is: 'x-plastic-button', customElementRegistry: customElements,
                });
              } catch (error) { conflictingCreationOptionsError = error.name; }
              const instances = [
                objectCreated, stringCreated, namespaceCreated,
                directCreated, imported, cloned,
              ].map((element) => [
                element.localName,
                element instanceof PlasticButton,
                element instanceof HTMLButtonElement,
                element.getAttribute('is'),
                element.outerHTML.includes('is="x-plastic-button"'),
                element.__glassCustomElementState,
              ]);
              return {
                parser: [
                  parser instanceof PlasticButton,
                  parser instanceof HTMLButtonElement,
                  parser.getAttribute('is'),
                ],
                instances,
                wrongLocal: [wrongLocal.localName, wrongLocal instanceof PlasticButton],
                attributeOnly: [attributeOnly.getAttribute('is'), attributeOnly instanceof PlasticButton],
                customName: customElements.getName(PlasticButton),
                wrongBaseError,
                unknownBaseError,
                customBaseError,
                unpublishedInvalidDefinitions: [
                  customElements.get('x-wrong-button-base') === undefined,
                  customElements.get('x-unknown-button-base') === undefined,
                  customElements.get('x-custom-button-base') === undefined,
                ],
                conflictingCreationOptionsError,
                constructedCount: trace.filter((item) => item === 'construct:button').length,
                attributeTrace: trace.filter((item) => item.startsWith('attribute:')),
                connectionTrace: trace.filter((item) => item.startsWith('connected:')),
                serialized: objectCreated.outerHTML,
                clickCount,
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!({
            "parser": [true, true, "x-plastic-button"],
            "instances": [
                ["button", true, true, null, true, "custom"],
                ["button", true, true, null, true, "custom"],
                ["button", true, true, null, true, "custom"],
                ["button", true, true, null, true, "custom"],
                ["button", true, true, null, true, "custom"],
                ["button", true, true, null, true, "custom"],
            ],
            "wrongLocal": ["div", false],
            "attributeOnly": ["x-plastic-button", false],
            "customName": "x-plastic-button",
            "wrongBaseError": "NotSupportedError",
            "unknownBaseError": "NotSupportedError",
            "customBaseError": "NotSupportedError",
            "unpublishedInvalidDefinitions": [true, true, true],
            "conflictingCreationOptionsError": "NotSupportedError",
            "constructedCount": 7,
            "attributeTrace": ["attribute:data-value:null:parser"],
            "connectionTrace": [
                "connected:parser",
                "connected:object",
                "connected:string",
                "connected:namespace",
                "connected:direct",
                "connected:object",
                "connected:object",
            ],
            "serialized": "<button data-label=\"object\" is=\"x-plastic-button\"></button>",
            "clickCount": 1,
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_custom_elements_have_frame_local_registries_in_selected_frame_realms() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_custom_elements_have_frame_local_registries_in_selected_frame_realms_inner(),
        );
    });
}

async fn native_custom_elements_have_frame_local_registries_in_selected_frame_realms_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native frame custom-element request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<!doctype html><body><x-frame-probe id='parent-parser' data-value='parent'></x-frame-probe><button id='parent-button' is='x-frame-button'></button><iframe id='child' src='/child'></iframe></body>"
                }
                Some("/child") => {
                    "<!doctype html><body><x-frame-probe id='child-parser' data-value='child'></x-frame-probe><button id='child-button' is='x-frame-button'></button></body>"
                }
                other => panic!("unexpected native frame custom-element path: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin child frame")
        .id
        .clone();

    session.select_frame(&child_id).await.unwrap();
    let child_definition = session
        .script(
            r##"await (async () => {
              const trace = [];
              const parserElement = document.getElementById('child-parser');
              const customizedButton = document.getElementById('child-button');
              const pending = customElements.whenDefined('x-frame-probe');
              class FrameProbe extends HTMLElement {
                static get observedAttributes() { return ['data-value']; }
                constructor() { super(); trace.push('construct:' + this.id); }
                attributeChangedCallback(name, oldValue, newValue, namespace) {
                  trace.push('attribute:' + name + ':' + oldValue + ':' + newValue + ':' + namespace);
                }
                connectedCallback() { trace.push('connected:' + this.id); }
              }
              customElements.define('x-frame-probe', FrameProbe);
              class FrameButton extends HTMLButtonElement { constructor() { super(); } }
              customElements.define('x-frame-button', FrameButton, { extends: 'button' });
              const resolved = await pending;
              globalThis.__frameCustomElementRegistry = customElements;
              globalThis.__frameCustomElementConstructor = FrameProbe;
              globalThis.__frameCustomElementParser = parserElement;
              globalThis.__frameCustomElementTrace = trace;
              globalThis.__frameCustomizedButtonConstructor = FrameButton;
              globalThis.__frameCustomizedButton = customizedButton;
              return {
                registryIsWindowLocal: window.customElements === customElements
                  && document.customElementRegistry === customElements,
                resolved: resolved === FrameProbe,
                parserUpgraded: parserElement instanceof FrameProbe,
                ownerDocument: parserElement.ownerDocument === document,
                customizedBuiltIn: [
                  customizedButton instanceof FrameButton,
                  customizedButton instanceof HTMLButtonElement,
                  customizedButton.localName,
                  customizedButton.getAttribute('is'),
                ],
                trace,
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        child_definition.value,
        serde_json::json!({
            "registryIsWindowLocal": true,
            "resolved": true,
            "parserUpgraded": true,
            "ownerDocument": true,
            "customizedBuiltIn": [true, true, "button", "x-frame-button"],
            "trace": [
                "construct:child-parser",
                "attribute:data-value:null:child:null",
                "connected:child-parser",
            ],
        })
    );

    session.select_frame("native-context:main").await.unwrap();
    let parent_definition = session
        .script(
            r##"(() => {
              const absentBeforeDefinition = customElements.get('x-frame-probe') === undefined;
              const parserElement = document.getElementById('parent-parser');
              const customizedButton = document.getElementById('parent-button');
              class ParentProbe extends HTMLElement {
                constructor() { super(); }
                connectedCallback() { globalThis.__parentCustomElementConnected = this.id; }
              }
              class ParentButton extends HTMLButtonElement { constructor() { super(); } }
              customElements.define('x-frame-button', ParentButton, { extends: 'button' });
              customElements.define('x-frame-probe', ParentProbe);
              globalThis.__parentCustomElementRegistry = customElements;
              globalThis.__parentCustomElementConstructor = ParentProbe;
              return {
                registryIsWindowLocal: window.customElements === customElements
                  && document.customElementRegistry === customElements,
                absentBeforeDefinition,
                parentParserUpgraded: parserElement instanceof ParentProbe,
                connected: globalThis.__parentCustomElementConnected,
                parentLookup: customElements.get('x-frame-probe') === ParentProbe,
                customizedBuiltIn: [
                  customizedButton instanceof ParentButton,
                  customizedButton instanceof HTMLButtonElement,
                  customizedButton.localName,
                  customizedButton.getAttribute('is'),
                ],
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        parent_definition.value,
        serde_json::json!({
            "registryIsWindowLocal": true,
            "absentBeforeDefinition": true,
            "parentParserUpgraded": true,
            "connected": "parent-parser",
            "parentLookup": true,
            "customizedBuiltIn": [true, true, "button", "x-frame-button"],
        })
    );

    session.select_frame(&child_id).await.unwrap();
    let child_refreshed = session
        .script(
            r##"(() => {
              const parserElement = document.getElementById('child-parser');
              parserElement.setAttribute('data-value', 'updated');
              return {
                registryStable: window.customElements === globalThis.__frameCustomElementRegistry,
                parentConstructorIsDifferent: globalThis.__frameCustomElementConstructor
                  !== globalThis.__parentCustomElementConstructor,
                definitionStable: customElements.get('x-frame-probe')
                  === globalThis.__frameCustomElementConstructor,
                parserStable: parserElement === globalThis.__frameCustomElementParser
                  && parserElement instanceof globalThis.__frameCustomElementConstructor,
                customizedButtonStable: document.getElementById('child-button')
                  === globalThis.__frameCustomizedButton
                  && globalThis.__frameCustomizedButton
                    instanceof globalThis.__frameCustomizedButtonConstructor,
                customizedButtonRegistryStable: customElements.get('x-frame-button')
                  === globalThis.__frameCustomizedButtonConstructor,
                ownerDocument: parserElement.ownerDocument === document,
                trace: globalThis.__frameCustomElementTrace,
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        child_refreshed.value,
        serde_json::json!({
            "registryStable": true,
            "parentConstructorIsDifferent": true,
            "definitionStable": true,
            "parserStable": true,
            "customizedButtonStable": true,
            "customizedButtonRegistryStable": true,
            "ownerDocument": true,
            "trace": [
                "construct:child-parser",
                "attribute:data-value:null:child:null",
                "connected:child-parser",
                "attribute:data-value:child:updated:null",
            ],
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_custom_elements_recover_from_reentrancy_and_respect_bounds() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_custom_elements_recover_from_reentrancy_and_respect_bounds_inner());
    });
}

async fn native_custom_elements_recover_from_reentrancy_and_respect_bounds_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native custom-element reentrancy request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<!doctype html><html><body></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/custom-element-reentrancy")),
    )
    .await
    .unwrap();
    let result = session
        .script(
            r##"await (async () => {
              let reentrantDefinitionError = null;
              const reentrantInnerConstructor = class extends HTMLElement {};
              class ReentrantDefinitionProbe extends HTMLElement {
                static get observedAttributes() {
                  try { customElements.define('x-reentrant-inner', reentrantInnerConstructor); }
                  catch (error) { reentrantDefinitionError = error.name; }
                  return [];
                }
                attributeChangedCallback() {}
              }
              customElements.define('x-reentrant-outer', ReentrantDefinitionProbe);
              const reentrantDefinitionWasAtomic = customElements.get('x-reentrant-inner') === undefined;
              customElements.define('x-reentrant-inner', reentrantInnerConstructor);

              let observedAttributeIterationCount = 0;
              let observedAttributeIteratorClosed = false;
              function* oversizedObservedAttributes() {
                try {
                  for (let index = 0; index < 10000; index += 1) {
                    observedAttributeIterationCount += 1;
                    yield `data-attribute-${index}`;
                  }
                } finally { observedAttributeIteratorClosed = true; }
              }
              let observedAttributeLimitError = null;
              class OversizedObservedAttributesProbe extends HTMLElement {
                static get observedAttributes() { return oversizedObservedAttributes(); }
                attributeChangedCallback() {}
              }
              try { customElements.define('x-observed-bound', OversizedObservedAttributesProbe); }
              catch (error) { observedAttributeLimitError = error.name; }
              const observedAttributeDefinitionWasAtomic = customElements.get('x-observed-bound') === undefined;
              class ObservedAttributeBoundRecovery extends HTMLElement {}
              customElements.define('x-observed-bound', ObservedAttributeBoundRecovery);

              let rejectedDefinitionMessage = null;
              class RejectedDefinitionProbe extends HTMLElement {
                static get observedAttributes() { throw new Error('observed attributes rejected'); }
                attributeChangedCallback() {}
              }
              try { customElements.define('x-definition-retry', RejectedDefinitionProbe); }
              catch (error) { rejectedDefinitionMessage = error.message; }
              const rejectedDefinitionWasAtomic = customElements.get('x-definition-retry') === undefined;
              class RetriedDefinitionProbe extends HTMLElement {}
              customElements.define('x-definition-retry', RetriedDefinitionProbe);

              const nestedReactionOrder = [];
              class ReentrantReactionChild extends HTMLElement {
                connectedCallback() { nestedReactionOrder.push('child:connected'); }
              }
              class ReentrantReactionParent extends HTMLElement {
                connectedCallback() {
                  nestedReactionOrder.push('parent:start');
                  this.appendChild(document.createElement('x-reentrant-reaction-child'));
                  nestedReactionOrder.push('parent:end');
                }
              }
              customElements.define('x-reentrant-reaction-child', ReentrantReactionChild);
              customElements.define('x-reentrant-reaction-parent', ReentrantReactionParent);
              document.body.appendChild(document.createElement('x-reentrant-reaction-parent'));

              const queuedReactionOrder = [];
              const queuedReactionRoot = document.createElement('div');
              queuedReactionRoot.innerHTML = Array.from(
                { length: 300 },
                (_, index) => `<x-q n="${index}"></x-q>`,
              ).join('');
              class QueuedReactionProbe extends HTMLElement {
                static get observedAttributes() { return ['n']; }
                attributeChangedCallback(name, oldValue, newValue) {
                  queuedReactionOrder.push(Number(newValue));
                }
              }
              customElements.define('x-q', QueuedReactionProbe);
              customElements.upgrade(queuedReactionRoot);
              const queuedReactionOrderIsStable = queuedReactionOrder.length === 300
                && queuedReactionOrder.every((value, index) => value === index);

              const pendingDefinitions = [];
              for (let index = 0; index < 1024; index += 1) {
                pendingDefinitions.push(customElements.whenDefined(`x-pending-limit-${index}`));
              }
              let pendingLimitError = null;
              try { await customElements.whenDefined('x-pending-limit-overflow'); }
              catch (error) { pendingLimitError = error.name; }
              class PendingCapacityProbe extends HTMLElement {}
              customElements.define('x-pending-limit-0', PendingCapacityProbe);
              const pendingCapacityResolved = await pendingDefinitions[0] === PendingCapacityProbe;
              const releasedPendingDefinition = customElements.whenDefined('x-pending-limit-overflow');
              class ReleasedPendingProbe extends HTMLElement {}
              customElements.define('x-pending-limit-overflow', ReleasedPendingProbe);
              const pendingCapacityRecovered = await releasedPendingDefinition === ReleasedPendingProbe;

              return {
                reentrantDefinitionError,
                reentrantDefinitionWasAtomic,
                reentrantDefinitionRecovery: customElements.get('x-reentrant-inner') === reentrantInnerConstructor,
                observedAttributeLimitError,
                observedAttributeIterationCount,
                observedAttributeIteratorClosed,
                observedAttributeDefinitionWasAtomic,
                observedAttributeDefinitionRecovered: customElements.get('x-observed-bound')
                  === ObservedAttributeBoundRecovery,
                rejectedDefinitionMessage,
                rejectedDefinitionWasAtomic,
                rejectedDefinitionRecovery: customElements.get('x-definition-retry') === RetriedDefinitionProbe,
                nestedReactionOrder,
                queuedReactionOrderIsStable,
                pendingLimitError,
                pendingCapacityResolved,
                pendingCapacityRecovered,
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!({
            "reentrantDefinitionError": "NotSupportedError",
            "reentrantDefinitionWasAtomic": true,
            "reentrantDefinitionRecovery": true,
            "observedAttributeLimitError": "RangeError",
            "observedAttributeIterationCount": 257,
            "observedAttributeIteratorClosed": true,
            "observedAttributeDefinitionWasAtomic": true,
            "observedAttributeDefinitionRecovered": true,
            "rejectedDefinitionMessage": "observed attributes rejected",
            "rejectedDefinitionWasAtomic": true,
            "rejectedDefinitionRecovery": true,
            "nestedReactionOrder": ["parent:start", "parent:end", "child:connected"],
            "queuedReactionOrderIsStable": true,
            "pendingLimitError": "RangeError",
            "pendingCapacityResolved": true,
            "pendingCapacityRecovered": true,
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_custom_element_reaction_checkpoint_work_is_bounded_and_recovers() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_custom_element_reaction_checkpoint_work_is_bounded_and_recovers_inner(),
        );
    });
}

async fn native_custom_element_reaction_checkpoint_work_is_bounded_and_recovers_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native custom-element checkpoint-bound request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<!doctype html><html><body></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/custom-element-checkpoint-bound")),
    )
    .await
    .unwrap();
    let result = session
        .script(
            r##"(() => {
              let callbackCount = 0;
              let checkpointLimitReportCount = 0;
              let unexpectedReportCount = 0;
              globalThis.reportError = error => {
                if (error instanceof RangeError
                    && String(error.message).includes('reaction checkpoint exceeded its work limit')) {
                  checkpointLimitReportCount += 1;
                } else {
                  unexpectedReportCount += 1;
                }
              };
              class WorkLimitProbe extends HTMLElement {
                connectedCallback() {
                  callbackCount += 1;
                  if (callbackCount < 10002) {
                    globalThis.__glassQueueNativeCustomElementReaction(
                      this,
                      'connectedCallback',
                      [],
                    );
                  }
                }
              }
              customElements.define('x-work-limit-probe', WorkLimitProbe);
              document.body.appendChild(document.createElement('x-work-limit-probe'));

              let recoveryCallbackCount = 0;
              class RecoveryProbe extends HTMLElement {
                connectedCallback() { recoveryCallbackCount += 1; }
              }
              customElements.define('x-work-limit-recovery', RecoveryProbe);
              document.body.appendChild(document.createElement('x-work-limit-recovery'));

              return {
                callbackCount,
                checkpointLimitReportCount,
                unexpectedReportCount,
                recoveryCallbackCount,
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!({
            "callbackCount": 10000,
            "checkpointLimitReportCount": 1,
            "unexpectedReportCount": 0,
            "recoveryCallbackCount": 1,
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
