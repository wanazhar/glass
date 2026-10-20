#[path = "property_parser/color.rs"]
mod color;
pub(crate) use color::parse_color;
pub(super) use color::parse_svg_paint_declaration;

use super::*;

#[path = "property_parser/border_values.rs"]
mod border_values;
#[path = "property_parser/dimensions_and_spacing.rs"]
mod dimensions_and_spacing;
#[path = "property_parser/font_face.rs"]
mod font_face;
#[path = "property_parser/font_features.rs"]
mod font_features;
#[path = "property_parser/layout_values.rs"]
mod layout_values;
#[path = "property_parser/text_and_flex_values.rs"]
mod text_and_flex_values;
#[path = "property_parser/text_styles.rs"]
mod text_styles;
pub(super) use border_values::*;
pub(super) use dimensions_and_spacing::*;
pub(crate) use font_face::{
    font_family_hash, format_font_face_unicode_ranges, format_font_metric_override,
    format_font_size_adjust, format_font_stretch_range, parse_font_face_unicode_range,
    parse_font_metric_override, parse_font_size_adjust, parse_font_stretch_range,
    parse_font_stretch_value, parse_font_weight_range,
};
pub(super) use font_face::{
    parse_font_display, parse_font_face_family, parse_font_face_src, parse_font_family,
    parse_font_family_name, parse_font_style,
};
pub(crate) use font_features::{
    format_font_feature_settings, format_font_variation_settings, parse_font_face_variant,
    parse_font_feature_settings, parse_font_variation_settings,
};
pub(super) use layout_values::*;
pub(super) use text_and_flex_values::*;
pub(super) use text_styles::*;

mod font_text;
pub(in crate::native_engine::css) use font_text::*;
