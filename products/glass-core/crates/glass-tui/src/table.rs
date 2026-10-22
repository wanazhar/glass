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
    align: Vec<Alignment>,
}
#[derive(Clone, Copy)]
enum Alignment { Left, Center, Right }
impl Table {
    pub(crate) fn new(headers: Vec<String>, separator: &str) -> Self {
        let align = cells(separator).unwrap_or_default().iter().map(|cell| {
            match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => Alignment::Center,
                (false, true) => Alignment::Right,
                _ => Alignment::Left,
            }
        }).collect();
        Self { headers, row: 0, align }
    }
    fn columns(&self, width: usize) -> Option<Vec<usize>> {
        let n = self.headers.len();
        let available = width.saturating_sub((n - 1) * 3);
        let column = available / n;
        (column >= 12).then(|| (0..n).map(|i| column + usize::from(i < available % n)).collect())
    }
    pub(crate) fn header(&self, width: usize) -> Vec<Line<'static>> {
        if let Some(columns) = self.columns(width) {
            let mut rows = self.grid(&self.headers, &columns, true);
            rows.push(Line::from(Span::styled(
                columns.iter().map(|column| "─".repeat(*column)).collect::<Vec<_>>().join("─┼─"),
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
    fn grid(&self, cells: &[String], columns: &[usize], header: bool) -> Vec<Line<'static>> {
        let wrapped = cells
            .iter().zip(columns)
            .map(|(cell, width)| crate::markdown::cell_lines(cell, *width))
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
                    let padding = columns[x].saturating_sub(used);
                    let left = match self.align.get(x).unwrap_or(&Alignment::Left) {
                        Alignment::Left => 0,
                        Alignment::Center => padding / 2,
                        Alignment::Right => padding,
                    };
                    spans.push(Span::raw(" ".repeat(left)));
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
                    spans.push(Span::raw(" ".repeat(padding - left)));
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
        let mut lines = if let Some(columns) = self.columns(width) {
            self.grid(&cells, &columns, false)
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
    fn table_rows_and_dividers_fit_every_viewport() {
        for count in 2..=8 {
            let headers = (0..count).map(|i| format!("Column {i}")).collect::<Vec<_>>();
            let separator = format!("|{}|", vec!["---:"; count].join("|"));
            let raw = format!("|{}|", vec!["**界** long-value-without-spaces-1234567890"; count].join("|"));
            for width in 1..=100 {
                let mut table = Table::new(headers.clone(), &separator);
                let header = table.header(width);
                let grid = table.columns(width).is_some();
                if grid {
                    let divider = header.last().unwrap().to_string();
                    assert_eq!(crate::text::width(&divider), width);
                    assert_eq!(divider.matches('┼').count(), count - 1);
                }
                for row in header.into_iter().chain(table.row(&raw, width).unwrap()) {
                    assert!(crate::text::width(&row.to_string()) <= width, "width {width}: {row}");
                }
            }
        }
    }
    #[test]
    fn alignment_and_remainder_cells_are_preserved() {
        let mut table = Table::new(vec!["Name".into(), "Count".into()], "| :--- | ---: |");
        let row = table.row("| sample | 42 |", 40).unwrap().remove(0).to_string();
        assert!(row.starts_with("sample"));
        assert!(row.ends_with("42"));
        assert_eq!(crate::text::width(&row), 40);
    }
    #[test]
    fn malformed_pipes_and_unframed_tables_fall_back_without_panicking() {
        for raw in ["|", "||", "hello | world", "| one |"] {
            assert!(cells(raw).is_none());
        }
        assert!(cells("| a | b | c |").is_some());
        assert!(!separator("| -- | --- |", 2));
    }
}
