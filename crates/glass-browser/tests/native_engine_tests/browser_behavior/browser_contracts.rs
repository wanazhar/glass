use super::*;

#[test]
fn native_semantic_levels_and_region_expansion_are_revision_scoped() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_semantic_levels_and_region_expansion_are_revision_scoped_inner());
    });
}

async fn native_semantic_levels_and_region_expansion_are_revision_scoped_inner() {
    let session = BrowserSession::start(
        NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3ESemantic%3C%2Ftitle%3E%3Cmain%3E%3Clabel%20for%3D%22email%22%3EEmail%3C%2Flabel%3E%3Cinput%20id%3D%22email%22%20type%3D%22email%22%3E%3Cbutton%3ESend%3C%2Fbutton%3E%3C%2Fmain%3E",
        ),
    )
    .await
    .unwrap();

    let default_observation = session.observe().await.unwrap();
    assert_eq!(
        default_observation.level,
        SemanticObservationLevel::Structured
    );
    assert!(default_observation.revision > 0);

    let summary = session
        .semantic_observe(SemanticObservationLevel::Summary)
        .await
        .unwrap();
    assert_eq!(summary.level, SemanticObservationLevel::Summary);
    assert!(summary.text.is_none());
    assert!(
        summary
            .regions
            .iter()
            .all(|region| region.targets.is_empty())
    );

    let interactive = session
        .semantic_observe(SemanticObservationLevel::Interactive)
        .await
        .unwrap();
    assert_eq!(interactive.level, SemanticObservationLevel::Interactive);
    assert!(interactive.text.is_none());
    let button = interactive
        .regions
        .iter()
        .flat_map(|region| region.targets.iter())
        .find(|target| target.role == "button")
        .expect("interactive observation should expose the button target");
    let action = session
        .action_with_revision(
            SemanticAction::Click {
                target: button.reference.clone(),
            },
            interactive.revision,
        )
        .await
        .unwrap();
    assert!(action.accepted);

    let detailed = session
        .native_semantic_observe(SemanticObservationLevel::Detailed)
        .await
        .unwrap();
    assert!(detailed.text.is_some());
    assert!(detailed.accessibility.is_some());
    assert!(detailed.raw_accessibility.is_none());

    let raw = session
        .native_semantic_observe(SemanticObservationLevel::Raw)
        .await
        .unwrap();
    assert!(raw.accessibility.is_some());
    assert!(raw.raw_accessibility.is_some());

    let expanded = session
        .semantic_expand_region(
            "region_main",
            detailed.revision,
            SemanticObservationLevel::Detailed,
        )
        .await
        .unwrap();
    assert_eq!(expanded.regions.len(), 1);
    assert_eq!(expanded.regions[0].id, "region_main");
    assert_eq!(expanded.revision, detailed.revision);

    let stale = session
        .semantic_expand_region(
            "region_main",
            detailed.revision.saturating_add(1),
            SemanticObservationLevel::Detailed,
        )
        .await;
    assert!(stale.is_err());

    let form_values = session.native_form_values(false).await.unwrap();
    assert_eq!(form_values[0]["type"], "email");
    assert_eq!(form_values[0]["value"], "");
    session.close().await.unwrap();
}
#[test]
fn native_form_targets_keep_opener_and_reuse_named_targets() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_form_targets_keep_opener_and_reuse_named_targets_inner());
    });
}

async fn native_form_targets_keep_opener_and_reuse_named_targets_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://form-target-parent",
            "<title>Form opener</title><form action='fixture://form-target-child' target='report'><input name='query' value='hello'><button id='blank' type='submit' formtarget='_blank'>Blank</button><button id='named' type='submit'>Named</button></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://form-target-child?query=hello",
            "<title>Form target</title><p>submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://form-target-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    session
        .action(SemanticAction::Click {
            target: "id=blank".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        "fixture://form-target-parent"
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets.iter().filter(|target| !target.active).count(), 1);
    assert_eq!(
        targets.iter().find(|target| !target.active).unwrap().title,
        "Form target"
    );

    session
        .action(SemanticAction::Click {
            target: "id=named".into(),
        })
        .await
        .unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 3);
    session
        .action(SemanticAction::Click {
            target: "id=named".into(),
        })
        .await
        .unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 3);
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        "fixture://form-target-parent"
    );
    session.close().await.unwrap();
}
#[test]
fn native_external_blank_link_creates_popup_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_external_blank_link_creates_popup_target_inner());
    });
}

async fn native_external_blank_link_creates_popup_target_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/popup-parent" => {
                    "<title>HTTP opener</title><a id='open' target='_blank' href='/popup-child'>Open popup</a>"
                }
                "/popup-child" => "<title>HTTP popup</title><p>worker popup</p>",
                other => panic!("unexpected popup request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/popup-parent")),
    )
    .await
    .unwrap();
    let action = session
        .action(SemanticAction::Click {
            target: "id=open".into(),
        })
        .await
        .unwrap();
    assert!(action.accepted);
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "HTTP opener"
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let popup = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(popup.title, "HTTP popup");
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_page_script_blank_target_link_creates_popup_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_page_script_blank_target_link_creates_popup_target_inner());
    });
}

async fn native_page_script_blank_target_link_creates_popup_target_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://script-popup-parent",
            "<title>Script opener</title><a id='open' target='_blank' href='fixture://script-popup-child'>Open popup</a>",
        )
        .unwrap()
        .with_fixture(
            "fixture://script-popup-child",
            "<title>Script popup</title><p>script target</p>",
        )
        .unwrap()
        .with_initial_url("fixture://script-popup-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    assert_eq!(
        session
            .script("document.getElementById('open').click(); 'done'")
            .await
            .unwrap()
            .value,
        serde_json::json!("done")
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].title, "Script popup");
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "Script opener"
    );
    session.close().await.unwrap();
}
#[test]
fn native_root_overflow_ignores_fully_clipped_text_but_keeps_visible_text_extent() {
    let document = NativeDocument::parse(
        "<div id='clipped' style='display:block;width:16px;overflow:hidden;white-space:nowrap'>ABCDEFG</div><div id='visible' style='display:block;white-space:nowrap'>ABCD</div><div id='clip' style='display:block;width:16px;overflow:clip;white-space:nowrap'>HIJKLMN</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let clipped = document.resolve_target("id=clipped").unwrap();
    let visible = document.resolve_target("id=visible").unwrap();
    let clip = document.resolve_target("id=clip").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(clipped).unwrap().width, 16);
    assert_eq!(layout.box_for(clip).unwrap().width, 16);
    assert_eq!(layout.box_for(visible).unwrap().width, 24);
    assert_eq!(layout.content_width, 32);
    assert_eq!(layout.max_scroll_offset().x, 8);
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.node_id == clipped && run.text == "ABCDEFG")
    );
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.node_id == clip && run.text == "HIJKLMN")
    );

    let clipped_only = NativeDocument::parse(
        "<div id='clipped' style='display:block;width:16px;overflow:hidden;white-space:nowrap'>ABCDEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let clipped_only_layout = clipped_only.layout(viewport).unwrap();
    assert_eq!(clipped_only_layout.content_width, viewport.width);
    assert_eq!(clipped_only_layout.max_scroll_offset().x, 0);
}
#[test]
fn native_justified_text_expands_soft_wrapped_spaces_through_shared_artifacts() {
    let document = NativeDocument::parse(
        "<style>#justify { display:block; width:45px; text-align:justify; } #hard { display:block; width:45px; text-align:justify; white-space:pre-line; } #pre { display:block; width:45px; text-align:justify; white-space:pre-wrap; } #break { display:block; width:45px; text-align:justify; word-break:break-all; }</style><div id='justify'>A B C D</div><div id='hard'>A B C<br>D E F</div><div id='pre'>A B C D</div><div id='break'>ABCDEFGHIJ</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let justify = document.resolve_target("id=justify").unwrap();
    let hard = document.resolve_target("id=hard").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let break_all = document.resolve_target("id=break").unwrap();
    let layout = document.layout(viewport).unwrap();

    let justify_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == justify)
        .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
        .collect::<Vec<_>>();
    assert_eq!(
        justify_runs,
        vec![
            (NativePoint { x: 0, y: 0 }, "A", 0),
            (NativePoint { x: 8, y: 0 }, " B", 3),
            (NativePoint { x: 27, y: 0 }, " C", 2),
            (NativePoint { x: 0, y: 20 }, "D", 0),
        ]
    );
    assert_eq!(layout.box_for(justify).unwrap().height, 40);
    assert_eq!(layout.content_width, 45);

    let hard_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == hard)
        .collect::<Vec<_>>();
    assert!(hard_runs.iter().all(|run| run.justify_spacing == 0));

    let pre_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == pre)
        .collect::<Vec<_>>();
    assert!(pre_runs.iter().all(|run| run.justify_spacing == 0));
    let break_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == break_all)
        .collect::<Vec<_>>();
    assert!(break_runs.iter().all(|run| run.justify_spacing == 0));

    let list = document.display_list(viewport).unwrap();
    let justify_commands = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                justify_spacing,
                ..
            } if *node_id == justify => Some((*origin, text.as_str(), *justify_spacing)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(justify_commands, justify_runs);

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(19, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(16, 0), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(37, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 20), Some([0, 0, 0, 255]));
}
#[test]
fn native_justification_composes_authored_word_spacing() {
    let document = NativeDocument::parse(
        "<style>#target { display:block; width:45px; text-align:justify; word-spacing:2px; }</style><div id='target'>A B C D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let target = document.resolve_target("id=target").unwrap();
    let layout = document
        .layout(Viewport {
            width: 45,
            height: 80,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    let runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == target)
        .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![
            (NativePoint { x: 0, y: 0 }, "A", 0),
            (NativePoint { x: 8, y: 0 }, " B", 1),
            (NativePoint { x: 27, y: 0 }, " C", 0),
            (NativePoint { x: 0, y: 20 }, "D", 0),
        ]
    );

    let list = document
        .display_list(Viewport {
            width: 45,
            height: 80,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                text,
                word_spacing: 2,
                justify_spacing: 1,
                ..
            } if *node_id == target && text == " B"
        )
    }));
}
#[test]
fn native_letter_spacing_composes_with_word_spacing_across_consumers() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:48px; letter-spacing:2px; word-spacing:4px; } #pre { display:block; width:36px; letter-spacing:2px; word-spacing:4px; white-space:pre-wrap; } #wide { display:block; width:32px; letter-spacing:2px; word-spacing:4px; white-space:nowrap; } #align { display:block; width:48px; letter-spacing:2px; word-spacing:4px; text-align:right; white-space:nowrap; } #parent { display:block; width:80px; letter-spacing:2px; word-spacing:4px; } #override { letter-spacing:4px; }</style><div id='normal'>A B</div><div id='pre'>A  B</div><div id='wide'>A B C D E F G H</div><div id='align'>A B</div><div id='parent'>A <span id='override'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 160,
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
            (NativePoint { x: 10, y: 0 }, " B")
        ]
    );
    assert_eq!(
        runs_for(pre),
        vec![
            (NativePoint { x: 0, y: 20 }, "A "),
            (NativePoint { x: 0, y: 40 }, " B")
        ]
    );

    let wide_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == wide)
        .unwrap();
    assert_eq!(wide_run.origin, NativePoint { x: 0, y: 60 });
    assert_eq!(wide_run.text, "A B C D E F G H");
    assert_eq!(layout.content_width, 178);
    assert_eq!(layout.max_scroll_offset().x, 98);

    let align_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == align)
        .unwrap();
    assert_eq!(align_run.origin, NativePoint { x: 14, y: 80 });
    assert_eq!(align_run.text, "A B");

    let parent_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == parent)
        .unwrap();
    let override_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == override_id)
        .unwrap();
    assert_eq!(parent_run.origin, NativePoint { x: 0, y: 100 });
    assert_eq!(parent_run.text, "A");
    assert_eq!(override_run.origin, NativePoint { x: 24, y: 100 });
    assert_eq!(override_run.text, "B");

    let list = document.display_list(viewport).unwrap();
    let spacing_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                letter_spacing,
                word_spacing,
                ..
            } if *command_node == node_id => Some((*letter_spacing, *word_spacing)),
            _ => None,
        })
    };
    assert_eq!(spacing_for(normal), Some((2, 4)));
    assert_eq!(spacing_for(wide), Some((2, 4)));
    assert_eq!(spacing_for(override_id), Some((4, 4)));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(24, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(10, 0), Some([255, 255, 255, 255]));
}
#[test]
fn native_functional_alpha_colors_reach_display_list_and_raster() {
    let document = NativeDocument::parse(
        "<div id='background' style='display:block;width:8px;height:8px;background-color:rgba(255, 0, 0, 0.5)'></div><div id='border' style='display:block;width:8px;height:8px;border:1px solid rgba(0, 0, 255, 50%)'></div><div id='text' style='display:block;width:8px;height:8px;color:rgba(0, 128, 0, 0.5)'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 16,
        height: 28,
        device_scale_factor_milli: 1000,
    };
    let background = document.resolve_target("id=background").unwrap();
    let border = document.resolve_target("id=border").unwrap();
    let text = document.resolve_target("id=text").unwrap();
    let list = document.display_list(viewport).unwrap();
    let background_color = NativeColor {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 128,
    };
    let border_color = NativeColor {
        red: 0,
        green: 0,
        blue: 255,
        alpha: 128,
    };
    let text_color = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: 128,
    };
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == background && *color == background_color
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BorderRect { node_id, borders, .. }
                if *node_id == border && borders.top.color == border_color
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == text && *color == text_color
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(4, 4), Some([255, 127, 127, 255]));
    assert_eq!(surface.pixel(0, 8), Some([127, 127, 255, 255]));
    assert_eq!(surface.pixel(1, 18), Some([127, 191, 127, 255]));
}
#[test]
fn native_pattern_border_styles_feed_display_list_and_surface() {
    let document = NativeDocument::parse(
        "<div id='card' style='width:8px;height:6px;border-top:1px dashed red;border-right:1px dotted green;border-bottom:1px solid blue'></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 12,
        height: 8,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let border = list
        .commands
        .iter()
        .find_map(|command| match command {
            NativeDisplayCommand::BorderRect { borders, .. } => Some(*borders),
            _ => None,
        })
        .unwrap();
    assert_eq!(border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(border.right.style, NativeBorderStyle::Dotted);
    assert_eq!(border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(border.left.width, 0);

    let mut scrolled_list = list.clone();
    let unscrolled = list.rasterize().unwrap();
    assert_eq!(unscrolled.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(2, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(3, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(4, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(5, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(8, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(8, 1), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(8, 2), Some([0, 128, 0, 255]));

    scrolled_list.scroll_offset = NativePoint { x: 0, y: 1 };
    let scrolled = scrolled_list.rasterize().unwrap();
    assert_eq!(scrolled.pixel(8, 0), unscrolled.pixel(8, 1));
    assert_eq!(scrolled.pixel(8, 1), unscrolled.pixel(8, 2));
}
#[test]
fn native_inline_boxes_wrap_before_layout_materializes_geometry() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:16px'><span id='first' style='display:inline;width:6px;height:4px;padding:1px;margin:1px;background-color:red'>A</span><span id='second' style='display:inline;width:6px;height:4px;padding:1px;margin:1px;background-color:blue'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(container),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 40,
        })
    );
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 1,
            y: 1,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 1,
            y: 21,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(layout.hit_test(2, 2).unwrap(), Some(first));
    assert_eq!(layout.hit_test(2, 22).unwrap(), Some(second));

    let list = document.display_list(viewport).unwrap();
    let text_origins = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id, origin, ..
            } => Some((*node_id, *origin)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(text_origins.contains(&(first, NativePoint { x: 2, y: 2 })));
    assert!(text_origins.contains(&(second, NativePoint { x: 2, y: 22 })));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == first
                    && *rect == NativeRect { x: 1, y: 1, width: 8, height: 6 }
        )
    }));
}
#[test]
fn native_fixed_line_height_controls_wrapped_flow_and_preserves_explicit_height() {
    let document = NativeDocument::parse(
        "<style>#container { width:12px; line-height:28px; } #first { line-height:32px; }</style><div id='container'><span id='first' style='display:inline;width:8px'>A</span><span id='second' style='display:inline;width:8px;height:4px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(container),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 12,
            height: 60,
        })
    );
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 32,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 0,
            y: 32,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(first));
    assert_eq!(layout.hit_test(1, 33).unwrap(), Some(second));

    let list = document.display_list(viewport).unwrap();
    let text_origins = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id, origin, ..
            } => Some((*node_id, *origin)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(text_origins.contains(&(first, NativePoint { x: 0, y: 0 })));
    assert!(text_origins.contains(&(second, NativePoint { x: 0, y: 32 })));
}
#[test]
fn native_br_elements_create_bounded_hard_breaks_without_layout_nodes() {
    let document = NativeDocument::parse(
        "<div id='flow'><br id='leading'>A<br id='middle-a'><br id='middle-b'>B<br id='trailing'></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 100,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let leading = document.resolve_target("id=leading").unwrap();
    let middle_a = document.resolve_target("id=middle-a").unwrap();
    let middle_b = document.resolve_target("id=middle-b").unwrap();
    let trailing = document.resolve_target("id=trailing").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(leading), None);
    assert_eq!(layout.box_for(middle_a), None);
    assert_eq!(layout.box_for(middle_b), None);
    assert_eq!(layout.box_for(trailing), None);
    assert_eq!(layout.box_for(flow).unwrap().height, 100);
    assert_eq!(layout.text_runs.len(), 2);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 20 }, "A"),
            (flow, NativePoint { x: 0, y: 60 }, "B"),
        ]
    );

    let list = document.display_list(viewport).unwrap();
    let runs = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                ..
            } => Some((*node_id, *origin, text.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![
            (flow, NativePoint { x: 0, y: 20 }, "A"),
            (flow, NativePoint { x: 0, y: 60 }, "B"),
        ]
    );
    assert!(list.commands.iter().all(|command| match command {
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
        | NativeDisplayCommand::GlyphRun { node_id, .. } => {
            *node_id != leading
                && *node_id != middle_a
                && *node_id != middle_b
                && *node_id != trailing
        }
        NativeDisplayCommand::BeginOpacityGroup { .. }
        | NativeDisplayCommand::Clear { .. }
        | NativeDisplayCommand::SetNestedScrollOffset { .. }
        | NativeDisplayCommand::EndOpacityGroup { .. } => true,
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 20), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 60), Some([0, 0, 0, 255]));

    let hidden = NativeDocument::parse(
        "<div id='flow'>A<br id='hidden' hidden>B<br id='aria-hidden' aria-hidden='true'>C<br id='display-none' style='display:none'>D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let hidden_flow = hidden.resolve_target("id=flow").unwrap();
    let hidden_break = hidden.resolve_target("id=hidden").unwrap();
    let aria_hidden_break = hidden.resolve_target("id=aria-hidden").unwrap();
    let display_none_break = hidden.resolve_target("id=display-none").unwrap();
    let hidden_layout = hidden.layout(viewport).unwrap();
    assert_eq!(hidden_layout.box_for(hidden_break), None);
    assert_eq!(hidden_layout.box_for(aria_hidden_break), None);
    assert_eq!(hidden_layout.box_for(display_none_break), None);
    assert_eq!(hidden_layout.box_for(hidden_flow).unwrap().height, 20);
    assert!(hidden_layout.text_runs.iter().all(|run| run.origin.y == 0));
}
#[test]
fn native_pre_line_preserves_bounded_source_breaks_and_inherits_through_contents() {
    let document = NativeDocument::parse(
        "<style>#flow { white-space: pre-line; line-height: 24px; }</style><div id='flow'>\nA\n\n<span id='contents' style='display:contents'>B\r\nC</span>\n</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 144,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let contents = document.resolve_target("id=contents").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(contents), None);
    assert_eq!(layout.box_for(flow).unwrap().height, 144);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 24 }, "A"),
            (contents, NativePoint { x: 0, y: 72 }, "B"),
            (contents, NativePoint { x: 0, y: 96 }, "C"),
        ]
    );

    let normal = NativeDocument::parse(
        "<div id='normal'>A\nB</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let normal_flow = normal.resolve_target("id=normal").unwrap();
    let normal_layout = normal
        .layout(Viewport {
            width: 32,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(normal_layout.box_for(normal_flow).unwrap().height, 20);
    assert_eq!(normal_layout.text_runs.len(), 2);
    assert_eq!(
        normal_layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "A"),
            (NativePoint { x: 8, y: 0 }, " B"),
        ]
    );
}
#[test]
fn native_pre_preserves_bounded_whitespace_without_soft_wrap() {
    let document = NativeDocument::parse(
        "<style>#flow { white-space: pre; line-height: 24px; width: 32px; }</style><div id='flow'> A  B\n<span id='inline' style='display:inline'>\tCDEFG</span>\r\nD </div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(flow).unwrap().height, 72);
    assert_eq!(
        layout.box_for(inline),
        Some(NativeRect {
            x: 0,
            y: 24,
            width: 32,
            height: 24,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 0 }, " A  B"),
            (inline, NativePoint { x: 0, y: 24 }, "\tCDEFG"),
            (flow, NativePoint { x: 0, y: 48 }, "D "),
        ]
    );
    assert_eq!(
        document.visible_text(100),
        ("A B CDEFG D".to_owned(), false)
    );

    let list = document.display_list(viewport).unwrap();
    assert_eq!(
        list.commands
            .iter()
            .filter_map(|command| match command {
                NativeDisplayCommand::TextRun { origin, text, .. } =>
                    Some((*origin, text.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, " A  B"),
            (NativePoint { x: 0, y: 24 }, "\tCDEFG"),
            (NativePoint { x: 0, y: 48 }, "D "),
        ]
    );
}
#[test]
fn native_pre_wrap_preserves_whitespace_and_soft_wraps_at_fixed_cell_capacity() {
    let document = NativeDocument::parse(
        "<style>#flow { white-space: pre-wrap; line-height: 24px; width: 32px; }</style><div id='flow'>AB C D\n<span id='inline' style='display:inline; width:32px'> \tDE</span>\r\nF </div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 128,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(flow).unwrap().height, 96);
    assert_eq!(
        layout.box_for(inline),
        Some(NativeRect {
            x: 0,
            y: 48,
            width: 32,
            height: 24,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 0 }, "AB C"),
            (flow, NativePoint { x: 0, y: 24 }, " D"),
            (inline, NativePoint { x: 0, y: 48 }, " \tDE"),
            (flow, NativePoint { x: 0, y: 72 }, "F "),
        ]
    );
    assert_eq!(
        document.visible_text(100),
        ("AB C D DE F".to_owned(), false)
    );

    let list = document.display_list(viewport).unwrap();
    assert_eq!(
        list.commands
            .iter()
            .filter_map(|command| match command {
                NativeDisplayCommand::TextRun { origin, text, .. } =>
                    Some((*origin, text.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "AB C"),
            (NativePoint { x: 0, y: 24 }, " D"),
            (NativePoint { x: 0, y: 48 }, " \tDE"),
            (NativePoint { x: 0, y: 72 }, "F "),
        ]
    );
}
#[test]
fn native_explicit_dimension_inheritance_reaches_layout_and_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
        #normal-parent { width:24px; height:12px; min-width:20px; max-width:28px; min-height:10px; max-height:14px; background-color:blue; }
        #normal-child { display:block; width:inherit; height:inherit; min-width:inherit; max-width:inherit; min-height:inherit; max-height:inherit; box-sizing:border-box; padding:2px; border:1px solid red; background-color:red; }
        #flex-parent { display:flex; width:40px; height:12px; background-color:green; }
        #flex-child { display:block; width:inherit; height:inherit; flex-shrink:0; background-color:red; }
        #flex-sibling { display:block; width:8px; height:6px; flex-shrink:0; background-color:blue; }
        </style>
        <div id='normal-parent'><button id='normal-child'>Normal</button></div>
        <div id='flex-parent'><button id='flex-child'>Flex</button><button id='flex-sibling'>Sibling</button></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let normal_parent = document.resolve_target("id=normal-parent").unwrap();
    let normal_child = document.resolve_target("id=normal-child").unwrap();
    let flex_parent = document.resolve_target("id=flex-parent").unwrap();
    let flex_child = document.resolve_target("id=flex-child").unwrap();
    let flex_sibling = document.resolve_target("id=flex-sibling").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(normal_parent),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 12,
        })
    );
    assert_eq!(
        layout.box_for(normal_child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 12,
        })
    );
    assert_eq!(
        layout.box_for(flex_parent),
        Some(NativeRect {
            x: 0,
            y: 12,
            width: 40,
            height: 12,
        })
    );
    assert_eq!(
        layout.box_for(flex_child),
        Some(NativeRect {
            x: 0,
            y: 12,
            width: 40,
            height: 12,
        })
    );
    assert_eq!(
        layout.box_for(flex_sibling),
        Some(NativeRect {
            x: 40,
            y: 12,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(layout.content_width, 64);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(normal_child));
    assert_eq!(layout.hit_test(1, 13).unwrap(), Some(flex_child));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, .. } if *node_id == normal_child
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());

    let semantic_nodes = document.semantic_nodes();
    for pair in [normal_child, flex_child, flex_sibling].windows(2) {
        assert!(
            semantic_nodes
                .iter()
                .position(|node| node.node_id == pair[0])
                < semantic_nodes
                    .iter()
                    .position(|node| node.node_id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height"
            )
    }));
}
#[test]
fn recovered_malformed_document_commits_with_a_new_revision() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(recovered_malformed_document_commits_with_a_new_revision_inner());
    });
}

async fn recovered_malformed_document_commits_with_a_new_revision_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://broken", "<p title='unfinished>")
        .unwrap();
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let before = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();

    let navigation = dispatcher
        .navigate(NavigationRequest {
            url: "fixture://broken".into(),
        })
        .await
        .unwrap();
    assert_eq!(navigation.url, "fixture://broken");
    assert_eq!(navigation.revision, before.revision + 1);
    let after = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(after.url, navigation.url);
    assert!(after.title.is_empty());
    // EOF while the tokenizer is inside a quoted attribute discards the
    // unfinished tag instead of treating its source as document text.
    assert!(after.visible_text.is_empty());
    assert!(after.complete);
    dispatcher.close().await.unwrap();
}
#[tokio::test]
async fn fixture_relative_links_normalize_paths_queries_and_reject_opaque_bases() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://relative.test/docs/index",
            "<a id='parent' href='../next#target'>Parent</a><a id='dot' href='./child/../next?mode=fast#target'>Dot</a><a id='query' href='?view=full#target'>Query</a>",
        )
        .unwrap()
        .with_fixture("fixture://relative.test/next", "<p id='target'>Parent target</p>")
        .unwrap()
        .with_fixture(
            "fixture://relative.test/docs/next?mode=fast",
            "<p id='target'>Dot target</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://relative.test/docs/index?view=full",
            "<p id='target'>Query target</p>",
        )
        .unwrap()
        .with_initial_url("fixture://relative.test/docs/index");

    for (target, expected_url, expected_text) in [
        (
            "id=parent",
            "fixture://relative.test/next#target",
            "Parent target",
        ),
        (
            "id=dot",
            "fixture://relative.test/docs/next?mode=fast#target",
            "Dot target",
        ),
        (
            "id=query",
            "fixture://relative.test/docs/index?view=full#target",
            "Query target",
        ),
    ] {
        let mut engine = NativeEngine::new(config.clone()).unwrap();
        engine.initialize().unwrap();
        let result = engine
            .action(NativeAction::Click {
                target: target.into(),
            })
            .unwrap();
        assert!(result.accepted);
        assert_eq!(engine.snapshot().unwrap().url, expected_url);
        assert_eq!(engine.snapshot().unwrap().visible_text, expected_text);
    }

    let data_config = NativeEngineConfig::default().with_initial_url(
        "data:text/html,%3Ca%20id%3D%27relative%27%20href%3D%27next%27%3ERelative%3C%2Fa%3E",
    );
    let mut data_engine = NativeEngine::new(data_config).unwrap();
    data_engine.initialize().unwrap();
    let before_data = data_engine.snapshot().unwrap();
    let before_data_history = data_engine.history().clone();
    let data_error = data_engine
        .action(NativeAction::Click {
            target: "id=relative".into(),
        })
        .unwrap_err();
    assert!(matches!(
        data_error,
        NativeEngineError::UnsupportedUrl { .. }
    ));
    assert!(!data_error.to_string().contains("next"));
    assert_eq!(data_engine.snapshot().unwrap(), before_data);
    assert_eq!(data_engine.history(), &before_data_history);
}
#[tokio::test]
async fn percent_decoded_fragments_match_utf8_ids_and_fail_closed() {
    let html = native_html_without_body_margin(
        "<a id='jump' href='#pricing%20plan'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><button id='pricing plan'>Spaced target</button><button id='café'>UTF-8 target</button><button id='a+b'>Plus target</button><p id='duplicate name'>First duplicate</p><p id='duplicate name'>Second duplicate</p>",
    );
    let viewport = Viewport {
        width: 160,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let spaced_id = expected_document.resolve_target("id=pricing plan").unwrap();
    let spaced_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(spaced_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(spaced_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://encoded-fragments", &html)
        .unwrap()
        .with_initial_url("fixture://encoded-fragments");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let jumped = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jumped.accepted);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://encoded-fragments#pricing%20plan"
    );
    assert_eq!(engine.scroll_offset(), spaced_scroll);

    let utf8_id = NativeDocument::parse(&html, &NativeEngineLimits::default())
        .unwrap()
        .resolve_target("id=café")
        .unwrap();
    let utf8_scroll = NativeDocument::parse(&html, &NativeEngineLimits::default())
        .unwrap()
        .layout(viewport)
        .unwrap()
        .box_for(utf8_id)
        .unwrap()
        .y;
    engine
        .navigate("fixture://encoded-fragments#caf%C3%A9")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        utf8_scroll.min(expected_layout.max_scroll_offset().y)
    );

    engine.navigate("fixture://encoded-fragments#a+b").unwrap();
    let plus_scroll = engine.scroll_offset();
    assert!(plus_scroll.y > 0);
    engine
        .navigate("fixture://encoded-fragments#a%20b")
        .unwrap();
    assert_eq!(engine.scroll_offset(), plus_scroll);

    for fragment in ["duplicate%20name", "bad%ZZ", "bad%C3%28"] {
        let before = engine.scroll_offset();
        engine
            .navigate(format!("fixture://encoded-fragments#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), before, "fragment={fragment}");
    }
}
#[tokio::test]
async fn native_namespaced_attributes_preserve_attr_identity_and_commands() {
    let mut engine = NativeEngine::new(NativeEngineConfig::default().with_initial_url(
        "data:text/html,%3Cbody%3E%3Csvg%20id%3D%27icon%27%20xmlns%3Axlink%3D%27http%3A%2F%2Fwww.w3.org%2F1999%2Fxlink%27%20xlink%3Ahref%3D%27%23old%27%20xml%3Alang%3D%27en%27%3E%3C%2Fsvg%3E%3C%2Fbody%3E",
    ))
    .unwrap();
    engine.initialize_async().await.unwrap();

    let result = engine
        .evaluate_async(
            r#"(() => {
                const xlink = 'http://www.w3.org/1999/xlink';
                const xml = 'http://www.w3.org/XML/1998/namespace';
                const icon = document.getElementById('icon');
                const initial = icon.getAttributeNodeNS(xlink, 'href');
                const ordinaryLookup = icon.getAttributeNode('xlink:href');
                const xmlAttribute = icon.attributes.getNamedItemNS(xml, 'lang');
                const parsed = [
                    icon.getAttributeNS(xlink, 'href'),
                    initial && [initial.name, initial.localName, initial.prefix, initial.namespaceURI, initial.value],
                    ordinaryLookup && ordinaryLookup.namespaceURI,
                    xmlAttribute && [xmlAttribute.name, xmlAttribute.namespaceURI, xmlAttribute.value],
                ];
                icon.setAttributeNS(xlink, 'xlink:href', '#new');
                const updated = icon.getAttributeNodeNS(xlink, 'href');
                updated.value = '#setter';
                const afterSetter = icon.getAttributeNS(xlink, 'href');
                const created = document.createAttributeNS(xlink, 'xlink:title');
                created.value = 'title';
                const replaced = icon.setAttributeNodeNS(xlink, created);
                const createdOwner = created.ownerElement === icon;
                const clone = icon.cloneNode(false);
                const clonedHref = clone.getAttributeNodeNS(xlink, 'href');
                const clonedTitle = clone.attributes.getNamedItemNS(xlink, 'title');
                const parsedContainer = document.createElement('div');
                parsedContainer.innerHTML = "<svg xmlns:xlink='http://www.w3.org/1999/xlink' xlink:href='#inner'></svg>";
                const parsedDetached = parsedContainer.firstElementChild;
                let invalid = '';
                try { icon.setAttributeNS(xlink, 'href', 'wrong namespace prefix'); } catch (error) { invalid = error.name; }
                const map = icon.attributes;
                const mapState = [
                    map.getNamedItemNS(xlink, 'href') === updated,
                    map.getNamedItemNS(xlink, 'title') === created,
                    map.getNamedItemNS(xml, 'lang') === xmlAttribute,
                    map.length,
                ];
                const removed = map.removeNamedItemNS(xlink, 'title');
                icon.removeAttributeNS(xlink, 'href');
                return {
                    parsed,
                    afterSetter,
                    updated: [updated.namespaceURI, updated.localName, updated.prefix, updated.value],
                    created: [created.namespaceURI, created.localName, created.prefix, createdOwner],
                    replaced: replaced === null,
                    clone: [clone.namespaceURI, clonedHref && clonedHref.namespaceURI, clonedHref && clonedHref.value, clonedTitle && clonedTitle.value],
                    detached: [parsedDetached.namespaceURI, parsedDetached.getAttributeNS(xlink, 'href'), parsedDetached.getAttributeNode('xlink:href').namespaceURI],
                    mapState,
                    removed: [removed === created, created.ownerElement === null],
                    final: [icon.getAttributeNS(xlink, 'href'), icon.getAttributeNS(xlink, 'title'), icon.attributes.length],
                    invalid,
                };
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "parsed": [
                "#old",
                [
                    "xlink:href",
                    "href",
                    "xlink",
                    "http://www.w3.org/1999/xlink",
                    "#old",
                ],
                "http://www.w3.org/1999/xlink",
                ["xml:lang", "http://www.w3.org/XML/1998/namespace", "en"],
            ],
            "afterSetter": "#setter",
            "updated": [
                "http://www.w3.org/1999/xlink",
                "href",
                "xlink",
                "#setter",
            ],
            "created": [
                "http://www.w3.org/1999/xlink",
                "title",
                "xlink",
                true,
            ],
            "replaced": true,
            "clone": [
                "http://www.w3.org/2000/svg",
                "http://www.w3.org/1999/xlink",
                "#setter",
                "title",
            ],
            "detached": [
                "http://www.w3.org/2000/svg",
                "#inner",
                "http://www.w3.org/1999/xlink",
            ],
            "mapState": [true, true, true, 5],
            "removed": [true, true],
            "final": [null, null, 3],
            "invalid": "NamespaceError",
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn text_fragments_match_bounded_visible_runs_and_fail_closed() {
    let html = native_html_without_body_margin(
        "<a id='jump' href='#:~:text=target%20phrase,anchor'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><p style='white-space:pre'>target phrase anchor and more</p><p style='white-space:pre'>comma, target phrase</p><p style='white-space:pre'>repeat target</p><p style='white-space:pre'>repeat target</p><p>cross <span>run target</span></p><p style='display:none'>hidden target</p><p style='white-space:pre'>plus a+b</p>",
    );
    let viewport = Viewport {
        width: 240,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let range_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("target phrase") && run.text.contains("anchor"))
        .unwrap();
    let range_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(range_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(range_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://text-fragments", &html)
        .unwrap()
        .with_initial_url("fixture://text-fragments");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let jumped = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jumped.accepted);
    assert_eq!(engine.scroll_offset(), range_scroll);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://text-fragments#:~:text=target%20phrase,anchor"
    );

    let initial = engine.go_back().unwrap().unwrap();
    assert_eq!(initial.url, "fixture://text-fragments");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    let restored = engine.go_forward().unwrap().unwrap();
    assert_eq!(
        restored.url,
        "fixture://text-fragments#:~:text=target%20phrase,anchor"
    );
    assert_eq!(engine.scroll_offset(), range_scroll);

    let comma_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("comma, target phrase"))
        .unwrap();
    engine
        .navigate("fixture://text-fragments#:~:text=comma%2C%20target%20phrase")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(comma_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );

    let plus_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("plus a+b"))
        .unwrap();
    engine
        .navigate("fixture://text-fragments#:~:text=plus%20a%2Bb")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(plus_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );

    let first_repeat = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("repeat target"))
        .unwrap();
    engine
        .navigate("fixture://text-fragments#:~:text=repeat%20target")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(first_repeat.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );

    for fragment in [
        ":~:text=hidden%20target",
        ":~:text=cross,target",
        ":~:text=anchor,target",
        ":~:text=prefix-,target",
        ":~:text=target,-suffix",
        ":~:text=target,end,extra",
        ":~:text=bad%ZZ",
        ":~:text=bad%C3%28",
        ":~:text=missing",
    ] {
        let before = engine.scroll_offset();
        engine
            .navigate(format!("fixture://text-fragments#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), before, "fragment={fragment}");
    }
}
#[tokio::test]
async fn text_fragment_affixes_match_adjacent_terms_and_fail_closed() {
    let html = native_html_without_body_margin(
        "<a id='jump' href='#:~:text=prefix%20-,target%20phrase,anchor,-%20suffix'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><p style='white-space:pre'>prefix target phrase anchor suffix</p><p style='white-space:pre'>prefix target phrase mismatch</p><p style='white-space:pre'>target phrase anchor suffix</p><p>cross <span>target phrase</span></p><p style='display:none'>prefix target phrase anchor suffix</p><p style='white-space:pre'>target phrase anchor</p>",
    );
    let viewport = Viewport {
        width: 240,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let affix_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text == "prefix target phrase anchor suffix")
        .unwrap();
    let affix_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(affix_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(affix_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://text-affixes", &html)
        .unwrap()
        .with_initial_url("fixture://text-affixes");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let jumped = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jumped.accepted);
    assert_eq!(engine.scroll_offset(), affix_scroll);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://text-affixes#:~:text=prefix%20-,target%20phrase,anchor,-%20suffix"
    );

    let initial = engine.go_back().unwrap().unwrap();
    assert_eq!(initial.url, "fixture://text-affixes");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    let restored = engine.go_forward().unwrap().unwrap();
    assert_eq!(
        restored.url,
        "fixture://text-affixes#:~:text=prefix%20-,target%20phrase,anchor,-%20suffix"
    );
    assert_eq!(engine.scroll_offset(), affix_scroll);

    for fragment in [
        ":~:text=target%20phrase",
        ":~:text=target%20phrase,anchor",
        ":~:text=prefix%20-,target%20phrase",
        ":~:text=target%20phrase,-%20suffix",
        ":~:text=prefix%20-,target%20phrase,-%20suffix",
        ":~:text=prefix%20-,target%20phrase,anchor",
        ":~:text=target%20phrase,anchor,-%20suffix",
        ":~:text=prefix%20-,target%20phrase,anchor,-%20suffix",
    ] {
        engine
            .navigate(format!("fixture://text-affixes#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), affix_scroll, "fragment={fragment}");
    }

    for fragment in [
        ":~:text=prefix-,target%20phrase",
        ":~:text=target%20phrase,-suffix",
        ":~:text=wrong%20-,target%20phrase",
        ":~:text=target%20phrase,-%20wrong",
        ":~:text=anchor,target%20phrase",
        ":~:text=cross,target",
        ":~:text=hidden%20-,target%20phrase",
        ":~:text=target%20phrase,anchor,extra",
        ":~:text=prefix%20-,target%20phrase,anchor,extra",
        ":~:text=prefix%20-,target%20phrase,-%20suffix,extra",
        ":~:text=target%20phrase,anchor,-",
        ":~:text=bad%ZZ,target",
    ] {
        let before = engine.scroll_offset();
        engine
            .navigate(format!("fixture://text-affixes#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), before, "fragment={fragment}");
    }
}
#[test]
fn semantic_storage_uses_page_owned_native_state() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(semantic_storage_uses_page_owned_native_state_inner());
    });
}

async fn semantic_storage_uses_page_owned_native_state_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let backend = NativeEngineBackend::new(NativeEngineConfig::default()).unwrap();
    assert_eq!(
        backend
            .profile()
            .capability(BrowserCapability::Storage)
            .level,
        SupportLevel::Available
    );
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let written = dispatcher
        .storage(StorageRequest {
            context_id: "native-context".into(),
            scope: StorageScope::Local,
            operation: StorageOperation::Write {
                key: "theme".into(),
                value: "dark".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(written.entries.get("theme"), Some(&"dark".to_owned()));
    let page_local = dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "localStorage.getItem('theme')".into(),
        })
        .await
        .unwrap();
    assert_eq!(page_local.value, serde_json::json!("dark"));

    let session = dispatcher
        .storage(StorageRequest {
            context_id: "native-context".into(),
            scope: StorageScope::Session,
            operation: StorageOperation::Write {
                key: "tab".into(),
                value: "one".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(session.entries.get("tab"), Some(&"one".to_owned()));
    let page_session = dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "sessionStorage.getItem('tab')".into(),
        })
        .await
        .unwrap();
    assert_eq!(page_session.value, serde_json::json!("one"));

    let local = dispatcher
        .storage(StorageRequest {
            context_id: "native-context".into(),
            scope: StorageScope::Local,
            operation: StorageOperation::Read,
        })
        .await
        .unwrap();
    assert_eq!(local.entries.get("theme"), Some(&"dark".to_owned()));
    assert!(!local.entries.contains_key("tab"));

    let cleared = dispatcher
        .storage(StorageRequest {
            context_id: "native-context".into(),
            scope: StorageScope::Local,
            operation: StorageOperation::Clear,
        })
        .await
        .unwrap();
    assert!(cleared.entries.is_empty());
    let page_cleared = dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "localStorage.getItem('theme')".into(),
        })
        .await
        .unwrap();
    assert_eq!(page_cleared.value, serde_json::Value::Null);

    let cookies = dispatcher
        .storage(StorageRequest {
            context_id: "native-context".into(),
            scope: StorageScope::Cookies,
            operation: StorageOperation::Read,
        })
        .await
        .unwrap();
    assert!(cookies.entries.is_empty());
    dispatcher.close().await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/path/page"));
        let body = "<title>Cookie storage</title><p>Cookie fixture</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let cookie_url = format!("http://{address}/path/page");
    let cookie_backend = NativeEngineBackend::new(
        NativeEngineConfig::default().with_initial_url(cookie_url.clone()),
    )
    .unwrap();
    let cookie_dispatcher = BrowserBackendDispatcher::new(&cookie_backend);
    cookie_dispatcher.initialize().await.unwrap();
    let cookie_write = cookie_dispatcher
        .storage(StorageRequest {
            context_id: "native-context".into(),
            scope: StorageScope::Cookies,
            operation: StorageOperation::Write {
                key: "session".into(),
                value: "native".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(
        cookie_write.entries.get("session"),
        Some(&"native".to_owned())
    );
    let page_cookie = cookie_dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "document.cookie".into(),
        })
        .await
        .unwrap();
    assert_eq!(page_cookie.value, serde_json::json!("session=native"));
    cookie_dispatcher.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn lifecycle_state_is_terminal_after_close() {
    let mut engine = glass_browser::NativeEngine::new(NativeEngineConfig::default()).unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::New);
    assert_eq!(engine.runtime_state(), NativeRuntimeState::New);
    engine.initialize().unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::Running);
    assert_eq!(engine.runtime_state(), NativeRuntimeState::Running);
    let running_trace = engine.runtime_trace();
    assert!(
        running_trace
            .iter()
            .any(|event| event.kind == NativeRuntimeTraceKind::Started)
    );
    assert!(
        running_trace
            .iter()
            .any(|event| event.kind == NativeRuntimeTraceKind::TaskReady)
    );
    engine.close().unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::Closed);
    assert_eq!(engine.runtime_state(), NativeRuntimeState::Closed);
    let closed_trace = engine.runtime_trace();
    assert!(
        closed_trace
            .iter()
            .any(|event| event.kind == NativeRuntimeTraceKind::CancellationRequested)
    );
    assert!(
        closed_trace
            .iter()
            .any(|event| event.kind == NativeRuntimeTraceKind::Closed)
    );
    assert!(engine.initialize().is_err());
    assert!(engine.close().is_err());
}
#[test]
fn semantic_dom_references_and_locators_are_revision_bound() {
    let document = NativeDocument::parse(
        "<label for='query'>Query</label><input id='query' type='search'><button id='go'>Go</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let nodes = document.semantic_nodes();
    let query = nodes.iter().find(|node| node.role == "textbox").unwrap();
    assert_eq!(query.name, "Query");
    assert_eq!(document.resolve_target("id=query"), Ok(query.node_id));
    assert_eq!(
        document.resolve_target("role=textbox[name=Query]"),
        Ok(query.node_id)
    );
    assert_eq!(
        query.reference,
        format!("ref=r1:n{}", query.node_id.index())
    );
    assert!(document.resolve_target(&query.reference).is_ok());
}
#[test]
fn native_documents_adopt_nodes_within_their_browsing_contexts() {
    // The production TUI entrypoint assigns its Tokio runtime workers 4 MiB
    // stacks. This deep cross-document adoption case needs the same headroom
    // when the test harness invokes it on a fresh thread.
    std::thread::Builder::new()
        .name("native-adopt-node-test".into())
        .stack_size(4 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("native adoption test runtime");
            runtime.block_on(native_documents_adopt_nodes_within_their_browsing_contexts_inner());
        })
        .expect("native adoption test thread")
        .join()
        .expect("native adoption test thread must complete");
}

async fn native_documents_adopt_nodes_within_their_browsing_contexts_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native adopt-node request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<!doctype html><template id='top-inert'></template><div id='top-holder'><section id='top-source' data-origin='top'><span id='top-child'>child</span><template id='top-nested'><b id='top-nested-child'>nested</b></template></section><p id='same-source'>same</p><p id='cross-source'>cross</p><x-adoption-probe id='adoption-probe'></x-adoption-probe><div id='depth-source'></div><div id='node-source'></div></div><iframe id='child' src='/child'></iframe><iframe id='sibling' src='/sibling'></iframe>"
                }
                Some("/child") => {
                    "<!doctype html><template id='frame-inert'></template><div id='frame-holder'><section id='frame-source' data-origin='frame'><span id='frame-child'>child</span><template id='frame-nested'><i id='frame-nested-child'>nested</i></template></section><p id='frame-cross-source'>frame-cross</p><p id='sibling-cross-source'>sibling-cross</p></div>"
                }
                Some("/sibling") => {
                    "<!doctype html><div id='sibling-holder'></div><p id='sibling-source'>sibling</p>"
                }
                other => panic!("unexpected native adopt-node request path: {other:?}"),
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
    let adopted = session
        .script(
            r##"(() => {
                const childDocument = document.getElementById('child').contentDocument;
                const siblingDocument = document.getElementById('sibling').contentDocument;
                const adoptionTrace = [];
                class AdoptionProbe extends HTMLElement {
                  adoptedCallback(oldDocument, newDocument) {
                    adoptionTrace.push([
                      oldDocument === document,
                      newDocument === childDocument,
                      this.ownerDocument === childDocument,
                    ]);
                  }
                }
                customElements.define('x-adoption-probe', AdoptionProbe);
                const adoptionProbe = document.getElementById('adoption-probe');
                globalThis.__glassCustomAdoptionProbe = adoptionProbe;
                childDocument.adoptNode(adoptionProbe);
                globalThis.__glassCustomAdoptionTrace = adoptionTrace;
                const topTarget = document.getElementById('top-inert').content.ownerDocument;
                const frameTarget = childDocument.getElementById('frame-inert').content.ownerDocument;
                const topSource = document.getElementById('top-source');
                const topAttribute = topSource.getAttributeNode('data-origin');
                const topNested = topSource.querySelector('#top-nested');
                const topNestedContent = topNested.content;
                const topNestedChild = topNestedContent.firstElementChild;
                globalThis.__glassAdoptedTopSource = topSource;
                globalThis.__glassAdoptedTopTarget = topTarget;
                const topResult = topTarget.adoptNode(topSource);

                const frameSource = childDocument.getElementById('frame-source');
                const frameAttribute = frameSource.getAttributeNode('data-origin');
                const frameNested = frameSource.querySelector('#frame-nested');
                const frameNestedContent = frameNested.content;
                const frameNestedChild = frameNestedContent.firstElementChild;
                globalThis.__glassAdoptedFrameSource = frameSource;
                globalThis.__glassAdoptedFrameTarget = frameTarget;
                const frameResult = frameTarget.adoptNode(frameSource);

                const sameSource = document.getElementById('same-source');
                globalThis.__glassAdoptedSameSource = sameSource;
                const sameResult = document.adoptNode(sameSource);

                const text = document.createTextNode('detached text');
                const comment = document.createComment('detached comment');
                const type = document.implementation.createDocumentType('html', 'public', 'system');
                const fragment = document.createDocumentFragment();
                const fragmentChild = document.createElement('em');
                fragment.appendChild(fragmentChild);
                const adoptedText = topTarget.adoptNode(text);
                const adoptedComment = topTarget.adoptNode(comment);
                const adoptedType = topTarget.adoptNode(type);
                const adoptedFragment = topTarget.adoptNode(fragment);

                const crossSource = document.getElementById('cross-source');
                const crossParent = crossSource.parentNode;
                globalThis.__glassAdoptedCrossSource = crossSource;
                let crossError = null;
                let crossResult = null;
                try {
                  crossResult = childDocument.adoptNode(crossSource);
                  childDocument.getElementById('frame-holder').appendChild(crossResult);
                  crossResult.setAttribute('data-after-adopt', 'top-to-frame');
                } catch (error) { crossError = error.name; }

                const frameCrossSource = childDocument.getElementById('frame-cross-source');
                globalThis.__glassAdoptedFrameCrossSource = frameCrossSource;
                const frameCrossResult = document.adoptNode(frameCrossSource);
                document.getElementById('top-holder').appendChild(frameCrossResult);
                frameCrossResult.setAttribute('data-after-adopt', 'frame-to-top');

                const siblingSource = childDocument.getElementById('sibling-cross-source');
                globalThis.__glassAdoptedSiblingSource = siblingSource;
                const siblingResult = siblingDocument.adoptNode(siblingSource);
                siblingDocument.getElementById('sibling-holder').appendChild(siblingResult);
                siblingResult.setAttribute('data-after-adopt', 'frame-to-sibling');

                let documentError = null;
                try { document.adoptNode(document); } catch (error) { documentError = error.name; }
                let invalidNodeError = null;
                try { document.adoptNode({ nodeType: 1 }); } catch (error) { invalidNodeError = error.name; }
                const shadowRoot = Object.create(Node.prototype);
                Object.defineProperties(shadowRoot, {
                  nodeType: { value: 11 },
                  __glassShadowRoot: { value: true },
                });
                let shadowRootError = null;
                try { document.adoptNode(shadowRoot); } catch (error) { shadowRootError = error.name; }

                const depthSource = document.getElementById('depth-source');
                let depthCursor = depthSource;
                for (let index = 0; index < 256; index += 1) {
                  const synthetic = Object.create(Element.prototype);
                  Object.defineProperties(synthetic, {
                    nodeType: { value: 1 },
                    namespaceURI: { value: 'http://www.w3.org/1999/xhtml' },
                    localName: { value: 'div' },
                    __glassChildren: { value: [], writable: true },
                    getAttributeNames: { value: () => [] },
                    getAttributeNode: { value: () => null },
                  });
                  depthCursor.__glassChildren.push(synthetic);
                  depthCursor = synthetic;
                }
                let depthError = null;
                try { topTarget.adoptNode(depthSource); } catch (error) { depthError = error.name; }

                const nodeSource = document.getElementById('node-source');
                for (let index = 0; index < 5000; index += 1) {
                  const synthetic = Object.create(Text.prototype);
                  Object.defineProperty(synthetic, 'nodeType', { value: 3 });
                  nodeSource.__glassChildren.push(synthetic);
                }
                let nodeLimitError = null;
                try { topTarget.adoptNode(nodeSource); } catch (error) { nodeLimitError = error.name; }

                return {
                  top: [topResult === topSource, topSource.parentNode === null,
                    topSource.ownerDocument === topTarget,
                    topSource.firstElementChild.ownerDocument === topTarget,
                    topAttribute.ownerElement === topSource,
                    topAttribute.ownerDocument === topTarget,
                    topNested.content === topNestedContent,
                    topNestedContent.ownerDocument === topTarget,
                    topNestedChild.ownerDocument === topTarget],
                  frame: [frameSource instanceof Node, frameSource instanceof Element,
                    frameResult === frameSource, frameSource.parentNode === null,
                    frameSource.ownerDocument === frameTarget,
                    frameSource.firstElementChild.ownerDocument === frameTarget,
                    frameAttribute.ownerElement === frameSource,
                    frameAttribute.ownerDocument === frameTarget,
                    frameNested.content === frameNestedContent,
                    frameNestedContent.ownerDocument === frameTarget,
                    frameNestedChild.ownerDocument === frameTarget],
                  sameDocument: [sameResult === sameSource, sameSource.parentNode === null,
                    sameSource.ownerDocument === document],
                  otherNodes: [adoptedText === text, adoptedText.ownerDocument === topTarget,
                    adoptedComment === comment, adoptedComment.ownerDocument === topTarget,
                    adoptedType === type, adoptedType.ownerDocument === topTarget,
                    adoptedFragment === fragment, adoptedFragment.ownerDocument === topTarget,
                    fragmentChild.ownerDocument === topTarget],
                  errors: [documentError, invalidNodeError, shadowRootError, crossError,
                    crossSource.parentNode === childDocument.getElementById('frame-holder'),
                    crossSource.ownerDocument === childDocument,
                    crossSource.parentNode !== crossParent,
                    crossResult === crossSource],
                  transfers: [frameCrossResult === frameCrossSource,
                    frameCrossSource.parentNode === document.getElementById('top-holder'),
                    frameCrossSource.ownerDocument === document,
                    frameCrossSource.getAttribute('data-after-adopt'),
                    siblingResult === siblingSource,
                    siblingSource.parentNode === siblingDocument.getElementById('sibling-holder'),
                    siblingSource.ownerDocument === siblingDocument,
                    siblingSource.getAttribute('data-after-adopt')],
                  bounds: [depthError, depthSource.parentNode === document.getElementById('top-holder'),
                    depthSource.ownerDocument === document,
                    nodeLimitError, nodeSource.parentNode === document.getElementById('top-holder'),
                    nodeSource.ownerDocument === document],
                  customAdoption: [adoptionProbe.ownerDocument === childDocument,
                    adoptionTrace.length, adoptionTrace[0]],
                };
              })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        adopted.value,
        serde_json::json!({
            "top": [true, true, true, true, true, true, true, true, true],
            "frame": [true, true, true, true, true, true, true, true, true, true, true],
            "sameDocument": [true, true, true],
            "otherNodes": [true, true, true, true, true, true, true, true, true],
            "errors": [
                "NotSupportedError",
                "TypeError",
                "HierarchyRequestError",
                null,
                true,
                true,
                true,
                true,
            ],
            "transfers": [true, true, true, "frame-to-top", true, true, true, "frame-to-sibling"],
            "bounds": [
                "RangeError",
                true,
                true,
                "RangeError",
                true,
                true,
            ],
            "customAdoption": [true, 1, [true, true, true]],
        })
    );

    let persisted = session
        .script(
            r##"(() => {
                const childDocument = document.getElementById('child').contentDocument;
                const siblingDocument = document.getElementById('sibling').contentDocument;
                const topSource = globalThis.__glassAdoptedTopSource;
                const frameSource = globalThis.__glassAdoptedFrameSource;
                const sameSource = globalThis.__glassAdoptedSameSource;
                const crossSource = globalThis.__glassAdoptedCrossSource;
                const frameCrossSource = globalThis.__glassAdoptedFrameCrossSource;
                const siblingSource = globalThis.__glassAdoptedSiblingSource;
                const adoptionProbe = globalThis.__glassCustomAdoptionProbe;
                const topTarget = globalThis.__glassAdoptedTopTarget;
                const frameTarget = globalThis.__glassAdoptedFrameTarget;
                return [
                  document.getElementById('top-source') === null,
                  topSource.parentNode === null,
                  topSource.ownerDocument === topTarget,
                  topSource.querySelector('#top-nested').content.querySelector('b').ownerDocument === topTarget,
                  childDocument.getElementById('frame-source') === null,
                  frameSource.parentNode === null,
                  frameSource.ownerDocument === frameTarget,
                  frameSource.querySelector('#frame-nested').content.querySelector('i').ownerDocument === frameTarget,
                  document.getElementById('same-source') === null,
                  sameSource.parentNode === null,
                  sameSource.ownerDocument === document,
                  childDocument.getElementById('cross-source') === crossSource,
                  crossSource.parentNode === childDocument.getElementById('frame-holder'),
                  crossSource.getAttribute('data-after-adopt') === 'top-to-frame',
                  document.getElementById('frame-cross-source') === frameCrossSource,
                  frameCrossSource.parentNode === document.getElementById('top-holder'),
                  frameCrossSource.getAttribute('data-after-adopt') === 'frame-to-top',
                  siblingDocument.getElementById('sibling-cross-source') === siblingSource,
                  siblingSource.parentNode === siblingDocument.getElementById('sibling-holder'),
                  siblingSource.getAttribute('data-after-adopt') === 'frame-to-sibling',
                  adoptionProbe.ownerDocument === childDocument,
                  globalThis.__glassCustomAdoptionTrace.length === 1,
                  globalThis.__glassCustomAdoptionTrace[0][0],
                  globalThis.__glassCustomAdoptionTrace[0][1],
                  globalThis.__glassCustomAdoptionTrace[0][2],
                ];
              })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        persisted.value,
        serde_json::json!([
            true, true, true, true, true, true, true, true, true, true, true, true, true, true,
            true, true, true, true, true, true, true, true, true, true, true
        ])
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
