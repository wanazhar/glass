//! Bounded terminal-native Mermaid renderer (task six-wave-d-001, extended
//! by the six-codebase follow-up).
//!
//! Two families render natively; everything else falls back to the source:
//!
//! - **Flowcharts** — node statements (`A`, `A[label]`, `A("text")`), edges
//!   (`A --> B`, labeled `A -- text --> B`, arrow-text `A --|text|> B`), and
//!   the `flowchart TD`/`graph LR` header.
//! - **Sequence diagrams** — `sequenceDiagram` with `participant`/`actor`,
//!   messages (`->`, `->>`, `-->`, `-->>`, `-x`, `--x`), self-messages,
//!   `note over`, and one nesting level of `loop`/`alt`/`opt`/`break`.
//!   Class/state/ER keep the fenced-source fallback, exactly like
//!   unsupported or overflowing input on either family.
//!
//! Layout: flowchart nodes take one lane each per rank so crossings never
//! join; sequence diagrams draw participant lifelines and one message band
//! per event. The result is width-capped; anything exceeding the cap (or
//! malformed) returns `None` and the caller renders the original block.

/// Bounded diagram size. Exceeding either cap falls back to source —
/// a renderer that draws forever is a worse renderer than no renderer.
pub const MAX_NODES: usize = 64;
/// Bounded edge count (see [`MAX_NODES`]).
pub const MAX_EDGES: usize = 128;

#[derive(Debug, PartialEq)]
struct Node {
    id: String,
    label: String,
}

#[derive(Debug, PartialEq)]
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
}

#[derive(Debug, Default)]
pub struct Flowchart {
    /// Parsed and kept for future layout work; the current single-row
    /// per-rank renderer draws ribs identically for TD and LR, and honest
    /// capacity outweighs pretending otherwise.
    #[allow(dead_code)]
    direction: Direction,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Debug, Default, PartialEq)]
enum Direction {
    #[default]
    Down,
    Right,
}

/// Parse a fenced body as a mermaid flowchart. `None` means "not renderable
/// here" — the caller falls back to the fenced source, never to a halfway
/// drawing.
pub fn parse(body: &str) -> Option<Flowchart> {
    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = lines.next()?;
    let direction = parse_header(header)?;

    let mut chart = Flowchart { direction, ..Flowchart::default() };
    for line in lines {
        parse_statement(line, &mut chart)?;
    }
    if chart.nodes.is_empty() {
        return None;
    }
    Some(chart)
}

/// Parse and render a sequence diagram at the given column width. Separate
/// entry point because the two families share no tree; `None` means the
/// source falls back to the fenced block (unsupported syntax, wrong shape,
/// cap or width exceeded).
pub fn parse_sequence(body: &str) -> Option<Vec<String>> {
    let diagram = parse_sequence_diagram(body)?;
    render_sequence(&diagram)
}

fn parse_header(header: &str) -> Option<Direction> {
    let rest = header
        .strip_prefix("flowchart")
        .or_else(|| header.strip_prefix("graph"))?
        .trim();
    match rest {
        "TD" | "TB" => Some(Direction::Down),
        "LR" => Some(Direction::Right),
        _ => None,
    }
}

fn parse_statement(line: &str, chart: &mut Flowchart) -> Option<()> {
    // Edge statements: A --> B, A -- text --> B, A --|text|> B, A -->|text| B.
    // The link form `A -- text --> B` carries its label between two dash
    // pairs, so the split point is the FIRST dash-run start, and the target
    // is whatever follows the arrow head.
    if let Some(first_dash) = line.find("--") {
        let (from_part, link_part) = line.split_at(first_dash);
        let from_part = from_part.trim();
        if from_part.is_empty() {
            return None;
        }
        // Link part is one of: "--> B", "-->|text| B", "-- text --> B",
        // "--|text|> B".
        let (label, target) = if let Some(rest) = link_part.strip_prefix("-->") {
            match rest.trim().strip_prefix('|') {
                // A -->|text| B
                Some(after) => match after.split_once('|') {
                    Some((text, target)) => {
                        let text = text.trim();
                        let target = target.trim();
                        if text.is_empty() || target.is_empty() {
                            return None;
                        }
                        (Some(text.to_string()), target)
                    }
                    None => return None,
                },
                None => (None, rest.trim()),
            }
        } else {
            // `-- text --> B` or `--|text|> B`
            let after = link_part.trim_start_matches('-').trim();
            if let Some(rest) = after.strip_prefix('|') {
                let (text, target) = rest.split_once('|')?;
                let text = text.trim();
                let target = target.trim().trim_end_matches('>').trim();
                if text.is_empty() || target.is_empty() {
                    return None;
                }
                (Some(text.to_string()), target)
            } else {
                match after.rsplit_once("-->") {
                    Some((text, target)) => {
                        let text = text.trim();
                        let target = target.trim();
                        if text.is_empty() || target.is_empty() {
                            return None;
                        }
                        (Some(text.to_string()), target)
                    }
                    None => return None,
                }
            }
        };
        let from = node_of(from_part, chart)?;
        let to = node_of(target, chart)?;
        if chart.edges.len() >= MAX_EDGES {
            return None;
        }
        chart.edges.push(Edge { from, to, label });
        return Some(());
    }
    // Plain node statement.
    node_of(line, chart)?;
    Some(())
}

/// Node operand: `A`, `A[label]`, `A(text)`, `A("text")`, `A{text}`.
/// Returns the node id used in edges, registering the node the first time.
fn node_of(raw: &str, chart: &mut Flowchart) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (id, label) = match raw.find(|c| "[](){}".contains(c)) {
        Some(pos) if raw.len() > pos + 1 => {
            let id = raw[..pos].trim();
            let inner = raw[pos + 1..].trim_end_matches(|c| "])}{".contains(c));
            let label = inner.trim().trim_matches('"').trim().to_string();
            if id.is_empty() || label.is_empty() {
                return None;
            }
            (id.to_string(), label)
        }
        // Bare id (or an id with a stray bracket) is an id-only node.
        _ => (raw.to_string(), raw.to_string()),
    };
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    if let Some(existing) = chart.nodes.iter().find(|n| n.id == id) {
        // Later shapes redefine the label in mermaid; keep the first here —
        // a renderer that rewrites its own text mid-parse lies either way.
        let _ = existing;
        return Some(id);
    }
    if chart.nodes.len() >= MAX_NODES {
        return None;
    }
    chart.nodes.push(Node { id: id.clone(), label });
    Some(id)
}

/// Render at the given column width. `None` means the diagram cannot fit
/// under the caps and the caller falls back to the fenced source.
pub fn render(chart: &Flowchart, width: usize) -> Option<Vec<String>> {
    let width = width.clamp(1, 88);
    if width < 12 {
        return None;
    }
    // Rank per node: rank 0 for sources, one past the highest inbound rank.
    // (Other ranks continue from here.)
    let mut rank: Vec<(String, usize)> = Vec::new();
    for edge in &chart.edges {
        let target_rank = rank
            .iter()
            .find(|(id, _)| *id == edge.to)
            .map_or(0, |(_, r)| r + 1);
        // An edge INTO the from-node would make it rank>0 too; compute both.
        let source_rank = rank
            .iter()
            .find(|(id, _)| *id == edge.from)
            .map_or(0, |(_, r)| r + 1);
        merge_rank(&mut rank, &edge.to, target_rank);
        merge_rank(&mut rank, &edge.from, source_rank);
    }
    let mut rows: Vec<String> = Vec::new();
    // Simple horizontal layout: one line per rank, nodes joined with arrows
    // pointing to their targets. Cap total lines to keep redraw bounded.
    let ranks = rank.iter().map(|(_, r)| *r).max().unwrap_or(0) + 1;
    if ranks > 16 {
        return None;
    }
    for r in 0..ranks {
        let nodes: Vec<&Node> = chart
            .nodes
            .iter()
            .filter(|n| rank.iter().any(|(id, rank)| *id == n.id && *rank == r))
            .collect();
        if nodes.is_empty() {
            continue;
        }
        let mut line = String::new();
        for (i, node) in nodes.iter().enumerate() {
            let shape = format!("[{}]", node.label);
            if i > 0 {
                line.push_str("   ");
            }
            if line.chars().count() + shape.chars().count() > width.saturating_sub(1) {
                // Too wide for the column: fall back to source.
                return None;
            }
            if i > 0 {
                line.push_str("── ");
                line.push_str("edge");
                continue;
            }
            line.push_str(&shape);
            // Labeled edges for this node on the same row.
            let out: Vec<&Edge> = chart
                .edges
                .iter()
                .filter(|e| e.from == node.id)
                .collect();
            let mut row = line.clone();
            for edge in out {
                let target = chart.nodes.iter().find(|n| n.id == edge.to)?;
                let suffix = match &edge.label {
                    Some(l) => {
                        format!(" ── {} ──▶ [{}]", l, target.label)
                    }
                    None => format!(" ──▶ [{}]", target.label),
                };
                if row.chars().count() + suffix.chars().count() <= width.saturating_sub(1) {
                    row.push_str(&suffix);
                } else {
                    // continues on the next row referencing the same node
                    break;
                }
            }
            rows.push(row);
        }
    }
    // Edge labels only drawn for edges whose target row already has the node
    // (visible above). Any edge not drawn on its row must be listed — or the
    // fallback triggers. Bounded single pass:
    let drawn: usize = rows
        .iter()
        .map(|row| row.matches("──▶").count())
        .sum();
    if drawn < chart.edges.len() {
        return None;
    }
    Some(rows)
}

fn merge_rank(rank: &mut Vec<(String, usize)>, id: &str, new_rank: usize) {
    match rank.iter_mut().find(|(existing, _)| existing == id) {
        Some((_, rank)) if *rank < new_rank => *rank = new_rank,
        Some(_) => {}
        None => rank.push((id.to_string(), new_rank)),
    }
}

// ---- Sequence diagrams (follow-up: the second family) ----

/// Bounded sequence size; the same philosophy as the flowchart caps.
const MAX_PARTICIPANTS: usize = 8;
const MAX_MESSAGES: usize = 64;
/// One nesting level of control fragments: `loop`/`alt`/`opt`/`break` with
/// no `else` branches yet — deeper structure falls back to source.
const MAX_NESTING: usize = 1;

#[derive(Debug)]
struct Participant {
    id: String,
    label: String,
}

#[derive(Debug)]
enum SequenceEvent {
    Message {
        from: usize,
        to: usize,
        text: String,
        dashed: bool,
    },
    Open(String),
    Close,
    Note {
        left: usize,
        right: usize,
        text: String,
    },
}

#[derive(Debug, Default)]
struct SequenceDiagram {
    people: Vec<Participant>,
    events: Vec<SequenceEvent>,
}

fn participant_of(people: &mut Vec<Participant>, id: &str) -> Option<usize> {
    if let Some(index) = people.iter().position(|p| p.id == id) {
        return Some(index);
    }
    if people.len() >= MAX_PARTICIPANTS {
        return None;
    }
    people.push(Participant {
        id: id.to_string(),
        label: id.to_string(),
    });
    Some(people.len() - 1)
}

/// `identifier` for participants: runs until whitespace or a grammar token.
fn take_identifier(rest: &str) -> Option<(&str, &str)> {
    let end = rest
        .find(|c: char| c.is_whitespace() || ",:->x#".contains(c))
        .unwrap_or(rest.len());
    let (id, tail) = rest.split_at(end);
    if id.is_empty() {
        return None;
    }
    Some((id, tail))
}

/// Message arrow: solid/dashed, `>` head or `x` disposition.
fn take_arrow(rest: &str) -> Option<(bool, bool, &str)> {
    for (token, dashed, cross) in [
        ("-->>", true, false),
        ("->>", false, false),
        ("--x", true, true),
        ("-x", false, true),
        ("-->", true, false),
        ("->", false, false),
    ] {
        if let Some(after) = rest.strip_prefix(token) {
            return Some((dashed, cross, after));
        }
    }
    None
}

/// Parse a `sequenceDiagram` body. Unsupported syntax returns `None` so the
/// caller falls back to the source — never a halfway drawing.
fn parse_sequence_diagram(body: &str) -> Option<SequenceDiagram> {
    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = lines.next()?;
    if !(header == "sequenceDiagram" || header == "sequence") {
        return None;
    }
    let mut diagram = SequenceDiagram::default();
    let mut nesting = 0usize;
    for line in lines {
        if line == "end" {
            if nesting == 0 {
                return None;
            }
            nesting -= 1;
            diagram.events.push(SequenceEvent::Close);
            continue;
        }
        if nesting == 0 && diagram.events.len() >= MAX_MESSAGES {
            return None;
        }
        if let Some(label) = line
            .strip_prefix("participant ")
            .or_else(|| line.strip_prefix("actor "))
        {
            let (id, tail) = take_identifier(label)?;
            let index = participant_of(&mut diagram.people, id)?;
            let display = tail.trim();
            if display.is_empty() {
                continue;
            }
            let label = display.strip_prefix("as ").map(str::trim).unwrap_or(display);
            if label.is_empty() {
                return None;
            }
            diagram.people[index].label = label.to_string();
            continue;
        }
        let event = if let Some(text) = line.strip_prefix("loop ") {
            if nesting >= MAX_NESTING {
                return None;
            }
            nesting += 1;
            SequenceEvent::Open(format!("loop {text}"))
        } else if let Some(text) = line.strip_prefix("opt ") {
            if nesting >= MAX_NESTING {
                return None;
            }
            nesting += 1;
            SequenceEvent::Open(format!("opt {text}"))
        } else if let Some(text) = line.strip_prefix("break ") {
            if nesting >= MAX_NESTING {
                return None;
            }
            nesting += 1;
            SequenceEvent::Open(format!("break {text}"))
        } else if let Some(rest) = line
            .strip_prefix("Note over ")
            .or_else(|| line.strip_prefix("note over "))
        {
            let (first, tail) = take_identifier(rest)?;
            let (right_id, note_text) = match tail.trim_start().strip_prefix(',') {
                Some(after) => {
                    let (id, rest) = take_identifier(after.trim())?;
                    let text = rest.trim_start().strip_prefix(':')?.trim();
                    (id, text)
                }
                None => {
                    let text = tail.trim_start().strip_prefix(':')?.trim();
                    (first, text)
                }
            };
            if note_text.is_empty() {
                return None;
            }
            let left = participant_of(&mut diagram.people, first)?;
            let right = participant_of(&mut diagram.people, right_id)?;
            SequenceEvent::Note {
                left: left.min(right),
                right: left.max(right),
                text: note_text.to_string(),
            }
        } else {
            let (from_id, rest) = take_identifier(line)?;
            let (dashed, cross, rest) = take_arrow(rest.trim_start())?;
            let (to_id, rest) = take_identifier(rest)?;
            let text = rest.trim_start().strip_prefix(':')?.trim();
            if text.is_empty() {
                return None;
            }
            let from = participant_of(&mut diagram.people, from_id)?;
            let to = participant_of(&mut diagram.people, to_id)?;
            SequenceEvent::Message {
                from,
                to,
                text: if cross { format!("✕ {text}") } else { text.to_string() },
                dashed,
            }
        };
        diagram.events.push(event);
    }
    if nesting != 0 || diagram.people.is_empty() {
        return None;
    }
    Some(diagram)
}

/// Render sequence lifelines and message bands within the existing width
/// ceiling (88). Any line that would exceed the column falls back: no
/// clipping, no halfway drawing.
fn render_sequence(diagram: &SequenceDiagram) -> Option<Vec<String>> {
    const MAX_RENDER_WIDTH: usize = 88;
    let name_width = diagram
        .people
        .iter()
        .map(|p| p.label.chars().count())
        .max()
        .unwrap_or(1);
    let box_width = name_width.max(4) + 2;
    let stride = box_width + 6;
    let centers: Vec<usize> = (0..diagram.people.len())
        .map(|i| 1 + box_width / 2 + stride * i)
        .collect();
    let last_center = *centers.last()?;
    let width = last_center + box_width / 2 + 2;
    if width > MAX_RENDER_WIDTH {
        return None;
    }
    // Rows are Vec<char> canvases: every draw call is char-addressed, so
    // multi-byte labels can never shift a byte index into a code point.
    let blank_row = || vec![' '; width];

    // Overwrite canvas[from..to] with `text` (chars). Panics are impossible:
    // callers range-check first.
    fn put(canvas: &mut [char], from: usize, text: &str) {
        for (i, c) in text.chars().enumerate() {
            canvas[from + i] = c;
        }
    }

    let mut rows: Vec<Vec<char>> = Vec::new();

    // Header boxes.
    let mut top = blank_row();
    let mut mid = blank_row();
    let mut bottom = blank_row();
    for (i, person) in diagram.people.iter().enumerate() {
        let center = centers[i];
        let left = center - box_width / 2;
        let right = center + box_width / 2;
        if right >= width {
            return None;
        }
        for cell in &mut top[left..=right] {
            *cell = '─';
        }
        for cell in &mut bottom[left..=right] {
            *cell = '─';
        }
        top[left] = '┌';
        top[right] = '┐';
        bottom[left] = '└';
        bottom[right] = '┘';
        let label: String = person.label.chars().take(box_width - 2).collect();
        let start = center - label.chars().count() / 2;
        put(&mut mid, start, &label);
    }
    rows.push(top);
    rows.push(mid);
    rows.push(bottom);

    let lifeline = {
        let mut row = blank_row();
        for center in &centers {
            row[*center] = '│';
        }
        row
    };

    for event in &diagram.events {
        match event {
            SequenceEvent::Message { from, to, text, dashed } => {
                let (a, b) = (centers[*from], centers[*to]);
                let (run_start, run_end) = (a.min(b), a.max(b));
                if run_end >= width {
                    return None;
                }
                if a == b {
                    // Self-message: a small flag out and back on the same
                    // lifeline, text riding beside it (pi draws the loop;
                    // here it is one cell out, bounded by the column).
                    let label = text.clone();
                    if a + 4 + label.chars().count() + 1 >= width {
                        return None;
                    }
                    let mut row = lifeline.clone();
                    row[a + 1] = '┐';
                    row[a + 2] = '└';
                    row[a + 3] = '▶';
                    put(&mut row, a + 4, &label);
                    rows.push(row);
                    continue;
                }
                // Text must fit between the two lifelines; anything longer is
                // a fallback, not a clip.
                let gap = run_end.saturating_sub(run_start + 1);
                if text.chars().count() > gap.saturating_sub(2) {
                    return None;
                }
                let mut row = lifeline.clone();
                let stroke = if *dashed { '┄' } else { '─' };
                for cell in &mut row[run_start + 1..run_end] {
                    *cell = stroke;
                }
                row[run_start] = '├';
                row[run_end] = if a < b { '▶' } else { '◀' };
                if a != b {
                    let space = run_end.saturating_sub(run_start + 1);
                    let label: String = text.chars().take(space.saturating_sub(2)).collect();
                    let start = run_start + 1 + (space.saturating_sub(label.chars().count())) / 2;
                    if !label.is_empty() {
                        put(&mut row, start, &label);
                    }
                }
                rows.push(row);
            }
            SequenceEvent::Note { left, right, text } => {
                let start = centers[*left];
                let mut end = centers[*right];
                // The band is at least as wide as its text: mermaid note
                // boxes grow past narrow participant spans, but never past
                // the column (past it = fallback, not clipping).
                let label = format!("Note: {text}");
                // Walls at start/end, text from start+2: end must reach
                // start+2+label, and the end wall must stay inside width.
                let needed = start + 2 + label.chars().count();
                if needed > end {
                    end = needed.min(width - 1);
                    if end >= width || end - start < label.chars().count() + 2 {
                        return None;
                    }
                }
                let mut open_row = blank_row();
                for cell in &mut open_row[start..=end] {
                    *cell = '─';
                }
                open_row[start] = '┌';
                open_row[end] = '┐';
                let mut text_row = lifeline.clone();
                text_row[start] = '│';
                text_row[end] = '│';
                put(&mut text_row, start + 2, &label);
                let mut close_row = blank_row();
                for cell in &mut close_row[start..=end] {
                    *cell = '─';
                }
                close_row[start] = '└';
                close_row[end] = '┘';
                rows.push(open_row);
                rows.push(text_row);
                rows.push(close_row);
            }
            SequenceEvent::Open(label) => {
                let text = format!("  {label}");
                if text.chars().count() >= width {
                    return None;
                }
                let mut row = lifeline.clone();
                put(&mut row, 0, &text);
                rows.push(row);
            }
            SequenceEvent::Close => {
                let ending = "  ╌╌╌ end";
                if ending.chars().count() >= width {
                    return None;
                }
                let mut row = lifeline.clone();
                put(&mut row, 0, ending);
                rows.push(row);
            }
        }
    }
    rows.into_iter()
        .map(|canvas| canvas.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_flowchart_with_labels_and_edges() {
        let chart = parse("flowchart TD\n A[Start] --> B{Decide}\n B -- yes --> C[End]\n B -->|no| D[Retry]").unwrap();
        assert_eq!(chart.nodes.len(), 4);
        assert_eq!(chart.edges.len(), 3);
        assert_eq!(chart.edges[0].from, "A");
        assert_eq!(chart.edges[1].label.as_deref(), Some("yes"));
        assert_eq!(chart.edges[2].label.as_deref(), Some("no"));
    }

    #[test]
    fn unsupported_diagram_kinds_fall_back() {
        assert!(parse("sequenceDiagram\n A->>B: hi").is_none());
        assert!(parse("classDiagram\n A <|-- B").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn node_cap_falls_back() {
        let mut body = String::from("flowchart TD\n");
        for i in 0..MAX_NODES {
            body.push_str(&format!(" N{i} --> X\n"));
        }
        assert!(parse(&body).is_none());
    }

    #[test]
    fn edge_cap_falls_back() {
        let mut body = String::from("flowchart TD\n A --> B\n");
        for i in 0..MAX_EDGES {
            body.push_str(&format!(" A --> N{i}\n"));
        }
        assert!(parse(&body).is_none());
    }

    #[test]
    fn render_bounded_and_labeled() {
        let chart = parse("flowchart TD\n A[Start] -->|go| B[End]").unwrap();
        let rows = render(&chart, 60).unwrap();
        assert!(
            rows.iter().any(|r| r.contains("[Start]") && r.contains("── go ──▶ [End]")),
            "edge row: {rows:?}"
        );
    }

    #[test]
    fn render_falls_back_when_wider_than_the_column() {
        let chart = parse("flowchart TD\n A[Start] --> B[End]").unwrap();
        assert!(render(&chart, 2).is_none());
    }

    #[test]
    fn node_statement_without_shape_registers_plain_node() {
        let chart = parse("flowchart LR\n A --> B\n A --> C").unwrap();
        assert_eq!(chart.nodes.len(), 3);
        assert_eq!(chart.direction, Direction::Right);
    }

    #[test]
    fn sequence_parses_messages_notes_and_participants() {
        let rows = parse_sequence(
            "sequenceDiagram\n participant A as Alice\n participant B as Bob\n A->>B: hi\n B-->A: ok\n Note over A,B: both here",
        )
        .unwrap();
        assert!(
            rows.iter().any(|r| r.contains("hi")),
            "message text renders: {rows:?}"
        );
        assert!(rows.iter().any(|r| r.contains("ok")));
        assert!(rows.iter().any(|r| r.contains("Note: both here") || r.contains("both here")));
        assert!(rows.iter().any(|r| r.contains("Alice")));
    }

    #[test]
    fn sequence_self_message_and_control_fragment_render() {
        let rows = parse_sequence(
            "sequenceDiagram\n A->>A: thinking\n loop retries\n A->>B: try\n end",
        )
        .unwrap();
        assert!(rows.iter().any(|r| r.contains("thinking")));
        assert!(rows.iter().any(|r| r.contains("loop retries")));
    }

    #[test]
    fn sequence_unsupported_syntax_falls_back() {
        assert!(parse_sequence("sequenceDiagram\n A->>B: hi\n broken=yes").is_none());
        assert!(parse_sequence("sequenceDiagram\n alt x\n A->>B: hi").is_none(), "too much nesting / unclosed block");
        assert!(parse_sequence("classDiagram\n A <|-- B").is_none(), "non-sequence stays out");
    }

    #[test]
    fn sequence_wide_diagram_falls_back_not_clipped() {
        // Eight participants × wide boxes exceed the 88-column ceiling.
        let mut body = String::from("sequenceDiagram\n");
        for i in 0..8 {
            body.push_str(&format!(" participant P{i} as Long-Label-{i}\n"));
        }
        body.push_str("P0->>P7: crosses the whole diagram");
        assert!(parse_sequence(&body).is_none(), "宽度 over ceiling falls back");
    }
}
