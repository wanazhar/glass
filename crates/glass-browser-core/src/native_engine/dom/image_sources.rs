use super::*;

pub(super) fn select_image_srcset_source(
    srcset: &str,
    sizes: Option<&str>,
    viewport: Viewport,
) -> Option<String> {
    let candidates = parse_image_srcset(srcset);
    if candidates.is_empty() {
        return None;
    }
    let source_size = image_source_size(sizes, viewport);
    let device_scale_factor = viewport
        .device_scale_factor_milli
        .max(DEFAULT_IMAGE_DENSITY_MILLI);
    let selected = match candidates[0].descriptor {
        NativeImageCandidateDescriptor::Density(_) => candidates.iter().min_by_key(|candidate| {
            let density = match candidate.descriptor {
                NativeImageCandidateDescriptor::Density(value) => value,
                NativeImageCandidateDescriptor::Width(_) => {
                    unreachable!("mixed image candidate descriptors were rejected during parsing")
                }
            };
            (
                density < device_scale_factor,
                density.abs_diff(device_scale_factor),
            )
        }),
        NativeImageCandidateDescriptor::Width(_) => {
            let target_width = u64::from(source_size)
                .saturating_mul(u64::from(device_scale_factor))
                .saturating_add(u64::from(DEFAULT_IMAGE_DENSITY_MILLI - 1))
                / u64::from(DEFAULT_IMAGE_DENSITY_MILLI);
            candidates.iter().min_by_key(|candidate| {
                let width = match candidate.descriptor {
                    NativeImageCandidateDescriptor::Width(value) => value,
                    NativeImageCandidateDescriptor::Density(_) => unreachable!(
                        "mixed image candidate descriptors were rejected during parsing"
                    ),
                };
                (
                    u64::from(width) < target_width,
                    u64::from(width).abs_diff(target_width),
                )
            })
        }
    }?;
    Some(selected.source.clone())
}

pub(super) fn image_media_matches(media: Option<&str>, viewport: Viewport) -> bool {
    media.is_none_or(|media| {
        let media = media.trim();
        media.is_empty() || image_size_condition_matches(media, viewport)
    })
}

pub(super) fn image_type_is_supported(image_type: Option<&str>) -> bool {
    image_type.is_none_or(|image_type| {
        let image_type = image_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        image_type.is_empty()
            || matches!(
                image_type.as_str(),
                "image/png"
                    | "image/apng"
                    | "image/jpeg"
                    | "image/webp"
                    | "image/gif"
                    | "image/svg+xml"
            )
    })
}

pub(super) fn media_type_is_supported(media_type: Option<&str>) -> bool {
    media_type.is_none_or(|media_type| {
        let media_type = media_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        media_type.is_empty()
            || matches!(
                media_type.as_str(),
                "audio/mpeg"
                    | "audio/mp3"
                    | "audio/ogg"
                    | "application/ogg"
                    | "audio/wav"
                    | "audio/wave"
                    | "audio/x-wav"
                    | "audio/webm"
                    | "audio/mp4"
                    | "video/mp4"
                    | "video/ogg"
                    | "video/webm"
                    | "application/vnd.apple.mpegurl"
                    | "application/x-mpegurl"
            )
    })
}

pub(super) fn image_content_point(
    layout: &NativeLayoutSnapshot,
    image_id: NativeNodeId,
    x: i64,
    y: i64,
) -> Option<(f64, f64)> {
    let (visible_rect, source_offset) = layout.viewport_projection_for(image_id)?;
    let point = NativePoint {
        x: u32::try_from(x).ok()?,
        y: u32::try_from(y).ok()?,
    };
    if !visible_rect.contains(point) {
        return None;
    }
    let layout_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == image_id)?;
    let inset_x = layout_box.content_rect.x.saturating_sub(layout_box.rect.x);
    let inset_y = layout_box.content_rect.y.saturating_sub(layout_box.rect.y);
    let local_x = i64::from(point.x)
        .saturating_sub(i64::from(visible_rect.x))
        .saturating_add(i64::from(source_offset.x))
        .saturating_sub(i64::from(inset_x));
    let local_y = i64::from(point.y)
        .saturating_sub(i64::from(visible_rect.y))
        .saturating_add(i64::from(source_offset.y))
        .saturating_sub(i64::from(inset_y));
    if local_x < 0
        || local_y < 0
        || local_x >= i64::from(layout_box.content_rect.width)
        || local_y >= i64::from(layout_box.content_rect.height)
    {
        return None;
    }
    Some((local_x as f64, local_y as f64))
}

pub(super) fn image_map_shape_contains(node: &NativeNode, x: f64, y: f64) -> bool {
    let shape = image_map_shape(node);
    if shape == NativeImageMapShape::Default {
        return true;
    }
    let mut coords = parse_image_map_coordinates(node.attribute("coords"));
    match shape {
        NativeImageMapShape::Default => true,
        NativeImageMapShape::Rectangle => {
            if coords.len() < 4 {
                return false;
            }
            coords.truncate(4);
            let (mut left, mut top, mut right, mut bottom) =
                (coords[0], coords[1], coords[2], coords[3]);
            if left > right {
                std::mem::swap(&mut left, &mut right);
            }
            if top > bottom {
                std::mem::swap(&mut top, &mut bottom);
            }
            x >= left && x <= right && y >= top && y <= bottom
        }
        NativeImageMapShape::Circle => {
            if coords.len() < 3 {
                return false;
            }
            coords.truncate(3);
            let radius = coords[2];
            if radius <= 0.0 {
                return false;
            }
            let delta_x = x - coords[0];
            let delta_y = y - coords[1];
            delta_x.mul_add(delta_x, delta_y * delta_y) <= radius * radius
        }
        NativeImageMapShape::Polygon => {
            if coords.len() < 6 {
                return false;
            }
            if !coords.len().is_multiple_of(2) {
                coords.pop();
            }
            image_map_polygon_contains(&coords, x, y)
        }
    }
}

pub(super) fn image_map_area_has_nonempty_shape(node: &NativeNode) -> bool {
    let shape = image_map_shape(node);
    if shape == NativeImageMapShape::Default {
        return true;
    }
    let mut coordinates = parse_image_map_coordinates(node.attribute("coords"));
    match shape {
        NativeImageMapShape::Default => true,
        NativeImageMapShape::Rectangle => {
            if coordinates.len() < 4 {
                return false;
            }
            coordinates.truncate(4);
            let (mut left, mut top, mut right, mut bottom) = (
                coordinates[0],
                coordinates[1],
                coordinates[2],
                coordinates[3],
            );
            if left > right {
                std::mem::swap(&mut left, &mut right);
            }
            if top > bottom {
                std::mem::swap(&mut top, &mut bottom);
            }
            left < right && top < bottom
        }
        NativeImageMapShape::Circle => coordinates.len() >= 3 && coordinates[2] > 0.0,
        NativeImageMapShape::Polygon => {
            if coordinates.len() < 6 {
                return false;
            }
            if !coordinates.len().is_multiple_of(2) {
                coordinates.pop();
            }
            let xs = coordinates.iter().step_by(2).copied();
            let (min_x, max_x) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), x| {
                (min.min(x), max.max(x))
            });
            let ys = coordinates.iter().skip(1).step_by(2).copied();
            let (min_y, max_y) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), y| {
                (min.min(y), max.max(y))
            });
            min_x < max_x && min_y < max_y
        }
    }
}

pub(super) fn image_map_shape(node: &NativeNode) -> NativeImageMapShape {
    match node.attribute("shape") {
        Some(value) if value.eq_ignore_ascii_case("default") => NativeImageMapShape::Default,
        Some(value) if value.eq_ignore_ascii_case("circle") => NativeImageMapShape::Circle,
        Some(value) if value.eq_ignore_ascii_case("poly") => NativeImageMapShape::Polygon,
        _ => NativeImageMapShape::Rectangle,
    }
}

pub(super) fn parse_image_map_coordinates(value: Option<&str>) -> Vec<f64> {
    let Some(value) = value else {
        return Vec::new();
    };
    let mut coordinates = Vec::new();
    let mut position = 0;
    while position < value.len() {
        let character = value[position..]
            .chars()
            .next()
            .expect("position is within the coordinate string");
        if character == ',' || character == ';' || character.is_ascii_whitespace() {
            position += character.len_utf8();
            continue;
        }
        if !character.is_ascii_digit() && !matches!(character, '.' | '-') {
            position += character.len_utf8();
            continue;
        }

        let start = position;
        while position < value.len() {
            let character = value[position..]
                .chars()
                .next()
                .expect("position is within the coordinate string");
            if character == ',' || character == ';' || character.is_ascii_whitespace() {
                break;
            }
            position += character.len_utf8();
        }
        coordinates.push(
            value[start..position]
                .parse::<f64>()
                .ok()
                .filter(|coordinate| coordinate.is_finite())
                .unwrap_or(0.0),
        );
    }
    coordinates
}

pub(super) fn parse_native_tab_index(value: &str) -> Option<i32> {
    let value = value.trim_matches(|character: char| character.is_ascii_whitespace());
    let value = value.strip_prefix('+').unwrap_or(value);
    value.parse::<i32>().ok()
}

pub(super) fn image_map_polygon_contains(coords: &[f64], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut previous = coords.len() - 2;
    for current in (0..coords.len()).step_by(2) {
        let x1 = coords[previous];
        let y1 = coords[previous + 1];
        let x2 = coords[current];
        let y2 = coords[current + 1];
        let cross = (x - x1) * (y2 - y1) - (y - y1) * (x2 - x1);
        if cross == 0.0 && x >= x1.min(x2) && x <= x1.max(x2) && y >= y1.min(y2) && y <= y1.max(y2)
        {
            return true;
        }
        if (y1 > y) != (y2 > y) {
            let crossing_x = (x2 - x1) * (y - y1) / (y2 - y1) + x1;
            if x < crossing_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

pub(super) fn parse_image_srcset(value: &str) -> Vec<NativeImageCandidate> {
    let mut candidates = Vec::new();
    let mut descriptor_kind = None;
    for component in value.split(',').take(MAX_IMAGE_SRCSET_CANDIDATES) {
        let mut tokens = component.split_ascii_whitespace();
        let Some(source) = tokens.next() else {
            continue;
        };
        if source.is_empty()
            || source.len() > MAX_ATTRIBUTE_BYTES
            || source.bytes().any(|byte| byte.is_ascii_control())
            || tokens.next().is_some_and(|_| tokens.next().is_some())
        {
            continue;
        }
        let descriptor_token = component
            .split_ascii_whitespace()
            .nth(1)
            .unwrap_or_default();
        let descriptor = if descriptor_token.is_empty() {
            NativeImageCandidateDescriptor::Density(DEFAULT_IMAGE_DENSITY_MILLI)
        } else if let Some(value) = descriptor_token.strip_suffix('w') {
            let Some(width) = value.parse::<u32>().ok().filter(|width| *width > 0) else {
                continue;
            };
            NativeImageCandidateDescriptor::Width(width)
        } else if let Some(value) = descriptor_token.strip_suffix('x') {
            let Some(density) = parse_image_density(value) else {
                continue;
            };
            NativeImageCandidateDescriptor::Density(density)
        } else {
            continue;
        };
        let is_density = matches!(descriptor, NativeImageCandidateDescriptor::Density(_));
        if descriptor_kind.is_some_and(|kind| kind != is_density) {
            return Vec::new();
        }
        descriptor_kind = Some(is_density);
        if candidates
            .iter()
            .any(|candidate: &NativeImageCandidate| candidate.descriptor == descriptor)
        {
            continue;
        }
        candidates.push(NativeImageCandidate {
            source: source.to_owned(),
            descriptor,
        });
    }
    candidates
}

pub(super) fn image_source_is_declared(
    nodes: &[NativeNode],
    node_index: usize,
    source: &str,
) -> bool {
    let Some(node) = nodes.get(node_index) else {
        return false;
    };
    if is_svg_image_node_in(nodes, node_index) {
        return svg_image_href(node) == Some(source);
    }
    if node.attribute("src") == Some(source)
        || node.attribute("srcset").is_some_and(|srcset| {
            parse_image_srcset(srcset)
                .iter()
                .any(|candidate| candidate.source == source)
        })
    {
        return true;
    }
    let Some(parent_index) = node.parent().map(NativeNodeId::index) else {
        return false;
    };
    let Some(parent) = usize::try_from(parent_index)
        .ok()
        .and_then(|index| nodes.get(index))
    else {
        return false;
    };
    if parent.element_name() != Some("picture") {
        return false;
    }
    for child_id in parent.children() {
        let Some(child_index) = usize::try_from(child_id.index()).ok() else {
            continue;
        };
        if child_index == node_index {
            break;
        }
        if nodes
            .get(child_index)
            .is_some_and(|child| child.element_name() == Some("source"))
            && nodes[child_index]
                .attribute("srcset")
                .is_some_and(|srcset| {
                    parse_image_srcset(srcset)
                        .iter()
                        .any(|candidate| candidate.source == source)
                })
        {
            return true;
        }
    }
    false
}

pub(super) fn video_poster_source_is_declared(
    nodes: &[NativeNode],
    node_index: usize,
    source: &str,
) -> bool {
    nodes.get(node_index).is_some_and(|node| {
        node.element_name() == Some("video")
            && node.state.namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI)
            && node.attribute("poster") == Some(source)
    })
}

pub(super) fn is_image_resource_node_in(nodes: &[NativeNode], node_index: usize) -> bool {
    nodes.get(node_index).is_some_and(|node| {
        node.element_name() == Some("img") || is_svg_image_node_in(nodes, node_index)
    })
}

pub(super) fn is_svg_image_node_in(nodes: &[NativeNode], node_index: usize) -> bool {
    let Some(node) = nodes.get(node_index) else {
        return false;
    };
    if node.element_name() != Some("image") || node.namespace_uri() != Some(SVG_NAMESPACE_URI) {
        return false;
    }
    let mut parent_id = node.parent();
    while let Some(id) = parent_id {
        let Some(parent) = usize::try_from(id.index())
            .ok()
            .and_then(|parent_index| nodes.get(parent_index))
        else {
            return false;
        };
        if parent.element_name() == Some("svg") && parent.namespace_uri() == Some(SVG_NAMESPACE_URI)
        {
            return true;
        }
        if parent.namespace_uri() == Some(HTML_NAMESPACE_URI) {
            return false;
        }
        parent_id = parent.parent();
    }
    false
}

pub(super) fn svg_image_href(node: &NativeNode) -> Option<&str> {
    node.attribute("href")
        .or_else(|| node.attribute_in_namespace(Some(XLINK_NAMESPACE_URI), "href"))
        .or_else(|| node.attribute("xlink:href"))
}

pub(super) fn media_source_is_declared(
    nodes: &[NativeNode],
    node_index: usize,
    source: &str,
) -> bool {
    let Some(node) = nodes.get(node_index) else {
        return false;
    };
    if node.attribute("src") == Some(source) {
        return true;
    }
    node.children().iter().any(|child_id| {
        usize::try_from(child_id.index())
            .ok()
            .and_then(|index| nodes.get(index))
            .is_some_and(|child| {
                child.element_name() == Some("source") && child.attribute("src") == Some(source)
            })
    })
}

pub(super) fn parse_image_density(value: &str) -> Option<u32> {
    let density = value.parse::<f64>().ok()?;
    if !density.is_finite()
        || density <= 0.0
        || density * 1000.0 > f64::from(MAX_IMAGE_DENSITY_MILLI)
    {
        return None;
    }
    let density = (density * 1000.0).round();
    (density >= 1.0).then_some(density as u32)
}

pub(super) fn image_source_size(sizes: Option<&str>, viewport: Viewport) -> u32 {
    let default_size = viewport.width.max(1);
    let Some(sizes) = sizes else {
        return default_size;
    };
    for component in sizes.split(',') {
        let component = component.trim();
        if component.is_empty() {
            continue;
        }
        let (condition, length) = if component.starts_with('(') {
            let Some(condition_end) = component.find(')') else {
                continue;
            };
            (
                Some(&component[..=condition_end]),
                component[condition_end.saturating_add(1)..].trim(),
            )
        } else {
            (None, component)
        };
        if condition.is_some_and(|condition| !image_size_condition_matches(condition, viewport)) {
            continue;
        }
        if let Some(size) = parse_image_source_length(length, viewport) {
            return size.max(1);
        }
    }
    default_size
}

pub(super) fn image_size_condition_matches(condition: &str, viewport: Viewport) -> bool {
    let Some(condition) = condition
        .strip_prefix('(')
        .and_then(|condition| condition.strip_suffix(')'))
    else {
        return false;
    };
    let Some((feature, value)) = condition.split_once(':') else {
        return false;
    };
    let Some(value) = parse_fixed_css_pixels(value.trim()) else {
        return false;
    };
    let viewport_width = f64::from(viewport.width);
    match feature.trim().to_ascii_lowercase().as_str() {
        "max-width" => viewport_width <= value,
        "min-width" => viewport_width >= value,
        "width" => (viewport_width - value).abs() < f64::EPSILON,
        _ => false,
    }
}

pub(super) fn parse_image_source_length(value: &str, viewport: Viewport) -> Option<u32> {
    let value = value.trim();
    let (number, unit) = if let Some(number) = value.strip_suffix("px") {
        (number, "px")
    } else {
        let number = value.strip_suffix("vw")?;
        (number, "vw")
    };
    let number = number.trim().parse::<f64>().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    let pixels = if unit == "vw" {
        number * f64::from(viewport.width) / 100.0
    } else {
        number
    };
    if !pixels.is_finite() {
        return None;
    }
    Some(pixels.round().clamp(1.0, f64::from(u32::MAX)) as u32)
}

pub(super) fn parse_fixed_css_pixels(value: &str) -> Option<f64> {
    let number = value.strip_suffix("px")?.trim().parse::<f64>().ok()?;
    (!number.is_nan() && number.is_finite() && number >= 0.0).then_some(number)
}
