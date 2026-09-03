use super::config::MAX_NATIVE_NODES;
use super::css::{
    FontStyleValue, FontWeightValue, NativeBorderRadius, NativeBorderStyle, NativeColor,
    TextDecorationValue,
};
use super::dom::{NativeDocument, NativeNodeId};
use super::error::NativeEngineError;
use super::layout::{NativeLayoutPaintOrder, NativeLayoutSnapshot, NativePoint, NativeRect};

/// Maximum number of immutable commands retained in one native display list.
pub const MAX_NATIVE_DISPLAY_COMMANDS: usize = MAX_NATIVE_NODES.saturating_mul(4).saturating_add(1);

/// One physical border side carried by a native display-list border command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeBorderPaintSide {
    pub width: u32,
    pub style: NativeBorderStyle,
    pub color: NativeColor,
}

/// Bounded physical border paint data for the top, right, bottom, and left
/// sides of one outer layout box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeBorderPaint {
    pub top: NativeBorderPaintSide,
    pub right: NativeBorderPaintSide,
    pub bottom: NativeBorderPaintSide,
    pub left: NativeBorderPaintSide,
}

impl NativeBorderPaint {
    pub(crate) fn from_style(border: super::css::NativeBorder) -> Self {
        Self {
            top: Self::side_from_style(border.top()),
            right: Self::side_from_style(border.right()),
            bottom: Self::side_from_style(border.bottom()),
            left: Self::side_from_style(border.left()),
        }
    }

    const fn side_from_style(side: super::css::NativeBorderSide) -> NativeBorderPaintSide {
        NativeBorderPaintSide {
            width: side.width(),
            style: side.style(),
            color: side.color(),
        }
    }
}

/// One bounded command consumed by the native software rasterizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeDisplayCommand {
    BeginOpacityGroup {
        node_id: NativeNodeId,
        opacity: u8,
    },
    Clear {
        color: NativeColor,
    },
    FillRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        radius: NativeBorderRadius,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    BorderRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        radius: NativeBorderRadius,
        borders: NativeBorderPaint,
        clip: Option<NativeRect>,
    },
    TextRun {
        node_id: NativeNodeId,
        origin: NativePoint,
        text: String,
        truncated: bool,
        color: NativeColor,
        underline: bool,
        bold: bool,
        italic: bool,
        word_spacing: u32,
        letter_spacing: u32,
        justify_spacing: u32,
        clip: Option<NativeRect>,
    },
    EndOpacityGroup {
        node_id: NativeNodeId,
    },
}

/// Immutable display-list projection for one native document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDisplayList {
    pub revision: u64,
    pub viewport: super::config::Viewport,
    /// The root viewport offset used when replaying document-space commands.
    pub scroll_offset: NativePoint,
    pub commands: Vec<NativeDisplayCommand>,
}

impl NativeDisplayList {
    pub(crate) fn build(
        document: &NativeDocument,
        layout: &NativeLayoutSnapshot,
    ) -> Result<Self, NativeEngineError> {
        if layout.revision != document.revision() {
            return Err(NativeEngineError::invalid(
                "layout snapshot",
                "layout revision does not match the current native document",
            ));
        }
        let mut commands = Vec::with_capacity(
            layout
                .boxes
                .len()
                .saturating_add(layout.text_runs.len())
                .saturating_add(1),
        );
        push_command(
            &mut commands,
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
        )?;
        for entry in &layout.paint_order {
            match *entry {
                NativeLayoutPaintOrder::BeginOpacityGroup { node_id, opacity } => {
                    push_command(
                        &mut commands,
                        NativeDisplayCommand::BeginOpacityGroup { node_id, opacity },
                    )?;
                }
                NativeLayoutPaintOrder::Box(box_index) => {
                    let layout_box = layout.boxes.get(box_index).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "layout paint order",
                            "box entry is missing from the layout snapshot",
                        )
                    })?;
                    let style = document.computed_style_for_layout(layout_box.node_id);
                    let clip = paint_clip(document, layout, layout_box.node_id);
                    if let Some(color) = style.background_color() {
                        push_command(
                            &mut commands,
                            NativeDisplayCommand::FillRect {
                                node_id: layout_box.node_id,
                                rect: layout_box.rect,
                                radius: style.border_radius(),
                                color,
                                clip,
                            },
                        )?;
                    }
                    if let Some(border) = style.border()
                        && border.any_width()
                    {
                        let borders = NativeBorderPaint::from_style(border);
                        push_command(
                            &mut commands,
                            NativeDisplayCommand::BorderRect {
                                node_id: layout_box.node_id,
                                rect: layout_box.rect,
                                radius: style.border_radius(),
                                borders,
                                clip,
                            },
                        )?;
                    }
                }
                NativeLayoutPaintOrder::Text(text_index) => {
                    let text_run = layout.text_runs.get(text_index).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "layout paint order",
                            "text entry is missing from the layout snapshot",
                        )
                    })?;
                    if text_run.text.is_empty() {
                        continue;
                    }
                    let style = document.computed_style_for_layout(text_run.node_id);
                    let clip = paint_clip(document, layout, text_run.node_id);
                    push_command(
                        &mut commands,
                        NativeDisplayCommand::TextRun {
                            node_id: text_run.node_id,
                            origin: text_run.origin,
                            text: text_run.text.clone(),
                            truncated: text_run.truncated,
                            color: style.color().unwrap_or(NativeColor::BLACK),
                            underline: style.text_decoration() == TextDecorationValue::Underline,
                            bold: style.font_weight() == FontWeightValue::Bold,
                            italic: style.font_style() == FontStyleValue::Italic,
                            word_spacing: style.word_spacing(),
                            letter_spacing: style.letter_spacing(),
                            justify_spacing: text_run.justify_spacing,
                            clip,
                        },
                    )?;
                }
                NativeLayoutPaintOrder::EndOpacityGroup { node_id } => {
                    push_command(
                        &mut commands,
                        NativeDisplayCommand::EndOpacityGroup { node_id },
                    )?;
                }
            }
        }
        Ok(Self {
            revision: document.revision(),
            viewport: layout.viewport,
            scroll_offset: layout.scroll_offset,
            commands,
        })
    }
}

fn paint_clip(
    document: &NativeDocument,
    layout: &NativeLayoutSnapshot,
    id: NativeNodeId,
) -> Option<NativeRect> {
    layout.overflow_clip_for(document, id)
}

fn push_command(
    commands: &mut Vec<NativeDisplayCommand>,
    command: NativeDisplayCommand,
) -> Result<(), NativeEngineError> {
    if commands.len() >= MAX_NATIVE_DISPLAY_COMMANDS {
        return Err(NativeEngineError::limit(
            "native display commands",
            MAX_NATIVE_DISPLAY_COMMANDS,
            commands.len().saturating_add(1),
        ));
    }
    commands.push(command);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::native_engine::{NativeEngineLimits, Viewport};

    #[test]
    fn display_list_rejects_stale_layout_revision() {
        let mut document =
            NativeDocument::parse("<main><p>Paint</p></main>", &NativeEngineLimits::default())
                .unwrap();
        let layout = document
            .layout(Viewport {
                width: 320,
                height: 200,
                device_scale_factor_milli: 1000,
            })
            .unwrap();
        document.set_revision(2);

        assert!(matches!(
            NativeDisplayList::build(&document, &layout),
            Err(NativeEngineError::InvalidConfiguration { field, .. })
                if field == "layout snapshot"
        ));
    }
}
