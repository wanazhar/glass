//! Compact completed-tool blocks. Preview work and allocations are bounded.
use crate::{
    markdown, text,
    theme::{self, Tone},
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// Shared status classification for live, replayed and completed tool cards.
pub fn status(summary: &str) -> String {
    if let Some(rest) = summary.strip_prefix("[exit ")
        && let Some((code, _)) = rest.split_once(']') {
        return format!("exit {code}");
    }
    if ["error:", "killed", "refused:", "blocked"].iter().any(|prefix|summary.starts_with(prefix)) {
        "failed".into()
    } else { "completed".into() }
}

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
    if name=="state" && output.starts_with("## Tasks ") && !failed {
        let preview=output.split("\n\nrevision:").next().unwrap_or(output);
        let content=markdown::lines(preview,body_width);
        let count=if compact{2}else{8};
        rows.extend(content.iter().take(count).cloned());
        if content.len()>count {rows.push(line("  ","… /todo for full checklist",Tone::Muted,width));}
        return rows;
    }
    if !compact && !command.is_empty() {
        let clipped = text::clip(&markdown::sanitize(command), body_width * 3);
        let prefix = if matches!(name, "read" | "edit" | "write" | "view_image" | "web_fetch" | "web_search" | "browser_verify") { "◆" } else { "$" };
        let wrapped = text::wrap_words(&format!("{prefix} {clipped}"), body_width);
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
    let native_file = output.lines().take(2).any(|s| s.starts_with("[file "));
    let visual = matches!(name, "view_image" | "web_fetch" | "web_search" | "browser_verify");
    let budget = if native_file || visual { 6 } else { 3 };
    let mut shown = 0;
    let mut hidden = 0;
    let mut partial = false;
    for raw in source {
        if visual && raw.trim().is_empty() { continue; }
        if shown >= budget || (compact && !failed) {
            hidden += 1;
            continue;
        }
        let display = if native_file && raw.starts_with("[file ") {
            raw.split_once(" · revision ")
                .map(|(path, _)| text::clip(&format!("◆ {}", &path[6..]), body_width))
                .unwrap_or_else(|| raw.to_string())
        } else if visual && raw.starts_with("[web source · ") {
            let status = raw.split(" · ").find(|part| part.starts_with("HTTP ")).unwrap_or("Page");
            format!("◎ Page · {status}")
        } else if visual && raw.starts_with("[web search · ") {
            let parts: Vec<_> = raw.trim_end_matches(']').split(" · ").collect();
            format!("⌕ {} · {}", parts.get(2).unwrap_or(&"Sources"), parts.get(1).unwrap_or(&"web"))
        } else if visual && (raw.trim_start().starts_with("http://") || raw.trim_start().starts_with("https://")) {
            text::clip(raw.trim(), body_width * 2)
        } else if visual && raw.starts_with("[image ") {
            raw.split_once(" · ").map(|(_, rest)| format!("▧ {}", rest.trim_end_matches(']'))).unwrap_or_else(|| raw.to_string())
        } else if native_file && raw.starts_with("[diagnostics ") {
            if raw.starts_with("[diagnostics ok") {
                "✓ syntax checked".into()
            } else if raw.starts_with("[diagnostics error") {
                "× syntax error".into()
            } else {
                "syntax unchecked".into()
            }
        } else {
            text::clip(raw, body_width * 4)
        };
        let clean = markdown::sanitize(&text::clip(&display, body_width * 4));
        let wrapped = text::wrap(&clean, body_width);
        let remaining = budget - shown;
        for row in wrapped.iter().take(remaining) {
            rows.push(line(
                if shown == 0 { "└ " } else { "  " },
                row,
                output_tone(raw, failed, native_file, visual),
                width,
            ));
            shown += 1;
        }
        partial |= wrapped.len() > remaining
            || (!native_file || !raw.starts_with("[file ")) && text::width(raw) > body_width * 4;
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
    rows.push(Line::default());
    rows
}
/// Keep detailed and compact tool output colors consistent.
pub(crate) fn output_tone(raw: &str, failed: bool, native_file: bool, visual: bool) -> Tone {
    if failed || (native_file && (raw.starts_with("- ") || raw.starts_with("error ")
        || raw.starts_with("[diagnostics error"))) {
        Tone::Error
    } else if native_file && (raw.starts_with("+ ") || raw.starts_with("[diagnostics ok")) {
        Tone::Success
    } else if visual && (raw.starts_with("[web ") || raw.starts_with("[image ")
        || raw.starts_with("[browser ")) {
        Tone::Accent
    } else if visual && (raw.trim_start().starts_with("http://") || raw.trim_start().starts_with("https://")) {
        Tone::Secondary
    } else if native_file && raw.starts_with("[file ") {
        Tone::Accent
    } else {
        Tone::Muted
    }
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
    fn status_preserves_failures_across_render_paths() {
        assert_eq!(status("[exit 2] failed"), "exit 2");
        assert_eq!(status("[exit 0] done"), "exit 0");
        assert_eq!(status("error: invalid args"), "failed");
        assert_eq!(status("refused: protected"), "failed");
        assert_eq!(status("done"), "completed");
    }

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
    fn native_preview_preserves_indent_and_uses_semantic_colors() {
        let output = "[exit 0]\n[file src/main.rs · revision deadbeef]\n[diagnostics ok · json]\n+1 -1\n-   old\n+   new";
        let rows = lines("bash", "edit", false, 0.1, output, 40, false);
        assert!(rows.iter().any(|r| r.to_string().contains("-   old")));
        let removed = rows
            .iter()
            .find(|r| r.to_string().contains("-   old"))
            .unwrap();
        let added = rows
            .iter()
            .find(|r| r.to_string().contains("+   new"))
            .unwrap();
        assert_eq!(removed.spans[0].style.fg, Some(theme::color(Tone::Error)));
        assert_eq!(added.spans[0].style.fg, Some(theme::color(Tone::Success)));
        assert!(!rows.iter().any(|r| r.to_string().contains("deadbeef")));
        for width in [1, 10, 18, 38] {
            assert!(
                lines("bash", "edit", false, 0.1, output, width, false)
                    .iter()
                    .all(|r| text::width(&r.to_string()) <= width)
            );
        }
    }
    #[test]
    fn direct_file_paths_are_not_presented_as_shell_commands() {
        for name in ["read", "edit", "write"] {
            let rows = lines(name, "src/main.rs", true, 0.1, "error: refused", 40, false);
            let rendered = rows.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
            assert!(rendered.contains("◆ src/main.rs"), "{rendered}");
            assert!(!rendered.contains("$ src/main.rs"));
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
