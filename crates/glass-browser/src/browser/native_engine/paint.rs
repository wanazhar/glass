use super::config::MAX_NATIVE_NODES;
use super::css::NativeColor;
use super::dom::{NativeDocument, NativeNodeId};
use super::error::NativeEngineError;
use super::layout::{NativeLayoutSnapshot, NativePoint, NativeRect};

/// Maximum number of immutable commands retained in one native display list.
pub const MAX_NATIVE_DISPLAY_COMMANDS: usize = MAX_NATIVE_NODES.saturating_mul(2).saturating_add(1);

/// One bounded command consumed by a future software rasterizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeDisplayCommand {
    Clear {
        color: NativeColor,
    },
    FillRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        color: NativeColor,
    },
    TextRun {
        node_id: NativeNodeId,
        origin: NativePoint,
        text: String,
        truncated: bool,
        color: NativeColor,
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
            if let Some(color) = style.background_color() {
                push_command(
                    &mut commands,
                    NativeDisplayCommand::FillRect {
                        node_id: layout_box.node_id,
                        rect: layout_box.rect,
                        color,
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
                            x: layout_box.rect.x,
                            y: layout_box.rect.y,
                        },
                        text,
                        truncated,
                        color: style.color().unwrap_or(NativeColor::BLACK),
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
