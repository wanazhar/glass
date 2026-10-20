use super::*;

#[test]
fn native_text_alignment_shifts_complete_fixed_cell_line_items() {
    let document = NativeDocument::parse(
        "<style>#center { display: block; width: 32px; text-align: center; } #right { display: block; width: 32px; text-align: right; } #inline { display: block; width: 32px; text-align: center; } #chip { display: inline-block; width: 8px; height: 8px; background-color: red; }</style><div id='center'>A B C</div><div id='right'>D</div><div id='inline'><span id='chip'></span>Q</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let center = document.resolve_target("id=center").unwrap();
    let right = document.resolve_target("id=right").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let chip = document.resolve_target("id=chip").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(center).unwrap().width, 32);
    assert_eq!(layout.box_for(center).unwrap().height, 40);
    assert_eq!(layout.box_for(right).unwrap().y, 40);
    assert_eq!(layout.box_for(inline).unwrap().y, 60);
    assert_eq!(layout.box_for(chip).unwrap().x, 8);
    assert_eq!(layout.box_for(chip).unwrap().y, 60);
    assert_eq!(layout.hit_test(9, 61).unwrap(), Some(chip));

    let center_text = layout
        .text_runs
        .iter()
        .filter(|text_run| text_run.node_id == center)
        .map(|text_run| (text_run.origin, text_run.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        center_text,
        vec![
            (NativePoint { x: 4, y: 0 }, "A"),
            (NativePoint { x: 12, y: 0 }, " B"),
            (NativePoint { x: 12, y: 20 }, "C"),
        ]
    );
    let right_text = layout
        .text_runs
        .iter()
        .find(|text_run| text_run.node_id == right)
        .unwrap();
    assert_eq!(right_text.origin, NativePoint { x: 24, y: 40 });
    let inline_text = layout
        .text_runs
        .iter()
        .find(|text_run| text_run.node_id == inline)
        .unwrap();
    assert_eq!(inline_text.origin, NativePoint { x: 16, y: 60 });

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == chip && rect.x == 8 && rect.y == 60
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, origin, .. }
                if *node_id == inline && *origin == NativePoint { x: 16, y: 60 }
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(9, 61), Some([255, 0, 0, 255]));
}

#[test]
fn native_text_presentation_important_priority_reaches_shared_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #winner { display:block; width:32px; white-space:pre !important; text-align:center !important; text-transform:uppercase !important; text-decoration:underline !important; text-decoration-style:dashed !important; font-weight:bold !important; line-height:24px !important; } #rollback { display:block; width:32px; text-transform:uppercase !important; text-decoration-style:dashed !important; } } @layer theme { #winner { white-space:normal !important; text-align:right !important; text-transform:lowercase !important; text-decoration:overline !important; text-decoration-style:dotted !important; font-weight:normal !important; line-height:12px !important; } #rollback { text-transform:revert-layer !important; text-decoration-style:revert-layer !important; } } #winner { white-space:nowrap; text-align:left; text-transform:none; text-decoration:none; text-decoration-style:solid; font-weight:normal; line-height:8px; }</style><div id='winner'>ab</div><div id='rollback'>a</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !matches!(
            diagnostic.code,
            NativeDiagnosticCode::UnsupportedCssProperty
                | NativeDiagnosticCode::UnsupportedCssValue
        )
    }));

    let viewport = Viewport {
        width: 40,
        height: 60,
        device_scale_factor_milli: 1000,
    };
    let winner = document.resolve_target("id=winner").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(winner).unwrap().height, 24);
    assert_eq!(layout.box_for(rollback).unwrap().y, 24);
    let winner_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == winner)
        .unwrap();
    assert_eq!(winner_run.text, "AB");
    assert!(winner_run.origin.x > 0);

    let list = document.display_list(viewport).unwrap();
    let run_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                text,
                decoration_style,
                underline,
                overline,
                bold,
                ..
            } if *command_node == node_id => Some((
                text.as_str(),
                *decoration_style,
                *underline,
                *overline,
                *bold,
            )),
            _ => None,
        })
    };
    assert_eq!(
        run_for(winner),
        Some(("AB", NativeTextDecorationStyle::Dashed, true, false, true))
    );
    assert_eq!(
        run_for(rollback),
        Some(("A", NativeTextDecorationStyle::Dashed, false, false, false))
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(winner_run.origin.x + 1, winner_run.origin.y + 1),
        Some([0, 0, 0, 255])
    );
}

#[test]
fn native_text_alignment_revert_layer_preserves_inheritance_and_owner_paths() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:45px; } @layer base { #named { text-align:left; } #repeat { text-align:center; } #final { text-align:justify; text-align-last:right; text-justify:none; } #parent { text-align:center; } } @layer theme { #named { text-align:center; } #repeat { text-align:revert-layer; } #final { text-align-last:justify; text-justify:inter-word; } } @layer top { #repeat { text-align:revert-layer; } #final { text-align-last:revert-layer; text-justify:revert-layer; } } #named { text-align:revert-layer; } #repeat { text-align:revert-layer; } #final { text-align-last:revert-layer; text-justify:revert-layer; }</style><div id='named' class='line'>A</div><div id='repeat' class='line'>B</div><div id='final' class='line'>A B C</div><div id='parent' class='line'><span id='child' style='text-align:revert-layer'>C</span></div><div id='fallback' class='line' style='text-align:REVERT-LAYER'>D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "text-align" | "text-align-last" | "text-justify"
            ))
    }));
    let viewport = Viewport {
        width: 45,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let text_origin = |id: &str| {
        let node = document.resolve_target(&format!("id={id}")).unwrap();
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node)
            .map(|run| run.origin)
            .unwrap()
    };

    // Unlayered rollback exposes the highest named candidate.
    assert_eq!(text_origin("named").x, 18);
    // Rollback in theme, top, and then the unlayered bucket reaches base.
    assert_eq!(text_origin("repeat").x, 18);
    // The child rollback is inherited from the centered parent, while the
    // root rollback keeps the documented left default.
    assert_eq!(text_origin("child").x, 18);
    assert_eq!(text_origin("fallback").x, 0);

    let final_node = document.resolve_target("id=final").unwrap();
    let final_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == final_node)
        .collect::<Vec<_>>();
    assert_eq!(final_runs.len(), 3);
    assert!(final_runs.iter().any(|run| run.justify_spacing > 0));
    assert_eq!(final_runs[0].text, "A");
    assert_eq!(final_runs[1].text, " B");
    assert_eq!(final_runs[2].text, " C");
}

#[test]
fn native_text_alignment_maps_logical_edges_through_direction_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#ltr-start,#ltr-end,#rtl-start,#rtl-end,#physical-left,#physical-right,#inline { display:block; width:32px; height:20px; } #ltr-start { direction:ltr; text-align:start; } #ltr-end { direction:ltr; text-align:end; } #rtl-start { text-align:start; } #rtl-end { text-align:end; } #physical-left { text-align:left; } #physical-right { text-align:right; } #rtl-wrapped { display:block; width:32px; text-align:start; } #inline { text-align:start; } #chip { display:inline-block; width:8px; height:8px; background-color:red; }</style><div id='root' style='direction:rtl'><div id='ltr-start'>L</div><div id='ltr-end'>E</div><div id='rtl-start'>S</div><div id='rtl-end'>N</div><div id='physical-left'>P</div><div id='physical-right'>R</div><div id='rtl-wrapped'>A B C</div><div id='inline'><span id='chip'></span>Q</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let expected_text_x = [
        ("ltr-start", 0),
        ("ltr-end", 24),
        ("rtl-start", 24),
        ("rtl-end", 0),
        ("physical-left", 0),
        ("physical-right", 24),
    ];
    for (target, expected_x) in expected_text_x {
        let node = document.resolve_target(&format!("id={target}")).unwrap();
        let text_run = layout
            .text_runs
            .iter()
            .find(|text_run| text_run.node_id == node)
            .unwrap();
        assert_eq!(text_run.origin.x, expected_x, "{target}");
    }

    let inline = document.resolve_target("id=inline").unwrap();
    let chip = document.resolve_target("id=chip").unwrap();
    let rtl_wrapped = document.resolve_target("id=rtl-wrapped").unwrap();
    let inline_rect = layout.box_for(inline).unwrap();
    let chip_rect = layout.box_for(chip).unwrap();
    assert_eq!(chip_rect.x, 16);
    assert_eq!(chip_rect.y, inline_rect.y);
    let inline_text = layout
        .text_runs
        .iter()
        .find(|text_run| text_run.node_id == inline)
        .unwrap();
    assert_eq!(
        inline_text.origin,
        NativePoint {
            x: 24,
            y: inline_rect.y
        }
    );
    assert_eq!(
        layout.hit_test(17, i64::from(inline_rect.y + 1)),
        Ok(Some(chip))
    );

    let wrapped_rect = layout.box_for(rtl_wrapped).unwrap();
    let wrapped_text = layout
        .text_runs
        .iter()
        .filter(|text_run| text_run.node_id == rtl_wrapped)
        .map(|text_run| (text_run.origin, text_run.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        wrapped_text,
        vec![
            (
                NativePoint {
                    x: 8,
                    y: wrapped_rect.y
                },
                "A"
            ),
            (
                NativePoint {
                    x: 16,
                    y: wrapped_rect.y
                },
                " B"
            ),
            (
                NativePoint {
                    x: 24,
                    y: wrapped_rect.y + 20
                },
                "C"
            ),
        ]
    );

    let display_list = document.display_list(viewport).unwrap();
    assert!(display_list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == chip && rect.x == 16 && rect.y == inline_rect.y
        )
    }));
    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(17, inline_rect.y + 1), Some([255, 0, 0, 255]));

    let visible = document.visible_text(1024).0;
    let mut search_start = 0;
    for token in ["L", "E", "S", "N", "P", "R", "A", "B", "C", "Q"] {
        let offset = visible[search_start..].find(token).unwrap();
        search_start = search_start.saturating_add(offset + token.len());
    }
}

#[test]
fn native_text_align_last_resolves_final_lines_through_shared_artifacts() {
    let document = NativeDocument::parse(
        "<style>.block { display:block; width:45px; text-align:justify; } #center { text-align-last:center; } #right { text-align-last:right; } #start-rtl { direction:rtl; text-align-last:start; } #end-rtl { direction:rtl; text-align-last:end; } #auto { text-align-last:auto; } #hard { text-align-last:right; }</style><div id='center' class='block'>A B C D</div><div id='right' class='block'>A B C D</div><div id='start-rtl' class='block'>A B C D</div><div id='end-rtl' class='block'>A B C D</div><div id='auto' class='block'>A B C D</div><div id='hard' class='block'>A B C<br>D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 280,
        device_scale_factor_milli: 1000,
    };
    let center = document.resolve_target("id=center").unwrap();
    let right = document.resolve_target("id=right").unwrap();
    let start_rtl = document.resolve_target("id=start-rtl").unwrap();
    let end_rtl = document.resolve_target("id=end-rtl").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let hard = document.resolve_target("id=hard").unwrap();
    let layout = document.layout(viewport).unwrap();

    let final_origin = |node_id| {
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id && run.text == "D" && run.origin.y > 0)
            .map(|run| run.origin)
            .unwrap()
    };
    assert_eq!(final_origin(center), NativePoint { x: 18, y: 20 });
    assert_eq!(final_origin(right), NativePoint { x: 37, y: 60 });
    assert_eq!(final_origin(start_rtl), NativePoint { x: 37, y: 100 });
    assert_eq!(final_origin(end_rtl), NativePoint { x: 0, y: 140 });
    assert_eq!(final_origin(auto), NativePoint { x: 0, y: 180 });
    assert_eq!(final_origin(hard), NativePoint { x: 37, y: 220 });

    let hard_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == hard)
        .collect::<Vec<_>>();
    assert!(hard_runs.iter().all(|run| run.justify_spacing == 0));
    assert_eq!(layout.box_for(center).unwrap().height, 40);

    let display_list = document.display_list(viewport).unwrap();
    assert!(display_list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                origin: NativePoint { x: 18, y: 20 },
                text,
                ..
            } if *node_id == center && text == "D"
        )
    }));
    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(18, 20), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_align_last_justifies_final_lines_without_changing_other_paths() {
    let document = NativeDocument::parse(
        "<style>.block { display:block; width:45px; text-align-last:justify; } #tight { width:40px; } #pre { white-space:pre; } #pre-wrap { white-space:pre-wrap; } #break { word-break:break-all; } #spacing { word-spacing:2px; }</style><div id='target' class='block'>A B C</div><div id='tight' class='block'>A B C</div><div id='hard' class='block'>A B<br>C D</div><div id='pre' class='block'>A B C</div><div id='pre-wrap' class='block'>A B C</div><div id='break' class='block'>A B C</div><div id='spacing' class='block'>A B C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let target = document.resolve_target("id=target").unwrap();
    let tight = document.resolve_target("id=tight").unwrap();
    let hard = document.resolve_target("id=hard").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let pre_wrap = document.resolve_target("id=pre-wrap").unwrap();
    let break_all = document.resolve_target("id=break").unwrap();
    let spacing = document.resolve_target("id=spacing").unwrap();
    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        runs_for(target),
        vec![
            (NativePoint { x: 0, y: 0 }, "A", 0),
            (NativePoint { x: 8, y: 0 }, " B", 3),
            (NativePoint { x: 27, y: 0 }, " C", 2),
        ]
    );
    assert_eq!(
        runs_for(tight),
        vec![
            (NativePoint { x: 0, y: 20 }, "A", 0),
            (NativePoint { x: 8, y: 20 }, " B", 0),
            (NativePoint { x: 24, y: 20 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(hard),
        vec![
            (NativePoint { x: 0, y: 40 }, "A", 0),
            (NativePoint { x: 8, y: 40 }, " B", 0),
            (NativePoint { x: 0, y: 60 }, "C", 0),
            (NativePoint { x: 8, y: 60 }, " D", 21),
        ]
    );
    assert_eq!(
        runs_for(pre),
        vec![(NativePoint { x: 0, y: 80 }, "A B C", 0)]
    );
    assert_eq!(
        runs_for(pre_wrap),
        vec![(NativePoint { x: 0, y: 100 }, "A B C", 0)]
    );
    assert_eq!(
        runs_for(break_all),
        vec![
            (NativePoint { x: 0, y: 120 }, "A", 0),
            (NativePoint { x: 8, y: 120 }, " B", 0),
            (NativePoint { x: 24, y: 120 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(spacing),
        vec![
            (NativePoint { x: 0, y: 140 }, "A", 0),
            (NativePoint { x: 8, y: 140 }, " B", 1),
            (NativePoint { x: 27, y: 140 }, " C", 0),
        ]
    );
    assert_eq!(layout.box_for(target).unwrap().height, 20);

    let display_list = document.display_list(viewport).unwrap();
    let target_commands = display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                justify_spacing,
                ..
            } if *node_id == target => Some((*origin, text.as_str(), *justify_spacing)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(target_commands, runs_for(target));
    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_justify_controls_both_justification_flushes() {
    let document = NativeDocument::parse(
        "<style>.soft { display:block; width:45px; text-align:justify; } #none-soft { text-justify:none; } #inter-word-soft { text-justify:inter-word; } .final { display:block; width:45px; text-align:left; text-align-last:justify; } #none-final { text-justify:none; } #alone { display:block; width:45px; text-align:left; text-justify:inter-word; } #spacing-none { display:block; width:45px; text-align:justify; word-spacing:2px; text-justify:none; }</style><div id='auto-soft' class='soft'>A B C D</div><div id='none-soft' class='soft'>A B C D</div><div id='inter-word-soft' class='soft'>A B C D</div><div id='auto-final' class='final'>A B C</div><div id='none-final' class='final'>A B C</div><div id='alone' >A B C</div><div id='spacing-none'>A B C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let auto_soft = document.resolve_target("id=auto-soft").unwrap();
    let none_soft = document.resolve_target("id=none-soft").unwrap();
    let inter_word_soft = document.resolve_target("id=inter-word-soft").unwrap();
    let auto_final = document.resolve_target("id=auto-final").unwrap();
    let none_final = document.resolve_target("id=none-final").unwrap();
    let alone = document.resolve_target("id=alone").unwrap();
    let spacing_none = document.resolve_target("id=spacing-none").unwrap();
    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
            .collect::<Vec<_>>()
    };
    let first_line = |y: u32, spacing: [u32; 2]| {
        vec![
            (NativePoint { x: 0, y }, "A", 0),
            (NativePoint { x: 8, y }, " B", spacing[0]),
            (NativePoint { x: 27, y }, " C", spacing[1]),
            (NativePoint { x: 0, y: y + 20 }, "D", 0),
        ]
    };

    assert_eq!(runs_for(auto_soft), first_line(0, [3, 2]));
    assert_eq!(
        runs_for(none_soft),
        vec![
            (NativePoint { x: 0, y: 40 }, "A", 0),
            (NativePoint { x: 8, y: 40 }, " B", 0),
            (NativePoint { x: 24, y: 40 }, " C", 0),
            (NativePoint { x: 0, y: 60 }, "D", 0),
        ]
    );
    assert_eq!(runs_for(inter_word_soft), first_line(80, [3, 2]));
    assert_eq!(
        runs_for(auto_final),
        first_line(120, [3, 2])
            .into_iter()
            .take(3)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        runs_for(none_final),
        vec![
            (NativePoint { x: 0, y: 140 }, "A", 0),
            (NativePoint { x: 8, y: 140 }, " B", 0),
            (NativePoint { x: 24, y: 140 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(alone),
        vec![
            (NativePoint { x: 0, y: 160 }, "A", 0),
            (NativePoint { x: 8, y: 160 }, " B", 0),
            (NativePoint { x: 24, y: 160 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(spacing_none),
        vec![
            (NativePoint { x: 0, y: 180 }, "A", 0),
            (NativePoint { x: 8, y: 180 }, " B", 0),
            (NativePoint { x: 26, y: 180 }, " C", 0),
        ]
    );

    let display_list = document.display_list(viewport).unwrap();
    let none_commands = display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                justify_spacing,
                ..
            } if *node_id == none_final => Some((*origin, text.as_str(), *justify_spacing)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(none_commands, runs_for(none_final));

    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_double_decoration_preserves_style_inheritance_and_geometry() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:24px; line-height:20px; color:black; text-decoration:underline overline line-through; text-decoration-style:double; text-decoration-thickness:2px; text-underline-offset:1px; } #one { text-decoration-style:DoUbLe; text-decoration-thickness:1px; text-underline-offset:0px; } #two { text-decoration-style:double; text-decoration-thickness:2px; text-underline-offset:2px; } #parent { text-decoration-style:double; text-decoration-thickness:2px; }</style><div id='one' class='line'>AB</div><div id='two' class='line'>AB</div><div id='parent' class='line'><span id='inherited'>A</span><span id='override' style='text-decoration-style:solid'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let one = document.resolve_target("id=one").unwrap();
    let two = document.resolve_target("id=two").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let override_node = document.resolve_target("id=override").unwrap();
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                decoration_thickness,
                underline_offset,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *decoration_style,
                *decoration_thickness,
                *underline_offset,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let one_command = text_command(one).expect("one double-decoration command");
    let two_command = text_command(two).expect("two double-decoration command");
    let inherited_command = text_command(inherited).expect("inherited double-decoration command");
    let override_command = text_command(override_node).expect("override decoration command");

    assert_eq!(one_command.1, NativeTextDecorationStyle::Double);
    assert_eq!(one_command.2, 1);
    assert_eq!(one_command.3, 0);
    assert!(one_command.4 && one_command.5 && one_command.6);
    assert_eq!(two_command.1, NativeTextDecorationStyle::Double);
    assert_eq!(two_command.2, 2);
    assert_eq!(two_command.3, 2);
    assert!(two_command.4 && two_command.5 && two_command.6);
    assert_eq!(inherited_command.1, NativeTextDecorationStyle::Double);
    assert_eq!(inherited_command.2, 2);
    assert_eq!(inherited_command.3, 1);
    assert!(inherited_command.4 && inherited_command.5 && inherited_command.6);
    assert_eq!(override_command.1, NativeTextDecorationStyle::Solid);
    assert_eq!(override_command.2, 2);
    assert_eq!(override_command.3, 1);
    assert!(override_command.4 && override_command.5 && override_command.6);

    assert_eq!(
        layout.box_for(one),
        layout.box_for(two).map(|rect| NativeRect {
            y: rect.y.saturating_sub(24),
            ..rect
        })
    );
    let surface = list.rasterize().unwrap();
    let line_pixel =
        |command: (
            NativePoint,
            NativeTextDecorationStyle,
            u32,
            i32,
            bool,
            bool,
            bool,
        ),
         y: u32| surface.pixel(command.0.x, command.0.y.saturating_add(y));
    assert_eq!(line_pixel(one_command, 7), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(one_command, 8), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(one_command, 9), Some([0, 0, 0, 255]));
    for y in [9, 10, 12, 13] {
        assert_eq!(line_pixel(two_command, y), Some([0, 0, 0, 255]));
    }
    assert_eq!(line_pixel(two_command, 11), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(override_command, 10), Some([255, 255, 255, 255]));
}

#[test]
fn native_text_wavy_decoration_preserves_phase_inheritance_and_geometry() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:24px; line-height:20px; color:black; white-space:pre; text-decoration:underline; text-decoration-style:wavy; text-decoration-thickness:2px; text-underline-offset:1px; } #one { text-decoration-style:WaVy; text-decoration-thickness:1px; text-underline-offset:0px; } #two { text-decoration-style:wavy; text-decoration-thickness:2px; text-underline-offset:2px; } #parent { text-decoration-style:wavy; text-decoration-thickness:2px; }</style><div id='one' class='line'>        </div><div id='two' class='line'>        </div><div id='parent' class='line'><span id='inherited'>        </span><span id='override' style='text-decoration-style:solid'>        </span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let one = document.resolve_target("id=one").unwrap();
    let two = document.resolve_target("id=two").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let override_node = document.resolve_target("id=override").unwrap();
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                decoration_thickness,
                underline_offset,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *decoration_style,
                *decoration_thickness,
                *underline_offset,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let one_command = text_command(one).expect("one wavy-decoration command");
    let two_command = text_command(two).expect("two wavy-decoration command");
    let inherited_command = text_command(inherited).expect("inherited wavy-decoration command");
    let override_command = text_command(override_node).expect("override decoration command");

    for command in [
        one_command,
        two_command,
        inherited_command,
        override_command,
    ] {
        assert!(!command.5 && !command.6);
        assert!(command.4);
    }
    assert_eq!(one_command.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(one_command.2, 1);
    assert_eq!(one_command.3, 0);
    assert_eq!(two_command.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(two_command.2, 2);
    assert_eq!(two_command.3, 2);
    assert_eq!(inherited_command.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(inherited_command.2, 2);
    assert_eq!(inherited_command.3, 1);
    assert_eq!(override_command.1, NativeTextDecorationStyle::Solid);
    assert_eq!(override_command.2, 2);
    assert_eq!(override_command.3, 1);

    assert_eq!(
        layout.box_for(one),
        layout.box_for(two).map(|rect| NativeRect {
            y: rect.y.saturating_sub(24),
            ..rect
        })
    );
    let surface = list.rasterize().unwrap();
    let line_pixel = |command: (
        NativePoint,
        NativeTextDecorationStyle,
        u32,
        i32,
        bool,
        bool,
        bool,
    ),
                      x: u32,
                      y: u32| {
        surface.pixel(command.0.x.saturating_add(x), command.0.y.saturating_add(y))
    };
    for (x, y) in [
        (0, 7),
        (1, 8),
        (2, 9),
        (3, 8),
        (4, 7),
        (5, 6),
        (6, 5),
        (7, 6),
    ] {
        assert_eq!(line_pixel(one_command, x, y), Some([0, 0, 0, 255]));
    }
    for (x, top) in [
        (0, 9),
        (1, 10),
        (2, 11),
        (3, 10),
        (4, 9),
        (5, 8),
        (6, 7),
        (7, 8),
    ] {
        assert_eq!(line_pixel(two_command, x, top), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(two_command, x, top + 1), Some([0, 0, 0, 255]));
    }
    assert_eq!(line_pixel(inherited_command, 0, 8), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(inherited_command, 0, 9), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(override_command, 0, 8), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(override_command, 2, 8), Some([0, 0, 0, 255]));
    assert_eq!(
        line_pixel(override_command, 2, 7),
        Some([255, 255, 255, 255])
    );
}

#[test]
fn native_text_transform_aligns_layout_and_display_with_source_text_preserved() {
    let document = NativeDocument::parse(
        "<style>#upper { display:block; width:80px; text-transform:uppercase; } #lower { display:block; width:80px; text-transform:lowercase; } #parent { display:block; width:80px; text-transform:uppercase; } #clear { text-transform:none; } #contents { display:contents; text-transform:lowercase; } #pre { display:block; width:80px; white-space:pre-wrap; text-transform:uppercase; } #nowrap { display:block; text-transform:uppercase; white-space:nowrap; }</style><button id='upper'>aBc dEf</button><div id='lower'>aBc dEf</div><div id='parent'>One <span id='clear'>aBc</span> <span id='contents'><span id='nested'>aBc</span></span></div><div id='pre'>aB\ncD</div><div id='nowrap'>aB cD</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 140,
        device_scale_factor_milli: 1000,
    };
    let upper = document.resolve_target("id=upper").unwrap();
    let lower = document.resolve_target("id=lower").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let nowrap = document.resolve_target("id=nowrap").unwrap();

    let layout = document.layout(viewport).unwrap();
    let run_text = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(run_text(upper), vec!["ABC", " DEF"]);
    assert_eq!(run_text(lower), vec!["abc", " def"]);
    assert!(run_text(parent).contains(&"ONE"));
    assert_eq!(run_text(clear), vec!["aBc"]);
    assert_eq!(run_text(nested), vec!["abc"]);
    assert_eq!(run_text(pre), vec!["AB", "CD"]);
    assert_eq!(run_text(nowrap), vec!["AB CD"]);

    let (source_text, truncated) = document.visible_text(1024);
    assert!(!truncated);
    assert!(source_text.contains("aBc dEf"));
    assert!(!source_text.contains("ABC DEF"));
    assert!(document.resolve_target("text=aBc dEf").is_ok());

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
}

#[test]
fn native_text_transform_feeds_text_fragments_without_mutating_evidence() {
    let html = "<!doctype html><div style='display:block;height:40px'>one</div><div id='target' style='display:block;height:40px;text-transform:uppercase;white-space:pre'>target phrase</div>";
    let viewport = Viewport {
        width: 240,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let target_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text == "TARGET PHRASE")
        .unwrap();
    let expected_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(target_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(expected_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://text-transform-fragment", html)
        .unwrap()
        .with_initial_url("fixture://text-transform-fragment");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(engine.snapshot().unwrap().visible_text, "one target phrase");

    engine
        .navigate("fixture://text-transform-fragment#:~:text=TARGET%20PHRASE")
        .unwrap();
    assert_eq!(
        engine.scroll_offset(),
        NativePoint {
            x: expected_scroll.x,
            y: expected_scroll.y + 8,
        }
    );
    assert_eq!(engine.snapshot().unwrap().visible_text, "one target phrase");
}

#[test]
fn native_inherited_text_presentation_revert_layer_preserves_consumers() {
    let document = NativeDocument::parse(
        "<style>.line { display:block;width:40px; } @layer base { #transform { text-transform:lowercase; } #weight { font-weight:normal; } #style { font-style:normal; } #break { word-break:break-all; } #unlayered { text-transform:lowercase; } #inline { text-transform:lowercase; } #parent { text-transform:uppercase;font-weight:bold;font-style:italic;word-break:break-all; } } @layer theme { #transform { text-transform:revert-layer; } #weight { font-weight:bold; } #style { font-style:italic; } #break { word-break:normal; } #unlayered { text-transform:uppercase; } #inline { text-transform:uppercase; } #parent { text-transform:lowercase;font-weight:normal;font-style:normal;word-break:normal; } } @layer top { #transform { text-transform:revert-layer; } #weight { font-weight:revert-layer; } #style { font-style:revert-layer; } #break { word-break:revert-layer; } #parent { text-transform:revert-layer;font-weight:revert-layer;font-style:revert-layer;word-break:revert-layer; } } #unlayered { text-transform:revert-layer; }</style><div id='transform' class='line'>aB cD</div><div id='weight' class='line'>A</div><div id='style' class='line'>A</div><div id='break' class='line'>ABC DEFG</div><div id='unlayered' class='line'>aB</div><div id='inline' class='line' style='text-transform:revert-layer'>aB</div><div id='parent' class='line'>P<span id='child' style='text-transform:revert-layer;font-weight:revert-layer;font-style:revert-layer;word-break:revert-layer'>aB</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "text-transform" | "font-weight" | "font-style" | "word-break"
            ))
    }));

    let transform = document.resolve_target("id=transform").unwrap();
    let weight = document.resolve_target("id=weight").unwrap();
    let style = document.resolve_target("id=style").unwrap();
    let break_all = document.resolve_target("id=break").unwrap();
    let unlayered = document.resolve_target("id=unlayered").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let viewport = Viewport {
        width: 64,
        height: 220,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let text_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(text_for(transform), vec!["ab", " cd"]);
    assert_eq!(text_for(break_all), vec!["ABC", "DEFG"]);
    assert_eq!(text_for(unlayered), vec!["AB"]);
    assert_eq!(text_for(inline), vec!["AB"]);
    assert_eq!(text_for(child), vec!["ab"]);

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
    assert_eq!(style_for(weight), Some((true, false)));
    assert_eq!(style_for(style), Some((false, true)));
    assert_eq!(style_for(child), Some((false, false)));

    let weight_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == weight)
        .unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(weight_run.origin.x + 1, weight_run.origin.y + 1),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        layout.hit_test(1, i64::from(weight_run.origin.y + 1)),
        Ok(Some(weight))
    );
}
