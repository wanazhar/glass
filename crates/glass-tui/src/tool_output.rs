//! Compact completed-tool blocks. Preview work and allocations are bounded.
use crate::{
    markdown, text,
    theme::{self, Tone},
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub fn lines(
    name: &str,
    command: &str,
    failed: bool,
    elapsed: f32,
    output: &str,
    width: usize,
    compact: bool,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    let tone = if failed { Tone::Error } else { Tone::Success };
    let exit = output
        .lines()
        .next()
        .and_then(|s| s.strip_prefix("[exit "))
        .and_then(|s| s.strip_suffix(']'));
    let title = if failed {
        if let Some(code) = exit {
            if width < 30 {
                format!("× {name} · exit{code} {elapsed:.1}s")
            } else {
                format!("× {name} · failed (exit {code}) · {elapsed:.1}s")
            }
        } else {
            format!("× {name} · failed · {elapsed:.1}s")
        }
    } else {
        format!("✓ {name} · {elapsed:.1}s")
    };
    let mut rows = vec![Line::from(Span::styled(
        text::clip(&markdown::sanitize(&title), width),
        Style::new()
            .fg(theme::color(tone))
            .add_modifier(Modifier::BOLD),
    ))];
    let body_width = width.saturating_sub(2).max(1);
    if !compact && !command.is_empty() {
        let clipped = text::clip(&markdown::sanitize(command), body_width * 3);
        let wrapped = text::wrap_words(&format!("$ {clipped}"), body_width);
        for row in wrapped.iter().take(2) {
            rows.push(line("  ", row, Tone::Secondary, width));
        }
        if wrapped.len() > 2 {
            rows.push(line("  ", "… command: /details", Tone::Muted, width));
        }
    }
    let mut source = output.lines();
    let first = source.next();
    let skip_exit = first.is_some_and(|s| s.starts_with("[exit ") && s.ends_with(']'));
    let source = first.filter(|_| !skip_exit).into_iter().chain(source);
    let mut shown = 0;
    let mut hidden = 0;
    let mut partial = false;
    for raw in source {
        if shown >= 3 || (compact && !failed) {
            hidden += 1;
            continue;
        }
        let clean = markdown::sanitize(&text::clip(raw, body_width * 4));
        let wrapped = text::wrap_words(&clean, body_width);
        let remaining = 3 - shown;
        for row in wrapped.iter().take(remaining) {
            rows.push(line(
                if shown == 0 { "└ " } else { "  " },
                row,
                if failed { Tone::Error } else { Tone::Muted },
                width,
            ));
            shown += 1;
        }
        partial |= wrapped.len() > remaining || text::width(raw) > body_width * 4;
    }
    if hidden > 0 || partial {
        let hint = if partial {
            "… more output · /details".into()
        } else {
            format!(
                "+ {hidden} {}lines · /details",
                if shown == 0 { "output " } else { "more " }
            )
        };
        for row in text::wrap_words(&hint, body_width) {
            rows.push(line("  ", &row, Tone::Muted, width));
        }
    } else if shown == 0 && !compact {
        rows.push(line("└ ", "No output", Tone::Muted, width));
    }
    rows
}
fn line(prefix: &str, body: &str, tone: Tone, width: usize) -> Line<'static> {
    let prefix = text::clip(prefix, width.saturating_sub(1));
    let remaining = width.saturating_sub(text::width(&prefix));
    Line::from(Span::styled(
        format!("{prefix}{}", text::clip(body, remaining)),
        Style::new().fg(theme::color(tone)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_has_one_outcome_bounded_output_and_exact_remaining_count() {
        let output = "[exit 0]\none\ntwo\nthree\nfour\nfive";
        let rows = lines("bash", "cargo test", false, 0.2, output, 40, false);
        let rendered = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("✓ bash"));
        assert!(rendered.contains("$ cargo test"));
        assert!(rendered.contains("└ one"));
        assert!(rendered.contains("+ 2 more lines"));
        assert!(!rendered.contains("[exit"));
        assert!(!rendered.contains("five"));
        for width in [1, 10, 18, 38, 80] {
            assert!(
                lines("bash", "a long command", true, 1.0, output, width, false)
                    .iter()
                    .all(|r| text::width(&r.to_string()) <= width)
            );
        }
    }
    #[test]
    fn failure_exit_code_survives_an_empty_output_summary() {
        for width in [18, 40] {
            let rows = lines("bash", "false", true, 0.1, "[exit 1]", width, false);
            let text = rows
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("exit1") || text.contains("exit 1"), "{text}");
        }
    }

    #[test]
    fn large_one_line_output_has_a_bounded_preview() {
        let rows = lines(
            "bash",
            "echo",
            false,
            1.0,
            &"x".repeat(1_000_000),
            20,
            false,
        );
        assert!(rows.len() < 12);
        assert!(
            rows.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
                .contains("more output")
        );
    }
}
