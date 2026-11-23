use serde::{Deserialize, Serialize};

pub const MESSAGE: &str = "Couldn't get a reply from the model. Try again.";

pub(crate) fn transient(error: &str) -> bool {
    matches!(
        FailureInfo::of(error).category.as_str(),
        "network" | "timeout"
    ) || ["HTTP 429", "HTTP 502", "HTTP 503", "HTTP 504"]
        .iter()
        .any(|status| error.contains(status))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureInfo {
    pub category: String,
    pub retryable: bool,
    pub diagnostics: String,
}

impl FailureInfo {
    pub fn of(error: &str) -> Self {
        let error = error.to_lowercase();
        let category = if error.contains("provider_rejected")
            || error.contains("[generation failed]")
            || error.contains("provider's requirements")
            || error.contains("rejected")
        {
            "provider_rejected"
        } else if error.contains("timed out") || error.contains("timeout") {
            "timeout"
        } else if ["network", "connection", "transport", "broken pipe", "eof"]
            .iter()
            .any(|word| error.contains(word))
        {
            "network"
        } else {
            "unknown"
        };
        Self {
            category: category.into(),
            retryable: true,
            diagnostics: format!(
                "Engine turn failed; category={category}. Provider response and request data omitted."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_categorize_without_retaining_provider_or_request_secrets() {
        for (raw, category) in [
            ("[Generation failed] secret-token", "provider_rejected"),
            ("connection failed token=secret-token", "network"),
            ("request timed out secret-token", "timeout"),
            ("", "unknown"),
            ("unexpected secret-token", "unknown"),
        ] {
            let failure = FailureInfo::of(raw);
            assert_eq!(failure.category, category);
            assert!(failure.retryable);
            assert!(
                !serde_json::to_string(&failure)
                    .unwrap()
                    .contains("secret-token")
            );
        }
    }
}
