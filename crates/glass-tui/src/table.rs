//! Bounded, row-at-a-time Markdown tables; no whole-table buffering.
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub(crate) fn cells(raw: &str) -> Option<Vec<String>> {
    let raw = raw.trim();
    if raw.len() < 3 || raw.len() > 8192 || !raw.starts_with('|') || !raw.ends_with('|') {
        return None;
    }
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    let mut code = false;
    for c in raw[1..raw.len() - 1].chars() {
        if escaped {
            if c != '|' {
                cell.push('\\');
            }
            cell.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '`' => {
                code = !code;
                cell.push(c);
            }
            '|' if !code => {
                cells.push(cell.trim().to_string());
                cell.clear();
                if cells.len() >= 32 {
                    return None;
                }
            }
            _ => cell.push(c),
        }
    }
    if escaped {
        cell.push('\\');
    }
    cells.push(cell.trim().to_string());
    (cells.len() >= 2).then_some(cells)
}

pub(crate) fn separator(raw: &str, count: usize) -> bool {
    cells(raw).is_some_and(|cells| {
        cells.len() == count
            && cells.iter().all(|c| {
                let c = c.trim_matches(':');
                c.len() >= 3 && c.chars().all(|c| c == '-')
            })
    })
}

pub(crate) struct Table {
    headers: Vec<String>,
    row: usize,
}
impl Table {
    pub(crate) fn new(headers: Vec<String>) -> Self {
        Self { headers, row: 0 }
    }
    fn columns(&self, width: usize) -> Option<usize> {
        let n = self.headers.len();
        let column = width.saturating_sub((n - 1) * 3) / n;
        (column >= 12).then_some(column)
    }
    pub(crate) fn header(&self, width: usize) -> Vec<Line<'static>> {
        if let Some(column) = self.columns(width) {
            let mut rows = self.grid(&self.headers, column, true);
            rows.push(Line::from(Span::styled(
                "─".repeat((column + 3) * self.headers.len() - 3),
                crate::markdown::border_style(),
            )));
            rows
        } else {
            let mut rows = crate::markdown::cell_lines(&self.headers.join(" · "), width);
            for row in &mut rows {
                for span in &mut row.spans {
                    span.style = span.style.patch(crate::markdown::border_style());
                }
            }
            rows
        }
    }
    fn grid(&self, cells: &[String], width: usize, header: bool) -> Vec<Line<'static>> {
        let wrapped = cells
            .iter()
            .map(|cell| crate::markdown::cell_lines(cell, width))
            .collect::<Vec<_>>();
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        (0..height)
            .map(|y| {
                let mut spans = Vec::new();
                for (x, lines) in wrapped.iter().enumerate() {
                    if x > 0 {
                        spans.push(Span::styled(" │ ", crate::markdown::border_style()));
                    }
                    let line = lines.get(y).cloned().unwrap_or_default();
                    let used = crate::text::width(&line.to_string());
                    for mut span in line.spans {
                        if header {
                            span.style = span.style.patch(
                                Style::new()
                                    .fg(crate::theme::color(crate::theme::Tone::Accent))
                                    .add_modifier(Modifier::BOLD),
                            );
                        }
                        spans.push(span);
                    }
                    if x + 1 < wrapped.len() {
                        spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
                    }
                }
                Line::from(spans)
            })
            .collect()
    }
    pub(crate) fn row(&mut self, raw: &str, width: usize) -> Option<Vec<Line<'static>>> {
        let cells = cells(raw)?;
        if cells.len() != self.headers.len() {
            return None;
        }
        let mut lines = if let Some(column) = self.columns(width) {
            self.grid(&cells, column, false)
        } else {
            let mut lines = Vec::new();
            for (header, value) in self.headers.iter().zip(&cells) {
                let mut heading = crate::markdown::cell_lines(header, width);
                for line in &mut heading {
                    for span in &mut line.spans {
                        span.style = span.style.patch(
                            Style::new()
                                .fg(crate::theme::color(crate::theme::Tone::Accent))
                                .add_modifier(Modifier::BOLD),
                        );
                    }
                }
                lines.extend(heading);
                lines.extend(crate::markdown::indented_cell_lines(value, width));
            }
            lines
        };
        if self.row > 0 && self.columns(width).is_none() {
            lines.insert(0, Line::default());
        }
        self.row += 1;
        Some(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_pipes_and_unframed_tables_fall_back_without_panicking() {
        for raw in ["|", "||", "hello | world", "| one |"] {
            assert!(cells(raw).is_none());
        }
        assert!(cells("| a | b | c |").is_some());
        assert!(!separator("| -- | --- |", 2));
    }
}
