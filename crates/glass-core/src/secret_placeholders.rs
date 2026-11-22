//! `{{secret:NAME}}` placeholders for desktop text entry. Values come from
//! `GLASS_SECRET_<NAME>` environment variables (the same ones glass-browser
//! reads), are substituted only into text that desktop_input and desktop_act
//! enter, and are replaced by their placeholder in every desktop tool result.

use serde_json::Value;
use std::sync::OnceLock;

const PREFIX: &str = "GLASS_SECRET_";
const OPEN: &str = "{{secret:";
/// Shorter values are ignored: redacting them would rewrite ordinary text.
const MIN_LEN: usize = 4;

type Secrets = Vec<(String, String)>;

fn from_vars(vars: impl Iterator<Item = (String, String)>) -> Secrets {
    let mut secrets: Secrets = vars
        .filter_map(|(key, value)| {
            let name = key.strip_prefix(PREFIX)?;
            let valid = (1..=64).contains(&name.len())
                && name
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
            (valid && value.chars().count() >= MIN_LEN).then(|| (name.to_owned(), value))
        })
        .collect();
    secrets.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    secrets
}

fn loaded() -> &'static Secrets {
    static SECRETS: OnceLock<Secrets> = OnceLock::new();
    SECRETS.get_or_init(|| from_vars(std::env::vars()))
}

fn names(secrets: &Secrets) -> Vec<&str> {
    let mut names: Vec<&str> = secrets.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    names
}

/// The sentence that tells the model which secrets it may type, if any.
pub fn description() -> Option<String> {
    let names = names(loaded());
    (!names.is_empty()).then(|| {
        format!(
            " For a password, token or other secret, send {{{{secret:NAME}}}} as the text instead of the value: the real value is entered and results show the placeholder. Secrets set: {}.",
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
            .ok_or_else(|| match names(secrets) {
                names if names.is_empty() => format!(
                    "no secret named {name}: no secrets are set. Ask the user to export {PREFIX}{name} before starting Glass, or ask them for the value."
                ),
                names => format!("no secret named {name}. Secrets set: {}.", names.join(", ")),
            })?;
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn inject_with(secrets: &Secrets, tool: &str, arguments: &mut Value) -> Result<(), String> {
    let types_text = match tool {
        "desktop_input" => arguments["action"] == "text",
        "desktop_act" => arguments["action"] == "set_text",
        _ => false,
    };
    if types_text && let Some(text) = arguments["text"].as_str() {
        arguments["text"] = Value::String(substitute(secrets, text)?);
    } else if arguments.to_string().contains(OPEN) {
        return Err(format!(
            "{tool} does not accept {{{{secret:NAME}}}} placeholders here; type secrets with desktop_act set_text or desktop_input text."
        ));
    }
    Ok(())
}

/// Replace placeholders in the text a desktop tool will enter.
pub fn inject(tool: &str, arguments: &mut Value) -> Result<(), String> {
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
pub fn redact(text: &str) -> String {
    redact_with(loaded(), text)
}

pub(crate) fn has_secrets() -> bool {
    !loaded().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialized_desktop_results_redact_escaped_credentials() {
        let secrets = vec![("QUOTED".into(), "pass\"word\\token\nend".into())];
        let result = json!({"text":secrets[0].1,"nested":[secrets[0].1]});
        let redacted = redact_with(&secrets, &result.to_string());
        let value: Value = serde_json::from_str(&redacted).unwrap();
        assert_eq!(value["text"], "{{secret:QUOTED}}");
        assert_eq!(value["nested"][0], "{{secret:QUOTED}}");
        assert_eq!(redact_with(&secrets, ""), "");
        assert_eq!(redact_with(&secrets, "ordinary"), "ordinary");
    }

    fn secrets() -> Secrets {
        from_vars(
            [
                ("GLASS_SECRET_VPN_PASSWORD", "s3cret-vpn"),
                ("GLASS_SECRET_PIN", "123"),
                ("GLASS_SECRET_bad", "ignored-value"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        )
    }

    #[test]
    fn desktop_text_entry_takes_placeholders_and_results_hide_values() {
        let secrets = secrets();
        assert_eq!(names(&secrets), ["VPN_PASSWORD"]);
        let mut input = json!({"action":"text","sessionId":"s","text":"{{secret:VPN_PASSWORD}}\n"});
        inject_with(&secrets, "desktop_input", &mut input).unwrap();
        assert_eq!(input["text"], "s3cret-vpn\n");
        let mut act = json!({"action":"set_text","text":"{{secret:VPN_PASSWORD}}"});
        inject_with(&secrets, "desktop_act", &mut act).unwrap();
        assert_eq!(act["text"], "s3cret-vpn");
        let mut clipboard = json!({"action":"write","text":"{{secret:VPN_PASSWORD}}"});
        let refused = inject_with(&secrets, "desktop_clipboard", &mut clipboard).unwrap_err();
        assert!(refused.contains("set_text"), "{refused}");
        let mut unknown = json!({"action":"text","text":"{{secret:BANK}}"});
        let unknown = inject_with(&secrets, "desktop_input", &mut unknown).unwrap_err();
        assert!(unknown.contains("Secrets set: VPN_PASSWORD"), "{unknown}");
        let mut none = json!({"action":"text","text":"{{secret:BANK}}"});
        let none = inject_with(&Vec::new(), "desktop_input", &mut none).unwrap_err();
        assert!(none.contains("GLASS_SECRET_BANK"), "{none}");
        assert_eq!(
            redact_with(&secrets, r#"{"text":"s3cret-vpn","pin":"123"}"#),
            r#"{"text":"{{secret:VPN_PASSWORD}}","pin":"123"}"#
        );
    }
}
