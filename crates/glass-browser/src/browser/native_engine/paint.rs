use super::config::{MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_NODES};
use super::css::NativeColor;
use super::dom::{NativeDocument, NativeNodeId};
use super::error::NativeEngineError;
use super::layout::{NativeLayoutSnapshot, NativePoint, NativeRect};

/// Maximum number of immutable commands retained in one native display list.
pub const MAX_NATIVE_DISPLAY_COMMANDS: usize = MAX_NATIVE_NODES.saturating_mul(2).saturating_add(1);

/// One bounded command consumed by the native software rasterizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeDisplayCommand {
    Clear {
        color: NativeColor,
    },
    FillRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    BorderRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        width: u32,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    TextRun {
        node_id: NativeNodeId,
        origin: NativePoint,
        text: String,
        truncated: bool,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
}

/// Immutable display-list projection for one native document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDisplayList {
    pub revision: u64,
    pub viewport: super::config::Viewport,
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
        let mut commands = Vec::with_capacity(layout.boxes.len().saturating_add(1));
        push_command(
            &mut commands,
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
        )?;
        for layout_box in &layout.boxes {
            let style = document.computed_style_for_layout(layout_box.node_id);
            let clip = paint_clip(document, layout, layout_box.node_id);
            if let Some(color) = style.background_color() {
                push_command(
                    &mut commands,
                    NativeDisplayCommand::FillRect {
                        node_id: layout_box.node_id,
                        rect: layout_box.rect,
                        color,
                        clip,
                    },
                )?;
            }
            if let Some(border) = style.border()
                && border.width() > 0
            {
                push_command(
                    &mut commands,
                    NativeDisplayCommand::BorderRect {
                        node_id: layout_box.node_id,
                        rect: layout_box.rect,
                        width: border.width(),
                        color: border.color(),
                        clip,
                    },
                )?;
            }
            let (text, truncated) = document.direct_text(layout_box.node_id);
            if !text.is_empty() {
                push_command(
                    &mut commands,
                    NativeDisplayCommand::TextRun {
                        node_id: layout_box.node_id,
                        origin: NativePoint {
                            x: layout_box.content_rect.x,
                            y: layout_box.content_rect.y,
                        },
                        text,
                        truncated,
                        color: style.color().unwrap_or(NativeColor::BLACK),
                        clip,
                    },
                )?;
            }
        }
        Ok(Self {
            revision: document.revision(),
            viewport: layout.viewport,
            commands,
        })
    }
}

fn paint_clip(
    document: &NativeDocument,
    layout: &NativeLayoutSnapshot,
    id: NativeNodeId,
) -> Option<NativeRect> {
    let mut current = Some(id);
    let mut clip = None;
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let Some(current_id) = current else {
            break;
        };
        let style = document.computed_style_for_layout(current_id);
        if style.overflow_hidden()
            && let Some(rect) = layout.box_for(current_id)
        {
            clip = Some(match clip {
                Some(existing) => intersect_rect(existing, rect),
                None => rect,
            });
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    clip
}

fn intersect_rect(first: NativeRect, second: NativeRect) -> NativeRect {
    let left = first.x.max(second.x);
    let top = first.y.max(second.y);
    let right = first.right().min(second.right());
    let bottom = first.bottom().min(second.bottom());
    NativeRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
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
