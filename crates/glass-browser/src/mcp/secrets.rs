//! `{{secret:NAME}}` placeholders for the agent profile. Values come from
//! `GLASS_SECRET_<NAME>` environment variables, are substituted only into the
//! text that `browser_type` and `browser_fill_form` enter, and are replaced by
//! their placeholder in every agent tool result.

use serde_json::Value;
use std::sync::OnceLock;

const PREFIX: &str = "GLASS_SECRET_";
const OPEN: &str = "{{secret:";
/// Shorter values are ignored: redacting them would rewrite ordinary text.
const MIN_LEN: usize = 4;

type Secrets = Vec<(String, String)>;

fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn from_vars(vars: impl Iterator<Item = (String, String)>) -> Secrets {
    let mut secrets: Secrets = vars
        .filter_map(|(key, value)| {
            let name = key.strip_prefix(PREFIX)?;
            (valid_name(name) && value.chars().count() >= MIN_LEN).then(|| (name.to_owned(), value))
        })
        .collect();
    // Longest values first, so a secret that contains another is redacted whole.
    secrets.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    secrets
}

fn loaded() -> &'static Secrets {
    static SECRETS: OnceLock<Secrets> = OnceLock::new();
    SECRETS.get_or_init(|| from_os_vars(std::env::vars_os()))
}

fn from_os_vars(vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> Secrets {
    from_vars(
        vars.filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?))),
    )
}

fn sorted_names(secrets: &Secrets) -> Vec<&str> {
    let mut names: Vec<&str> = secrets.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    names
}

/// The sentence that tells the model which secrets it may type, if any.
pub(crate) fn description() -> Option<String> {
    let names = sorted_names(loaded());
    (!names.is_empty()).then(|| {
        format!(
            "For a password, token or other secret, send {{{{secret:NAME}}}} instead of the value: the real value is entered and every result shows the placeholder. Secrets set: {}.",
            names.join(", ")
        )
    })
}

fn substitute(secrets: &Secrets, text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let end = after
            .find("}}")
            .ok_or("a {{secret:NAME}} placeholder is missing its closing }}")?;
        let name = &after[..end];
        let value = secrets
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value)
            .ok_or_else(|| {
                let names = sorted_names(secrets);
                if names.is_empty() {
                    format!(
                        "no secret named {name}: no secrets are set. Ask the user to export {PREFIX}{name} before starting Glass, or ask them for the value."
                    )
                } else {
                    format!(
                        "no secret named {name}. Secrets set: {}.",
                        names.join(", ")
                    )
                }
            })?;
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn inject_with(secrets: &Secrets, tool: &str, arguments: &Value) -> Result<Value, String> {
    let mut arguments = arguments.clone();
    match tool {
        "browser_type" => {
            if let Some(text) = arguments["text"].as_str() {
                arguments["text"] = Value::String(substitute(secrets, text)?);
            }
        }
        "browser_fill_form" => {
            if let Some(fields) = arguments["fields"].as_array_mut() {
                for field in fields {
                    if let Some(value) = field["value"].as_str() {
                        field["value"] = Value::String(substitute(secrets, value)?);
                    }
                }
            }
        }
        _ if arguments.to_string().contains(OPEN) => {
            return Err(format!(
                "{tool} does not accept {{{{secret:NAME}}}} placeholders; type secrets with browser_type or browser_fill_form."
            ));
        }
        _ => {}
    }
    Ok(arguments)
}

/// Replace placeholders in the text an agent tool will enter.
pub(crate) fn inject(tool: &str, arguments: &Value) -> Result<Value, String> {
    if loaded().is_empty() && !arguments.to_string().contains(OPEN) {
        return Ok(arguments.clone());
    }
    inject_with(loaded(), tool, arguments)
}

fn redact_with(secrets: &Secrets, text: &str) -> String {
    let mut text = text.to_owned();
    for (name, value) in secrets {
        let placeholder = format!("{OPEN}{name}}}}}");
        let escaped = serde_json::to_string(value).expect("string serialization");
        text = text.replace(&escaped[1..escaped.len() - 1], &placeholder);
        text = text.replace(value.as_str(), &placeholder);
    }
    text
}

/// Replace every secret value in a tool result with its placeholder.
pub(crate) fn redact(text: &str) -> String {
    redact_with(loaded(), text)
}

pub(crate) fn redact_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = redact(text),
        Value::Array(values) => values.iter_mut().for_each(redact_value),
        Value::Object(object) => {
            let original = std::mem::take(object);
            for (key, mut value) in original {
                redact_value(&mut value);
                object.insert(redact(&key), value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn non_utf8_environment_entries_are_ignored_without_losing_valid_secrets() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let invalid = OsString::from_vec(vec![0xff]);
        let vars = vec![
            (invalid.clone(), "unrelated".into()),
            ("UNRELATED".into(), invalid.clone()),
            ("GLASS_SECRET_INVALID".into(), invalid),
            ("GLASS_SECRET_VALID".into(), "valid-value".into()),
        ];
        let secrets = from_os_vars(vars.into_iter());
        assert_eq!(
            secrets,
            vec![("VALID".to_owned(), "valid-value".to_owned())]
        );
        assert!(from_os_vars(std::iter::empty()).is_empty());
    }

    fn secrets() -> Secrets {
        from_vars(
            [
                ("GLASS_SECRET_SITE_PASSWORD", "hunter2-long"),
                ("GLASS_SECRET_PIN", "123"),
                ("GLASS_SECRET_lower", "ignored-value"),
                ("GLASS_SECRET_API_TOKEN", "tok_abc"),
                ("HOME", "/home/user"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        )
    }

    #[test]
    fn only_well_named_long_enough_secrets_load() {
        assert_eq!(sorted_names(&secrets()), ["API_TOKEN", "SITE_PASSWORD"]);
    }

    #[test]
    fn placeholders_fill_typed_text_and_form_values_only() {
        let secrets = secrets();
        let typed = inject_with(
            &secrets,
            "browser_type",
            &json!({"target":"name=Password","text":"{{secret:SITE_PASSWORD}}"}),
        )
        .unwrap();
        assert_eq!(typed["text"], "hunter2-long");
        let form = inject_with(
            &secrets,
            "browser_fill_form",
            &json!({"fields":[{"target":"name=User","value":"ada"},{"target":"name=Token","value":"Bearer {{secret:API_TOKEN}}"}]}),
        )
        .unwrap();
        assert_eq!(form["fields"][0]["value"], "ada");
        assert_eq!(form["fields"][1]["value"], "Bearer tok_abc");
        let navigate = inject_with(
            &secrets,
            "browser_navigate",
            &json!({"url":"https://x.test/?k={{secret:API_TOKEN}}"}),
        )
        .unwrap_err();
        assert!(navigate.contains("browser_type"), "{navigate}");
    }

    #[test]
    fn unknown_or_malformed_placeholders_name_the_fix() {
        let unknown = substitute(&secrets(), "{{secret:BANK}}").unwrap_err();
        assert!(unknown.contains("API_TOKEN, SITE_PASSWORD"), "{unknown}");
        let none = substitute(&Vec::new(), "{{secret:BANK}}").unwrap_err();
        assert!(none.contains("GLASS_SECRET_BANK"), "{none}");
        assert!(
            substitute(&secrets(), "{{secret:BANK")
                .unwrap_err()
                .contains("}}")
        );
    }

    #[test]
    fn results_show_placeholders_instead_of_values() {
        let text = "Signed in with hunter2-long; token tok_abc; pin 123";
        assert_eq!(
            redact_with(&secrets(), text),
            "Signed in with {{secret:SITE_PASSWORD}}; token {{secret:API_TOKEN}}; pin 123"
        );
    }

    #[test]
    fn escaped_json_strings_are_redacted() {
        let secrets = vec![("QUOTED".into(), "pass\"word\\token\nend".into())];
        let text = json!({"text":secrets[0].1}).to_string();
        let redacted = redact_with(&secrets, &text);
        assert_eq!(
            serde_json::from_str::<Value>(&redacted).unwrap()["text"],
            "{{secret:QUOTED}}"
        );
    }
}
