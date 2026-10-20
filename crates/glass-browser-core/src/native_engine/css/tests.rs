use super::*;
use crate::native_engine::config::NativeEngineLimits;

fn node(html: &str) -> NativeNode {
    let document = super::super::dom::NativeDocument::parse(html, &NativeEngineLimits::default())
        .expect("fixture document");
    let root = document.node(document.root()).expect("document root");
    let html_id = root
        .children()
        .iter()
        .copied()
        .find(|id| document.node(*id).and_then(NativeNode::element_name) == Some("html"))
        .expect("parsed document element");
    let html = document.node(html_id).expect("parsed html element");
    let body_id = html
        .children()
        .iter()
        .copied()
        .find(|id| document.node(*id).and_then(NativeNode::element_name) == Some("body"))
        .expect("parsed body element");
    let body = document.node(body_id).expect("parsed body element");
    let child = body
        .children()
        .iter()
        .copied()
        .find(|id| {
            document
                .node(*id)
                .is_some_and(|node| node.element_name().is_some())
        })
        .expect("fixture element in body");
    document.node(child).expect("fixture element").clone()
}

fn border_side(width: u32, color: NativeColor) -> NativeBorderSide {
    styled_border_side(width, NativeBorderStyle::Solid, color)
}

fn styled_border_side(
    width: u32,
    style: NativeBorderStyle,
    color: NativeColor,
) -> NativeBorderSide {
    NativeBorderSide {
        width,
        style,
        color,
    }
}

fn uniform_border(width: u32, color: NativeColor) -> NativeBorder {
    let side = border_side(width, color);
    NativeBorder {
        top: side,
        right: side,
        bottom: side,
        left: side,
    }
}

fn border_color_value(color: NativeColor) -> NativeBorderColorValue {
    NativeBorderColorValue::Color(color)
}

fn uniform_styled_border(width: u32, style: NativeBorderStyle, color: NativeColor) -> NativeBorder {
    let side = styled_border_side(width, style, color);
    NativeBorder {
        top: side,
        right: side,
        bottom: side,
        left: side,
    }
}

#[path = "tests/border_cascade.rs"]
mod border_cascade;
#[path = "tests/border_values.rs"]
mod border_values;
#[path = "tests/box_presentation.rs"]
mod box_presentation;
#[path = "tests/cascade_paint.rs"]
mod cascade_paint;
#[path = "tests/declaration_values.rs"]
mod declaration_values;
#[path = "tests/flex_cascade.rs"]
mod flex_cascade;
#[path = "tests/flex_values.rs"]
mod flex_values;
#[path = "tests/font_parsing.rs"]
mod font_parsing;
#[path = "tests/inherited_declarations.rs"]
mod inherited_declarations;
#[path = "tests/inherited_typography.rs"]
mod inherited_typography;
#[path = "tests/layout_properties.rs"]
mod layout_properties;
#[path = "tests/rules_and_selectors.rs"]
mod rules_and_selectors;
#[path = "tests/text_declarations.rs"]
mod text_declarations;
#[path = "tests/text_decoration.rs"]
mod text_decoration;
#[path = "tests/text_metrics.rs"]
mod text_metrics;
