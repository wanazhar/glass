//! CSS color tokens and bounded SVG paint parsing.

use super::*;

pub(crate) fn parse_color(value: &str) -> Option<NativeColor> {
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "black" => Some(NativeColor::BLACK),
        "white" => Some(NativeColor::WHITE),
        "red" => Some(NativeColor::RED),
        "green" => Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        }),
        "blue" => Some(NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        }),
        "transparent" => Some(NativeColor {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        }),
        _ if value.starts_with('#') => parse_hex_color(&value[1..]),
        _ if value.starts_with("rgb(") && value.ends_with(')') => {
            let values = value[4..value.len() - 1]
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>();
            if values.len() != 3 {
                return None;
            }
            Some(NativeColor {
                red: values[0].parse().ok()?,
                green: values[1].parse().ok()?,
                blue: values[2].parse().ok()?,
                alpha: u8::MAX,
            })
        }
        _ if value.starts_with("rgba(") && value.ends_with(')') => {
            let values = value[5..value.len() - 1]
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>();
            if values.len() != 4 {
                return None;
            }
            Some(NativeColor {
                red: parse_color_channel(values[0])?,
                green: parse_color_channel(values[1])?,
                blue: parse_color_channel(values[2])?,
                alpha: parse_opacity(values[3])?,
            })
        }
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_svg_paint_declaration(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("none") || parse_color(value).is_some()
}

pub(super) fn parse_color_channel(value: &str) -> Option<u8> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

pub(super) fn parse_hex_color(value: &str) -> Option<NativeColor> {
    if !value.is_ascii() {
        return None;
    }
    let component = |value: &str| u8::from_str_radix(value, 16).ok();
    match value.len() {
        3 => Some(NativeColor {
            red: component(&value[0..1])?.saturating_mul(17),
            green: component(&value[1..2])?.saturating_mul(17),
            blue: component(&value[2..3])?.saturating_mul(17),
            alpha: u8::MAX,
        }),
        6 => Some(NativeColor {
            red: component(&value[0..2])?,
            green: component(&value[2..4])?,
            blue: component(&value[4..6])?,
            alpha: u8::MAX,
        }),
        8 => Some(NativeColor {
            red: component(&value[0..2])?,
            green: component(&value[2..4])?,
            blue: component(&value[4..6])?,
            alpha: component(&value[6..8])?,
        }),
        _ => None,
    }
}
