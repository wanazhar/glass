use super::config::Viewport;
use super::css::DisplayValue;
use super::dom::{NativeDocument, NativeNodeId, NativeNodeKind};
use super::error::NativeEngineError;

const DEFAULT_LINE_HEIGHT: u32 = 20;
const DEFAULT_CONTROL_HEIGHT: u32 = 24;
const DEFAULT_CONTROL_WIDTH: u32 = 160;
const DEFAULT_BUTTON_WIDTH: u32 = 80;
const CHARACTER_WIDTH: u32 = 8;

/// An integer-pixel point in the native viewport coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePoint {
    pub x: u32,
    pub y: u32,
}

/// A half-open integer-pixel rectangle in the native viewport coordinate
/// space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl NativeRect {
    pub const fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }

    pub const fn bottom(self) -> u32 {
        self.y.saturating_add(self.height)
    }

    pub const fn contains(self, point: NativePoint) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }
}

/// One derived layout box for a visible native element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeLayoutBox {
    pub node_id: NativeNodeId,
    pub rect: NativeRect,
    pub depth: usize,
}

/// Deterministic layout derived from one current native document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLayoutSnapshot {
    pub revision: u64,
    pub viewport: Viewport,
    pub boxes: Vec<NativeLayoutBox>,
}

impl NativeLayoutSnapshot {
    pub(crate) fn compute(
        document: &NativeDocument,
        viewport: Viewport,
    ) -> Result<Self, NativeEngineError> {
        viewport.validate()?;
        let mut builder = LayoutBuilder {
            document,
            boxes: Vec::new(),
        };
        builder.layout_children(document.root(), 0, 0, viewport.width, 0);
        Ok(Self {
            revision: document.revision(),
            viewport,
            boxes: builder.boxes,
        })
    }

    pub fn box_for(&self, node_id: NativeNodeId) -> Option<NativeRect> {
        self.boxes
            .iter()
            .find(|layout_box| layout_box.node_id == node_id)
            .map(|layout_box| layout_box.rect)
    }

    /// Return the deepest visible element at an integer point. A point outside
    /// the viewport is rejected instead of being adjusted or scrolled into
    /// view. Later document boxes win equal-depth overlaps deterministically.
    pub fn hit_test(&self, x: i64, y: i64) -> Result<Option<NativeNodeId>, NativeEngineError> {
        if x < 0 || y < 0 {
            return Err(NativeEngineError::invalid(
                "hit-test point",
                "coordinates must be finite and non-negative",
            ));
        }
        let point = NativePoint {
            x: u32::try_from(x).map_err(|_| {
                NativeEngineError::invalid("hit-test point", "coordinates exceed integer bounds")
            })?,
            y: u32::try_from(y).map_err(|_| {
                NativeEngineError::invalid("hit-test point", "coordinates exceed integer bounds")
            })?,
        };
        if point.x >= self.viewport.width || point.y >= self.viewport.height {
            return Err(NativeEngineError::invalid(
                "hit-test point",
                "coordinates are outside the native viewport",
            ));
        }

        let mut best: Option<(usize, usize, NativeNodeId)> = None;
        for (order, layout_box) in self.boxes.iter().enumerate() {
            if !layout_box.rect.contains(point) {
                continue;
            }
            let replaces = best.is_none_or(|(best_depth, best_order, _)| {
                (layout_box.depth, order) > (best_depth, best_order)
            });
            if replaces {
                best = Some((layout_box.depth, order, layout_box.node_id));
            }
        }
        Ok(best.map(|(_, _, node_id)| node_id))
    }
}

struct LayoutBuilder<'a> {
    document: &'a NativeDocument,
    boxes: Vec<NativeLayoutBox>,
}

#[derive(Debug, Clone, Copy, Default)]
struct FlowSize {
    width: u32,
    height: u32,
}

struct FlowCursor {
    start_x: u32,
    start_y: u32,
    available_width: u32,
    x: u32,
    y: u32,
    line_height: u32,
    line_has_content: bool,
    max_right: u32,
    max_bottom: u32,
}

impl FlowCursor {
    fn new(x: u32, y: u32, available_width: u32) -> Self {
        Self {
            start_x: x,
            start_y: y,
            available_width,
            x,
            y,
            line_height: 0,
            line_has_content: false,
            max_right: x,
            max_bottom: y,
        }
    }

    fn flush_line(&mut self) {
        if self.line_has_content {
            self.y = self
                .y
                .saturating_add(self.line_height.max(DEFAULT_LINE_HEIGHT));
            self.max_bottom = self.max_bottom.max(self.y);
            self.x = self.start_x;
            self.line_height = 0;
            self.line_has_content = false;
        }
    }

    fn place_inline(&mut self, mut width: u32, height: u32) {
        if self.available_width == 0 {
            return;
        }
        width = width.min(self.available_width);
        if self.line_has_content
            && self.x.saturating_sub(self.start_x).saturating_add(width) > self.available_width
        {
            self.flush_line();
        }
        self.x = self.x.saturating_add(width);
        self.line_height = self.line_height.max(height.max(DEFAULT_LINE_HEIGHT));
        self.line_has_content = true;
        self.max_right = self.max_right.max(self.x);
        self.max_bottom = self.max_bottom.max(self.y.saturating_add(self.line_height));
    }

    fn place_block(&mut self, height: u32) {
        self.flush_line();
        self.y = self.y.saturating_add(height);
        self.max_bottom = self.max_bottom.max(self.y);
    }

    fn finish(mut self) -> FlowSize {
        self.flush_line();
        FlowSize {
            width: self.max_right.saturating_sub(self.start_x),
            height: self.max_bottom.saturating_sub(self.start_y),
        }
    }
}

impl<'a> LayoutBuilder<'a> {
    fn layout_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let mut flow = FlowCursor::new(x, y, available_width);
        self.process_children(parent, &mut flow, depth);
        let bottom = flow.max_bottom;
        let start_y = y;
        let mut result = flow.finish();
        result.height = bottom.saturating_sub(start_y).max(result.height);
        result
    }

    fn process_children(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, depth: usize) {
        let children = self
            .document
            .node(parent)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();
        for child in children {
            let Some(node) = self.document.node(child) else {
                continue;
            };
            match node.kind() {
                NativeNodeKind::Text(value) => {
                    self.place_text(flow, value);
                }
                NativeNodeKind::Document => {
                    self.process_children(child, flow, depth);
                }
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(child) || self.document.is_hidden_for_layout(child) {
                        continue;
                    }
                    let display = self.effective_display(child);
                    match display {
                        DisplayValue::None => {}
                        DisplayValue::Contents => self.process_children(child, flow, depth + 1),
                        DisplayValue::Block => {
                            flow.flush_line();
                            let size = self.layout_element(
                                child,
                                flow.start_x,
                                flow.y,
                                flow.available_width,
                                depth,
                            );
                            flow.max_right =
                                flow.max_right.max(flow.start_x.saturating_add(size.width));
                            flow.place_block(size.height);
                        }
                        DisplayValue::Auto | DisplayValue::Inline | DisplayValue::Other => {
                            let size = self.layout_element(
                                child,
                                flow.x,
                                flow.y,
                                flow.available_width,
                                depth,
                            );
                            flow.place_inline(size.width, size.height);
                        }
                    }
                }
            }
        }
    }

    fn layout_element(
        &mut self,
        id: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let style = self.document.computed_style_for_layout(id);
        let display = self.effective_display(id);
        if display == DisplayValue::None
            || self.is_non_rendered(id)
            || self.document.is_hidden_for_layout(id)
        {
            return FlowSize::default();
        }
        if display == DisplayValue::Contents {
            return self.layout_children(id, x, y, available_width, depth);
        }

        let is_block = display == DisplayValue::Block;
        let default_width = if is_block {
            available_width
        } else {
            self.intrinsic_inline_width(id)
        };
        let width = style.width().unwrap_or(default_width).min(available_width);
        let default_height = if is_block {
            DEFAULT_LINE_HEIGHT
        } else {
            self.intrinsic_inline_height(id)
        };
        let box_index = self.boxes.len();
        self.boxes.push(NativeLayoutBox {
            node_id: id,
            rect: NativeRect {
                x,
                y,
                width,
                height: default_height,
            },
            depth,
        });

        let children = self.layout_children(id, x, y, width, depth + 1);
        let height = style
            .height()
            .unwrap_or(default_height.max(children.height));
        self.boxes[box_index].rect.height = height;
        FlowSize { width, height }
    }

    fn place_text(&self, flow: &mut FlowCursor, value: &str) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let characters = u32::try_from(trimmed.chars().count()).unwrap_or(u32::MAX);
        let width = characters
            .saturating_mul(CHARACTER_WIDTH)
            .max(CHARACTER_WIDTH);
        if flow.available_width == 0 {
            return;
        }
        let lines = width
            .saturating_add(flow.available_width.saturating_sub(1))
            .checked_div(flow.available_width)
            .unwrap_or(1)
            .max(1);
        if lines > 1 && flow.line_has_content {
            flow.flush_line();
        }
        for line in 0..lines {
            if line > 0 {
                flow.flush_line();
            }
            let line_width = if line + 1 == lines {
                width.saturating_sub(flow.available_width.saturating_mul(line))
            } else {
                flow.available_width
            };
            flow.place_inline(line_width, DEFAULT_LINE_HEIGHT);
        }
    }

    fn intrinsic_inline_width(&self, id: NativeNodeId) -> u32 {
        let Some(node) = self.document.node(id) else {
            return 0;
        };
        match node.element_name() {
            Some("input" | "textarea" | "select") => DEFAULT_CONTROL_WIDTH,
            Some("button") => self
                .document
                .layout_text_width(id)
                .saturating_add(24)
                .max(DEFAULT_BUTTON_WIDTH),
            Some("option") => self
                .document
                .layout_text_width(id)
                .saturating_add(16)
                .max(DEFAULT_BUTTON_WIDTH),
            Some(_) => self
                .document
                .layout_text_width(id)
                .saturating_mul(CHARACTER_WIDTH)
                .max(CHARACTER_WIDTH),
            None => 0,
        }
    }

    fn intrinsic_inline_height(&self, id: NativeNodeId) -> u32 {
        match self.document.node(id).and_then(|node| node.element_name()) {
            Some("input" | "textarea" | "select" | "button") => DEFAULT_CONTROL_HEIGHT,
            _ => DEFAULT_LINE_HEIGHT,
        }
    }

    fn effective_display(&self, id: NativeNodeId) -> DisplayValue {
        let style = self.document.computed_style_for_layout(id);
        match style.display() {
            DisplayValue::Auto | DisplayValue::Other => self.default_display(id),
            display => display,
        }
    }

    fn is_non_rendered(&self, id: NativeNodeId) -> bool {
        self.document
            .node(id)
            .and_then(|node| node.element_name())
            .is_some_and(|name| matches!(name, "head" | "script" | "style" | "template" | "title"))
    }

    fn default_display(&self, id: NativeNodeId) -> DisplayValue {
        let Some(name) = self.document.node(id).and_then(|node| node.element_name()) else {
            return DisplayValue::Inline;
        };
        if matches!(
            name,
            "html"
                | "body"
                | "main"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "nav"
                | "aside"
                | "div"
                | "p"
                | "form"
                | "ul"
                | "ol"
                | "li"
                | "dl"
                | "dt"
                | "dd"
                | "fieldset"
                | "legend"
                | "blockquote"
                | "pre"
                | "table"
                | "thead"
                | "tbody"
                | "tfoot"
                | "tr"
                | "td"
                | "th"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "hr"
        ) {
            DisplayValue::Block
        } else {
            DisplayValue::Inline
        }
    }
}
