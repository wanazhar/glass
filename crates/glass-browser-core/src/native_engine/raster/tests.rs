use super::*;

use crate::native_engine::{
    NativeBorderPaint, NativeBorderPaintSide, NativeBorderStyle, NativeColor, NativeDisplayCommand,
    NativeDisplayList, NativeDocument, NativeFontRun, NativeGlyph, NativeGlyphComposite,
    NativePoint, NativeTextDecorationSkipInk, NativeTextDecorationStyle, Viewport,
};
use std::io::Cursor;
use std::sync::Arc;

fn display_list(commands: Vec<NativeDisplayCommand>, width: u32, height: u32) -> NativeDisplayList {
    NativeDisplayList {
        revision: 1,
        viewport: Viewport {
            width,
            height,
            device_scale_factor_milli: 1000,
        },
        scroll_offset: NativePoint { x: 0, y: 0 },
        commands,
        text_run_boundaries: Vec::new(),
    }
}

fn uniform_border(width: u32, color: NativeColor) -> NativeBorderPaint {
    let side = NativeBorderPaintSide {
        width,
        style: NativeBorderStyle::Solid,
        color,
    };
    NativeBorderPaint {
        top: side,
        right: side,
        bottom: side,
        left: side,
    }
}

fn styled_border(
    width: u32,
    style: NativeBorderStyle,
    color: NativeColor,
) -> NativeBorderPaintSide {
    NativeBorderPaintSide {
        width,
        style,
        color,
    }
}

#[test]
fn surface_encodes_deterministic_rgba_png() {
    let surface = NativeSurface::from_display_list(&display_list(
        vec![NativeDisplayCommand::Clear {
            color: NativeColor {
                red: 12,
                green: 34,
                blue: 56,
                alpha: 255,
            },
        }],
        2,
        1,
    ))
    .unwrap();
    let first = surface.to_png().unwrap();
    assert_eq!(first, surface.to_png().unwrap());
    assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");

    let decoder = png::Decoder::new(Cursor::new(first));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (2, 1));
    assert_eq!(
        &decoded[..output.buffer_size()],
        &[12, 34, 56, 255, 12, 34, 56, 255]
    );
}

#[test]
fn surface_replays_clear_fill_and_source_over_alpha() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::FillRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 1,
                    y: 1,
                    width: 2,
                    height: 1,
                },
                radius: NativeBorderRadius::default(),
                color: NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 128,
                },
                clip: None,
            },
        ],
        4,
        3,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.width(), 4);
    assert_eq!(surface.height(), 3);
    assert_eq!(surface.rgba().len(), 4 * 3 * 4);
    assert_eq!(surface.pixel(0, 0), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(1, 1), Some([127, 127, 255, 255]));
    assert_eq!(surface.pixel(3, 2), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(4, 0), None);
}

#[test]
fn surface_composites_nested_opacity_groups_inside_out() {
    let node = NativeDocument::empty().root();
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::BeginOpacityGroup {
                node_id: node,
                opacity: 128,
            },
            NativeDisplayCommand::FillRect {
                node_id: node,
                rect: NativeRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                radius: NativeBorderRadius::default(),
                color: NativeColor::RED,
                clip: None,
            },
            NativeDisplayCommand::BeginOpacityGroup {
                node_id: node,
                opacity: 128,
            },
            NativeDisplayCommand::FillRect {
                node_id: node,
                rect: NativeRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                radius: NativeBorderRadius::default(),
                color: NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
                clip: None,
            },
            NativeDisplayCommand::EndOpacityGroup { node_id: node },
            NativeDisplayCommand::EndOpacityGroup { node_id: node },
        ],
        2,
        1,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 0), Some([255, 127, 127, 255]));
    assert_eq!(surface.pixel(0, 0), Some([191, 127, 191, 255]));
}

#[test]
fn surface_rejects_unbalanced_and_over_budget_opacity_groups() {
    let node = NativeDocument::empty().root();
    let unclosed = display_list(
        vec![NativeDisplayCommand::BeginOpacityGroup {
            node_id: node,
            opacity: 128,
        }],
        2,
        2,
    );
    assert!(matches!(
        unclosed.rasterize(),
        Err(NativeEngineError::InvalidConfiguration { field, .. })
            if field == "native opacity group"
    ));

    let mut commands = vec![NativeDisplayCommand::Clear {
        color: NativeColor::WHITE,
    }];
    for _ in 0..4 {
        commands.push(NativeDisplayCommand::BeginOpacityGroup {
            node_id: node,
            opacity: 128,
        });
    }
    let over_budget = display_list(commands, 2_000, 2_000);
    assert!(matches!(
        over_budget.rasterize(),
        Err(NativeEngineError::LimitExceeded { resource, .. })
            if resource == "native opacity layer pixels"
    ));
}

#[test]
fn surface_draws_known_glyphs_and_clips_to_viewport() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 3, y: 2 },
                text: "A?".into(),
                truncated: false,
                color: NativeColor::RED,
                decoration_color: NativeColor::RED,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        8,
        8,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(3, 2), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(4, 2), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(7, 2), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(7, 7), Some([255, 0, 0, 255]));
}

#[test]
fn surface_preserves_per_glyph_color_over_text_paint() {
    let node_id = NativeDocument::empty().root();
    let run = NativeFontRun {
        glyphs: vec![NativeGlyph {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            advance: 1,
            color: Some(NativeColor::RED),
            composite: NativeGlyphComposite::SourceOver,
            gradient: None,
            bitmap: None,
            coverage: Arc::<[u8]>::from(vec![u8::MAX]),
        }],
        space_ranges: Vec::new(),
        width: 1,
        ascent: 1,
        line_height: 1,
    };
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::GlyphRun {
                node_id,
                origin: NativePoint { x: 1, y: 1 },
                text: "A".into(),
                truncated: false,
                run,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        3,
        3,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
}

#[test]
fn surface_composites_per_pixel_bitmap_glyph_alpha() {
    let node_id = NativeDocument::empty().root();
    let run = NativeFontRun {
        glyphs: vec![NativeGlyph {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            advance: 1,
            color: None,
            composite: NativeGlyphComposite::SourceOver,
            gradient: None,
            bitmap: Some(Arc::<[u8]>::from(vec![0, u8::MAX, 0, 128])),
            coverage: Arc::<[u8]>::from(vec![128]),
        }],
        space_ranges: Vec::new(),
        width: 1,
        ascent: 1,
        line_height: 1,
    };
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::GlyphRun {
                node_id,
                origin: NativePoint { x: 1, y: 1 },
                text: "A".into(),
                truncated: false,
                run,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        3,
        3,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 1), Some([127, 255, 127, 255]));
}

#[test]
fn surface_rejects_malformed_bitmap_payload_without_text_fallback() {
    let run = NativeFontRun {
        glyphs: vec![NativeGlyph {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            advance: 1,
            color: None,
            composite: NativeGlyphComposite::SourceOver,
            gradient: None,
            bitmap: Some(Arc::<[u8]>::from(vec![0, u8::MAX, 0])),
            coverage: Arc::<[u8]>::from(vec![u8::MAX]),
        }],
        space_ranges: Vec::new(),
        width: 1,
        ascent: 1,
        line_height: 1,
    };
    let mut surface = NativeSurface {
        width: 1,
        height: 1,
        rgba: vec![u8::MAX; 4],
    };
    surface.draw_glyph_run(
        GlyphRunPlacement {
            origin: NativePoint { x: 0, y: 0 },
            clip: None,
            scroll_offset: NativePoint { x: 0, y: 0 },
        },
        "A",
        &run,
        TextPaint {
            color: NativeColor::RED,
            decoration_color: NativeColor::RED,
            decoration_style: NativeTextDecorationStyle::Solid,
            decoration_skip_ink: NativeTextDecorationSkipInk::None,
            decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
            decoration_thickness: 1,
            underline_offset: 0,
            underline: false,
            overline: false,
            line_through: false,
            bold: false,
            italic: false,
            word_spacing: 0,
            letter_spacing: 0,
            justify_spacing: 0,
        },
        NativeTextLineBoundary::default(),
    );
    assert_eq!(surface.pixel(0, 0), Some([u8::MAX; 4]));
}

#[test]
fn destination_over_color_composite_preserves_opaque_destination() {
    let mut surface = NativeSurface {
        width: 1,
        height: 1,
        rgba: vec![0, 0, u8::MAX, u8::MAX],
    };
    surface.blend_coverage_pixel(
        0,
        0,
        NativeColor::RED,
        u8::MAX,
        NativeGlyphComposite::DestinationOver,
    );

    assert_eq!(surface.pixel(0, 0), Some([0, 0, u8::MAX, u8::MAX]));
}

#[test]
fn surface_bold_glyph_dilation_preserves_fixed_cell_advance() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 0 },
                text: "A".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 10 },
                text: "A".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: true,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        8,
        18,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 1), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(1, 11), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 11), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(6, 11), Some([255, 255, 255, 255]));
}

#[test]
fn surface_italic_glyph_shear_preserves_fixed_cell_advance() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 0 },
                text: "A".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 10 },
                text: "A".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: true,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 20 },
                text: "A".into(),
                truncated: false,
                color: NativeColor {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 128,
                },
                decoration_color: NativeColor {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 128,
                },
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: true,
                italic: true,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        10,
        28,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 10), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(3, 10), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 16), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(3, 20), Some([127, 127, 127, 255]));
    assert_eq!(surface.pixel(6, 20), Some([127, 127, 127, 255]));
    assert_eq!(surface.pixel(7, 20), Some([255, 255, 255, 255]));
}

#[test]
fn surface_draws_alpha_underlines_across_fixed_cells_and_clips_them() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 1, y: 0 },
                text: "A?".into(),
                truncated: false,
                color: NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 128,
                },
                decoration_color: NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 128,
                },
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: true,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: Some(NativeRect {
                    x: 2,
                    y: 7,
                    width: 4,
                    height: 1,
                }),
            },
        ],
        8,
        8,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 7), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(2, 7), Some([127, 191, 127, 255]));
    assert_eq!(surface.pixel(5, 7), Some([127, 191, 127, 255]));
    assert_eq!(surface.pixel(6, 7), Some([255, 255, 255, 255]));
}

#[test]
fn surface_draws_overline_and_line_through_with_scroll_translation() {
    let root = NativeDocument::empty().root();
    let mut list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::TextRun {
                node_id: root,
                origin: NativePoint { x: 1, y: 10 },
                text: "A?".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: true,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: Some(NativeRect {
                    x: 2,
                    y: 9,
                    width: 4,
                    height: 1,
                }),
            },
            NativeDisplayCommand::TextRun {
                node_id: root,
                origin: NativePoint { x: 1, y: 20 },
                text: "A?".into(),
                truncated: false,
                color: NativeColor::RED,
                decoration_color: NativeColor::RED,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: true,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
            NativeDisplayCommand::TextRun {
                node_id: root,
                origin: NativePoint { x: 1, y: 30 },
                text: "A?".into(),
                truncated: false,
                color: NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 128,
                },
                decoration_color: NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 128,
                },
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: true,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        16,
        36,
    );
    list.scroll_offset = NativePoint { x: 0, y: 5 };

    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 4), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(2, 4), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 4), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(6, 4), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(12, 18), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(13, 18), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(12, 32), Some([127, 191, 127, 255]));
}

#[test]
fn surface_enforces_command_clip_rectangles() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::FillRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 3,
                },
                radius: NativeBorderRadius::default(),
                color: NativeColor::RED,
                clip: Some(NativeRect {
                    x: 1,
                    y: 1,
                    width: 2,
                    height: 1,
                }),
            },
        ],
        4,
        3,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(0, 0), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(2, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(3, 1), Some([255, 255, 255, 255]));
}

#[test]
fn surface_replays_inside_border_ring_and_clip() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::BorderRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 1,
                    y: 1,
                    width: 4,
                    height: 3,
                },
                radius: NativeBorderRadius::default(),
                borders: uniform_border(1, NativeColor::RED),
                clip: Some(NativeRect {
                    x: 1,
                    y: 1,
                    width: 3,
                    height: 2,
                }),
            },
        ],
        6,
        5,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(3, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 2), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(2, 2), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(4, 1), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(2, 3), Some([255, 255, 255, 255]));
}

#[test]
fn surface_replays_side_specific_borders_with_deterministic_corners() {
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::BorderRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 1,
                    y: 1,
                    width: 6,
                    height: 5,
                },
                radius: NativeBorderRadius::default(),
                borders: NativeBorderPaint {
                    top: NativeBorderPaintSide {
                        width: 1,
                        style: NativeBorderStyle::Solid,
                        color: NativeColor::RED,
                    },
                    right: NativeBorderPaintSide {
                        width: 1,
                        style: NativeBorderStyle::Solid,
                        color: NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: 255,
                        },
                    },
                    bottom: NativeBorderPaintSide {
                        width: 1,
                        style: NativeBorderStyle::Solid,
                        color: NativeColor {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        },
                    },
                    left: NativeBorderPaintSide {
                        width: 1,
                        style: NativeBorderStyle::Solid,
                        color: NativeColor::BLACK,
                    },
                },
                clip: None,
            },
        ],
        8,
        8,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(3, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(6, 3), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(3, 5), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(1, 3), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(3, 3), Some([255, 255, 255, 255]));
}

#[test]
fn surface_replays_dashed_and_dotted_borders_with_document_phase() {
    let mut list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::BorderRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 1,
                    y: 0,
                    width: 8,
                    height: 6,
                },
                radius: NativeBorderRadius::default(),
                borders: NativeBorderPaint {
                    top: styled_border(1, NativeBorderStyle::Dashed, NativeColor::RED),
                    right: styled_border(
                        1,
                        NativeBorderStyle::Dotted,
                        NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: 255,
                        },
                    ),
                    bottom: styled_border(
                        1,
                        NativeBorderStyle::Solid,
                        NativeColor {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        },
                    ),
                    left: styled_border(0, NativeBorderStyle::Solid, NativeColor::BLACK),
                },
                clip: None,
            },
        ],
        10,
        6,
    );
    let unscrolled = list.rasterize().unwrap();

    assert_eq!(unscrolled.pixel(1, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(3, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(4, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(5, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(6, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(8, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(8, 1), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(8, 2), Some([0, 128, 0, 255]));

    list.scroll_offset = NativePoint { x: 0, y: 1 };
    let scrolled = list.rasterize().unwrap();
    assert_eq!(scrolled.pixel(8, 0), unscrolled.pixel(8, 1));
    assert_eq!(scrolled.pixel(8, 1), unscrolled.pixel(8, 2));
}

#[test]
fn surface_replays_rounded_fill_and_border_with_document_geometry() {
    let radius = NativeBorderRadius {
        top_left: 3,
        top_right: 3,
        bottom_right: 3,
        bottom_left: 3,
    };
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::FillRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 1,
                    y: 1,
                    width: 8,
                    height: 8,
                },
                radius,
                color: NativeColor::RED,
                clip: None,
            },
            NativeDisplayCommand::BorderRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 1,
                    y: 1,
                    width: 8,
                    height: 8,
                },
                radius,
                borders: uniform_border(1, NativeColor::BLACK),
                clip: None,
            },
        ],
        10,
        10,
    );
    let unscrolled = list.rasterize().unwrap();

    assert_eq!(unscrolled.pixel(1, 1), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(3, 1), Some([0, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(1, 4), Some([0, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(4, 4), Some([255, 0, 0, 255]));

    let mut scrolled_list = list;
    scrolled_list.scroll_offset = NativePoint { x: 0, y: 1 };
    let scrolled = scrolled_list.rasterize().unwrap();
    assert_eq!(scrolled.pixel(3, 0), unscrolled.pixel(3, 1));
    assert_eq!(scrolled.pixel(4, 3), unscrolled.pixel(4, 4));
}

#[test]
fn rounded_mask_normalizes_radii_to_the_concrete_box() {
    let rect = NativeRect {
        x: 0,
        y: 0,
        width: 10,
        height: 10,
    };
    let radius = NativeBorderRadius {
        top_left: 100,
        top_right: 100,
        bottom_right: 100,
        bottom_left: 100,
    };
    assert!(!rounded_rect_contains(
        rect,
        radius,
        NativePoint { x: 0, y: 0 }
    ));
    assert!(rounded_rect_contains(
        rect,
        radius,
        NativePoint { x: 5, y: 5 }
    ));
    assert!(rounded_rect_contains(
        rect,
        radius,
        NativePoint { x: 0, y: 5 }
    ));
}

#[test]
fn surface_translates_scrolled_geometry_without_pinning_offscreen_edges() {
    let mut list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            NativeDisplayCommand::BorderRect {
                node_id: NativeDocument::empty().root(),
                rect: NativeRect {
                    x: 0,
                    y: 0,
                    width: 16,
                    height: 20,
                },
                radius: NativeBorderRadius::default(),
                borders: uniform_border(2, NativeColor::RED),
                clip: None,
            },
            NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 0 },
                text: "A".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        16,
        16,
    );
    list.scroll_offset = NativePoint { x: 0, y: 10 };

    let surface = NativeSurface::from_display_list(&list).unwrap();
    assert_eq!(surface.pixel(8, 0), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(8, 8), Some([255, 0, 0, 255]));
}

#[test]
fn surface_clamps_untrusted_decoration_thickness() {
    let list = display_list(
        vec![NativeDisplayCommand::TextRun {
            node_id: NativeDocument::empty().root(),
            origin: NativePoint { x: 0, y: 0 },
            text: "A".into(),
            truncated: false,
            color: NativeColor::BLACK,
            decoration_color: NativeColor::BLACK,
            decoration_style: NativeTextDecorationStyle::Solid,
            decoration_skip_ink: NativeTextDecorationSkipInk::None,
            decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
            decoration_thickness: u32::MAX,
            underline_offset: 0,
            underline: true,
            overline: false,
            line_through: false,
            bold: false,
            italic: false,
            word_spacing: 0,
            letter_spacing: 0,
            justify_spacing: 0,
            clip: None,
        }],
        8,
        12,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(0, 7), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 10), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 11), Some([0, 0, 0, 0]));
}

#[test]
fn surface_clamps_untrusted_underline_offset() {
    let list = display_list(
        vec![NativeDisplayCommand::TextRun {
            node_id: NativeDocument::empty().root(),
            origin: NativePoint { x: 0, y: 0 },
            text: " ".into(),
            truncated: false,
            color: NativeColor::BLACK,
            decoration_color: NativeColor::BLACK,
            decoration_style: NativeTextDecorationStyle::Solid,
            decoration_skip_ink: NativeTextDecorationSkipInk::None,
            decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
            decoration_thickness: 1,
            underline_offset: i32::MAX,
            underline: true,
            overline: false,
            line_through: false,
            bold: false,
            italic: false,
            word_spacing: 0,
            letter_spacing: 0,
            justify_spacing: 0,
            clip: None,
        }],
        8,
        12,
    );
    let surface = list.rasterize().unwrap();

    assert_eq!(surface.pixel(0, 7), Some([0, 0, 0, 0]));
    assert_eq!(surface.pixel(0, 10), Some([0, 0, 0, 0]));
    assert_eq!(surface.pixel(0, 11), Some([0, 0, 0, 255]));
}

#[test]
fn surface_replays_double_decoration_as_two_thick_bands() {
    let node_id = NativeDocument::empty().root();
    let text_run =
        |origin: NativePoint, underline_offset: i32, line| NativeDisplayCommand::TextRun {
            node_id,
            origin,
            text: " ".into(),
            truncated: false,
            color: NativeColor::BLACK,
            decoration_color: NativeColor::BLACK,
            decoration_style: NativeTextDecorationStyle::Double,
            decoration_skip_ink: NativeTextDecorationSkipInk::None,
            decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
            decoration_thickness: 2,
            underline_offset,
            underline: line == "underline",
            overline: line == "overline",
            line_through: line == "line-through",
            bold: false,
            italic: false,
            word_spacing: 0,
            letter_spacing: 0,
            justify_spacing: 0,
            clip: None,
        };
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            text_run(NativePoint { x: 0, y: 10 }, 0, "overline"),
            text_run(NativePoint { x: 0, y: 30 }, 0, "line-through"),
            text_run(NativePoint { x: 0, y: 50 }, -2, "underline"),
        ],
        8,
        64,
    );
    let surface = list.rasterize().unwrap();

    for y in [9, 10, 12, 13, 33, 34, 36, 37, 55, 56, 58, 59] {
        assert_eq!(surface.pixel(0, y), Some([0, 0, 0, 255]), "painted row {y}");
    }
    for y in [11, 35, 57] {
        assert_eq!(
            surface.pixel(0, y),
            Some([255, 255, 255, 255]),
            "gap row {y}"
        );
    }
}

#[test]
fn surface_replays_wavy_decoration_with_fixed_phase_and_run_reset() {
    let node_id = NativeDocument::empty().root();
    let text_run = |origin: NativePoint, text: &str, thickness| NativeDisplayCommand::TextRun {
        node_id,
        origin,
        text: text.into(),
        truncated: false,
        color: NativeColor::BLACK,
        decoration_color: NativeColor::BLACK,
        decoration_style: NativeTextDecorationStyle::Wavy,
        decoration_skip_ink: NativeTextDecorationSkipInk::None,
        decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
        decoration_thickness: thickness,
        underline_offset: 0,
        underline: true,
        overline: false,
        line_through: false,
        bold: false,
        italic: false,
        word_spacing: 0,
        letter_spacing: 0,
        justify_spacing: 0,
        clip: None,
    };
    let list = display_list(
        vec![
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            text_run(NativePoint { x: 0, y: 10 }, "  ", 2),
            text_run(NativePoint { x: 3, y: 30 }, " ", 1),
        ],
        16,
        48,
    );
    let surface = list.rasterize().unwrap();

    for (x, top) in [
        (0, 17),
        (1, 18),
        (2, 19),
        (3, 18),
        (4, 17),
        (5, 16),
        (6, 15),
        (7, 16),
        (8, 17),
        (9, 18),
        (10, 19),
        (11, 18),
    ] {
        assert_eq!(surface.pixel(x, top), Some([0, 0, 0, 255]), "top x={x}");
        assert_eq!(
            surface.pixel(x, top + 1),
            Some([0, 0, 0, 255]),
            "thickness x={x}"
        );
        assert_eq!(surface.pixel(x, top - 1), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(x, top + 2), Some([255, 255, 255, 255]));
    }

    assert_eq!(surface.pixel(3, 37), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(3, 38), Some([255, 255, 255, 255]));
}

#[test]
fn surface_skip_ink_auto_uses_the_existing_bold_italic_glyph_mask() {
    let node_id = NativeDocument::empty().root();
    let text_run = |skip_ink| NativeDisplayCommand::TextRun {
        node_id,
        origin: NativePoint { x: 0, y: 10 },
        text: "A".into(),
        truncated: false,
        color: NativeColor::BLACK,
        decoration_color: NativeColor::RED,
        decoration_style: NativeTextDecorationStyle::Wavy,
        decoration_skip_ink: skip_ink,
        decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
        decoration_thickness: 1,
        underline_offset: -1,
        underline: true,
        overline: true,
        line_through: true,
        bold: true,
        italic: true,
        word_spacing: 0,
        letter_spacing: 0,
        justify_spacing: 0,
        clip: None,
    };
    let rasterize = |skip_ink| {
        display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                text_run(skip_ink),
            ],
            8,
            20,
        )
        .rasterize()
        .unwrap()
    };
    let auto = rasterize(NativeTextDecorationSkipInk::Auto);
    let none = rasterize(NativeTextDecorationSkipInk::None);

    assert_eq!(auto.pixel(4, 16), Some([0, 0, 0, 255]));
    assert_eq!(none.pixel(4, 16), Some([255, 0, 0, 255]));
    assert_eq!(auto.pixel(5, 15), Some([0, 0, 0, 255]));
    assert_eq!(none.pixel(5, 15), Some([255, 0, 0, 255]));
    assert_eq!(auto.pixel(2, 11), Some([0, 0, 0, 255]));
    assert_eq!(none.pixel(2, 11), Some([255, 0, 0, 255]));
    assert_eq!(auto.pixel(1, 17), Some([255, 0, 0, 255]));
    assert_eq!(none.pixel(1, 17), Some([255, 0, 0, 255]));
    assert_eq!(auto.pixel(1, 14), Some([255, 0, 0, 255]));
    assert_eq!(none.pixel(1, 14), Some([255, 0, 0, 255]));
}

#[test]
fn surface_skip_spaces_all_covers_space_and_authored_spacing_advances() {
    let node_id = NativeDocument::empty().root();
    let text_run = |skip_spaces| NativeDisplayCommand::TextRun {
        node_id,
        origin: NativePoint { x: 0, y: 10 },
        text: "A B".into(),
        truncated: false,
        color: NativeColor::BLACK,
        decoration_color: NativeColor::RED,
        decoration_style: NativeTextDecorationStyle::Solid,
        decoration_skip_ink: NativeTextDecorationSkipInk::None,
        decoration_skip_spaces: skip_spaces,
        decoration_thickness: 1,
        underline_offset: 0,
        underline: true,
        overline: true,
        line_through: true,
        bold: false,
        italic: false,
        word_spacing: 2,
        letter_spacing: 1,
        justify_spacing: 2,
        clip: None,
    };
    let rasterize = |skip_spaces| {
        display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                text_run(skip_spaces),
            ],
            32,
            24,
        )
        .rasterize()
        .unwrap()
    };
    let none = rasterize(NativeTextDecorationSkipSpaces::None);
    let all = rasterize(NativeTextDecorationSkipSpaces::All);

    for y in [9, 13, 17] {
        assert_eq!(none.pixel(6, y), Some([255, 0, 0, 255]), "none start y={y}");
        assert_eq!(none.pixel(17, y), Some([255, 0, 0, 255]), "none end y={y}");
        assert_eq!(
            all.pixel(6, y),
            Some([255, 255, 255, 255]),
            "all start y={y}"
        );
        assert_eq!(
            all.pixel(17, y),
            Some([255, 255, 255, 255]),
            "all end y={y}"
        );
        assert_eq!(all.pixel(0, y), Some([255, 0, 0, 255]), "all before y={y}");
        assert_eq!(all.pixel(18, y), Some([255, 0, 0, 255]), "all after y={y}");
    }
}

#[test]
fn surface_skip_spaces_all_covers_unicode_whitespace_intervals() {
    let node_id = NativeDocument::empty().root();
    let text_run = |skip_spaces| NativeDisplayCommand::TextRun {
        node_id,
        origin: NativePoint { x: 0, y: 10 },
        text: "A\tB\u{00a0}C".into(),
        truncated: false,
        color: NativeColor::BLACK,
        decoration_color: NativeColor::RED,
        decoration_style: NativeTextDecorationStyle::Solid,
        decoration_skip_ink: NativeTextDecorationSkipInk::None,
        decoration_skip_spaces: skip_spaces,
        decoration_thickness: 1,
        underline_offset: 0,
        underline: true,
        overline: true,
        line_through: true,
        bold: false,
        italic: false,
        word_spacing: 0,
        letter_spacing: 0,
        justify_spacing: 0,
        clip: None,
    };
    let rasterize = |skip_spaces| {
        display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                text_run(skip_spaces),
            ],
            48,
            24,
        )
        .rasterize()
        .unwrap()
    };
    let none = rasterize(NativeTextDecorationSkipSpaces::None);
    let all = rasterize(NativeTextDecorationSkipSpaces::All);

    for y in [9, 13, 17] {
        for x in [6, 18] {
            assert_eq!(
                none.pixel(x, y),
                Some([255, 0, 0, 255]),
                "none whitespace x={x} y={y}"
            );
            assert_eq!(
                all.pixel(x, y),
                Some([255, 255, 255, 255]),
                "all whitespace x={x} y={y}"
            );
        }
        for x in [0, 12, 24] {
            assert_eq!(
                all.pixel(x, y),
                Some([255, 0, 0, 255]),
                "all non-whitespace x={x} y={y}"
            );
        }
    }
}

#[test]
fn surface_rejects_unbounded_pixel_count_before_allocation() {
    let list = display_list(Vec::new(), 4096, 1025);
    assert!(matches!(
        list.rasterize(),
        Err(NativeEngineError::LimitExceeded { resource, .. })
            if resource == "native surface pixels"
    ));
}
