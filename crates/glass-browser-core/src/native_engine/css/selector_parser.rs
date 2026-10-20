use super::*;

pub(super) fn parse_selector(source: &str) -> Option<NativeSelector> {
    parse_selector_with_namespaces(source, &BTreeMap::new())
}

pub(super) fn parse_selector_with_namespaces(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<NativeSelector> {
    let source = source.trim();
    if source.is_empty() || source.len() > MAX_SELECTOR_BYTES {
        return None;
    }
    let (compound_sources, combinators) = split_selector_parts(source)?;
    if compound_sources.len() > MAX_SELECTOR_PARTS {
        return None;
    }
    let compounds = compound_sources
        .into_iter()
        .map(|compound| parse_compound_selector(compound, namespaces))
        .collect::<Option<Vec<_>>>()?;
    let specificity = compounds.iter().try_fold(0u32, |specificity, compound| {
        specificity.checked_add(u32::from(compound.specificity))
    })?;
    let specificity = u16::try_from(specificity).ok()?;
    if specificity > MAX_NATIVE_SELECTOR_SPECIFICITY {
        return None;
    }
    Some(NativeSelector {
        compounds,
        combinators,
        specificity,
    })
}

pub(super) fn split_selector_compounds(source: &str) -> Option<Vec<&str>> {
    split_selector_parts(source).map(|(compounds, _)| compounds)
}

pub(super) fn split_selector_parts(
    source: &str,
) -> Option<(Vec<&str>, Vec<NativeSelectorCombinator>)> {
    let bytes = source.as_bytes();
    let mut compounds = Vec::new();
    let mut combinators = Vec::new();
    let mut token_start = None;
    let mut pending_whitespace = false;
    let mut cursor = 0;
    let mut bracket_depth = 0usize;
    let mut parentheses = 0usize;
    let mut quote = None;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        if bracket_depth == 0
            && parentheses == 0
            && !byte.is_ascii_whitespace()
            && !matches!(byte, b'>' | b'+' | b'~')
            && token_start.is_none()
        {
            if !compounds.is_empty() && combinators.len() == compounds.len().saturating_sub(1) {
                if pending_whitespace {
                    combinators.push(NativeSelectorCombinator::Descendant);
                } else {
                    return None;
                }
            }
            token_start = Some(cursor);
            pending_whitespace = false;
        }
        match byte {
            b'\'' | b'"' if bracket_depth > 0 => quote = Some(byte),
            b'[' => bracket_depth = bracket_depth.saturating_add(1),
            b']' => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
            }
            b'(' if bracket_depth == 0 => parentheses = parentheses.saturating_add(1),
            b')' if bracket_depth == 0 => {
                parentheses = parentheses.checked_sub(1)?;
            }
            byte if byte.is_ascii_whitespace() && bracket_depth == 0 && parentheses == 0 => {
                if let Some(start) = token_start.take() {
                    compounds.push(&source[start..cursor]);
                    pending_whitespace = true;
                }
            }
            byte @ (b'>' | b'+' | b'~') if bracket_depth == 0 && parentheses == 0 => {
                if let Some(start) = token_start.take() {
                    compounds.push(&source[start..cursor]);
                }
                if compounds.is_empty() || combinators.len() != compounds.len().saturating_sub(1) {
                    return None;
                }
                combinators.push(match byte {
                    b'>' => NativeSelectorCombinator::Child,
                    b'+' => NativeSelectorCombinator::NextSibling,
                    b'~' => NativeSelectorCombinator::SubsequentSibling,
                    _ => unreachable!("selector combinator was matched above"),
                });
                pending_whitespace = false;
            }
            _ => {}
        }
        cursor += 1;
    }
    if bracket_depth != 0 || parentheses != 0 || quote.is_some() {
        return None;
    }
    if let Some(start) = token_start {
        compounds.push(&source[start..]);
    }
    (!compounds.is_empty() && combinators.len() + 1 == compounds.len())
        .then_some((compounds, combinators))
}

pub(super) fn parse_selector_namespace(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<NativeSelectorNamespace> {
    let source = source.to_ascii_lowercase();
    match source.as_str() {
        "html" => Some(NativeSelectorNamespace::Html),
        "svg" => Some(NativeSelectorNamespace::Svg),
        "math" | "mathml" => Some(NativeSelectorNamespace::MathMl),
        "xml" => Some(NativeSelectorNamespace::Xml),
        "xmlns" => Some(NativeSelectorNamespace::Xmlns),
        "xlink" => Some(NativeSelectorNamespace::Xlink),
        _ => namespaces.get(&source).copied(),
    }
}

pub(super) fn parse_namespace_local_name(
    source: &str,
    start: usize,
) -> Option<(Option<String>, usize)> {
    if source.as_bytes().get(start) == Some(&b'*') {
        return Some((None, start + 1));
    }
    let (name, next) = read_identifier(source, start)?;
    Some((Some(name.to_ascii_lowercase()), next))
}

pub(super) fn parse_qualified_type_selector(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<(Option<String>, NativeSelectorNamespace, usize)> {
    let bytes = source.as_bytes();
    let Some(&first) = bytes.first() else {
        return Some((None, NativeSelectorNamespace::Any, 0));
    };
    let default_namespace = namespaces
        .get("")
        .copied()
        .unwrap_or(NativeSelectorNamespace::Any);
    match first {
        b'*' if bytes.get(1) == Some(&b'|') => {
            let (tag, next) = parse_namespace_local_name(source, 2)?;
            Some((tag, NativeSelectorNamespace::Any, next))
        }
        b'*' => Some((None, default_namespace, 1)),
        b'|' => {
            let (tag, next) = parse_namespace_local_name(source, 1)?;
            Some((tag, NativeSelectorNamespace::NoNamespace, next))
        }
        byte if is_identifier_start(byte) => {
            let (prefix, next) = read_identifier(source, 0)?;
            if bytes.get(next) != Some(&b'|') {
                return Some((Some(prefix.to_ascii_lowercase()), default_namespace, next));
            }
            let namespace = parse_selector_namespace(&prefix, namespaces)?;
            let (tag, next) = parse_namespace_local_name(source, next + 1)?;
            Some((tag, namespace, next))
        }
        _ => Some((None, NativeSelectorNamespace::Any, 0)),
    }
}

pub(super) fn parse_compound_selector(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<NativeCompoundSelector> {
    let bytes = source.as_bytes();
    let (tag, tag_namespace, mut cursor) = parse_qualified_type_selector(source, namespaces)?;
    let mut selector = NativeCompoundSelector {
        tag,
        tag_namespace,
        id: None,
        classes: Vec::new(),
        attributes: Vec::new(),
        pseudo_classes: Vec::new(),
        specificity: 0,
    };
    if selector.tag.is_some() {
        selector.specificity = 1;
    }
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'#' => {
                if selector.id.is_some() {
                    return None;
                }
                let (id, next) = read_identifier(source, cursor + 1)?;
                selector.id = Some(id);
                selector.specificity = selector.specificity.saturating_add(100);
                cursor = next;
            }
            b'.' => {
                let (class, next) = read_identifier(source, cursor + 1)?;
                selector.classes.push(class);
                selector.specificity = selector.specificity.saturating_add(10);
                cursor = next;
            }
            b'[' => {
                let close = source[cursor + 1..].find(']')? + cursor + 1;
                let content = source[cursor + 1..close].trim();
                let (name, namespace, operator, value, case_sensitivity) =
                    parse_attribute_selector(content, namespaces)?;
                selector.attributes.push(NativeAttributeSelector {
                    name,
                    namespace,
                    operator,
                    value,
                    case_sensitivity,
                });
                selector.specificity = selector.specificity.saturating_add(10);
                cursor = close + 1;
            }
            b':' => {
                let (name, next) = read_identifier(source, cursor + 1)?;
                if bytes.get(next) == Some(&b'(') {
                    let close = find_selector_function_close(source, next)?;
                    let (pseudo, specificity) =
                        parse_functional_pseudo_class(&name, &source[next + 1..close], namespaces)?;
                    selector.pseudo_classes.push(pseudo);
                    selector.specificity = selector.specificity.saturating_add(specificity);
                    cursor = close + 1;
                } else {
                    selector.pseudo_classes.push(parse_pseudo_class(&name)?);
                    selector.specificity = selector.specificity.saturating_add(10);
                    cursor = next;
                }
            }
            _ => return None,
        }
    }
    (selector.tag.is_some()
        || selector.id.is_some()
        || !selector.classes.is_empty()
        || !selector.attributes.is_empty()
        || !selector.pseudo_classes.is_empty()
        || source == "*"
        || source.contains('|'))
    .then_some(selector)
}

pub(super) fn parse_pseudo_class(source: &str) -> Option<NativePseudoClass> {
    match source.to_ascii_lowercase().as_str() {
        "root" => Some(NativePseudoClass::Root),
        "scope" => Some(NativePseudoClass::Scope),
        "first-child" => Some(NativePseudoClass::FirstChild),
        "last-child" => Some(NativePseudoClass::LastChild),
        "only-child" => Some(NativePseudoClass::OnlyChild),
        "first-of-type" => Some(NativePseudoClass::FirstOfType),
        "last-of-type" => Some(NativePseudoClass::LastOfType),
        "only-of-type" => Some(NativePseudoClass::OnlyOfType),
        "empty" => Some(NativePseudoClass::Empty),
        "checked" => Some(NativePseudoClass::Checked),
        "disabled" => Some(NativePseudoClass::Disabled),
        "enabled" => Some(NativePseudoClass::Enabled),
        "required" => Some(NativePseudoClass::Required),
        "optional" => Some(NativePseudoClass::Optional),
        "focus" => Some(NativePseudoClass::Focus),
        "focus-within" => Some(NativePseudoClass::FocusWithin),
        "focus-visible" => Some(NativePseudoClass::FocusVisible),
        "hover" => Some(NativePseudoClass::Hover),
        "valid" => Some(NativePseudoClass::Valid),
        "invalid" => Some(NativePseudoClass::Invalid),
        "user-valid" => Some(NativePseudoClass::UserValid),
        "read-write" => Some(NativePseudoClass::ReadWrite),
        "placeholder-shown" => Some(NativePseudoClass::PlaceholderShown),
        "user-invalid" => Some(NativePseudoClass::UserInvalid),
        "read-only" => Some(NativePseudoClass::ReadOnly),
        "default" => Some(NativePseudoClass::Default),
        "in-range" => Some(NativePseudoClass::InRange),
        "out-of-range" => Some(NativePseudoClass::OutOfRange),
        "indeterminate" => Some(NativePseudoClass::Indeterminate),
        "target" => Some(NativePseudoClass::Target),
        "link" => Some(NativePseudoClass::Link),
        "any-link" => Some(NativePseudoClass::AnyLink),
        _ => None,
    }
}

pub(super) fn parse_functional_pseudo_class(
    name: &str,
    argument: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<(NativePseudoClass, u16)> {
    if argument.len() > MAX_SELECTOR_BYTES {
        return None;
    }
    let name = name.to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type"
    ) {
        let (formula, of_source) = split_nth_of_clause(argument)?;
        let (a, b) = parse_nth_formula(formula)?;
        let of_selectors = match of_source {
            Some(source) => {
                if matches!(name.as_str(), "nth-of-type" | "nth-last-of-type") {
                    return None;
                }
                Some(
                    split_selector_list(source)?
                        .into_iter()
                        .map(|selector| parse_selector_with_namespaces(selector, namespaces))
                        .collect::<Option<Vec<_>>>()?,
                )
            }
            None => None,
        };
        let specificity = 10u16.saturating_add(
            of_selectors
                .as_ref()
                .and_then(|selectors| selectors.iter().map(|selector| selector.specificity).max())
                .unwrap_or_default(),
        );
        let pseudo = match name.as_str() {
            "nth-child" => NativePseudoClass::NthChild {
                a,
                b,
                of: of_selectors,
            },
            "nth-last-child" => NativePseudoClass::NthLastChild {
                a,
                b,
                of: of_selectors,
            },
            "nth-of-type" => NativePseudoClass::NthOfType { a, b, of: None },
            "nth-last-of-type" => NativePseudoClass::NthLastOfType { a, b, of: None },
            _ => return None,
        };
        return Some((pseudo, specificity));
    }
    if name == "lang" {
        return Some((NativePseudoClass::Lang(parse_language_range(argument)?), 10));
    }
    if name == "dir" {
        return Some((
            NativePseudoClass::Dir(parse_direction(argument.trim())?),
            10,
        ));
    }
    if name == "has" {
        let selectors = split_selector_list(argument)?
            .into_iter()
            .map(|selector| parse_relative_selector(selector, namespaces))
            .collect::<Option<Vec<_>>>()?;
        let specificity = selectors
            .iter()
            .map(|relative| relative.selector.specificity)
            .max()
            .unwrap_or_default();
        return Some((NativePseudoClass::Has(selectors), specificity));
    }
    let selectors = split_selector_list(argument)?
        .into_iter()
        .map(|selector| parse_selector_with_namespaces(selector, namespaces))
        .collect::<Option<Vec<_>>>()?;
    let specificity = selectors
        .iter()
        .map(|selector| selector.specificity)
        .max()
        .unwrap_or_default();
    match name.as_str() {
        "not" => Some((NativePseudoClass::Not(selectors), specificity)),
        "is" => Some((NativePseudoClass::Is(selectors), specificity)),
        "where" => Some((NativePseudoClass::Where(selectors), 0)),
        _ => None,
    }
}

pub(super) fn parse_language_range(argument: &str) -> Option<String> {
    let argument = argument.trim();
    if argument.is_empty() {
        return None;
    }
    let range = if argument.len() >= 2
        && matches!(
            (argument.as_bytes().first(), argument.as_bytes().last()),
            (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"'))
        ) {
        &argument[1..argument.len() - 1]
    } else {
        argument
    };
    let range = range.to_ascii_lowercase();
    if range.is_empty()
        || !range
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'*')
    {
        return None;
    }
    if range != "*" && range.split('-').any(str::is_empty) {
        return None;
    }
    if range.contains('*') && range != "*" && !range.ends_with("-*") {
        return None;
    }
    Some(range)
}

pub(super) fn split_nth_of_clause(source: &str) -> Option<(&str, Option<&str>)> {
    let bytes = source.as_bytes();
    let mut bracket_depth = 0usize;
    let mut parentheses = 0usize;
    let mut quote = None;
    for cursor in 0..bytes.len() {
        let byte = bytes[cursor];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            continue;
        }
        if bracket_depth == 0
            && parentheses == 0
            && cursor + 1 < bytes.len()
            && bytes[cursor..cursor + 2].eq_ignore_ascii_case(b"of")
            && (cursor == 0 || bytes[cursor - 1].is_ascii_whitespace())
            && (cursor + 2 == bytes.len() || bytes[cursor + 2].is_ascii_whitespace())
        {
            let formula = source[..cursor].trim();
            let selector_source = source[cursor + 2..].trim();
            if formula.is_empty() || selector_source.is_empty() {
                return None;
            }
            return Some((formula, Some(selector_source)));
        }
        match byte {
            b'\'' | b'"' if bracket_depth > 0 => quote = Some(byte),
            b'[' => bracket_depth = bracket_depth.saturating_add(1),
            b']' => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
            }
            b'(' if bracket_depth == 0 => parentheses = parentheses.saturating_add(1),
            b')' if bracket_depth == 0 => {
                parentheses = parentheses.checked_sub(1)?;
            }
            _ => {}
        }
    }
    (bracket_depth == 0 && parentheses == 0 && quote.is_none()).then_some((source.trim(), None))
}

pub(super) fn parse_nth_formula(argument: &str) -> Option<(i32, i32)> {
    let argument = argument.trim();
    if argument.eq_ignore_ascii_case("odd") {
        return Some((2, 1));
    }
    if argument.eq_ignore_ascii_case("even") {
        return Some((2, 0));
    }
    let mut compact = String::with_capacity(argument.len());
    for byte in argument.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let byte = byte.to_ascii_lowercase();
        if !matches!(byte, b'+' | b'-' | b'n' | b'0'..=b'9') {
            return None;
        }
        compact.push(char::from(byte));
    }
    if compact.is_empty() {
        return None;
    }
    let Some(n_index) = compact.find('n') else {
        return compact.parse::<i32>().ok().map(|b| (0, b));
    };
    if compact[n_index + 1..].contains('n') {
        return None;
    }
    let coefficient = &compact[..n_index];
    let remainder = &compact[n_index + 1..];
    let a = match coefficient {
        "" | "+" => 1,
        "-" => -1,
        coefficient => coefficient.parse::<i32>().ok()?,
    };
    if matches!(remainder, "+" | "-") {
        return None;
    }
    let b = if remainder.is_empty() {
        0
    } else {
        remainder.parse::<i32>().ok()?
    };
    Some((a, b))
}

pub(super) fn parse_relative_selector(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<NativeRelativeSelector> {
    let source = source.trim_start();
    let (combinator, selector_source) = match source.as_bytes().first().copied() {
        Some(b'>') => (NativeSelectorCombinator::Child, &source[1..]),
        Some(b'+') => (NativeSelectorCombinator::NextSibling, &source[1..]),
        Some(b'~') => (NativeSelectorCombinator::SubsequentSibling, &source[1..]),
        _ => (NativeSelectorCombinator::Descendant, source),
    };
    let selector = parse_selector_with_namespaces(selector_source.trim(), namespaces)?;
    Some(NativeRelativeSelector {
        combinator,
        selector,
    })
}

pub(super) fn find_selector_function_close(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut quote = None;
    for (cursor, byte) in bytes.iter().copied().enumerate().skip(open) {
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' if bracket_depth > 0 => quote = Some(byte),
            b'[' => bracket_depth = bracket_depth.saturating_add(1),
            b']' => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
            }
            b'(' if bracket_depth == 0 => depth = depth.saturating_add(1),
            b')' if bracket_depth == 0 => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn split_selector_list(source: &str) -> Option<Vec<&str>> {
    split_selector_list_with_offsets(source).map(|selectors| {
        selectors
            .into_iter()
            .map(|(_, selector)| selector)
            .collect()
    })
}

pub(super) fn split_selector_list_with_offsets(source: &str) -> Option<Vec<(usize, &str)>> {
    let mut selectors = Vec::new();
    let mut start = 0;
    let mut bracket_depth = 0usize;
    let mut parentheses = 0usize;
    let mut quote = None;
    for (cursor, byte) in source.bytes().enumerate() {
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' if bracket_depth > 0 => quote = Some(byte),
            b'[' => bracket_depth = bracket_depth.saturating_add(1),
            b']' => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
            }
            b'(' if bracket_depth == 0 => parentheses = parentheses.saturating_add(1),
            b')' if bracket_depth == 0 => {
                parentheses = parentheses.checked_sub(1)?;
            }
            b',' if bracket_depth == 0 && parentheses == 0 => {
                let raw_selector = &source[start..cursor];
                let leading = raw_selector
                    .len()
                    .saturating_sub(raw_selector.trim_start().len());
                let selector = raw_selector.trim();
                if selector.is_empty() {
                    return None;
                }
                selectors.push((start.saturating_add(leading), selector));
                if selectors.len() > MAX_SELECTOR_PARTS {
                    return None;
                }
                start = cursor.saturating_add(1);
            }
            _ => {}
        }
    }
    if bracket_depth != 0 || parentheses != 0 || quote.is_some() {
        return None;
    }
    let raw_selector = &source[start..];
    let leading = raw_selector
        .len()
        .saturating_sub(raw_selector.trim_start().len());
    let selector = raw_selector.trim();
    if selector.is_empty() {
        return None;
    }
    selectors.push((start.saturating_add(leading), selector));
    (selectors.len() <= MAX_SELECTOR_PARTS).then_some(selectors)
}

pub(super) fn split_attribute_case_modifier(
    source: &str,
) -> (&str, NativeAttributeCaseSensitivity) {
    let source = source.trim();
    let Some(separator) = source
        .char_indices()
        .rev()
        .find_map(|(index, character)| character.is_ascii_whitespace().then_some(index))
    else {
        return (source, NativeAttributeCaseSensitivity::Default);
    };
    let modifier = source[separator..].trim();
    let case_sensitivity = if modifier.eq_ignore_ascii_case("i") {
        NativeAttributeCaseSensitivity::Insensitive
    } else if modifier.eq_ignore_ascii_case("s") {
        NativeAttributeCaseSensitivity::Sensitive
    } else {
        return (source, NativeAttributeCaseSensitivity::Default);
    };
    (source[..separator].trim_end(), case_sensitivity)
}

pub(super) fn attribute_namespace_from_selector_namespace(
    namespace: NativeSelectorNamespace,
) -> Option<NativeAttributeNamespace> {
    match namespace {
        NativeSelectorNamespace::Any => Some(NativeAttributeNamespace::Any),
        NativeSelectorNamespace::NoNamespace => Some(NativeAttributeNamespace::NoNamespace),
        NativeSelectorNamespace::Html => Some(NativeAttributeNamespace::Html),
        NativeSelectorNamespace::Svg => Some(NativeAttributeNamespace::Svg),
        NativeSelectorNamespace::MathMl => Some(NativeAttributeNamespace::MathMl),
        NativeSelectorNamespace::Xml => Some(NativeAttributeNamespace::Xml),
        NativeSelectorNamespace::Xmlns => Some(NativeAttributeNamespace::Xmlns),
        NativeSelectorNamespace::Xlink => Some(NativeAttributeNamespace::Xlink),
    }
}

pub(super) fn parse_attribute_namespace(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<NativeAttributeNamespace> {
    let source = source.to_ascii_lowercase();
    match source.as_str() {
        "html" => Some(NativeAttributeNamespace::Html),
        "svg" => Some(NativeAttributeNamespace::Svg),
        "math" | "mathml" => Some(NativeAttributeNamespace::MathMl),
        "xml" => Some(NativeAttributeNamespace::Xml),
        "xmlns" => Some(NativeAttributeNamespace::Xmlns),
        "xlink" => Some(NativeAttributeNamespace::Xlink),
        _ => namespaces
            .get(&source)
            .copied()
            .and_then(attribute_namespace_from_selector_namespace),
    }
}

pub(super) fn parse_qualified_attribute_name(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<(String, NativeAttributeNamespace)> {
    let source = source.trim();
    let Some(separator) = source.find('|') else {
        let (name, next) = read_identifier(source, 0)?;
        return (next == source.len()).then_some((
            name.to_ascii_lowercase(),
            NativeAttributeNamespace::Unqualified,
        ));
    };
    let namespace = match &source[..separator] {
        "" => NativeAttributeNamespace::NoNamespace,
        "*" => NativeAttributeNamespace::Any,
        prefix => parse_attribute_namespace(prefix, namespaces)?,
    };
    let local_name = &source[separator + 1..];
    let (name, next) = read_identifier(local_name, 0)?;
    (next == local_name.len()).then_some((name.to_ascii_lowercase(), namespace))
}

pub(super) fn parse_attribute_selector(
    source: &str,
    namespaces: &BTreeMap<String, NativeSelectorNamespace>,
) -> Option<(
    String,
    NativeAttributeNamespace,
    NativeAttributeOperator,
    Option<String>,
    NativeAttributeCaseSensitivity,
)> {
    let source = source.trim();
    let (raw_name, operator, raw_value) =
        source
            .find('=')
            .map_or((source, NativeAttributeOperator::Exists, None), |equals| {
                let raw_name = source[..equals].trim();
                let (raw_name, operator) = match raw_name.as_bytes().last().copied() {
                    Some(b'~') => (
                        &raw_name[..raw_name.len().saturating_sub(1)],
                        NativeAttributeOperator::Includes,
                    ),
                    Some(b'|') => (
                        &raw_name[..raw_name.len().saturating_sub(1)],
                        NativeAttributeOperator::DashMatch,
                    ),
                    Some(b'^') => (
                        &raw_name[..raw_name.len().saturating_sub(1)],
                        NativeAttributeOperator::Prefix,
                    ),
                    Some(b'$') => (
                        &raw_name[..raw_name.len().saturating_sub(1)],
                        NativeAttributeOperator::Suffix,
                    ),
                    Some(b'*') => (
                        &raw_name[..raw_name.len().saturating_sub(1)],
                        NativeAttributeOperator::Substring,
                    ),
                    _ => (raw_name, NativeAttributeOperator::Equals),
                };
                (raw_name, operator, Some(source[equals + 1..].trim()))
            });
    let (name, namespace) = parse_qualified_attribute_name(raw_name, namespaces)?;
    let (raw_value, case_sensitivity) =
        raw_value.map_or((None, NativeAttributeCaseSensitivity::Default), |value| {
            let (value, case_sensitivity) = split_attribute_case_modifier(value);
            (Some(value), case_sensitivity)
        });
    let value = match raw_value {
        Some(value)
            if value.len() >= 2
                && matches!(
                    (value.as_bytes().first(), value.as_bytes().last()),
                    (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\''))
                ) =>
        {
            Some(value[1..value.len() - 1].to_owned())
        }
        Some(value)
            if value.is_empty()
                || value.chars().any(char::is_whitespace)
                || value.contains(['"', '\'', '=']) =>
        {
            return None;
        }
        Some(value) => Some(value.to_owned()),
        None => None,
    };
    if !matches!(
        operator,
        NativeAttributeOperator::Exists | NativeAttributeOperator::Equals
    ) && value.as_deref().is_none_or(|value| value.is_empty())
    {
        return None;
    }
    Some((name, namespace, operator, value, case_sensitivity))
}

pub(super) fn read_identifier(source: &str, start: usize) -> Option<(String, usize)> {
    if start >= source.len() || !is_identifier_start(source.as_bytes()[start]) {
        return None;
    }
    let mut end = start + 1;
    while end < source.len() && is_identifier_char(source.as_bytes()[end]) {
        end += 1;
    }
    Some((source[start..end].to_owned(), end))
}

pub(super) fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'-')
}

pub(super) fn is_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

pub(super) fn strip_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find("/*") {
        let start = cursor + relative_start;
        output.push_str(&source[cursor..start]);
        let Some(relative_end) = source[start + 2..].find("*/") else {
            return output;
        };
        cursor = start + 2 + relative_end + 2;
    }
    output.push_str(&source[cursor..]);
    output
}
