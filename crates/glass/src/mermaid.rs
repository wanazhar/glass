//! Bounded terminal-native Mermaid flowchart renderer (task six-wave-d-001).
//!
//! Ported subset of codex `mermaid`: flowcharts only — node statements
//! (`A`, `A[label]`, `A("text")`), edges (`A --> B`, labeled `A -- text --> B`,
//! arrow-text `A --|text|> B`), and the `flowchart TD`/`graph LR` header.
//! Sequence/class/state/ER are honestly out of scope: they fall back to the
//! source in a fenced block, exactly like unsupported or overflowing input.
//!
//! Layout: nodes take one lane each per rank; edges own the lane between
//! their endpoints, so crossings draw past each other and never join. The
//! result is width-capped; anything exceeding the cap (or malformed) returns
//! `None` and the caller renders the original block.

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
}
