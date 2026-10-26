//! Compact, width-aware output for human-facing command-line reports.
//!
//! Protocol, JSON, and piped output should bypass this module. Callers supply
//! structured labels and values; this renderer only changes presentation.

use crate::{
    markdown, text,
    theme::{self, Tone},
};

/// Render a report with a product heading, one status line, and labeled rows.
/// Values are sanitized and wrapped to terminal cell width.
pub fn report(
    product: &str,
    title: &str,
    status: &str,
    rows: &[(&str, String)],
    width: usize,
    color: bool,
) -> String {
    let width = width.max(24);
    let mut output = heading(product, title, width, color);
    let status_tone = match status.to_ascii_lowercase().as_str() {
        "ready" | "ok" | "created" | "deleted" | "installed" | "updated" => Tone::Success,
        "degraded" | "warning" | "notice" => Tone::Warning,
        "error" | "failed" => Tone::Error,
        _ => Tone::Body,
    };
    row(
        &mut output,
        &status.to_uppercase(),
        width,
        status_tone,
        color,
        0,
    );
    if !rows.is_empty() {
        output.push('\n');
    }

    let label_width = rows
        .iter()
        .map(|(label, _)| text::width(&markdown::sanitize(label)))
        .max()
        .unwrap_or(0)
        .min(width / 3)
        .max(1);
    for (label, value) in rows {
        let label = markdown::sanitize(label);
        let value = markdown::sanitize(value);
        if width >= 72 && text::width(&label) <= label_width && label_width + 6 < width {
            let indent = 2;
            let value_indent = indent + label_width + 2;
            let available = width.saturating_sub(value_indent).max(1);
            let wrapped = text::wrap_words(&value, available);
            push_colored(&mut output, "  ", Tone::Body, color);
            push_colored(&mut output, &label, Tone::Secondary, color);
            output.push_str(&" ".repeat(label_width.saturating_sub(text::width(&label)) + 2));
            push_colored(
                &mut output,
                wrapped.first().map(String::as_str).unwrap_or(""),
                Tone::Body,
                color,
            );
            output.push('\n');
            for line in wrapped.iter().skip(1) {
                row(&mut output, line, width, Tone::Body, color, value_indent);
            }
        } else {
            row(&mut output, &label, width, Tone::Secondary, color, 2);
            row(&mut output, &value, width, Tone::Body, color, 4);
        }
    }
    output
}

/// Render the shared product heading and divider used by command reports.
pub fn heading(product: &str, title: &str, width: usize, color: bool) -> String {
    let width = width.max(1);
    let mut output = String::new();
    row(
        &mut output,
        &format!("{product} · {title}"),
        width,
        Tone::Accent,
        color,
        0,
    );
    row(
        &mut output,
        &"─".repeat(width.min(100)),
        width,
        Tone::Border,
        color,
        0,
    );
    output
}

/// Render an actionable human error for an interactive terminal.
pub fn error(product: &str, message: &str, width: usize, color: bool) -> String {
    report(
        product,
        "Error",
        "error",
        &[("Details", message.to_string())],
        width,
        color,
    )
}

fn row(output: &mut String, value: &str, width: usize, tone: Tone, color: bool, indent: usize) {
    let indent = indent.min(width.saturating_sub(1));
    let cleaned = markdown::sanitize(value);
    for line in text::wrap_words(&cleaned, width.saturating_sub(indent).max(1)) {
        output.push_str(&" ".repeat(indent));
        push_colored(output, &line, tone, color);
        output.push('\n');
    }
}

fn push_colored(output: &mut String, value: &str, tone: Tone, color: bool) {
    if color {
        output.push_str(&theme::ansi(tone));
    }
    output.push_str(value);
    if color {
        output.push_str("\x1b[0m");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_wrap_long_values_and_keep_pipe_mode_plain() {
        let rendered = report(
            "Glass",
            "Doctor",
            "ready",
            &[
                (
                    "Browser",
                    "Chromium is available at /opt/test/chrome".into(),
                ),
                ("MCP", "initialized".into()),
            ],
            58,
            false,
        );
        assert!(rendered.starts_with("Glass · Doctor\n"));
        assert!(rendered.contains("READY"));
        assert!(rendered.contains("/opt/test/chrome"));
        assert!(!rendered.contains("\x1b["));
        assert!(rendered.lines().all(|line| text::width(line) <= 58));
    }

    #[test]
    fn reports_keep_status_labels_and_respect_disabled_color() {
        let ready = report("Glass", "Doctor", "ready", &[], 80, true);
        let degraded = report("Glass", "Doctor", "degraded", &[], 80, true);
        assert!(ready.contains("READY"));
        assert!(degraded.contains("DEGRADED"));
        assert!(!report("Glass", "Doctor", "ready", &[], 80, false).contains("\x1b["));
    }

    #[test]
    fn errors_wrap_and_strip_terminal_controls() {
        let rendered = error(
            "Glass",
            "Browser failed\u{1b}[31m; run glass doctor to inspect support",
            58,
            false,
        );
        assert!(rendered.contains("ERROR"));
        assert!(rendered.contains("Browser failed; run glass doctor"));
        assert!(!rendered.contains("\x1b[31m"));
        assert!(rendered.lines().all(|line| text::width(line) <= 58));
    }
}
