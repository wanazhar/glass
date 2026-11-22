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
    SECRETS.get_or_init(|| from_os_vars(std::env::vars_os()))
}

fn from_os_vars(vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> Secrets {
    from_vars(
        vars.filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?))),
    )
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
    let mut patterns = Vec::with_capacity(secrets.len() * 2);
    for (name, value) in secrets {
        let placeholder = format!("{OPEN}{name}}}}}");
        let escaped = serde_json::to_string(value).expect("string serialization");
        patterns.push((value.clone(), placeholder.clone()));
        patterns.push((escaped[1..escaped.len() - 1].to_owned(), placeholder));
    }
    patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.0.len()));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix(OPEN)
            && let Some(end) = after.find("}}")
            && !after[..end].is_empty()
            && after[..end].len() <= 64
            && after[..end]
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            let length = OPEN.len() + end + 2;
            out.push_str(&rest[..length]);
            rest = &rest[length..];
        } else if let Some((value, placeholder)) =
            patterns.iter().find(|(value, _)| rest.starts_with(value))
        {
            out.push_str(placeholder);
            rest = &rest[value.len()..];
        } else {
            let ch = rest.chars().next().expect("nonempty text");
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
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
    fn redaction_is_single_pass_and_preserves_reusable_placeholders() {
        let secrets = vec![
            ("PASSWORD".into(), "secret".into()),
            ("TOKEN".into(), "PASSWORD".into()),
        ];
        let expected = "{{secret:PASSWORD}} {{secret:TOKEN}} {{secret:PASSWORD}}";
        assert_eq!(
            redact_with(&secrets, "secret PASSWORD {{secret:PASSWORD}}"),
            expected
        );
        assert_eq!(redact_with(&secrets, expected), expected);
        assert_eq!(redact_with(&secrets, ""), "");
    }

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
