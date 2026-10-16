//! Small, bounded semantic terminal layouts for Mermaid's data chart families.
//! No dependency graph engine, browser, date library or rendering subprocess.
use glass_tui::text::{width, wrap_words};
const MAX: usize = 64;
fn number(text: &str) -> Option<f64> {
    let n = text.trim().parse::<f64>().ok()?;
    (n.is_finite() && n.abs() <= 1e15).then_some(n)
}
fn label(text: &str) -> String {
    super::clean_label(text.trim().trim_matches('"'))
}
fn push(out: &mut Vec<String>, text: impl AsRef<str>, w: usize) {
    let text = text.as_ref();
    if width(text) <= w {
        out.push(text.to_owned());
    } else {
        out.extend(wrap_words(text, w));
    }
}
fn title(out: &mut Vec<String>, text: &str, w: usize) {
    push(out, format!("╭─ {}", label(text)), w);
}
fn numeric(text: &str) -> Option<Vec<f64>> {
    let text = text.trim().strip_prefix('[')?.strip_suffix(']')?;
    let values = text.split(',').map(number).collect::<Option<Vec<_>>>()?;
    (!values.is_empty() && values.len() <= MAX).then_some(values)
}
fn values(text: &str) -> Option<Vec<String>> {
    let raw = text.trim().strip_prefix('[')?.strip_suffix(']')?;
    let mut values = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (at, c) in raw.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == ',' && !quoted {
            values.push(label(&raw[start..at]));
            start = at + 1;
        }
    }
    if quoted {
        return None;
    }
    values.push(label(&raw[start..]));
    (!values.is_empty() && values.len() <= MAX && values.iter().all(|v| !v.is_empty()))
        .then_some(values)
}
fn magnitude(n: f64) -> String {
    if n == 0.0 {
        "0".into()
    } else if n.abs() >= 1e9 || n.abs() < 0.001 {
        format!("{n:e}")
    } else {
        n.to_string()
    }
}
fn bar(n: f64, scale: f64, w: usize) -> String {
    let count = if scale > 0.0 {
        ((n.abs() / scale).clamp(0.0, 1.0) * w as f64).round() as usize
    } else {
        0
    };
    format!("{}{}", if n < 0.0 { "◀" } else { "▏" }, "█".repeat(count))
}
/// Recognized types return a result; unrelated diagrams keep the existing path.
pub fn render(body: &str, w: usize) -> Option<Result<Vec<String>, String>> {
    if body.len() > 64 * 1024 {
        return Some(Err("chart input exceeds 64 KiB".into()));
    }
    let lines = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"))
        .take(257)
        .collect::<Vec<_>>();
    let head = *lines.first()?;
    let kind = head.split_whitespace().next()?;
    if !matches!(
        kind,
        "pie"
            | "xychart"
            | "xychart-beta"
            | "timeline"
            | "mindmap"
            | "journey"
            | "quadrantChart"
            | "gantt"
            | "sankey-beta"
    ) {
        return None;
    }
    if body.len() > 64 * 1024 || lines.len() > 256 || !(8..=200).contains(&w) {
        return Some(Err("chart input or viewport limit".into()));
    }
    let drawn = match kind {
        "pie" => pie(&lines, w),
        "xychart" | "xychart-beta" => xy(&lines, w),
        "timeline" => timeline(&lines, w),
        "mindmap" => mindmap(body, w),
        "journey" => journey(&lines, w),
        "quadrantChart" => quadrant(&lines, w),
        "gantt" => gantt(&lines, w),
        "sankey-beta" => sankey(&lines, w),
        _ => None,
    };
    Some(
        drawn
            .filter(|rows| rows.len() <= 512 && rows.iter().all(|row| width(row) <= w))
            .ok_or_else(|| format!("unsupported {kind} syntax, invalid data or chart limit")),
    )
}
fn pie(lines: &[&str], w: usize) -> Option<Vec<String>> {
    let header = lines[0].strip_prefix("pie")?.trim();
    let header = header.strip_prefix("showData").unwrap_or(header).trim();
    let mut out = Vec::new();
    if !header.is_empty() {
        title(&mut out, header.strip_prefix("title ")?, w);
    }
    let mut data = Vec::new();
    for line in &lines[1..] {
        if let Some(t) = line.strip_prefix("title ") {
            title(&mut out, t, w);
            continue;
        }
        let rest = line.strip_prefix('"')?;
        let (name, rest) = rest.split_once('"')?;
        let n = number(rest.trim().strip_prefix(':')?)?;
        if n <= 0.0 || data.len() >= MAX {
            return None;
        }
        data.push((label(name), n));
    }
    if data.is_empty() {
        return None;
    }
    let total: f64 = data.iter().map(|(_, n)| n).sum();
    if !total.is_finite() {
        return None;
    }
    push(
        &mut out,
        format!("Pie proportions · total {}", magnitude(total)),
        w,
    );
    for (name, n) in data {
        push(
            &mut out,
            format!("{name} · {} · {:.1}%", magnitude(n), n / total * 100.0),
            w,
        );
        push(&mut out, bar(n, total, w.saturating_sub(2).min(40)), w);
    }
    Some(out)
}
fn xy(lines: &[&str], w: usize) -> Option<Vec<String>> {
    if !matches!(
        lines[0],
        "xychart" | "xychart-beta" | "xychart horizontal" | "xychart-beta horizontal"
    ) {
        return None;
    }
    let mut out = Vec::new();
    let mut labels = None;
    let mut range = None;
    let mut series = Vec::new();
    let mut y_range = None;
    for line in &lines[1..] {
        if let Some(t) = line.strip_prefix("title ") {
            title(&mut out, t, w);
        } else if let Some(axis) = line.strip_prefix("x-axis ") {
            if labels.is_some() || range.is_some() {
                return None;
            }
            if let Some(at) = axis.find('[') {
                labels = Some(values(&axis[at..])?);
                if !axis[..at].trim().is_empty() {
                    push(&mut out, format!("X · {}", label(&axis[..at])), w);
                }
            } else {
                let (a, b) = axis.rsplit_once("-->")?;
                let start = a.split_whitespace().last()?;
                let a = number(start)?;
                let b = number(b)?;
                if b <= a {
                    return None;
                }
                push(&mut out, format!("X · {}", label(axis)), w);
                range = Some((a, b));
            }
        } else if let Some(axis) = line.strip_prefix("y-axis ") {
            let (a, b) = axis.rsplit_once("-->")?;
            let a = number(a.split_whitespace().last()?)?;
            let b = number(b)?;
            if b <= a {
                return None;
            }
            if y_range.is_some() {
                return None;
            }
            y_range = Some((a, b));
            push(&mut out, format!("Y · {}", label(axis)), w);
        } else if let Some(data) = line.strip_prefix("bar ") {
            series.push(("Bars", numeric(data)?));
        } else {
            let data = line.strip_prefix("line ")?;
            series.push(("Line", numeric(data)?));
        }
    }
    if series.is_empty() || series.len() > 4 {
        return None;
    }
    let count = series[0].1.len();
    if series.iter().any(|(_, v)| v.len() != count)
        || labels.as_ref().is_some_and(|v| v.len() != count)
    {
        return None;
    }
    let labels = labels.unwrap_or_else(|| {
        (0..count)
            .map(|i| {
                magnitude(range.map_or((i + 1) as f64, |(a, b)| {
                    if count == 1 {
                        a
                    } else {
                        a + (b - a) * i as f64 / (count - 1) as f64
                    }
                }))
            })
            .collect()
    });
    let scale = series
        .iter()
        .flat_map(|(_, v)| v)
        .map(|v| v.abs())
        .fold(0.0, f64::max);
    if let Some((a, b)) = y_range
        && series.iter().flat_map(|(_, v)| v).any(|n| *n < a || *n > b) {
            return None;
        }
    for (index, (kind, points)) in series.iter().enumerate() {
        push(&mut out, format!("{kind} · series {}", index + 1), w);
        if *kind == "Line" {
            let (min, max) = y_range.unwrap_or_else(|| {
                (
                    points.iter().copied().fold(f64::INFINITY, f64::min),
                    points.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                )
            });
            let glyphs = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            let spark: String = points
                .iter()
                .map(|n| {
                    glyphs[if max > min {
                        (((n - min) / (max - min)) * 7.0).round() as usize
                    } else {
                        3
                    }]
                })
                .collect();
            push(&mut out, spark, w);
        }
        for (name, n) in labels.iter().zip(points) {
            push(&mut out, format!("{name} · {}", magnitude(*n)), w);
            if *kind == "Bars" {
                let (value, scale) = y_range.map_or((*n, scale), |(a, b)| {
                    if a >= 0.0 {
                        (*n - a, b - a)
                    } else if b <= 0.0 {
                        (*n - b, b - a)
                    } else {
                        (*n, a.abs().max(b.abs()))
                    }
                });
                push(&mut out, bar(value, scale, w.saturating_sub(2).min(40)), w);
            }
        }
    }
    Some(out)
}
fn timeline(lines: &[&str], w: usize) -> Option<Vec<String>> {
    if lines[0] != "timeline" {
        return None;
    }
    let mut out = Vec::new();
    let mut previous = String::new();
    let mut count = 0;
    for line in &lines[1..] {
        if let Some(t) = line.strip_prefix("title ") {
            title(&mut out, t, w);
        } else if let Some(t) = line.strip_prefix("section ") {
            push(&mut out, format!("Section · {}", label(t)), w);
            previous.clear();
        } else {
            let (period, events) = line.split_once(':')?;
            if !period.trim().is_empty() {
                previous = label(period);
            }
            if previous.is_empty() {
                return None;
            }
            for event in events.split(':') {
                if event.trim().is_empty() {
                    return None;
                }
                count += 1;
                if count > MAX {
                    return None;
                }
                push(&mut out, format!("{} ──▶ {}", previous, label(event)), w);
            }
        }
    }
    (count > 0).then_some(out)
}
fn mindmap(body: &str, w: usize) -> Option<Vec<String>> {
    let mut nodes = Vec::new();
    let mut levels = Vec::new();
    let mut count = 0;
    for raw in body
        .lines()
        .skip_while(|l| l.trim().is_empty() || l.trim().starts_with("%%"))
        .skip(1)
    {
        let text = raw.trim();
        if text.is_empty() || text.starts_with("%%") {
            continue;
        }
        if text.contains(":::") || text.starts_with("::icon") || raw.contains('\t') {
            return None;
        }
        let indent = raw.len() - raw.trim_start().len();
        if levels.is_empty() {
            levels.push(indent);
        } else if indent > *levels.last()? {
            if levels.len() >= 16 {
                return None;
            }
            levels.push(indent);
        } else {
            while levels.last().is_some_and(|at| *at > indent) {
                levels.pop();
            }
            if levels.last() != Some(&indent) || levels.len() == 1 {
                return None;
            }
        }
        let name = if let Some(at) = text.find(['[', '(']) {
            let end = if text[at..].starts_with("((") {
                "))"
            } else if text[at..].starts_with('[') {
                "]"
            } else {
                ")"
            };
            let open = if end == "))" { 2 } else { 1 };
            label(text[at + open..].strip_suffix(end)?)
        } else {
            label(text)
        };
        if name.is_empty() {
            return None;
        }
        count += 1;
        if count > MAX {
            return None;
        }
        nodes.push((levels.len() - 1, name));
    }
    let mut out = Vec::new();
    let mut ancestry = Vec::new();
    for (index, (depth, name)) in nodes.iter().enumerate() {
        let last = nodes[index + 1..]
            .iter()
            .find(|(next, _)| next <= depth)
            .is_none_or(|(next, _)| next < depth);
        ancestry.truncate(*depth);
        ancestry.push(last);
        let mut prefix = String::new();
        if *depth == 0 {
            prefix.push_str("◆ ");
        } else {
            for is_last in ancestry.iter().take(*depth).skip(1) {
                prefix.push_str(if *is_last { "   " } else { "│  " });
            }
            prefix.push_str(if last { "└─ " } else { "├─ " });
        }
        let cells = width(&prefix);
        if cells >= w.saturating_sub(2) {
            push(&mut out, format!("L{depth} · {name}"), w);
            continue;
        }
        for (line, part) in wrap_words(name, w - cells).into_iter().enumerate() {
            out.push(format!(
                "{}{part}",
                if line == 0 {
                    prefix.clone()
                } else {
                    " ".repeat(cells)
                }
            ));
        }
    }
    (count > 0).then_some(out)
}

fn journey(lines: &[&str], w: usize) -> Option<Vec<String>> {
    if lines[0] != "journey" {
        return None;
    }
    let mut out = Vec::new();
    let mut count = 0;
    for line in &lines[1..] {
        if let Some(t) = line.strip_prefix("title ") {
            title(&mut out, t, w);
        } else if let Some(t) = line.strip_prefix("section ") {
            push(&mut out, format!("Section · {}", label(t)), w);
        } else {
            let fields = line.split(':').map(str::trim).collect::<Vec<_>>();
            if fields.len() != 3 || fields[0].is_empty() || fields[2].is_empty() {
                return None;
            }
            let score = fields[1].parse::<usize>().ok()?;
            if score > 5 {
                return None;
            }
            count += 1;
            if count > MAX {
                return None;
            }
            push(
                &mut out,
                format!(
                    "{} · {score}/5 {}{}",
                    label(fields[0]),
                    "●".repeat(score),
                    "○".repeat(5 - score)
                ),
                w,
            );
            push(&mut out, format!("Actors · {}", label(fields[2])), w);
        }
    }
    (count > 0).then_some(out)
}
fn quadrant(lines: &[&str], w: usize) -> Option<Vec<String>> {
    if lines[0] != "quadrantChart" {
        return None;
    }
    let mut out = Vec::new();
    let mut names = [
        "Upper right".to_owned(),
        "Upper left".into(),
        "Lower left".into(),
        "Lower right".into(),
    ];
    let mut points = Vec::new();
    for line in &lines[1..] {
        if let Some(t) = line.strip_prefix("title ") {
            title(&mut out, t, w);
        } else if line.starts_with("x-axis ") || line.starts_with("y-axis ") {
            push(&mut out, label(line), w);
        } else if let Some(rest) = line.strip_prefix("quadrant-") {
            let (n, name) = rest.split_once(' ')?;
            let n = n.parse::<usize>().ok()?;
            if !(1..=4).contains(&n) {
                return None;
            }
            names[n - 1] = label(name);
        } else {
            let (name, coords) = line.rsplit_once(':')?;
            let coords = numeric(coords)?;
            if coords.len() != 2
                || coords.iter().any(|n| !(0.0..=1.0).contains(n))
                || points.len() >= MAX
            {
                return None;
            }
            points.push((label(name), coords[0], coords[1]));
        }
    }
    for (i, name) in names.iter().enumerate() {
        push(&mut out, format!("Q{} · {name}", i + 1), w);
    }
    if points.is_empty() {
        return Some(out);
    }
    if w >= 24 {
        let columns = w.saturating_sub(2).min(49);
        let rows = 9;
        let mut grid = vec![vec![' '; columns]; rows];
        for row in &mut grid {
            row[columns / 2] = '│';
        }
        for cell in &mut grid[rows / 2] {
            *cell = '─';
        }
        grid[rows / 2][columns / 2] = '┼';
        for (_, x, y) in &points {
            let col = (x * (columns - 1) as f64).round() as usize;
            let row = ((1.0 - y) * (rows - 1) as f64).round() as usize;
            grid[row][col] = if matches!(grid[row][col], ' ' | '│' | '─' | '┼') {
                '●'
            } else {
                '+'
            };
        }
        for row in grid {
            out.push(row.into_iter().collect());
        }
        push(&mut out, "● point · + overlapping points", w);
    }
    for (name, x, y) in points {
        let q = if y >= 0.5 {
            if x >= 0.5 { 1 } else { 2 }
        } else if x < 0.5 {
            3
        } else {
            4
        };
        push(
            &mut out,
            format!("{name} · ({}, {}) · Q{q}", magnitude(x), magnitude(y)),
            w,
        );
    }
    Some(out)
}
fn csv(text: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            ',' => {
                fields.push(field.trim().to_owned());
                field.clear();
                closed = false;
            }
            '"' if field.trim().is_empty() && !closed => {
                field.clear();
                quoted = true;
            }
            '"' => return None,
            _ if closed && !c.is_whitespace() => return None,
            _ if !closed => field.push(c),
            _ => {}
        }
    }
    if quoted {
        return None;
    }
    fields.push(field.trim().to_owned());
    Some(fields)
}
fn sankey(lines: &[&str], w: usize) -> Option<Vec<String>> {
    if lines[0] != "sankey-beta" {
        return None;
    }
    let mut flows = Vec::new();
    for line in &lines[1..] {
        let fields = csv(line)?;
        if fields.len() != 3 || fields[..2].iter().any(|s| s.is_empty()) || flows.len() >= MAX {
            return None;
        }
        let n = number(&fields[2])?;
        if n <= 0.0 {
            return None;
        }
        flows.push((label(&fields[0]), label(&fields[1]), n));
    }
    if flows.is_empty() {
        return None;
    }
    let max = flows.iter().map(|(_, _, n)| *n).fold(0.0, f64::max);
    let mut out = Vec::new();
    push(&mut out, "Sankey · weighted flows", w);
    for (a, b, n) in flows {
        push(&mut out, format!("{a} ──▶ {b} · {}", magnitude(n)), w);
        push(&mut out, bar(n, max, w.saturating_sub(2).min(40)), w);
    }
    Some(out)
}
// Gregorian dates converted to an integer day without a date-time dependency.
fn date(text: &str) -> Option<i64> {
    let parts = text
        .split('-')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() != 3 {
        return None;
    }
    let (mut y, m, d) = (parts[0], parts[1], parts[2]);
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=9999).contains(&y)
        || !(1..=12).contains(&m)
        || !(1..=days[(m - 1) as usize]).contains(&d)
    {
        return None;
    }
    y -= i64::from(m <= 2);
    let era = y.div_euclid(400);
    let year = y - era * 400;
    let month = m + if m > 2 { -3 } else { 9 };
    Some(era * 146097 + year * 365 + year / 4 - year / 100 + (153 * month + 2) / 5 + d - 1)
}
fn calendar(day: i64) -> String {
    let era = day.div_euclid(146097);
    let days = day - era * 146097;
    let year = (days - days / 1460 + days / 36524 - days / 146096) / 365;
    let mut y = year + era * 400;
    let doy = days - (365 * year + year / 4 - year / 100);
    let month = (5 * doy + 2) / 153;
    let d = doy - (153 * month + 2) / 5 + 1;
    let m = month + if month < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
fn gantt(lines: &[&str], w: usize) -> Option<Vec<String>> {
    if lines[0] != "gantt" {
        return None;
    }
    let mut out = Vec::new();
    let mut tasks: Vec<(String, String, i64, i64, String, String)> = Vec::new();
    let mut section = String::new();
    for line in &lines[1..] {
        if let Some(t) = line.strip_prefix("title ") {
            title(&mut out, t, w);
            continue;
        }
        if let Some(t) = line.strip_prefix("section ") {
            section = label(t);
            continue;
        }
        if *line == "dateFormat YYYY-MM-DD" {
            continue;
        }
        if line.starts_with("axisFormat ")
            || line.starts_with("tickInterval ")
            || line.starts_with("todayMarker ")
        {
            continue;
        }
        let (name, rest) = line.split_once(':')?;
        let mut fields = rest.split(',').map(str::trim).collect::<Vec<_>>();
        let mut tags = Vec::new();
        while fields
            .first()
            .is_some_and(|s| matches!(*s, "done" | "active" | "crit" | "milestone"))
        {
            tags.push(fields.remove(0));
        }
        if !(1..=3).contains(&fields.len()) || tasks.len() >= MAX {
            return None;
        }
        let id = if fields.len() == 3 {
            fields.remove(0).to_owned()
        } else {
            format!("task{}", tasks.len() + 1)
        };
        if tasks.iter().any(|t| t.1 == id) {
            return None;
        }
        let (start, start_text, end_spec) = if fields.len() == 1 {
            let previous = tasks.last()?;
            (previous.3, "after previous".to_owned(), fields[0])
        } else if let Some(dep) = fields[0].strip_prefix("after ") {
            let dependencies = dep
                .split_whitespace()
                .map(|id| tasks.iter().find(|t| t.1 == id).map(|t| t.3))
                .collect::<Option<Vec<_>>>()?;
            (
                *dependencies.iter().max()?,
                format!("after {dep}"),
                fields[1],
            )
        } else {
            (date(fields[0])?, fields[0].to_owned(), fields[1])
        };
        let end = if let Some(days) = end_spec.strip_suffix('d') {
            start.checked_add(days.parse::<i64>().ok()?)?
        } else if let Some(weeks) = end_spec.strip_suffix('w') {
            start.checked_add(weeks.parse::<i64>().ok()?.checked_mul(7)?)?
        } else {
            date(end_spec)?
        };
        if end < start
            || end - start > 36500
            || (tags.contains(&"milestone") && end != start)
            || (end == start && !tags.contains(&"milestone"))
        {
            return None;
        }
        tasks.push((
            label(name),
            id,
            start,
            end,
            format!(
                "{}{}{}{}",
                calendar(start),
                if start == end {
                    String::new()
                } else {
                    format!(" → {}", calendar(end))
                },
                if tags.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", tags.join(", "))
                },
                if start_text.starts_with("after ") {
                    format!(" · {start_text}")
                } else {
                    String::new()
                }
            ),
            section.clone(),
        ));
    }
    if tasks.is_empty() {
        return None;
    }
    let min = tasks.iter().map(|t| t.2).min()?;
    let max = tasks.iter().map(|t| t.3).max()?;
    let span = (max - min).max(1) as f64;
    let columns = w.saturating_sub(2).min(48);
    push(&mut out, "Gantt · calendar days", w);
    let mut current = String::new();
    for (name, _, start, end, detail, section) in tasks {
        if section != current {
            push(&mut out, format!("Section · {section}"), w);
            current = section;
        }
        push(&mut out, name, w);
        push(&mut out, detail.trim_end_matches(" · "), w);
        let offset = (((start - min) as f64 / span) * columns as f64).floor() as usize;
        let length = ((((end - start) as f64 / span) * columns as f64).round() as usize)
            .max(1)
            .min(columns.saturating_sub(offset).max(1));
        push(
            &mut out,
            format!(
                "{}{}",
                " ".repeat(offset.min(columns.saturating_sub(1))),
                if start == end {
                    "◆".into()
                } else {
                    "━".repeat(length)
                }
            ),
            w,
        );
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const CASES: &[(&str, &str)] = &[
        (
            "pie",
            "pie showData\n title Allocations\n \"RootReady\": 30\n \"Second\": 70",
        ),
        (
            "xy",
            "xychart-beta\n title RootReady\n x-axis [Jan, Feb, Mar]\n y-axis \"Count\" 0 --> 100\n bar [10, 20, 30]\n line [5, 15, 25]",
        ),
        (
            "timeline",
            "timeline\n title RootReady\n section Phase\n 2025 : Built : Tested\n : Shipped\n 2026 : Improved",
        ),
        (
            "mindmap",
            "mindmap\n  root((RootReady))\n    First\n      Child\n    Second",
        ),
        (
            "journey",
            "journey\n title RootReady\n section Start\n Login: 3: User, Service\n Read: 5: User",
        ),
        (
            "quadrant",
            "quadrantChart\n title RootReady\n x-axis Low --> High\n y-axis Low --> High\n quadrant-1 Expand\n Alpha: [0.2, 0.7]\n Beta: [0.8, 0.2]",
        ),
        (
            "gantt",
            "gantt\n title RootReady\n dateFormat YYYY-MM-DD\n section Build\n Code :done, code, 2026-10-16, 3d\n Review :crit, review, after code, 2d\n Ship :milestone, ship, after review, 0d",
        ),
        (
            "sankey",
            "sankey-beta\nRootReady,Storage,10\n\"Storage, primary\",Output,5",
        ),
    ];
    #[test]
    fn every_family_is_native_bounded_and_keeps_data() {
        for (kind, source) in CASES {
            for w in [8, 16, 20, 40, 88, 200] {
                let rows = render(source, w)
                    .unwrap()
                    .unwrap_or_else(|e| panic!("{kind} at {w}: {e}"));
                assert!(rows.iter().all(|row| width(row) <= w), "{kind} {w}");
                assert!(
                    rows.join("").replace(' ', "").contains("RootReady"),
                    "{kind} {w}: {rows:?}"
                );
            }
        }
    }
    #[test]
    fn invalid_or_ambiguous_data_never_becomes_a_partial_chart() {
        for source in [
            "pie\n\"x\": NaN",
            "pie\n\"x\": 0",
            "xychart-beta\nx-axis [a,b]\nbar [1]",
            "xychart-beta\ny-axis 0 --> 2\nbar [3]",
            "quadrantChart\nx: [1.2,0]",
            "gantt\nx: 2026-02-30, 2d",
            "gantt\nx: after unknown, 2d",
            "sankey-beta\nx,y,-1",
            "mindmap\n root\n other",
            "journey\nx: 9: user",
            "timeline\n: no period",
        ] {
            assert!(render(source, 40).unwrap().is_err(), "{source}");
        }
    }
    #[test]
    fn dates_bounds_scales_and_tree_siblings_are_correct() {
        assert_eq!(date("2024-03-01").unwrap() - date("2024-02-28").unwrap(), 2);
        assert!(date("2025-02-29").is_none());
        for text in [
            "0001-01-01",
            "1600-02-29",
            "1900-03-01",
            "2000-02-29",
            "2026-10-16",
            "9999-12-31",
        ] {
            assert_eq!(calendar(date(text).unwrap()), text);
        }
        assert!(csv("foo\"bar,baz,1").is_none());
        assert!(csv("\"foo\"oops,baz,1").is_none());
        let rows = render(CASES[3].1, 40).unwrap().unwrap();
        assert!(rows.iter().any(|l| l == "├─ First"));
        assert!(rows.iter().any(|l| l == "│  └─ Child"));
        assert!(rows.iter().any(|l| l == "└─ Second"));
        let rows = render("xychart-beta\ny-axis 0 --> 100\nbar [50]", 40)
            .unwrap()
            .unwrap();
        assert_eq!(
            rows.last().unwrap().chars().filter(|c| *c == '█').count(),
            19
        );
    }
    #[test]
    fn hostile_sizes_and_labels_are_bounded() {
        let source = format!("pie\n{}", "\"x\":1\n".repeat(65));
        assert!(render(&source, 40).unwrap().is_err());
        assert!(render("pie\n\"x\":1", 7).unwrap().is_err());
        let rows = render("pie\n\"界界\\u{1b}\":1", 8).unwrap().unwrap();
        assert!(rows.iter().all(|r| width(r) <= 8));
    }
}
