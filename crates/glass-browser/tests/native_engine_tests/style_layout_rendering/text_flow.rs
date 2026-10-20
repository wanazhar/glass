use super::*;

#[test]
fn native_text_indent_shifts_only_block_first_lines_and_preserves_shared_consumers() {
    let document = NativeDocument::parse(
        "<style>#indented { display:block; width:48px; text-indent:16px; } #broken { display:block; width:48px; text-indent:16px; white-space:pre; } #inline-parent { display:block; width:48px; } #wide { display:block; width:48px; text-indent:16px; white-space:nowrap; }</style><div id='indented'>AB CD EF</div><div id='broken'>AB\nCD</div><div id='inline-parent'><span id='inline' style='text-indent:16px'>GH</span> IJ</div><div id='wide'>ABCDEFGH</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 100,
        device_scale_factor_milli: 1000,
    };
    let indented = document.resolve_target("id=indented").unwrap();
    let broken = document.resolve_target("id=broken").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let wide = document.resolve_target("id=wide").unwrap();
    let layout = document.layout(viewport).unwrap();

    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs_for(indented),
        vec![
            (NativePoint { x: 16, y: 0 }, "AB"),
            (NativePoint { x: 0, y: 20 }, "CD"),
            (NativePoint { x: 16, y: 20 }, " EF"),
        ]
    );
    assert_eq!(
        runs_for(broken),
        vec![
            (NativePoint { x: 16, y: 40 }, "AB"),
            (NativePoint { x: 0, y: 60 }, "CD"),
        ]
    );
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.node_id == inline && run.origin == NativePoint { x: 0, y: 80 })
    );
    assert_eq!(
        runs_for(wide),
        vec![(NativePoint { x: 16, y: 100 }, "ABCDEFGH")]
    );
    assert!(layout.content_width >= 80);
    assert!(layout.max_scroll_offset().x > 0);

    let list = document.display_list(viewport).unwrap();
    for run in &layout.text_runs {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::TextRun { node_id, origin, text, .. }
                    if *node_id == run.node_id && *origin == run.origin && text == &run.text
            )
        }));
    }
    assert_eq!(layout.hit_test(17, 1).unwrap(), Some(indented));
}

#[test]
fn native_text_indent_inherit_reaches_first_line_layout_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#parent{display:block;width:48px;text-indent:16px;}#inherit,#omitted,#invalid{display:block;width:48px;height:20px;line-height:20px;}#inherit{text-indent:inherit;background-color:red;}#omitted{background-color:green;}#invalid{text-indent:inherit 1px;background-color:blue;}</style><div id='parent'><button id='inherit'>AB CD</button><button id='omitted'>EF GH</button><button id='invalid'>IJ KL</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let viewport = Viewport {
        width: 56,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let first_run = |node_id| {
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .expect("text run")
    };
    assert_eq!(first_run(inherit).origin, NativePoint { x: 16, y: 0 });
    assert_eq!(first_run(omitted).origin, NativePoint { x: 0, y: 20 });
    assert_eq!(first_run(invalid).origin, NativePoint { x: 0, y: 40 });

    let semantic_nodes = document.semantic_nodes();
    let source_position = |node_id| {
        semantic_nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .unwrap()
    };
    assert!(source_position(inherit) < source_position(omitted));
    assert!(source_position(omitted) < source_position(invalid));

    let list = document.display_list(viewport).unwrap();
    for run in &layout.text_runs {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::TextRun { node_id, origin, text, .. }
                    if *node_id == run.node_id && *origin == run.origin && text == &run.text
            )
        }));
    }
    let invalid_rect = layout.box_for(invalid).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == invalid
                    && *rect == invalid_rect
                    && *color == NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    }
        )
    }));
    assert_eq!(
        layout.hit_test(i64::from(invalid_rect.x + 1), i64::from(invalid_rect.y + 1),),
        Ok(Some(invalid))
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(invalid_rect.x + 1, invalid_rect.y + 1),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-indent"
    }));
}

#[test]
fn native_text_overflow_inherit_reaches_truncation_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#parent{display:block;width:40px;text-overflow:ellipsis;}#inherit,#omitted,#invalid{display:block;width:40px;height:20px;line-height:20px;white-space:nowrap;overflow-x:clip;}#inherit{text-overflow:inherit;background-color:red;}#omitted{background-color:green;}#invalid{text-overflow:inherit ellipsis;background-color:blue;}</style><div id='parent'><button id='inherit'>ABCDEFG</button><button id='omitted'>ABCDEFG</button><button id='invalid'>ABCDEFG</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let viewport = Viewport {
        width: 48,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let first_run = |node_id| {
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .expect("text run")
    };
    assert_eq!(
        (
            first_run(inherit).text.as_str(),
            first_run(inherit).truncated
        ),
        ("AB...", true)
    );
    assert_eq!(
        (
            first_run(omitted).text.as_str(),
            first_run(omitted).truncated
        ),
        ("ABCDEFG", false)
    );
    assert_eq!(
        (
            first_run(invalid).text.as_str(),
            first_run(invalid).truncated
        ),
        ("ABCDEFG", false)
    );
    assert_eq!(layout.max_scroll_offset().x, 0);

    let semantic_nodes = document.semantic_nodes();
    let source_position = |node_id| {
        semantic_nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .unwrap()
    };
    assert!(source_position(inherit) < source_position(omitted));
    assert!(source_position(omitted) < source_position(invalid));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                text,
                truncated,
                ..
            } if *node_id == inherit && text == "AB..." && *truncated
        )
    }));
    let invalid_rect = layout.box_for(invalid).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == invalid
                    && *rect == invalid_rect
                    && *color == NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    }
        )
    }));
    assert_eq!(
        layout.hit_test(i64::from(invalid_rect.x + 1), i64::from(invalid_rect.y + 1),),
        Ok(Some(invalid))
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(invalid_rect.x + 1, invalid_rect.y + 1),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-overflow"
    }));
}

#[test]
fn native_word_spacing_shares_width_across_flow_paint_and_overflow() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:40px; word-spacing:4px; } #pre { display:block; width:32px; word-spacing:4px; white-space:pre-wrap; } #wide { display:block; width:32px; word-spacing:4px; white-space:nowrap; } #align { display:block; width:40px; word-spacing:4px; text-align:right; white-space:nowrap; } #parent { display:block; width:72px; word-spacing:4px; } #override { word-spacing:8px; }</style><div id='normal'>A   B C</div><div id='pre'>A  B</div><div id='wide'>A B C D E F G H</div><div id='align'>A B</div><div id='parent'>A <span id='override'>B C</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let wide = document.resolve_target("id=wide").unwrap();
    let align = document.resolve_target("id=align").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let override_id = document.resolve_target("id=override").unwrap();

    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs_for(normal),
        vec![
            (NativePoint { x: 0, y: 0 }, "A"),
            (NativePoint { x: 8, y: 0 }, " B"),
            (NativePoint { x: 0, y: 20 }, "C")
        ]
    );
    assert_eq!(
        runs_for(pre),
        vec![
            (NativePoint { x: 0, y: 40 }, "A  "),
            (NativePoint { x: 0, y: 60 }, "B")
        ]
    );

    let wide_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == wide)
        .unwrap();
    assert_eq!(wide_run.origin.x, 0);
    assert_eq!(wide_run.text, "A B C D E F G H");
    assert_eq!(layout.content_width, 148);
    assert_eq!(layout.max_scroll_offset().x, 68);

    let align_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == align)
        .unwrap();
    assert_eq!(align_run.origin.x, 12);
    assert_eq!(align_run.text, "A B");

    let parent_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == parent)
        .unwrap();
    let override_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == override_id)
        .map(|run| (run.origin, run.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(parent_run.text, "A");
    assert_eq!(
        override_runs,
        vec![
            (NativePoint { x: 20, y: 120 }, "B"),
            (NativePoint { x: 28, y: 120 }, " C"),
        ]
    );

    let list = document.display_list(viewport).unwrap();
    let spacing_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                word_spacing,
                ..
            } if *command_node == node_id => Some(*word_spacing),
            _ => None,
        })
    };
    assert_eq!(spacing_for(normal), Some(4));
    assert_eq!(spacing_for(wide), Some(4));
    assert_eq!(spacing_for(override_id), Some(8));

    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(wide_run.origin.x + 16, wide_run.origin.y),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(wide_run.origin.x + 6, wide_run.origin.y),
        Some([255, 255, 255, 255])
    );
}

#[test]
fn native_text_fragments_follow_inline_flow_and_source_order() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:24px'>AB<span id='middle' style='display:inline;width:8px;color:red'>C</span>DE</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let middle = document.resolve_target("id=middle").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(container),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 40,
        })
    );
    assert_eq!(layout.text_runs.len(), 3);
    assert_eq!(layout.text_runs[0].node_id, container);
    assert_eq!(layout.text_runs[0].origin, NativePoint { x: 0, y: 0 });
    assert_eq!(layout.text_runs[0].text, "AB");
    assert!(layout.text_runs[0].starts_line);
    assert!(!layout.text_runs[0].ends_line);
    assert_eq!(layout.text_runs[1].node_id, middle);
    assert_eq!(layout.text_runs[1].origin, NativePoint { x: 16, y: 0 });
    assert_eq!(layout.text_runs[1].text, "C");
    assert!(!layout.text_runs[1].starts_line);
    assert!(layout.text_runs[1].ends_line);
    assert_eq!(layout.text_runs[2].node_id, container);
    assert_eq!(layout.text_runs[2].origin, NativePoint { x: 0, y: 20 });
    assert_eq!(layout.text_runs[2].text, "DE");
    assert!(layout.text_runs[2].starts_line && layout.text_runs[2].ends_line);

    let list = document.display_list(viewport).unwrap();
    let runs = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                truncated,
                ..
            } => Some((*node_id, *origin, text.as_str(), *truncated)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![
            (container, NativePoint { x: 0, y: 0 }, "AB", false),
            (middle, NativePoint { x: 16, y: 0 }, "C", false),
            (container, NativePoint { x: 0, y: 20 }, "DE", false),
        ]
    );

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(17, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 20), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_fragments_use_collapsed_bounded_text_for_width_and_paint() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:16px'> A   B </div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 40);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "A", false),
            (NativePoint { x: 0, y: 20 }, "B", false),
        ]
    );
}

#[test]
fn native_text_fragments_wrap_words_and_split_only_wide_words() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:24px'>AB CD EFGHI</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 80);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "AB"),
            (NativePoint { x: 0, y: 20 }, "CD"),
            (NativePoint { x: 0, y: 40 }, "EFG"),
            (NativePoint { x: 0, y: 60 }, "HI"),
        ]
    );
}

#[test]
fn native_word_break_break_all_splits_collapsed_words_without_changing_semantics() {
    let document = NativeDocument::parse(
        "<div id='normal' style='display:block;width:40px;word-break:normal'>ABC DEFG</div><div id='break' style='display:block;width:40px;word-break:break-all'>ABC DEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let break_all = document.resolve_target("id=break").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(normal).unwrap().height, 40);
    assert_eq!(layout.box_for(break_all).unwrap().height, 40);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == normal)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "ABC"),
            (NativePoint { x: 0, y: 20 }, "DEFG"),
        ]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == break_all)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 40 }, "ABC"),
            (NativePoint { x: 24, y: 40 }, " D"),
            (NativePoint { x: 0, y: 60 }, "EFG"),
        ]
    );

    let (visible_text, truncated) = document.visible_text(1024);
    assert!(!truncated);
    assert!(visible_text.contains("ABC DEFG"));

    let modes = NativeDocument::parse(
        "<div id='pre' style='display:block;width:24px;white-space:pre;word-break:break-all'>ABC DEFG</div><div id='nowrap' style='display:block;width:24px;white-space:nowrap;word-break:break-all'>ABC DEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let mode_layout = modes.layout(viewport).unwrap();
    for (id, y) in [("pre", 0), ("nowrap", 20)] {
        let node_id = modes.resolve_target(&format!("id={id}")).unwrap();
        assert_eq!(mode_layout.box_for(node_id).unwrap().height, 20);
        assert_eq!(
            mode_layout
                .text_runs
                .iter()
                .filter(|run| run.node_id == node_id)
                .map(|run| (run.origin, run.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(NativePoint { x: 0, y }, "ABC DEFG")]
        );
    }
}

#[test]
fn native_text_overflow_ellipsis_truncates_only_eligible_clipped_nowrap_text() {
    let document = NativeDocument::parse(
        "<div id='clip' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:clip'>ABCDEFG</div><div id='ellipsis' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'>ABCDEFG</div><div id='fit' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'>ABC</div><div id='narrow' style='display:block;width:16px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'>ABCDEFG</div><div id='nested' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'><span id='nested-child'>ABCDEFG</span></div><div id='styled' style='display:block;width:56px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis;text-transform:uppercase;word-spacing:4px;letter-spacing:2px;font-weight:bold;font-style:italic;text-decoration:underline'>ab cd ef</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 128,
        device_scale_factor_milli: 1000,
    };
    let clip = document.resolve_target("id=clip").unwrap();
    let ellipsis = document.resolve_target("id=ellipsis").unwrap();
    let fit = document.resolve_target("id=fit").unwrap();
    let narrow = document.resolve_target("id=narrow").unwrap();
    let nested_child = document.resolve_target("id=nested-child").unwrap();
    let styled = document.resolve_target("id=styled").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(clip).unwrap().height, 20);
    assert_eq!(layout.box_for(ellipsis).unwrap().height, 20);
    assert_eq!(layout.box_for(fit).unwrap().height, 20);
    assert_eq!(layout.box_for(narrow).unwrap().height, 20);
    assert_eq!(layout.max_scroll_offset().x, 0);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == clip)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 0 }, "ABCDEFG", false)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == ellipsis)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 20 }, "AB...", true)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == fit)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 40 }, "ABC", false)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == narrow)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 60 }, "AB", true)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == nested_child)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 80 }, "ABCDEFG", false)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == styled)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 100 }, "AB...", true)]
    );

    let (visible_text, truncated) = document.visible_text(1024);
    assert!(!truncated);
    assert_eq!(visible_text, "ABCDEFG ABCDEFG ABC ABCDEFG ABCDEFG ab cd ef");

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                truncated,
                ..
            } if *node_id == ellipsis
                && *origin == NativePoint { x: 0, y: 20 }
                && text == "AB..."
                && *truncated
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                text,
                truncated,
                underline,
                bold,
                italic,
                word_spacing,
                letter_spacing,
                ..
            } if *node_id == styled
                && text == "AB..."
                && *truncated
                && *underline
                && *bold
                && *italic
                && *word_spacing == 4
                && *letter_spacing == 2
        )
    }));

    let visible_overflow = NativeDocument::parse(
        "<div id='visible' style='display:block;width:40px;white-space:nowrap;text-overflow:ellipsis'>ABCDEFG</div><div id='normal' style='display:block;width:40px;text-overflow:ellipsis'>ABC DEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let visible_layout = visible_overflow.layout(viewport).unwrap();
    let visible = visible_overflow.resolve_target("id=visible").unwrap();
    let normal = visible_overflow.resolve_target("id=normal").unwrap();
    assert_eq!(
        visible_layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == visible)
            .map(|run| (run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![("ABCDEFG", false)]
    );
    assert_eq!(
        visible_layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == normal)
            .map(|run| (run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![("ABC", false), ("DEFG", false)]
    );
}

#[test]
fn native_vertical_align_moves_inline_items_and_text_within_fixed_line_box() {
    let document = NativeDocument::parse(
        "<style>#flow { display:block; width:80px; line-height:40px; } .item { display:inline-block; width:8px; height:10px; } #base { vertical-align:baseline; } #top { vertical-align:top; } #middle { vertical-align:middle; } #bottom { vertical-align:bottom; } #text-bottom { vertical-align:bottom; }</style><div id='flow'><span id='base' class='item'></span><span id='top' class='item'></span><span id='middle' class='item'></span><span id='bottom' class='item'></span><span id='text-bottom' class='item'>B</span></div><div id='direct' style='line-height:40px'>D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 96,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let base = document.resolve_target("id=base").unwrap();
    let top = document.resolve_target("id=top").unwrap();
    let middle = document.resolve_target("id=middle").unwrap();
    let bottom = document.resolve_target("id=bottom").unwrap();
    let text_bottom = document.resolve_target("id=text-bottom").unwrap();
    let direct = document.resolve_target("id=direct").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(flow).unwrap().height, 40);
    for (node_id, x, y) in [
        (base, 0, 0),
        (top, 8, 0),
        (middle, 16, 15),
        (bottom, 24, 30),
        (text_bottom, 32, 30),
    ] {
        assert_eq!(
            layout
                .box_for(node_id)
                .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
            Some((x, y, 8, 10))
        );
    }
    assert_eq!(layout.box_for(direct).unwrap().y, 40);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == text_bottom)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 32, y: 30 }, "B")]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == direct)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 40 }, "D")]
    );
}

#[test]
fn native_inherited_vertical_align_revert_layer_preserves_line_geometry() {
    let document = NativeDocument::parse(
        "<style>#flow { display:block; width:80px; line-height:40px; } .item { display:inline-block; width:8px; height:10px; } @layer base { #named { vertical-align:top; } #repeat { vertical-align:middle; } #parent { vertical-align:top; } } @layer theme { #named { vertical-align:bottom; } #repeat { vertical-align:revert-layer; } #parent { vertical-align:bottom; } } @layer top { #repeat { vertical-align:revert-layer; } } #named { vertical-align:revert-layer; } #parent { vertical-align:revert-layer; }</style><div id='flow'><span id='named' class='item'></span><span id='repeat' class='item'></span><span id='parent' class='item'><span id='child' class='item' style='vertical-align:ReVeRt-LaYeR'></span></span><span id='fallback' class='item' style='vertical-align:revert-layer'></span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "vertical-align")
    }));
    let flow = document.resolve_target("id=flow").unwrap();
    let named = document.resolve_target("id=named").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let layout = document
        .layout(Viewport {
            width: 96,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(layout.box_for(flow).unwrap().height, 40);
    assert_eq!(layout.box_for(named).unwrap().y, 30);
    assert_eq!(layout.box_for(repeat).unwrap().y, 15);
    assert_eq!(layout.box_for(parent).unwrap().y, 30);
    assert_eq!(layout.box_for(child).unwrap().y, 60);
    assert_eq!(layout.box_for(fallback).unwrap().y, 0);
    assert_eq!(layout.hit_test(1, 31).unwrap(), Some(named));
}

#[test]
fn native_text_fragments_preserve_only_source_whitespace_boundaries() {
    let adjacent = NativeDocument::parse(
        "<div id='container' style='width:16px'>A<span style='display:contents'></span>B</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let container = adjacent.resolve_target("id=container").unwrap();
    let layout = adjacent.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 20);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "A"),
            (NativePoint { x: 8, y: 0 }, "B"),
        ]
    );

    let separated = NativeDocument::parse(
        "<div id='container' style='width:40px'>A <span id='middle' style='display:inline'>B</span> C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let container = separated.resolve_target("id=container").unwrap();
    let middle = separated.resolve_target("id=middle").unwrap();
    let layout = separated
        .layout(Viewport {
            width: 48,
            height: 48,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 20);
    assert_eq!(
        layout.box_for(middle),
        Some(NativeRect {
            x: 16,
            y: 0,
            width: 8,
            height: 20,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (container, NativePoint { x: 0, y: 0 }, "A"),
            (container, NativePoint { x: 8, y: 0 }, " "),
            (middle, NativePoint { x: 16, y: 0 }, "B"),
            (container, NativePoint { x: 24, y: 0 }, " C"),
        ]
    );
}

#[test]
fn native_white_space_revert_layer_preserves_modes_and_flow() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; line-height:24px; } @layer base { #wrapped { white-space:pre; } #broken { white-space:pre-line; } #inline { white-space:nowrap; } #parent { white-space:pre; } } @layer theme { #wrapped { white-space:pre-wrap; } #broken { white-space:pre-wrap; } #inline { white-space:pre; } } @layer top { #broken { white-space:revert-layer; } } #wrapped { white-space:revert-layer; } #broken { white-space:revert-layer; }</style><div id='wrapped' class='line'>A  B C D E</div><div id='broken' class='line'>A\nB</div><div id='inline' class='line' style='white-space:REVERT-LAYER'> A  B\n C</div><div id='parent' class='line'><span id='child' style='white-space:revert-layer'> E  F</span></div><div id='fallback' class='line' style='white-space:ReVeRt-LaYeR'>A\nB</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "white-space")
    }));
    let viewport = Viewport {
        width: 40,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let runs_for = |id: &str| {
        let node = document.resolve_target(&format!("id={id}")).unwrap();
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        runs_for("wrapped"),
        vec![
            (NativePoint { x: 0, y: 0 }, "A  B"),
            (NativePoint { x: 0, y: 24 }, " C D"),
            (NativePoint { x: 0, y: 48 }, " E"),
        ]
    );
    assert_eq!(
        runs_for("broken"),
        vec![
            (NativePoint { x: 0, y: 72 }, "A"),
            (NativePoint { x: 0, y: 96 }, "B"),
        ]
    );
    assert_eq!(
        runs_for("inline"),
        vec![
            (NativePoint { x: 0, y: 120 }, " A  B"),
            (NativePoint { x: 0, y: 144 }, " C"),
        ]
    );
    assert_eq!(
        runs_for("child"),
        vec![(NativePoint { x: 0, y: 168 }, " E  F")]
    );
    assert_eq!(
        runs_for("fallback"),
        vec![
            (NativePoint { x: 0, y: 192 }, "A"),
            (NativePoint { x: 8, y: 192 }, " B"),
        ]
    );
}

#[test]
fn native_text_boundary_separator_drops_when_inline_item_wraps() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:16px'>A <span id='middle' style='display:inline'>B</span> C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let middle = document.resolve_target("id=middle").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 60);
    assert_eq!(
        layout.box_for(middle),
        Some(NativeRect {
            x: 0,
            y: 20,
            width: 8,
            height: 20,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (container, NativePoint { x: 0, y: 0 }, "A"),
            (middle, NativePoint { x: 0, y: 20 }, "B"),
            (container, NativePoint { x: 0, y: 40 }, "C"),
        ]
    );
}

#[test]
fn native_overflow_scroll_keywords_establish_axis_clips_and_scroll_owner() {
    let document = NativeDocument::parse(
        "<style>#visible,#auto,#scroll,#hidden{display:block;width:16px;height:10px;margin:0px;padding:0;border:0;}#visible{overflow:visible;}#auto{overflow-x:AUTO;overflow-y:hidden;}#scroll{overflow-x:clip;overflow-y:ScRoLl;}#hidden{overflow:hidden;}.wide{display:block;width:56px;min-width:56px;height:10px;background-color:blue;}</style><button id='visible'><span id='visible-child' class='wide'></span></button><button id='auto'><span id='auto-child' class='wide'></span></button><button id='scroll'><span id='scroll-child' class='wide'></span></button><button id='hidden'><span id='hidden-child' class='wide'></span></button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let visible = document.resolve_target("id=visible").unwrap();
    let visible_child = document.resolve_target("id=visible-child").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let auto_child = document.resolve_target("id=auto-child").unwrap();
    let scroll = document.resolve_target("id=scroll").unwrap();
    let scroll_child = document.resolve_target("id=scroll-child").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();
    let hidden_child = document.resolve_target("id=hidden-child").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let expected_visible = |y| {
        Some(NativeRect {
            x: 0,
            y,
            width: 32,
            height: 10,
        })
    };
    let expected_x_clip = |y| {
        Some(NativeRect {
            x: 0,
            y,
            width: 16,
            height: 10,
        })
    };
    assert_eq!(layout.viewport_rect_for(visible_child), expected_visible(0));
    assert_eq!(layout.viewport_rect_for(auto_child), expected_x_clip(10));
    assert_eq!(layout.viewport_rect_for(scroll_child), expected_x_clip(20));
    assert_eq!(layout.viewport_rect_for(hidden_child), expected_x_clip(30));
    assert_eq!(layout.content_width, 56);
    assert_eq!(layout.max_scroll_offset().x, 24);
    assert_eq!(layout.hit_test(20, 5).unwrap(), Some(visible_child));
    assert_ne!(layout.hit_test(20, 15).unwrap(), Some(auto_child));
    assert_ne!(layout.hit_test(20, 25).unwrap(), Some(scroll_child));
    assert_ne!(layout.hit_test(20, 35).unwrap(), Some(hidden_child));

    let semantic_nodes = document.semantic_nodes();
    let source_position = |node_id| {
        semantic_nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .unwrap()
    };
    assert!(source_position(visible) < source_position(auto));
    assert!(source_position(auto) < source_position(scroll));
    assert!(source_position(scroll) < source_position(hidden));

    let list = document.display_list(viewport).unwrap();
    let fill = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                clip,
                ..
            } if *command_node_id == node_id => Some(*clip),
            _ => None,
        })
    };
    assert_eq!(fill(visible_child), Some(None));
    assert!(fill(auto_child).is_some_and(|clip| clip.is_some()));
    assert!(fill(scroll_child).is_some_and(|clip| clip.is_some()));
    assert!(fill(hidden_child).is_some_and(|clip| clip.is_some()));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(20, 5), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(20, 15), Some([255, 255, 255, 255]));
    assert_ne!(surface.pixel(20, 25), Some([0, 0, 255, 255]));
    assert_ne!(surface.pixel(20, 35), Some([0, 0, 255, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(!document.diagnostics().iter().any(|diagnostic| {
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
