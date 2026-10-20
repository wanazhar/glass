//! Bounded SVG geometry parsing and transform helpers.

use super::*;

pub(crate) fn svg_points(node: &NativeNode) -> Option<Vec<NativePoint>> {
    let value = node.attribute("points")?;
    let coordinates = value
        .split(|character: char| character == ',' || character.is_ascii_whitespace())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if coordinates.len() < 4
        || !coordinates.len().is_multiple_of(2)
        || coordinates.len() / 2 > MAX_NATIVE_SVG_POINTS
    {
        return None;
    }
    let mut points = Vec::with_capacity(coordinates.len() / 2);
    for pair in coordinates.chunks_exact(2) {
        points.push(NativePoint {
            x: pair[0].parse().ok()?,
            y: pair[1].parse().ok()?,
        });
    }
    Some(points)
}

pub(crate) fn svg_line_points(node: &NativeNode) -> Vec<NativePoint> {
    let number = |name: &str| {
        node.attribute(name)
            .and_then(|value| value.trim().parse::<u32>().ok())
            .unwrap_or(0)
    };
    vec![
        NativePoint {
            x: number("x1"),
            y: number("y1"),
        },
        NativePoint {
            x: number("x2"),
            y: number("y2"),
        },
    ]
}

pub(crate) fn svg_transform_for_node(
    document: &NativeDocument,
    node_id: NativeNodeId,
) -> Option<NativeSvgTransform> {
    let mut ancestors = Vec::new();
    let mut current = Some(node_id);
    while let Some(id) = current {
        let node = document.node(id)?;
        ancestors.push(id);
        current = node.parent();
    }
    let mut transform = NativeSvgTransform::IDENTITY;
    let mut in_svg = false;
    for id in ancestors.iter().rev() {
        let node = document.node(*id)?;
        if node.element_name() == Some("svg") {
            in_svg = true;
        }
        if in_svg {
            let mut local = NativeSvgTransform::IDENTITY;
            if node.element_name() == Some("svg")
                && let Some(value) = node.attribute("viewBox")
            {
                local = local.followed_by(svg_viewbox_transform(node, value)?);
            }
            if let Some(value) = node.attribute("transform") {
                local = local.followed_by(parse_svg_transform(value)?);
            }
            transform = local.followed_by(transform);
        }
    }
    Some(transform)
}

fn svg_viewbox_transform(node: &NativeNode, value: &str) -> Option<NativeSvgTransform> {
    let values = tokenize_svg_path(value)?
        .into_iter()
        .map(|token| match token {
            SvgPathToken::Number(value) => Some(value),
            SvgPathToken::Command(_) => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 4
        || !values.iter().all(|value| value.is_finite())
        || values[2] <= 0.0
        || values[3] <= 0.0
    {
        return None;
    }
    let viewport_width = node
        .attribute("width")
        .and_then(svg_length)
        .map(f64::from)
        .unwrap_or(f64::from(DEFAULT_SVG_WIDTH));
    let viewport_height = node
        .attribute("height")
        .and_then(svg_length)
        .map(f64::from)
        .unwrap_or(f64::from(DEFAULT_SVG_HEIGHT));
    if !viewport_width.is_finite()
        || viewport_width <= 0.0
        || !viewport_height.is_finite()
        || viewport_height <= 0.0
    {
        return None;
    }
    let (align_x, align_y, slice) = svg_preserve_aspect_ratio(node)?;
    let viewbox_width = values[2];
    let viewbox_height = values[3];
    let scale_x = viewport_width / viewbox_width;
    let scale_y = viewport_height / viewbox_height;
    let (scale_x, scale_y) = if align_x.is_none() {
        (scale_x, scale_y)
    } else {
        let scale = if slice {
            scale_x.max(scale_y)
        } else {
            scale_x.min(scale_y)
        };
        (scale, scale)
    };
    let rendered_width = viewbox_width * scale_x;
    let rendered_height = viewbox_height * scale_y;
    let offset_x = align_x.unwrap_or(0.0) * (viewport_width - rendered_width) - values[0] * scale_x;
    let offset_y =
        align_y.unwrap_or(0.0) * (viewport_height - rendered_height) - values[1] * scale_y;
    let transform = NativeSvgTransform {
        a: scale_x,
        d: scale_y,
        e: offset_x,
        f: offset_y,
        ..NativeSvgTransform::IDENTITY
    };
    [
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.e,
        transform.f,
    ]
    .iter()
    .all(|value| value.is_finite())
    .then_some(transform)
}

pub(super) fn svg_length(value: &str) -> Option<u32> {
    let value = value.trim();
    let value = value.strip_suffix("px").map(str::trim).unwrap_or(value);
    if value.is_empty() || value.ends_with('%') {
        return None;
    }
    let value = value.parse::<f64>().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    u32::try_from(value.ceil() as u64)
        .ok()
        .filter(|value| *value > 0)
}

pub(crate) fn svg_preserve_aspect_ratio(
    node: &NativeNode,
) -> Option<(Option<f64>, Option<f64>, bool)> {
    let value = node
        .attribute("preserveAspectRatio")
        .unwrap_or("xMidYMid meet");
    let mut tokens = value.split_ascii_whitespace();
    let mut token = tokens.next()?.to_ascii_lowercase();
    if token == "defer" {
        token = tokens.next()?.to_ascii_lowercase();
    }
    if token == "none" {
        return tokens.next().is_none().then_some((None, None, false));
    }
    let (align_x, align_y) = match token.as_str() {
        "xminymin" => (0.0, 0.0),
        "xminymid" => (0.0, 0.5),
        "xminymax" => (0.0, 1.0),
        "xmidymin" => (0.5, 0.0),
        "xmidymid" => (0.5, 0.5),
        "xmidymax" => (0.5, 1.0),
        "xmaxymin" => (1.0, 0.0),
        "xmaxymid" => (1.0, 0.5),
        "xmaxymax" => (1.0, 1.0),
        _ => return None,
    };
    let slice = match tokens
        .next()
        .unwrap_or("meet")
        .to_ascii_lowercase()
        .as_str()
    {
        "meet" => false,
        "slice" => true,
        _ => return None,
    };
    tokens
        .next()
        .is_none()
        .then_some((Some(align_x), Some(align_y), slice))
}

pub(crate) fn svg_image_viewport(node: &NativeNode) -> Option<(f64, f64, f64, f64)> {
    let length = |name: &str, default: Option<f64>| {
        let Some(value) = node.attribute(name) else {
            return default;
        };
        let value = value
            .trim()
            .strip_suffix("px")
            .map(str::trim)
            .unwrap_or(value.trim());
        if value.is_empty() || value.ends_with('%') {
            return None;
        }
        let value = value.parse::<f64>().ok()?;
        (value.is_finite() && value >= 0.0 && value <= f64::from(u32::MAX)).then_some(value)
    };
    let x = length("x", Some(0.0))?;
    let y = length("y", Some(0.0))?;
    let width = length("width", None)?;
    let height = length("height", None)?;
    (width > 0.0
        && height > 0.0
        && width <= f64::from(MAX_NATIVE_CANVAS_DIMENSION)
        && height <= f64::from(MAX_NATIVE_CANVAS_DIMENSION))
    .then_some((x, y, width, height))
}

pub(crate) fn svg_transformed_points(
    node: &NativeNode,
    transform: NativeSvgTransform,
) -> Option<Vec<NativePoint>> {
    let number = |name: &str| {
        node.attribute(name)
            .map(|value| value.trim().parse::<f64>().ok())
            .unwrap_or(Some(0.0))
            .filter(|value| value.is_finite())
    };
    let points = match node.element_name()? {
        "rect" => {
            let x = number("x")?;
            let y = number("y")?;
            let width = number("width")?;
            let height = number("height")?;
            if width < 0.0 || height < 0.0 {
                return None;
            }
            vec![
                (x, y),
                (x + width, y),
                (x + width, y + height),
                (x, y + height),
            ]
        }
        "circle" => {
            let center_x = number("cx")?;
            let center_y = number("cy")?;
            let radius = number("r")?;
            if radius < 0.0 {
                return None;
            }
            sampled_ellipse_points(center_x, center_y, radius, radius)
        }
        "ellipse" => {
            let center_x = number("cx")?;
            let center_y = number("cy")?;
            let radius_x = number("rx")?;
            let radius_y = number("ry")?;
            if radius_x < 0.0 || radius_y < 0.0 {
                return None;
            }
            sampled_ellipse_points(center_x, center_y, radius_x, radius_y)
        }
        "line" => svg_line_points(node)
            .into_iter()
            .map(|point| (f64::from(point.x), f64::from(point.y)))
            .collect(),
        "polyline" | "polygon" => svg_points(node)?
            .into_iter()
            .map(|point| (f64::from(point.x), f64::from(point.y)))
            .collect(),
        "image" => {
            let (x, y, width, height) = svg_image_viewport(node)?;
            vec![
                (x, y),
                (x + width, y),
                (x + width, y + height),
                (x, y + height),
            ]
        }
        _ => return None,
    };
    svg_transform_points(&points, transform)
}

pub(crate) fn svg_transformed_subpaths(
    node: &NativeNode,
    transform: NativeSvgTransform,
) -> Option<Vec<NativeSvgSubpath>> {
    svg_path_subpaths(node)?
        .into_iter()
        .map(|subpath| {
            Some(NativeSvgSubpath {
                points: svg_transform_points(
                    &subpath
                        .points
                        .iter()
                        .map(|point| (f64::from(point.x), f64::from(point.y)))
                        .collect::<Vec<_>>(),
                    transform,
                )?,
                closed: subpath.closed,
            })
        })
        .collect()
}

const MAX_NATIVE_SVG_ELLIPSE_POINTS: usize = 32;

fn sampled_ellipse_points(
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
) -> Vec<(f64, f64)> {
    (0..MAX_NATIVE_SVG_ELLIPSE_POINTS)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / MAX_NATIVE_SVG_ELLIPSE_POINTS as f64;
            (
                center_x + radius_x * angle.cos(),
                center_y + radius_y * angle.sin(),
            )
        })
        .collect()
}

pub(crate) fn svg_transform_points(
    points: &[(f64, f64)],
    transform: NativeSvgTransform,
) -> Option<Vec<NativePoint>> {
    if points.len() > MAX_NATIVE_SVG_POINTS {
        return None;
    }
    points
        .iter()
        .copied()
        .map(|point| transform.apply(point).map(svg_path_point))
        .collect()
}

fn parse_svg_transform(value: &str) -> Option<NativeSvgTransform> {
    let characters = value.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut transform = NativeSvgTransform::IDENTITY;
    while index < characters.len() {
        while index < characters.len()
            && (characters[index].is_ascii_whitespace() || characters[index] == ',')
        {
            index = index.saturating_add(1);
        }
        if index == characters.len() {
            break;
        }
        let name_start = index;
        while index < characters.len() && characters[index].is_ascii_alphabetic() {
            index = index.saturating_add(1);
        }
        if name_start == index {
            return None;
        }
        let name = characters[name_start..index]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        while index < characters.len() && characters[index].is_ascii_whitespace() {
            index = index.saturating_add(1);
        }
        if characters.get(index) != Some(&'(') {
            return None;
        }
        index = index.saturating_add(1);
        let argument_start = index;
        while index < characters.len() && characters[index] != ')' {
            if characters[index] == '(' {
                return None;
            }
            index = index.saturating_add(1);
        }
        if index == characters.len() {
            return None;
        }
        let argument_text = characters[argument_start..index].iter().collect::<String>();
        let arguments = tokenize_svg_path(&argument_text)?
            .into_iter()
            .map(|token| match token {
                SvgPathToken::Number(value) => Some(value),
                SvgPathToken::Command(_) => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let local = match name.as_str() {
            "matrix" if arguments.len() == 6 => NativeSvgTransform {
                a: arguments[0],
                b: arguments[1],
                c: arguments[2],
                d: arguments[3],
                e: arguments[4],
                f: arguments[5],
            },
            "translate" if matches!(arguments.len(), 1 | 2) => NativeSvgTransform::translation(
                arguments[0],
                arguments.get(1).copied().unwrap_or(0.0),
            ),
            "scale" if matches!(arguments.len(), 1 | 2) => NativeSvgTransform::scale(
                arguments[0],
                arguments.get(1).copied().unwrap_or(arguments[0]),
            ),
            "rotate" if matches!(arguments.len(), 1 | 3) => {
                let rotation = NativeSvgTransform::rotation(arguments[0].to_radians());
                if arguments.len() == 1 {
                    rotation
                } else {
                    NativeSvgTransform::translation(-arguments[1], -arguments[2])
                        .followed_by(rotation)
                        .followed_by(NativeSvgTransform::translation(arguments[1], arguments[2]))
                }
            }
            "skewx" if arguments.len() == 1 => {
                let tangent = arguments[0].to_radians().tan();
                NativeSvgTransform {
                    c: tangent,
                    ..NativeSvgTransform::IDENTITY
                }
            }
            "skewy" if arguments.len() == 1 => {
                let tangent = arguments[0].to_radians().tan();
                NativeSvgTransform {
                    b: tangent,
                    ..NativeSvgTransform::IDENTITY
                }
            }
            _ => return None,
        };
        if ![local.a, local.b, local.c, local.d, local.e, local.f]
            .iter()
            .all(|number| number.is_finite())
        {
            return None;
        }
        transform = transform.followed_by(local);
        index = index.saturating_add(1);
    }
    Some(transform)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SvgPathToken {
    Command(char),
    Number(f64),
}

pub(crate) fn svg_path_subpaths(node: &NativeNode) -> Option<Vec<NativeSvgSubpath>> {
    let value = node.attribute("d")?;
    let tokens = tokenize_svg_path(value)?;
    let mut subpaths: Vec<NativeSvgSubpath> = Vec::new();
    let mut current = (0.0, 0.0);
    let mut command = None;
    let mut previous_command = None;
    let mut previous_cubic_control = None;
    let mut previous_quadratic_control = None;
    let mut index = 0usize;
    while index < tokens.len() {
        if let SvgPathToken::Command(next) = tokens[index] {
            if !matches!(
                next,
                'M' | 'm'
                    | 'L'
                    | 'l'
                    | 'H'
                    | 'h'
                    | 'V'
                    | 'v'
                    | 'C'
                    | 'c'
                    | 'Q'
                    | 'q'
                    | 'S'
                    | 's'
                    | 'T'
                    | 't'
                    | 'A'
                    | 'a'
                    | 'Z'
                    | 'z'
            ) {
                return None;
            }
            index = index.saturating_add(1);
            if matches!(next, 'Z' | 'z') {
                let subpath = subpaths.last_mut()?;
                subpath.closed = true;
                current = subpath
                    .points
                    .first()
                    .map(|point| (f64::from(point.x), f64::from(point.y)))
                    .unwrap_or(current);
                command = None;
                previous_command = Some(next);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            } else {
                command = Some(next);
            }
            continue;
        }

        let next = command?;
        let relative = next.is_ascii_lowercase();
        let upper = next.to_ascii_uppercase();
        match upper {
            'M' | 'L' => {
                let (x, y) = svg_path_pair(&tokens, &mut index)?;
                let point = if relative {
                    (current.0 + x, current.1 + y)
                } else {
                    (x, y)
                };
                if upper == 'M' {
                    subpaths.push(NativeSvgSubpath {
                        points: vec![svg_path_point(point)],
                        closed: false,
                    });
                    if subpaths.len() > MAX_NATIVE_SVG_POINTS {
                        return None;
                    }
                    command = Some(if relative { 'l' } else { 'L' });
                } else {
                    let subpath = subpaths.last_mut()?;
                    subpath.points.push(svg_path_point(point));
                }
                current = point;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            'H' => {
                let x = svg_path_number(&tokens, &mut index)?;
                let point = (if relative { current.0 + x } else { x }, current.1);
                let subpath = subpaths.last_mut()?;
                subpath.points.push(svg_path_point(point));
                current = point;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            'V' => {
                let y = svg_path_number(&tokens, &mut index)?;
                let point = (current.0, if relative { current.1 + y } else { y });
                let subpath = subpaths.last_mut()?;
                subpath.points.push(svg_path_point(point));
                current = point;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            'C' => {
                let control_1 = svg_path_pair(&tokens, &mut index)?;
                let control_2 = svg_path_pair(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                let control_1 = if relative {
                    (current.0 + control_1.0, current.1 + control_1.1)
                } else {
                    control_1
                };
                let control_2 = if relative {
                    (current.0 + control_2.0, current.1 + control_2.1)
                } else {
                    control_2
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let subpath = subpaths.last_mut()?;
                append_cubic_curve(&mut subpath.points, current, control_1, control_2, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = Some(control_2);
                previous_quadratic_control = None;
            }
            'Q' => {
                let control = svg_path_pair(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                let control = if relative {
                    (current.0 + control.0, current.1 + control.1)
                } else {
                    control
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let subpath = subpaths.last_mut()?;
                append_quadratic_curve(&mut subpath.points, current, control, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = Some(control);
            }
            'S' => {
                let control_2 = svg_path_pair(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                let control_1 = if matches!(previous_command, Some('C' | 'S')) {
                    previous_cubic_control
                        .map(|control| (current.0 * 2.0 - control.0, current.1 * 2.0 - control.1))
                        .unwrap_or(current)
                } else {
                    current
                };
                let control_2 = if relative {
                    (current.0 + control_2.0, current.1 + control_2.1)
                } else {
                    control_2
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let subpath = subpaths.last_mut()?;
                append_cubic_curve(&mut subpath.points, current, control_1, control_2, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = Some(control_2);
                previous_quadratic_control = None;
            }
            'T' => {
                let end = svg_path_pair(&tokens, &mut index)?;
                let control = if matches!(previous_command, Some('Q' | 'T')) {
                    previous_quadratic_control
                        .map(|control| (current.0 * 2.0 - control.0, current.1 * 2.0 - control.1))
                        .unwrap_or(current)
                } else {
                    current
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let subpath = subpaths.last_mut()?;
                append_quadratic_curve(&mut subpath.points, current, control, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = Some(control);
            }
            'A' => {
                let radius_x = svg_path_number(&tokens, &mut index)?;
                let radius_y = svg_path_number(&tokens, &mut index)?;
                let rotation = svg_path_number(&tokens, &mut index)?;
                let large_arc = svg_path_number(&tokens, &mut index)?;
                let sweep = svg_path_number(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                if !matches!(large_arc, 0.0 | 1.0) || !matches!(sweep, 0.0 | 1.0) {
                    return None;
                }
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let subpath = subpaths.last_mut()?;
                if !append_elliptical_arc(
                    &mut subpath.points,
                    current,
                    radius_x,
                    radius_y,
                    rotation,
                    large_arc == 1.0,
                    sweep == 1.0,
                    end,
                ) {
                    return None;
                }
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            _ => return None,
        }
        let point_count = subpaths
            .iter()
            .map(|subpath| subpath.points.len())
            .sum::<usize>();
        if point_count > MAX_NATIVE_SVG_POINTS {
            return None;
        }
    }
    (!subpaths.is_empty()).then_some(subpaths)
}

fn tokenize_svg_path(value: &str) -> Option<Vec<SvgPathToken>> {
    let characters = value.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < characters.len() {
        let character = characters[index];
        if character.is_ascii_alphabetic() {
            tokens.push(SvgPathToken::Command(character));
            index = index.saturating_add(1);
            continue;
        }
        if character == ',' || character.is_ascii_whitespace() {
            index = index.saturating_add(1);
            continue;
        }
        let start = index;
        if matches!(character, '+' | '-') {
            index = index.saturating_add(1);
        }
        let mut has_digit = false;
        while index < characters.len() && characters[index].is_ascii_digit() {
            has_digit = true;
            index = index.saturating_add(1);
        }
        if index < characters.len() && characters[index] == '.' {
            index = index.saturating_add(1);
            while index < characters.len() && characters[index].is_ascii_digit() {
                has_digit = true;
                index = index.saturating_add(1);
            }
        }
        if !has_digit {
            return None;
        }
        if index < characters.len() && matches!(characters[index], 'e' | 'E') {
            let exponent = index;
            index = index.saturating_add(1);
            if index < characters.len() && matches!(characters[index], '+' | '-') {
                index = index.saturating_add(1);
            }
            let exponent_start = index;
            while index < characters.len() && characters[index].is_ascii_digit() {
                index = index.saturating_add(1);
            }
            if exponent_start == index {
                index = exponent;
            }
        }
        let number = characters[start..index].iter().collect::<String>();
        tokens.push(SvgPathToken::Number(number.parse().ok()?));
        if tokens.len() > MAX_NATIVE_SVG_POINTS.saturating_mul(4) {
            return None;
        }
    }
    Some(tokens)
}

fn svg_path_number(tokens: &[SvgPathToken], index: &mut usize) -> Option<f64> {
    let SvgPathToken::Number(value) = tokens.get(*index).copied()? else {
        return None;
    };
    *index = index.saturating_add(1);
    Some(value)
}

fn svg_path_pair(tokens: &[SvgPathToken], index: &mut usize) -> Option<(f64, f64)> {
    Some((
        svg_path_number(tokens, index)?,
        svg_path_number(tokens, index)?,
    ))
}

fn append_cubic_curve(
    points: &mut Vec<NativePoint>,
    start: (f64, f64),
    control_1: (f64, f64),
    control_2: (f64, f64),
    end: (f64, f64),
) {
    for segment in 1..=MAX_NATIVE_SVG_CURVE_SEGMENTS {
        let t = segment as f64 / MAX_NATIVE_SVG_CURVE_SEGMENTS as f64;
        let inverse = 1.0 - t;
        let inverse_squared = inverse * inverse;
        let t_squared = t * t;
        let x = inverse_squared * inverse * start.0
            + 3.0 * inverse_squared * t * control_1.0
            + 3.0 * inverse * t_squared * control_2.0
            + t_squared * t * end.0;
        let y = inverse_squared * inverse * start.1
            + 3.0 * inverse_squared * t * control_1.1
            + 3.0 * inverse * t_squared * control_2.1
            + t_squared * t * end.1;
        points.push(svg_path_point((x, y)));
    }
}

fn append_quadratic_curve(
    points: &mut Vec<NativePoint>,
    start: (f64, f64),
    control: (f64, f64),
    end: (f64, f64),
) {
    for segment in 1..=MAX_NATIVE_SVG_CURVE_SEGMENTS {
        let t = segment as f64 / MAX_NATIVE_SVG_CURVE_SEGMENTS as f64;
        let inverse = 1.0 - t;
        let x = inverse * inverse * start.0 + 2.0 * inverse * t * control.0 + t * t * end.0;
        let y = inverse * inverse * start.1 + 2.0 * inverse * t * control.1 + t * t * end.1;
        points.push(svg_path_point((x, y)));
    }
}

#[allow(clippy::too_many_arguments)]
fn append_elliptical_arc(
    points: &mut Vec<NativePoint>,
    start: (f64, f64),
    radius_x: f64,
    radius_y: f64,
    rotation_degrees: f64,
    large_arc: bool,
    sweep: bool,
    end: (f64, f64),
) -> bool {
    if ![
        start.0,
        start.1,
        radius_x,
        radius_y,
        rotation_degrees,
        end.0,
        end.1,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        return false;
    }
    let radius_x = radius_x.abs();
    let radius_y = radius_y.abs();
    if radius_x == 0.0 || radius_y == 0.0 {
        if points.len() >= MAX_NATIVE_SVG_POINTS {
            return false;
        }
        points.push(svg_path_point(end));
        return true;
    }
    if start == end {
        return true;
    }

    let rotation = rotation_degrees.to_radians();
    let (sin_rotation, cos_rotation) = rotation.sin_cos();
    let delta_x = (start.0 - end.0) / 2.0;
    let delta_y = (start.1 - end.1) / 2.0;
    let transformed_x = cos_rotation * delta_x + sin_rotation * delta_y;
    let transformed_y = -sin_rotation * delta_x + cos_rotation * delta_y;
    let radius_x_squared = radius_x * radius_x;
    let radius_y_squared = radius_y * radius_y;
    let transformed_x_squared = transformed_x * transformed_x;
    let transformed_y_squared = transformed_y * transformed_y;
    let radii_ratio =
        transformed_x_squared / radius_x_squared + transformed_y_squared / radius_y_squared;
    if !radii_ratio.is_finite() {
        return false;
    }
    let radius_scale = radii_ratio.sqrt().max(1.0);
    let radius_x = radius_x * radius_scale;
    let radius_y = radius_y * radius_scale;
    let radius_x_squared = radius_x * radius_x;
    let radius_y_squared = radius_y * radius_y;
    let numerator = radius_x_squared * radius_y_squared
        - radius_x_squared * transformed_y_squared
        - radius_y_squared * transformed_x_squared;
    let denominator =
        radius_x_squared * transformed_y_squared + radius_y_squared * transformed_x_squared;
    if ![radius_x, radius_y, numerator, denominator]
        .iter()
        .all(|value| value.is_finite())
        || denominator == 0.0
    {
        return false;
    }
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let center_scale = sign * (numerator.max(0.0) / denominator).sqrt();
    let center_x = center_scale * radius_x * transformed_y / radius_y;
    let center_y = center_scale * -radius_y * transformed_x / radius_x;
    let center = (
        cos_rotation * center_x - sin_rotation * center_y + (start.0 + end.0) / 2.0,
        sin_rotation * center_x + cos_rotation * center_y + (start.1 + end.1) / 2.0,
    );
    let start_vector = (
        (transformed_x - center_x) / radius_x,
        (transformed_y - center_y) / radius_y,
    );
    let end_vector = (
        (-transformed_x - center_x) / radius_x,
        (-transformed_y - center_y) / radius_y,
    );
    let start_angle = start_vector.1.atan2(start_vector.0);
    let mut sweep_angle = (start_vector.0 * end_vector.1 - start_vector.1 * end_vector.0)
        .atan2(start_vector.0 * end_vector.0 + start_vector.1 * end_vector.1);
    if sweep && sweep_angle < 0.0 {
        sweep_angle += std::f64::consts::TAU;
    } else if !sweep && sweep_angle > 0.0 {
        sweep_angle -= std::f64::consts::TAU;
    }
    if ![
        center.0,
        center.1,
        start_angle,
        sweep_angle,
        radius_x,
        radius_y,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        return false;
    }
    let segment_count = ((sweep_angle.abs() / (std::f64::consts::PI / 2.0)) * 16.0)
        .ceil()
        .max(1.0)
        .min(MAX_NATIVE_SVG_ARC_SEGMENTS as f64) as usize;
    if points.len().saturating_add(segment_count) > MAX_NATIVE_SVG_POINTS {
        return false;
    }
    for segment in 1..=segment_count {
        let progress = segment as f64 / segment_count as f64;
        let angle = start_angle + sweep_angle * progress;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let point = if segment == segment_count {
            end
        } else {
            (
                center.0 + cos_rotation * radius_x * cos_angle
                    - sin_rotation * radius_y * sin_angle,
                center.1
                    + sin_rotation * radius_x * cos_angle
                    + cos_rotation * radius_y * sin_angle,
            )
        };
        if !point.0.is_finite() || !point.1.is_finite() {
            return false;
        }
        points.push(svg_path_point(point));
    }
    true
}

fn svg_path_point((x, y): (f64, f64)) -> NativePoint {
    let coordinate = |value: f64| {
        if !value.is_finite() {
            return 0;
        }
        value.round().max(0.0).min(f64::from(u32::MAX)) as u32
    };
    NativePoint {
        x: coordinate(x),
        y: coordinate(y),
    }
}

pub(super) fn svg_points_bounds(points: &[NativePoint]) -> Option<(u32, u32, u32, u32)> {
    let first = points.first().copied()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x;
    let mut max_y = first.y;
    for point in &points[1..] {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    Some((min_x, min_y, max_x, max_y))
}
