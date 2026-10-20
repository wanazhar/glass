//! DOM action locator parsing and role helpers.

use super::*;

#[derive(Debug)]
pub(super) enum NativeLocator<'a> {
    Reference {
        revision: u64,
        index: u32,
    },
    Id(&'a str),
    Role {
        role: &'a str,
        name: Option<&'a str>,
    },
    Name(&'a str),
    Text(&'a str),
    Css(&'a str),
}

pub(super) fn parse_locator(locator: &str) -> Result<NativeLocator<'_>, NativeEngineError> {
    if locator.is_empty() {
        return Err(NativeEngineError::invalid(
            "action locator",
            "must not be empty",
        ));
    }
    if locator.len() > MAX_LOCATOR_BYTES {
        return Err(NativeEngineError::limit(
            "action locator",
            MAX_LOCATOR_BYTES,
            locator.len(),
        ));
    }
    if let Some(value) = locator.strip_prefix("ref=") {
        let Some(value) = value.strip_prefix('r') else {
            return Err(NativeEngineError::invalid(
                "action locator",
                "reference must use ref=r<revision>:n<arena-index>",
            ));
        };
        let Some((revision, index)) = value.split_once(":n") else {
            return Err(NativeEngineError::invalid(
                "action locator",
                "reference must use ref=r<revision>:n<arena-index>",
            ));
        };
        if revision.is_empty() || index.is_empty() || index.contains(":n") {
            return Err(NativeEngineError::invalid(
                "action locator",
                "reference must use ref=r<revision>:n<arena-index>",
            ));
        }
        let revision = revision.parse::<u64>().map_err(|_| {
            NativeEngineError::invalid(
                "action locator",
                "reference revision must be an unsigned integer",
            )
        })?;
        let index = index.parse::<u32>().map_err(|_| {
            NativeEngineError::invalid(
                "action locator",
                "reference node index must be an unsigned integer",
            )
        })?;
        return Ok(NativeLocator::Reference { revision, index });
    }
    if let Some(value) = locator.strip_prefix("id=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Id(value))
            .ok_or_else(|| NativeEngineError::invalid("action locator", "id must not be empty"));
    }
    if let Some(value) = locator.strip_prefix("role=") {
        if let Some((role, name)) = value.split_once("[name=") {
            if !name.ends_with(']') || name.len() == 1 || role.is_empty() {
                return Err(NativeEngineError::invalid(
                    "action locator",
                    "role/name locator is malformed",
                ));
            }
            let name = &name[..name.len() - 1];
            if name.is_empty() || !SUPPORTED_ROLES.contains(&role) {
                return Err(NativeEngineError::invalid(
                    "action locator",
                    "role/name locator uses an unsupported or empty value",
                ));
            }
            return Ok(NativeLocator::Role {
                role,
                name: Some(name),
            });
        }
        if value.is_empty() || !SUPPORTED_ROLES.contains(&value) {
            return Err(NativeEngineError::invalid(
                "action locator",
                "role locator uses an unsupported or empty role",
            ));
        }
        return Ok(NativeLocator::Role {
            role: value,
            name: None,
        });
    }
    if let Some(value) = locator.strip_prefix("name=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Name(value))
            .ok_or_else(|| NativeEngineError::invalid("action locator", "name must not be empty"));
    }
    if let Some(value) = locator.strip_prefix("text=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Text(value))
            .ok_or_else(|| NativeEngineError::invalid("action locator", "text must not be empty"));
    }
    if let Some(value) = locator.strip_prefix("css=") {
        return (!value.is_empty())
            .then_some(NativeLocator::Css(value))
            .ok_or_else(|| {
                NativeEngineError::invalid("action locator", "CSS selector must not be empty")
            });
    }
    Err(NativeEngineError::invalid(
        "action locator",
        "use ref=, id=, role=, name=, text=, or css=",
    ))
}

pub(super) fn unique_match(matches: Vec<NativeNodeId>) -> Result<NativeNodeId, NativeEngineError> {
    match matches.as_slice() {
        [] => Err(NativeEngineError::TargetNotFound),
        [id] => Ok(*id),
        _ => Err(NativeEngineError::AmbiguousTarget {
            matches: matches.len(),
        }),
    }
}

pub(super) fn supported_role(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();
    SUPPORTED_ROLES
        .iter()
        .copied()
        .find(|role| *role == normalized)
}

pub(super) fn has_hidden_signal(attributes: &BTreeMap<String, String>) -> bool {
    if attributes.contains_key("hidden")
        || attributes
            .get("aria-hidden")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        return true;
    }
    false
}
