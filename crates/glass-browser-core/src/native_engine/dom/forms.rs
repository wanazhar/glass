//! DOM form validation and serialization.

use super::*;

pub(super) fn text_input_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot::default();
    let required = node.attribute("required").is_some();
    if required && value.is_empty() {
        validity.value_missing = true;
    }
    if value.is_empty() {
        return finalize_validity(validity);
    }
    let input_type = node.attribute("type").unwrap_or("text");
    if input_type.eq_ignore_ascii_case("email") {
        let values = if node.attribute("multiple").is_some() {
            value.split(',').map(str::trim).collect::<Vec<_>>()
        } else {
            vec![value]
        };
        if values.is_empty() || values.iter().any(|item| !valid_email_value(item)) {
            validity.type_mismatch = true;
        }
    } else if input_type.eq_ignore_ascii_case("url") && url::Url::parse(value).is_err() {
        validity.type_mismatch = true;
    }
    if pattern_applies(input_type) {
        merge_validity(&mut validity, pattern_validity(node, value));
    }
    match input_type.to_ascii_lowercase().as_str() {
        "date" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::Date),
        ),
        "month" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::Month),
        ),
        "time" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::Time),
        ),
        "datetime-local" => merge_validity(
            &mut validity,
            temporal_validity(node, value, TemporalInputKind::DateTimeLocal),
        ),
        "number" | "range" => merge_validity(&mut validity, numeric_validity(node, value)),
        _ => {}
    }
    merge_validity(&mut validity, length_validity(node, value));
    finalize_validity(validity)
}

pub(super) fn textarea_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot {
        value_missing: node.attribute("required").is_some() && value.is_empty(),
        ..NativeValiditySnapshot::default()
    };
    merge_validity(&mut validity, length_validity(node, value));
    finalize_validity(validity)
}

pub(super) fn pattern_applies(input_type: &str) -> bool {
    !matches!(
        input_type.to_ascii_lowercase().as_str(),
        "hidden"
            | "button"
            | "submit"
            | "reset"
            | "image"
            | "checkbox"
            | "radio"
            | "file"
            | "date"
            | "month"
            | "week"
            | "time"
            | "datetime-local"
            | "number"
            | "range"
            | "color"
    )
}

pub(super) fn pattern_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let Some(pattern) = node.attribute("pattern") else {
        return NativeValiditySnapshot::default();
    };
    let expression = format!(r"\A(?:{pattern})\z");
    let Ok(regex) = regex::Regex::new(&expression) else {
        return NativeValiditySnapshot::default();
    };
    finalize_validity(NativeValiditySnapshot {
        pattern_mismatch: !regex.is_match(value),
        ..NativeValiditySnapshot::default()
    })
}

pub(super) fn finalize_validity(mut validity: NativeValiditySnapshot) -> NativeValiditySnapshot {
    validity.valid = !validity.bad_input
        && !validity.custom_error
        && !validity.pattern_mismatch
        && !validity.range_overflow
        && !validity.range_underflow
        && !validity.step_mismatch
        && !validity.too_long
        && !validity.too_short
        && !validity.type_mismatch
        && !validity.value_missing;
    validity
}

pub(super) fn merge_validity(target: &mut NativeValiditySnapshot, source: NativeValiditySnapshot) {
    target.bad_input |= source.bad_input;
    target.custom_error |= source.custom_error;
    target.pattern_mismatch |= source.pattern_mismatch;
    target.range_overflow |= source.range_overflow;
    target.range_underflow |= source.range_underflow;
    target.step_mismatch |= source.step_mismatch;
    target.too_long |= source.too_long;
    target.too_short |= source.too_short;
    target.type_mismatch |= source.type_mismatch;
    target.value_missing |= source.value_missing;
}

pub(super) fn validation_message(
    validity: &NativeValiditySnapshot,
    custom_validity: &str,
) -> String {
    if !custom_validity.is_empty() {
        return custom_validity.to_owned();
    }
    if validity.value_missing {
        return "Please fill out this field.".into();
    }
    if validity.type_mismatch || validity.bad_input {
        return "Please enter a valid value.".into();
    }
    if validity.too_short {
        return "Value is too short.".into();
    }
    if validity.too_long {
        return "Value is too long.".into();
    }
    if validity.range_underflow {
        return "Value is below the minimum.".into();
    }
    if validity.range_overflow {
        return "Value is above the maximum.".into();
    }
    if validity.step_mismatch {
        return "Value does not match the required step.".into();
    }
    if validity.pattern_mismatch {
        return "Please match the requested format.".into();
    }
    String::new()
}

pub(super) fn valid_email_value(value: &str) -> bool {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

pub(super) fn length_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let length = value.encode_utf16().count();
    let min_length = node
        .attribute("minlength")
        .and_then(|value| value.parse::<usize>().ok());
    let max_length = node
        .attribute("maxlength")
        .and_then(|value| value.parse::<usize>().ok());
    finalize_validity(NativeValiditySnapshot {
        too_short: min_length.is_some_and(|minimum| !value.is_empty() && length < minimum),
        too_long: max_length.is_some_and(|maximum| length > maximum),
        ..NativeValiditySnapshot::default()
    })
}

pub(super) fn numeric_validity(node: &NativeNode, value: &str) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot::default();
    if value.is_empty() {
        return finalize_validity(validity);
    }
    let Ok(number) = value.parse::<f64>() else {
        validity.bad_input = true;
        return finalize_validity(validity);
    };
    if !number.is_finite() {
        validity.bad_input = true;
        return finalize_validity(validity);
    }
    let minimum = node
        .attribute("min")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite());
    let maximum = node
        .attribute("max")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite());
    validity.range_underflow = minimum.is_some_and(|minimum| number < minimum);
    validity.range_overflow = maximum.is_some_and(|maximum| number > maximum);
    let step_value = node.attribute("step").unwrap_or("1");
    if step_value.eq_ignore_ascii_case("any") {
        return finalize_validity(validity);
    }
    let step = step_value
        .parse::<f64>()
        .ok()
        .filter(|step| step.is_finite() && *step > 0.0)
        .unwrap_or(1.0);
    let base = minimum.unwrap_or(0.0);
    let remainder = ((number - base) / step).fract().abs();
    validity.step_mismatch =
        remainder > f64::EPSILON * 32.0 && (1.0 - remainder) > f64::EPSILON * 32.0;
    finalize_validity(validity)
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TemporalInputKind {
    Date,
    Month,
    Time,
    DateTimeLocal,
}

pub(super) fn temporal_validity(
    node: &NativeNode,
    value: &str,
    kind: TemporalInputKind,
) -> NativeValiditySnapshot {
    let mut validity = NativeValiditySnapshot::default();
    if value.is_empty() {
        return finalize_validity(validity);
    }
    let Some(number) = parse_temporal_value(value, kind) else {
        validity.bad_input = true;
        return finalize_validity(validity);
    };
    let minimum = node
        .attribute("min")
        .and_then(|value| parse_temporal_value(value, kind));
    let maximum = node
        .attribute("max")
        .and_then(|value| parse_temporal_value(value, kind));
    validity.range_underflow = minimum.is_some_and(|minimum| number < minimum);
    validity.range_overflow = maximum.is_some_and(|maximum| number > maximum);
    let default_step = match kind {
        TemporalInputKind::Date => 86_400.0,
        TemporalInputKind::Month => 1.0,
        TemporalInputKind::Time | TemporalInputKind::DateTimeLocal => 60.0,
    };
    let step_unit = match kind {
        TemporalInputKind::Date | TemporalInputKind::DateTimeLocal => 86_400.0,
        TemporalInputKind::Month | TemporalInputKind::Time => 1.0,
    };
    let step_value = node.attribute("step").unwrap_or_default();
    if step_value.eq_ignore_ascii_case("any") {
        return finalize_validity(validity);
    }
    let step = if step_value.is_empty() {
        default_step
    } else {
        step_value
            .parse::<f64>()
            .ok()
            .filter(|step| step.is_finite() && *step > 0.0)
            .map(|step| step * step_unit)
            .filter(|step| step.is_finite() && *step > 0.0)
            .unwrap_or(default_step)
    };
    let base = minimum.unwrap_or_else(|| match kind {
        TemporalInputKind::Date | TemporalInputKind::DateTimeLocal => parse_temporal_value(
            if matches!(kind, TemporalInputKind::Date) {
                "1970-01-01"
            } else {
                "1970-01-01T00:00"
            },
            kind,
        )
        .unwrap_or(0.0),
        TemporalInputKind::Month => 1970.0 * 12.0,
        TemporalInputKind::Time => 0.0,
    });
    let remainder = ((number - base) / step).fract().abs();
    validity.step_mismatch =
        remainder > f64::EPSILON * 32.0 && (1.0 - remainder) > f64::EPSILON * 32.0;
    finalize_validity(validity)
}

pub(super) fn parse_temporal_value(value: &str, kind: TemporalInputKind) -> Option<f64> {
    match kind {
        TemporalInputKind::Date => parse_date_value(value).map(|days| days as f64 * 86_400.0),
        TemporalInputKind::Month => parse_month_value(value).map(|months| months as f64),
        TemporalInputKind::Time => parse_time_value(value),
        TemporalInputKind::DateTimeLocal => {
            let (date, time) = value.split_once('T')?;
            Some(parse_date_value(date)? as f64 * 86_400.0 + parse_time_value(time)?)
        }
    }
}

pub(super) fn parse_date_value(value: &str) -> Option<i64> {
    if value.len() != 10 || value.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    if value.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let year = value.get(..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..)?.parse::<i64>().ok()?;
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let first = days_from_civil(year, month, 1);
    let next_month = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    (1..=(next_month - first))
        .contains(&day)
        .then_some(first + day - 1)
}

pub(super) fn parse_month_value(value: &str) -> Option<i64> {
    if value.len() != 7 || value.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    let year = value.get(..4)?.parse::<i64>().ok()?;
    let month = value.get(5..)?.parse::<i64>().ok()?;
    (1..=9999)
        .contains(&year)
        .then_some(month)
        .filter(|month| (1..=12).contains(month))
        .map(|month| year * 12 + month - 1)
}

pub(super) fn parse_time_value(value: &str) -> Option<f64> {
    let (hour, remainder) = value.split_once(':')?;
    let (minute, seconds) = remainder
        .split_once(':')
        .map_or((remainder, None), |(m, s)| (m, Some(s)));
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let second = seconds.unwrap_or("0");
    let (whole, fraction) = second
        .split_once('.')
        .map_or((second, None), |(s, f)| (s, Some(f)));
    let whole = whole.parse::<u32>().ok()?;
    if whole > 59 || fraction.is_some_and(|fraction| fraction.is_empty()) {
        return None;
    }
    let fraction = match fraction {
        Some(fraction) => format!("0.{fraction}").parse::<f64>().ok()?,
        None => 0.0,
    };
    Some(hour as f64 * 3600.0 + minute as f64 * 60.0 + whole as f64 + fraction)
}

pub(super) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year / 400
    } else {
        (adjusted_year - 399) / 400
    };
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era
}

pub(super) fn append_form_value(
    pairs: &mut Vec<(String, NativeFormValue)>,
    name: &str,
    value: NativeFormValue,
) -> Result<(), NativeEngineError> {
    if pairs.len() >= MAX_FORM_CONTROLS {
        return Err(NativeEngineError::limit(
            "form controls",
            MAX_FORM_CONTROLS,
            pairs.len().saturating_add(1),
        ));
    }
    pairs.push((name.to_owned(), value));
    Ok(())
}

pub(super) fn form_value_text(value: &NativeFormValue) -> &str {
    match value {
        NativeFormValue::Text(value) => value,
        NativeFormValue::File(file) => &file.name,
        NativeFormValue::EmptyFile => "",
    }
}

pub(super) fn encode_urlencoded_form_data(
    pairs: &[(String, NativeFormValue)],
) -> Result<String, NativeEngineError> {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        query.append_pair(name, form_value_text(value));
    }
    let query = query.finish();
    if query.len() > MAX_LOCATOR_BYTES {
        return Err(NativeEngineError::limit(
            "form submission data",
            MAX_LOCATOR_BYTES,
            query.len(),
        ));
    }
    Ok(query)
}

pub(super) fn encode_form_data(
    pairs: &[(String, NativeFormValue)],
    encoding: NativeFormEncoding,
) -> Result<(NativeRequestBody, String), NativeEngineError> {
    match encoding {
        NativeFormEncoding::UrlEncoded => Ok((
            NativeRequestBody::Text(encode_urlencoded_form_data(pairs)?),
            "application/x-www-form-urlencoded".into(),
        )),
        NativeFormEncoding::TextPlain => {
            let mut body = String::new();
            for (name, value) in pairs {
                body.push_str(name);
                body.push('=');
                body.push_str(form_value_text(value));
                body.push_str("\r\n");
            }
            if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
                return Err(NativeEngineError::limit(
                    "form submission data",
                    MAX_NATIVE_FORM_BODY_BYTES,
                    body.len(),
                ));
            }
            Ok((NativeRequestBody::Text(body), "text/plain".into()))
        }
        NativeFormEncoding::Multipart => {
            if pairs.iter().any(|(name, _)| name.contains(['\r', '\n'])) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "multipart form field names must not contain line breaks".into(),
                });
            }
            let boundary = (0..16).find_map(|suffix| {
                let candidate = if suffix == 0 {
                    "----glass-native-form-boundary".to_owned()
                } else {
                    format!("----glass-native-form-boundary-{suffix}")
                };
                pairs
                    .iter()
                    .all(|(name, value)| {
                        !name.contains(&candidate)
                            && match value {
                                NativeFormValue::Text(value) => !value.contains(&candidate),
                                NativeFormValue::File(file) => {
                                    !file.name.contains(&candidate)
                                        && !file
                                            .bytes
                                            .windows(candidate.len())
                                            .any(|window| window == candidate.as_bytes())
                                }
                                NativeFormValue::EmptyFile => true,
                            }
                    })
                    .then_some(candidate)
            });
            let Some(boundary) = boundary else {
                return Err(NativeEngineError::limit(
                    "multipart form boundary attempts",
                    16,
                    17,
                ));
            };
            let mut body = Vec::new();
            for (name, value) in pairs {
                body.extend_from_slice(b"--");
                body.extend_from_slice(boundary.as_bytes());
                body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
                body.extend_from_slice(name.replace('\\', "\\\\").replace('"', "\\\"").as_bytes());
                body.extend_from_slice(b"\"");
                match value {
                    NativeFormValue::Text(value) => {
                        body.extend_from_slice(b"\r\n\r\n");
                        body.extend_from_slice(value.as_bytes());
                    }
                    NativeFormValue::File(file) => {
                        body.extend_from_slice(b"; filename=\"");
                        body.extend_from_slice(
                            file.name
                                .replace('\\', "\\\\")
                                .replace('"', "\\\"")
                                .as_bytes(),
                        );
                        body.extend_from_slice(b"\"\r\nContent-Type: ");
                        body.extend_from_slice(if file.media_type.is_empty() {
                            b"application/octet-stream".as_slice()
                        } else {
                            file.media_type.as_bytes()
                        });
                        body.extend_from_slice(b"\r\n\r\n");
                        body.extend_from_slice(&file.bytes);
                    }
                    NativeFormValue::EmptyFile => {
                        body.extend_from_slice(
                            b"; filename=\"\"\r\nContent-Type: application/octet-stream\r\n\r\n",
                        );
                    }
                }
                body.extend_from_slice(b"\r\n");
                if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
                    return Err(NativeEngineError::limit(
                        "form submission data",
                        MAX_NATIVE_FORM_BODY_BYTES,
                        body.len(),
                    ));
                }
            }
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"--\r\n");
            if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
                return Err(NativeEngineError::limit(
                    "form submission data",
                    MAX_NATIVE_FORM_BODY_BYTES,
                    body.len(),
                ));
            }
            Ok((
                NativeRequestBody::Bytes(body),
                format!("multipart/form-data; boundary={boundary}"),
            ))
        }
    }
}
