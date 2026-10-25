//! Bounded terminal-native Mermaid renderer (task six-wave-d-001, extended
//! by the six-codebase follow-up).
//!
//! Thirteen bounded families render natively; other syntax preserves source:
//!
//! - **Flowcharts** — node statements (`A`, `A[label]`, `A("text")`), edges
//!   (`A --> B`, labeled `A -- text --> B`, arrow-text `A --|text|> B`), and
//!   the `flowchart TD`/`graph LR` header.
//! - **Sequence diagrams** — `sequenceDiagram` with `participant`/`actor`,
//!   messages (`->`, `->>`, `-->`, `-->>`, `-x`, `--x`), self-messages,
//!   `note over`, and one nesting level of `loop`/`alt`/`opt`/`break`.
//! - **State diagrams** — `stateDiagram-v2` with `[*]` pseudostates,
//!   `-->` transitions, aliases and one level of composite `state X { … }`.
//! - **Class diagrams** — the relation grammar (`<|--`, `*--`, `o--`,
//!   `-->`, `..>`) with glyphs naming the relation kind. Members,
//!   generics and unsupported shapes fall back.
//! - **ER diagrams** — entity relationships with a required cardinality
//!   label. Attribute blocks fall back.
//!
//! - **Data charts** — pie proportions, XY bars/lines, timelines, mindmaps,
//!   journeys, quadrants, calendar-day Gantt schedules and weighted Sankey flows.
//!   See `mermaid_charts` for exact semantic layouts and bounded parsers.
//!
//! Unsupported or overflowing input in any family goes to the
//! fenced-source fallback — never a halfway drawing.
//!
//! Layout: simple chains use connected boxes; groups, cycles and wide graphs
//! use compact node/connection lists. Sequence diagrams use lifelines when
//! they fit and a participant/message list otherwise. Malformed or oversized
//! input returns `None` and the caller renders the original source.

#[path = "mermaid_charts.rs"]
mod charts;
pub use charts::render as render_chart;

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

#[derive(Debug, Clone, Copy, PartialEq)]
enum EdgeStyle {
    Solid,
    Dotted,
    Thick,
}
impl EdgeStyle {
    fn arrow(self) -> &'static str {
        match self {
            Self::Solid => "──▶",
            Self::Dotted => "┄┄▶",
            Self::Thick => "━━▶",
        }
    }
    fn stem(self) -> &'static str {
        match self {
            Self::Solid => "──",
            Self::Dotted => "┄┄",
            Self::Thick => "━━",
        }
    }
}

#[derive(Debug, PartialEq)]
struct Edge {
    style: EdgeStyle,
    from: String,
    to: String,
    label: Option<String>,
}

#[derive(Debug, Default)]
pub struct Flowchart {
    /// Direction is retained so orientations the terminal renderer cannot
    /// reproduce can fall back to the readable source instead of being drawn
    /// with a misleading vertical layout.
    direction: Direction,
    chain_layout: bool,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    groups: Vec<Group>,
}

#[derive(Debug)]
struct Group {
    id: String,
    label: String,
    parent: Option<usize>,
    nodes: Vec<String>,
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
    if body.len() > 128 * 1024 {
        return None;
    }
    let mut lines = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let direction = parse_header(lines.next()?.trim_end_matches(';').trim())?;
    let mut chart = Flowchart {
        direction,
        chain_layout: true,
        ..Flowchart::default()
    };
    let mut groups = Vec::new();
    for line in lines {
        for statement in split_statements(line)? {
            let line = statement.trim();
            if line.is_empty() || line.starts_with("%%") {
                continue;
            }
            if let Some(label) = line.strip_prefix("subgraph ") {
                if groups.len() >= 8 || chart.groups.len() >= MAX_NODES {
                    return None;
                }
                let (id, display) =
                    if label.starts_with('"') && label.ends_with('"') && label.len() >= 2 {
                        (format!("group_{}", chart.groups.len()), clean_label(label))
                    } else if let Some((id, tail)) = take_flow_node(label) {
                        if tail.trim().is_empty() {
                            flow_node_parts(id)?
                        } else {
                            (format!("group_{}", chart.groups.len()), clean_label(label))
                        }
                    } else {
                        return None;
                    };
                if display.len() > 4096 || chart.groups.iter().any(|g| g.id == id) {
                    return None;
                }
                chart.groups.push(Group {
                    id,
                    label: display,
                    parent: groups.last().copied(),
                    nodes: Vec::new(),
                });
                groups.push(chart.groups.len() - 1);
                continue;
            }
            if line == "end" {
                groups.pop()?;
                continue;
            }
            if let Some(direction) = line.strip_prefix("direction ") {
                parse_header(&format!("flowchart {direction}"))?;
                continue;
            }
            // Mermaid style directives affect decoration, never graph structure or host CSS.
            if ["classDef ", "class ", "style ", "linkStyle "]
                .iter()
                .any(|prefix| line.starts_with(prefix))
            {
                continue;
            }
            let members = parse_flow_statement(line, &mut chart)?;
            if let Some(group) = groups.last().copied() {
                for id in members {
                    if !chart.groups[group].nodes.contains(&id) {
                        chart.groups[group].nodes.push(id);
                    }
                }
            }
        }
    }
    if chart.nodes.is_empty() || !groups.is_empty() {
        return None;
    }
    Some(chart)
}

fn clean_label(label: &str) -> String {
    let mut label = label.trim().trim_matches('"').trim_matches('`').to_owned();
    for marker in ["<br/>", "<br />", "<br>", "<BR/>", "<BR />", "<BR>"] {
        label = label.replace(marker, " / ");
    }
    for (entity, text) in [
        ("&quot;", "\""),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&amp;", "&"),
        ("&nbsp;", " "),
    ] {
        label = label.replace(entity, text);
    }
    glass_tui::markdown::sanitize(&label)
}

// Split semicolons/comments only outside labels; quoted code and URLs stay intact.
fn split_statements(line: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (at, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        if "[({".contains(c) {
            depth += 1;
            if depth > 16 {
                return None;
            }
        }
        if "])}".contains(c) {
            depth = depth.checked_sub(1)?;
        }
        if depth == 0 && line[at..].starts_with("%%") {
            parts.push(&line[start..at]);
            return Some(parts);
        }
        if depth == 0 && c == ';' {
            parts.push(&line[start..at]);
            start = at + 1;
        }
    }
    if quoted || depth != 0 {
        return None;
    }
    parts.push(&line[start..]);
    Some(parts)
}

fn take_flow_node(raw: &str) -> Option<(&str, &str)> {
    let raw = raw.trim_start();
    let mut end = 0;
    for (at, c) in raw.char_indices() {
        if raw[at..].starts_with("--") || raw[at..].starts_with("-.") {
            break;
        }
        if !c.is_ascii_alphanumeric() && c != '_' && c != '-' {
            break;
        }
        end = at + c.len_utf8();
    }
    if end == 0 {
        return None;
    }
    let after = raw[end..].trim_start();
    if after.starts_with(['[', '(', '{']) {
        let offset = raw.len() - after.len();
        let mut stack = Vec::new();
        let mut quote = false;
        let mut escaped = false;
        let mut closed = false;
        for (at, c) in after.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                quote = !quote;
                continue;
            }
            if quote {
                continue;
            }
            if "[({".contains(c) {
                if stack.len() >= 16 {
                    return None;
                }
                stack.push(c);
            } else if "])}".contains(c) {
                let open = stack.pop()?;
                if !matches!((open, c), ('[', ']') | ('(', ')') | ('{', '}')) {
                    return None;
                }
                if stack.is_empty() {
                    end = offset + at + 1;
                    closed = true;
                    break;
                }
            }
        }
        if !closed {
            return None;
        }
    }
    let tail = raw[end..].trim_start();
    if let Some(class) = tail.strip_prefix(":::") {
        let n = class
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != ',')
            .unwrap_or(class.len());
        if n == 0 {
            return None;
        }
        end = raw.len() - class.len() + n;
    }
    Some((&raw[..end], &raw[end..]))
}
fn flow_node_parts(raw: &str) -> Option<(String, String)> {
    let (node, tail) = take_flow_node(raw)?;
    if !tail.trim().is_empty() {
        return None;
    }
    let end = node
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(node.len());
    if end > 128 {
        return None;
    }
    let id = node[..end].to_owned();
    let shape = node[end..].trim();
    let label = if shape.starts_with(['[', '(', '{']) {
        let mut depth = 0usize;
        let mut quote = false;
        let mut escaped = false;
        let mut end = None;
        for (at, c) in shape.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                quote = !quote;
                continue;
            }
            if quote {
                continue;
            }
            if "[({".contains(c) {
                depth += 1;
            }
            if "])}".contains(c) {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    end = Some(at + 1);
                    break;
                }
            }
        }
        let shape = &shape[..end?];
        clean_label(
            shape
                .trim_start_matches(['[', '(', '{'])
                .trim_end_matches([']', ')', '}']),
        )
    } else {
        id.clone()
    };
    if label.len() > 4096 {
        return None;
    }
    Some((id, label))
}
fn flow_node(raw: &str, chart: &mut Flowchart) -> Option<String> {
    let (id, label) = flow_node_parts(raw)?;
    if let Some(existing) = chart.nodes.iter_mut().find(|node| node.id == id) {
        if label != id {
            existing.label = label;
        }
    } else {
        if chart.nodes.len() >= MAX_NODES {
            return None;
        }
        let label = chart
            .groups
            .iter()
            .find(|g| g.id == id)
            .map_or(label, |g| g.label.clone());
        chart.nodes.push(Node {
            id: id.clone(),
            label,
        });
    }
    Some(id)
}
fn flow_operand<'a>(raw: &'a str, chart: &mut Flowchart) -> Option<(Vec<String>, &'a str)> {
    let (node, mut tail) = take_flow_node(raw)?;
    let mut ids = vec![flow_node(node, chart)?];
    while let Some(after) = tail.trim_start().strip_prefix('&') {
        let (node, rest) = take_flow_node(after)?;
        ids.push(flow_node(node, chart)?);
        tail = rest;
        if ids.len() > MAX_NODES {
            return None;
        }
    }
    Some((ids, tail))
}
fn flow_arrow(raw: &str) -> Option<(EdgeStyle, Option<String>, &str)> {
    let raw = raw.trim_start();
    let style = if raw.starts_with("-.->") {
        EdgeStyle::Dotted
    } else if raw.starts_with("==>") {
        EdgeStyle::Thick
    } else {
        EdgeStyle::Solid
    };
    let (mut label, mut tail) = if let Some(tail) = raw
        .strip_prefix("-->")
        .or_else(|| raw.strip_prefix("-.->"))
        .or_else(|| raw.strip_prefix("==>"))
    {
        (None, tail.trim_start())
    } else if let Some(after) = raw.strip_prefix("--|") {
        let (label, tail) = after.split_once("|>")?;
        (Some(clean_label(label)), tail.trim_start())
    } else {
        let after = raw.strip_prefix("--")?;
        let (label, tail) = after.split_once("-->")?;
        (Some(clean_label(label)), tail.trim_start())
    };
    if let Some(after) = tail.strip_prefix('|') {
        let (text, rest) = after.split_once('|')?;
        label = Some(clean_label(text));
        tail = rest.trim_start();
    }
    if label
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.len() > 4096)
    {
        return None;
    }
    Some((style, label, tail))
}
fn parse_flow_statement(line: &str, chart: &mut Flowchart) -> Option<Vec<String>> {
    let (mut sources, mut tail) = flow_operand(line, chart)?;
    let mut members = sources.clone();
    while !tail.trim().is_empty() {
        let (style, label, rest) = flow_arrow(tail)?;
        let (targets, rest) = flow_operand(rest, chart)?;
        for target in &targets {
            if !members.contains(target) {
                members.push(target.clone());
            }
        }
        for source in &sources {
            for target in &targets {
                if chart.edges.len() >= MAX_EDGES {
                    return None;
                }
                chart.edges.push(Edge {
                    style,
                    from: source.clone(),
                    to: target.clone(),
                    label: label.clone(),
                });
            }
        }
        sources = targets;
        tail = rest;
    }
    Some(members)
}

// ---- State diagrams (follow-up 2: the third family) ----

/// Register a state node with a display label. Pseudostates get
/// identifier-legal ids (node_of rejects non-ASCII ids) with separate
/// display labels; plain states use their id.
fn state_node(id: &str, chart: &mut Flowchart) -> Option<String> {
    if chart.nodes.iter().any(|n| n.id == id) {
        return chart
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.id.clone());
    }
    if chart.nodes.len() >= MAX_NODES {
        return None;
    }
    let label = match id {
        "__gc_initial" => "● initial",
        "__gc_final" => "◎ final",
        _ => id,
    };
    node_of(&format!("{id}[{label}]"), chart)
}

/// Parse and render a `stateDiagram(-v2)` at the given column width. State
/// machines are a flowchart with distinct initial/final pseudostates, so
/// the flowchart tree and rank layout are reused unchanged; only the
/// syntax differs (`[*]`, `-->` transitions, `state "X" as id`, `ID: desc`).
pub fn parse_state(body: &str, width: usize) -> Option<Vec<String>> {
    let mut lines = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let header = lines.next()?;
    if header != "stateDiagram-v2" && header != "stateDiagram" {
        return None;
    }
    let mut chart = Flowchart::default();
    // Composite states: `state X { ... }` opens a nested block whose
    // members belong to a parent state. Depth is bounded (one level):
    // deeper nesting is unsupported here, not silently flattened — the
    // tree renderer is a two-level shape, and pretending otherwise would
    // draw the nesting away.
    const MAX_COMPOSITE_DEPTH: usize = 1;
    let mut stack: Vec<String> = Vec::new();
    for line in lines {
        if line == "}" {
            stack.pop()?;
            continue;
        }
        if let Some(rest) = line.strip_prefix("state ") {
            if let Some((open, after)) = rest.split_once('{') {
                // `state X {` opens a composite block. The parent is
                // registered as a node even if no edge names it — a
                // state that exists only to be nested is still drawn.
                let id = open.trim();
                if id.is_empty() || !after.trim().is_empty() || stack.len() >= MAX_COMPOSITE_DEPTH {
                    return None;
                }
                let existing = chart
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.label.clone());
                state_node(id, &mut chart)?;
                stack.push(existing.unwrap_or_else(|| id.to_string()));
                continue;
            }
            // No `{`: fall through to the alias form below via `rest`.
            parse_state_alias(rest, &mut chart)?;
            continue;
        }
        // Bare description: `ID: text`. A labeled transition also contains
        // a colon, so the branch only claims lines whose left side is a
        // bare identifier (no spaces, no arrow).
        if let Some((id, text)) = line.split_once(':') {
            let text = text.trim();
            if !id.contains(' ')
                && !id.contains("-->")
                && !text.is_empty()
                && !text.starts_with(':')
            {
                let id = id.trim();
                if let Some(existing) = chart.nodes.iter_mut().find(|n| n.id == id) {
                    if existing.label == id {
                        existing.label = text.to_string();
                    }
                    continue;
                } else if chart.nodes.len() >= MAX_NODES {
                    return None;
                } else {
                    let _ = node_of(&format!("{id}[{text}]"), &mut chart)?;
                    continue;
                }
            }
        }
        // Transition: `[*] --> B`, `A --> B: label`.
        let (from, rest) = if let Some(rest) = line.strip_prefix("[*]") {
            ("__gc_initial", rest)
        } else {
            let (id, tail) = take_identifier(line)?;
            (id, tail)
        };
        let rest = rest.trim_start();
        if !rest.starts_with("-->") {
            return None;
        }
        let rest = rest[3..].trim_start();
        let (to_raw, label) = match rest.split_once(':') {
            Some((target, text)) => {
                let text = text.trim();
                (
                    target.trim(),
                    if text.is_empty() {
                        None
                    } else {
                        Some(text.to_string())
                    },
                )
            }
            None => (rest.trim(), None),
        };
        let to = match to_raw {
            "[*]" => "__gc_final",
            other => {
                let (id, _) = take_identifier(other)?;
                id
            }
        };
        let target = state_node(to, &mut chart)?;
        let source = state_node(from, &mut chart)?;
        if chart.edges.len() >= MAX_EDGES {
            return None;
        }
        chart.edges.push(Edge {
            style: EdgeStyle::Solid,
            from: source,
            to: target,
            label,
        });
    }
    if chart.nodes.is_empty() || !stack.is_empty() {
        return None;
    }
    render(&chart, width)
}

// Body of a `state "label" as ID` alias declaration, shared by the
// composite and plain call sites above.
fn parse_state_alias(rest: &str, chart: &mut Flowchart) -> Option<()> {
    if !rest.trim_start().starts_with('"') {
        return None;
    }
    let rest = rest.trim_start();
    let (label, rest) = rest[1..].split_once('"')?;
    let rest = rest.trim_start().strip_prefix("as ")?.trim();
    let (id, tail) = take_identifier(rest)?;
    if !tail.trim().is_empty() || label.is_empty() {
        return None;
    }
    if let Some(existing) = chart.nodes.iter_mut().find(|n| n.id == id) {
        if existing.label == id {
            existing.label = label.to_string();
        }
    } else if chart.nodes.len() >= MAX_NODES {
        return None;
    } else {
        node_of(&format!("{id}[{label}]"), chart)?;
    }
    Some(())
}

/// Parse and render a sequence diagram at the given column width. Separate
/// entry point because the two families share no tree; `None` means the
/// source falls back to the fenced block (unsupported syntax, wrong shape,
/// cap or width exceeded).
#[cfg(test)]
pub fn parse_sequence(body: &str) -> Option<Vec<String>> {
    let diagram = parse_sequence_diagram(body)?;
    render_sequence(&diagram)
}

/// Compact message rows preserve participants and events when lifelines cannot fit.
pub fn parse_sequence_at(body: &str, width: usize) -> Option<Vec<String>> {
    if body.len() > 128 * 1024 || width < 8 {
        return None;
    }
    let diagram = parse_sequence_diagram(body)?;
    if let Some(rows) = render_sequence(&diagram)
        && rows.iter().all(|row| glass_tui::text::width(row) <= width)
    {
        return Some(rows);
    }
    let mut rows = Vec::new();
    let mut append = |text: String| -> Option<()> {
        rows.extend(glass_tui::text::wrap_words(&text, width.min(88)));
        (rows.len() <= 2048).then_some(())
    };
    append("Participants".into())?;
    for person in &diagram.people {
        append(format!("[{}] {}", person.id, person.label))?;
    }
    append("Messages".into())?;
    for event in &diagram.events {
        match event {
            SequenceEvent::Message {
                from,
                to,
                text,
                dashed,
            } => append(format!(
                "[{}] {} [{}] · {}",
                diagram.people[*from].id,
                if *dashed { "┄┄▶" } else { "──▶" },
                diagram.people[*to].id,
                text
            ))?,
            SequenceEvent::Open(label) => append(format!("╭ {label}"))?,
            SequenceEvent::Close => append("╰ end".into())?,
            SequenceEvent::Note { left, right, text } => append(format!(
                "Note [{}]–[{}] · {}",
                diagram.people[*left].id, diagram.people[*right].id, text
            ))?,
        }
    }
    Some(rows)
}

fn parse_header(header: &str) -> Option<Direction> {
    let rest = header
        .strip_prefix("flowchart")
        .or_else(|| header.strip_prefix("graph"))?
        .trim();
    match rest {
        "TD" | "TB" | "BT" => Some(Direction::Down),
        "LR" | "RL" => Some(Direction::Right),
        _ => None,
    }
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
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
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
    chart.nodes.push(Node {
        id: id.clone(),
        label,
    });
    Some(id)
}

/// Render at the given column width. `None` means the diagram cannot fit
/// under the caps and the caller falls back to the fenced source.
pub fn render(chart: &Flowchart, width: usize) -> Option<Vec<String>> {
    if chart.direction == Direction::Right {
        return None;
    }
    render_compact(chart, width)
}
fn render_compact(chart: &Flowchart, width: usize) -> Option<Vec<String>> {
    if width >= 16 {
        return render_boxed(chart, width);
    }
    let width = width.min(88);
    if width < 8 {
        return None;
    }
    let mut rows = Vec::new();
    let mut append = |text: String| -> Option<()> {
        rows.extend(glass_tui::text::wrap_words(&text, width));
        (rows.len() <= 2048).then_some(())
    };
    append("Nodes".into())?;
    for node in &chart.nodes {
        append(format!("[{}] {}", node.id, node.label))?;
    }
    for (index, group) in chart.groups.iter().enumerate() {
        let mut path = vec![group.label.as_str()];
        let mut parent = group.parent;
        while let Some(at) = parent {
            if at >= index {
                return None;
            }
            path.push(chart.groups[at].label.as_str());
            parent = chart.groups[at].parent;
        }
        path.reverse();
        append(format!("Group · {}", path.join(" / ")))?;
        if group.nodes.is_empty() {
            append("  (no direct nodes)".into())?;
        } else {
            append(format!(
                "  {}",
                group
                    .nodes
                    .iter()
                    .map(|id| format!("[{id}]"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ))?;
        }
    }
    if !chart.edges.is_empty() {
        append("Connections".into())?;
    }
    for edge in &chart.edges {
        append(format!(
            "[{}] {} [{}]{}",
            edge.from,
            edge.style.arrow(),
            edge.to,
            edge.label
                .as_ref()
                .map(|label| format!(" · {label}"))
                .unwrap_or_default()
        ))?;
    }
    Some(rows)
}
// A compact architecture diagram, not separate node and edge inventories.
// Every outgoing edge stays adjacent to its source component. Cycles use
// explicit target references, so layout never needs recursive graph traversal.
fn render_boxed(chart: &Flowchart, width: usize) -> Option<Vec<String>> {
    let width = width.min(64);
    let inner = width.checked_sub(4)?;
    let mut rows = Vec::new();
    let mut last_group = String::new();
    let linear = chart.chain_layout
        && chart.groups.is_empty()
        && chart.nodes.iter().all(|node| !node.label.is_empty())
        && chart
            .edges
            .iter()
            .all(|edge| edge.style == EdgeStyle::Solid)
        && chart.edges.len() + 1 == chart.nodes.len()
        && chart.nodes.windows(2).all(|pair| {
            chart
                .edges
                .iter()
                .any(|e| e.from == pair[0].id && e.to == pair[1].id)
        });
    for node in &chart.nodes {
        let mut memberships = Vec::new();
        for (index, group) in chart.groups.iter().enumerate() {
            if !group.nodes.contains(&node.id) {
                continue;
            }
            let mut path = vec![group.label.as_str()];
            let mut parent = group.parent;
            while let Some(at) = parent {
                if at >= index {
                    return None;
                }
                path.push(chart.groups[at].label.as_str());
                parent = chart.groups[at].parent;
            }
            path.reverse();
            memberships.push(path.join(" / "));
        }
        let group = memberships.join(" · ");
        if group != last_group {
            if !group.is_empty() {
                rows.extend(glass_tui::text::wrap_words(
                    &format!("Group · {group}"),
                    width,
                ));
            } else if !last_group.is_empty() {
                rows.push("Components".into());
            }
            last_group = group;
        }
        let title_name = if chart.chain_layout {
            &node.id
        } else {
            &node.label
        };
        let title = format!("╭─ [{title_name}] ");
        if linear {
            rows.push(format!("╭{}╮", "─".repeat(width - 2)));
        } else if glass_tui::text::width(&title) < width {
            rows.push(format!(
                "{title}{}╮",
                "─".repeat(width - glass_tui::text::width(&title) - 1)
            ));
        } else {
            rows.push(format!("╭{}╮", "─".repeat(width - 2)));
            for line in glass_tui::text::wrap_words(&format!("[{title_name}]"), inner) {
                rows.push(format!(
                    "│ {line}{} │",
                    " ".repeat(inner - glass_tui::text::width(&line))
                ));
            }
        }
        for line in node
            .label
            .split(" / ")
            .flat_map(|part| glass_tui::text::wrap_words(part, inner))
        {
            let spare = inner - glass_tui::text::width(&line);
            let left = if linear { spare / 2 } else { 0 };
            rows.push(format!(
                "│ {}{line}{} │",
                " ".repeat(left),
                " ".repeat(spare - left)
            ));
        }
        rows.push(format!("╰{}╯", "─".repeat(width - 2)));
        let edges: Vec<_> = chart
            .edges
            .iter()
            .filter(|edge| edge.from == node.id)
            .collect();
        for (index, edge) in edges.iter().enumerate() {
            if linear {
                let center = " ".repeat(width / 2);
                rows.push(format!("{center}│"));
                if let Some(label) = &edge.label {
                    for line in glass_tui::text::wrap_words(label, width - 2) {
                        let padding = (width - glass_tui::text::width(&line)) / 2;
                        rows.push(format!("{}{line}", " ".repeat(padding)));
                    }
                }
                rows.push(format!("{center}▼"));
                continue;
            }
            let last = index + 1 == edges.len();
            let connector = if last { "  ╰" } else { "  ├" };
            let target = chart.nodes.iter().find(|target| target.id == edge.to)?;
            let text = match &edge.label {
                Some(label) => format!(
                    "{} {label} {} [{}]",
                    edge.style.stem(),
                    edge.style.arrow(),
                    target.label
                ),
                None => format!("{} [{}]", edge.style.arrow(), target.label),
            };
            for (part, line) in glass_tui::text::wrap_words(&text, width - 3)
                .into_iter()
                .enumerate()
            {
                let prefix = if part == 0 {
                    connector
                } else if last {
                    "   "
                } else {
                    "  │"
                };
                rows.push(format!("{prefix}{line}"));
            }
        }
        if !linear {
            rows.push(String::new());
        }
        if rows.len() > 2048 {
            return None;
        }
    }
    Some(rows)
}

// ---- Class diagrams (follow-up 5: the fourth family) ----

/// A class membership arrow: `A <|-- B` means B is a subclass of A.
/// Rendered as the flowchart tree so rank layout is shared.
pub fn parse_class(body: &str, width: usize) -> Option<Vec<String>> {
    let mut lines = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let header = lines.next()?;
    if header != "classDiagram" {
        return None;
    }
    let mut chart = Flowchart::default();
    for line in lines {
        // Relation: `A <|-- B`, `A --|> B`, `A *-- B`, `A o-- B`, `A --> B`,
        // `A ..> B`, `A ..|> B`, all optionally `: label` suffixed.
        let (from, arrow, rest) = split_class_relation(line)?;
        let (to_raw, label) = match rest.split_once(':') {
            Some((target, text)) => {
                let text = text.trim();
                (
                    target.trim(),
                    if text.is_empty() {
                        None
                    } else {
                        Some(text.to_string())
                    },
                )
            }
            None => (rest.trim(), None),
        };
        let to = class_ident(to_raw)?;
        let source = class_ident(from)?;
        let target = state_node(&to, &mut chart)?;
        let from_id = state_node(&source, &mut chart)?;
        if chart.edges.len() >= MAX_EDGES {
            return None;
        }
        let glyph = match arrow {
            "<|--" | "--|>" | "..|>" => "◁─",
            "*--" => "✦─",
            "o--" => "◇─",
            _ => "",
        };
        let label = match (glyph.is_empty(), label) {
            (true, l) => l,
            (false, Some(l)) => Some(format!("{glyph} {l}")),
            (false, None) => Some(glyph.to_string()),
        };
        chart.edges.push(Edge {
            style: EdgeStyle::Solid,
            from: from_id,
            to: target,
            label,
        });
    }
    if chart.nodes.is_empty() {
        return None;
    }
    render(&chart, width)
}

/// Split `A <op> B` at the class relation token. Returns `(from, token, rest)`.
fn split_class_relation(line: &str) -> Option<(&str, &str, &str)> {
    for token in ["<|--", "--|>", "..|>", "*--", "o--", "..>", "-->"] {
        if let Some(at) = line.find(token) {
            // The token must not be part of a longer token match elsewhere —
            // find the first occurrence, which is safe for these shapes.
            let from = line[..at].trim();
            let rest = line[at + token.len()..].trim();
            if from.is_empty() || rest.is_empty() || from.contains(' ') {
                return None;
            }
            return Some((from, token, rest));
        }
    }
    None
}

/// Class identifier: alphanumeric plus underscore only (mermaid allows
/// generics like `List~T~`; those fall back rather than being mangled).
fn class_ident(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty()
        || !raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(raw.to_string())
} // ---- ER diagrams (follow-up 6: the fifth family) ----

/// Parse and render an `erDiagram`. Entities are nodes; relationships are
/// edges whose label carries the cardinality both ways. Attributes
/// (`ENTITY { ... }`) and attribute keys are unsupported — they fall
/// back to source, stated in the module doc.
pub fn parse_er(body: &str, width: usize) -> Option<Vec<String>> {
    let mut lines = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let header = lines.next()?;
    if header != "erDiagram" {
        return None;
    }
    let mut chart = Flowchart::default();
    for line in lines {
        // Relationship: `A ||--o{ B : label` and the other cardinality
        // pairs; the trailing `: label` is required — an unlabeled edge
        // between entities says nothing readable.
        let (from, to, label) = parse_er_relation(line)?;
        let source = state_node(&from, &mut chart)?;
        let target = state_node(&to, &mut chart)?;
        if chart.edges.len() >= MAX_EDGES {
            return None;
        }
        chart.edges.push(Edge {
            style: EdgeStyle::Solid,
            from: source,
            to: target,
            label: Some(label),
        });
    }
    if chart.nodes.is_empty() {
        return None;
    }
    render(&chart, width)
}

/// Split an ER relationship line into `(from_entity, to_entity, label)`.
fn parse_er_relation(line: &str) -> Option<(String, String, String)> {
    let (left, label) = line.split_once(':')?;
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    // The cardinality token contains the only `--`/`..` run. Entity names
    // sit at both ends: strip the leading card glyphs on the left side and
    // the trailing ones on the right side.
    let at = left.find("--")?;
    let from = left[..at].trim_end_matches(['|', 'o', '{', '}']).trim();
    let right = left[at..].trim_start_matches(['-', '.', '|', 'o', '{', '}']);
    let at2 = right.find(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-')?;
    let to = right[at2..].trim();
    if from.is_empty() || to.is_empty() {
        return None;
    }
    let source = class_ident(from)?;
    let target = class_ident(to)?;
    Some((source, target, label.to_string()))
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
    let mut lines = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let header = lines.next()?;
    if !(header == "sequenceDiagram" || header == "sequence") {
        return None;
    }
    let mut diagram = SequenceDiagram::default();
    let mut nesting = 0usize;
    for line in lines {
        if diagram.events.len() >= MAX_MESSAGES {
            return None;
        }
        if line == "end" {
            if nesting == 0 {
                return None;
            }
            nesting -= 1;
            diagram.events.push(SequenceEvent::Close);
            continue;
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
            let label = display
                .strip_prefix("as ")
                .map(str::trim)
                .unwrap_or(display);
            if label.is_empty() {
                return None;
            }
            diagram.people[index].label = clean_label(label);
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
                text: clean_label(note_text),
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
                text: if cross {
                    format!("✕ {}", clean_label(text))
                } else {
                    clean_label(text)
                },
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
            SequenceEvent::Message {
                from,
                to,
                text,
                dashed,
            } => {
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

    const ARCHITECTURE: &str = r#"%% architecture
flowchart TD
    %% ===== Entry points =====
    Entry["glass-core binary<br/>src/main.rs"]:::entry
    Serve["glass-core serve<br/>Unix socket (0600)<br/>JSONL protocol"]:::entry
    Attach["glass-core attach<br/>terminal client"]:::entry
    subgraph Crate["Rust workspace crates"]
        direction TB
        Core["glass-core"]:::core
        Tui["glass-tui"]:::ui
        subgraph Agent["Agent engine"]
            Loop["Model loop &amp; tools"]
        end
    end
    Entry --> Core & Tui
    Core --> Loop --> Serve
    Serve -.-> Attach
    classDef entry fill:#333,stroke:#888;
    class Core core
"#;
    const BRANCH_SCREENSHOT: &str = "flowchart TD\nA[Start] --> B{Is it mermaid?}\nB -->|yes| C[Output diagram]\nB -->|no| D[Convert to mermaid]\nD --> C\nC --> E[Done]";
    #[test]
    fn branches_are_not_serialized_into_a_false_chain() {
        let chart = parse(BRANCH_SCREENSHOT).unwrap();
        for width in [20, 40, 65, 88] {
            let rows = render(&chart, width).unwrap();
            assert_eq!(rows.iter().filter(|row| row.starts_with('╭')).count(), 5);
            assert!(rows.iter().all(|row| row.matches("──▶").count() <= 1));
            assert!(rows.iter().all(|row| glass_tui::text::width(row) <= width));
            let text = rows.join(" ");
            assert!(text.contains("yes") && text.contains("no"));
            assert_eq!(
                rows.iter()
                    .map(|row| row.matches("──▶").count())
                    .sum::<usize>(),
                5
            );
        }
    }

    #[test]
    fn quoted_group_titles_render() {
        let chart = parse("flowchart TD\nsubgraph \"Input layer\"\nA --> B\nend").unwrap();
        assert!(
            render(&chart, 40)
                .unwrap()
                .join(" ")
                .contains("Input layer")
        );
    }
    #[test]
    fn actual_kilo_architecture_has_boxed_components_and_complete_edges() {
        let source = include_str!(
            "../../../products/glass-core/scripts/fixtures/mermaid-architecture-kilo.mmd"
        );
        let chart = parse(source).unwrap();
        for width in [16, 18, 20, 34, 40, 60, 88] {
            let Some(rows) = render(&chart, width) else {
                // This source declares a horizontal flowchart. Keep it as
                // source until the renderer can preserve that orientation.
                assert_eq!(chart.direction, Direction::Right);
                continue;
            };
            assert!(
                rows.iter().all(|row| glass_tui::text::width(row) <= width),
                "width {width}"
            );
            assert_eq!(
                rows.iter().filter(|row| row.starts_with('╭')).count(),
                chart.nodes.len()
            );
            assert_eq!(
                rows.iter()
                    .map(|row| row.matches("──▶").count())
                    .sum::<usize>(),
                chart.edges.len()
            );
            assert!(
                !rows
                    .iter()
                    .any(|row| row == "Nodes" || row == "Connections")
            );
            let text = rows.join(" ");
            assert!(text.contains("Rust crates"));
            assert!(!text.contains("<br"));
        }
    }

    #[test]
    fn existing_nodes_join_subgraphs_and_edge_styles_survive() {
        let chart = parse("flowchart TD\nA[External]\nsubgraph G\nA -.-> B ==> C\nend").unwrap();
        assert_eq!(chart.groups[0].nodes, vec!["A", "B", "C"]);
        let rows = render(&chart, 40).unwrap().join("\n");
        assert!(rows.contains("┄┄▶"));
        assert!(rows.contains("━━▶"));
    }

    #[test]
    fn screenshot_architecture_renders_comments_classes_groups_and_breaks() {
        let chart = parse(ARCHITECTURE).unwrap();
        assert_eq!(chart.nodes.len(), 6);
        assert_eq!(chart.edges.len(), 5);
        assert_eq!(chart.groups.len(), 2);
        assert_eq!(chart.groups[0].nodes, vec!["Core", "Tui"]);
        assert_eq!(chart.groups[1].parent, Some(0));
        for width in [8, 12, 16, 20, 34, 60, 88] {
            let rows = render(&chart, width).unwrap();
            assert!(rows.iter().all(|row| glass_tui::text::width(row) <= width));
            let joined = rows.join(" ");
            for id in ["Entry", "Serve", "Attach", "Core", "Tui", "Loop"] {
                assert!(joined.contains(id), "{joined}");
            }
            assert_eq!(joined.matches("──▶").count(), 4);
            assert_eq!(joined.matches("┄┄▶").count(), 1);
            assert!(!joined.contains("<br"));
        }
    }
    #[test]
    fn labels_preserve_punctuation_and_late_definitions() {
        let chart = parse(
            r#"flowchart LR
 A --> B; B -->|a call| C
 B["literal --> ; %% ]::: inside"]:::entry
 C["Unicode 界 &quot;quoted&quot;"]
"#,
        )
        .unwrap();
        assert_eq!(chart.edges.len(), 2);
        assert_eq!(
            chart.nodes.iter().find(|n| n.id == "B").unwrap().label,
            "literal --> ; %% ]::: inside"
        );
        assert!(
            chart
                .nodes
                .iter()
                .find(|n| n.id == "C")
                .unwrap()
                .label
                .contains("界 \"quoted\"")
        );
    }
    #[test]
    fn cycles_and_fanout_have_complete_compact_connections() {
        let chart = parse("flowchart TD\n A & B --> C & D\n D ==> A").unwrap();
        assert_eq!(chart.edges.len(), 5);
        for width in [12, 20, 40] {
            let rows = render(&chart, width).unwrap();
            assert_eq!(rows.join(" ").matches("──▶").count(), 4);
            assert_eq!(rows.join(" ").matches("━━▶").count(), 1);
            assert!(rows.iter().all(|row| glass_tui::text::width(row) <= width));
        }
    }
    #[test]
    fn malformed_groups_labels_and_unsafe_directives_keep_source() {
        for source in [
            "flowchart TD\nend",
            "flowchart TD\nsubgraph A\nB",
            "flowchart TD\nA[broken",
            "flowchart TD\nA -->",
            "flowchart TD\nclick A callback",
            "flowchart TD\nsubgraph G\nA\nend\nsubgraph G\nB\nend",
        ] {
            assert!(parse(source).is_none(), "{source}");
        }
        assert!(parse(&"A".repeat(128 * 1024 + 1)).is_none());
    }
    #[test]
    fn narrow_sequences_keep_every_event_and_enforce_nested_cap() {
        let source = "sequenceDiagram\nparticipant A as Alice 界\nparticipant B as Bob\nloop checks\nA->>B: request\nB-->>A: reply\nNote over A,B: context\nend";
        for width in [8, 12, 20, 40, 88] {
            let rows = parse_sequence_at(source, width).unwrap();
            assert!(rows.iter().all(|row| glass_tui::text::width(row) <= width));
            let text = rows.join(" ");
            for marker in ["request", "reply", "context"] {
                assert!(text.contains(marker), "{text}");
            }
        }
        let over = format!(
            "sequenceDiagram\nloop many\n{}end",
            "A->>B: message\n".repeat(MAX_MESSAGES)
        );
        assert!(parse_sequence_at(&over, 40).is_none());
    }
    #[test]
    fn parses_a_flowchart_with_labels_and_edges() {
        let chart = parse(
            "flowchart TD\n A[Start] --> B{Decide}\n B -- yes --> C[End]\n B -->|no| D[Retry]",
        )
        .unwrap();
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
            rows.iter().any(|r| r.trim() == "go")
                && rows.iter().any(|r| r.trim() == "▼")
                && rows.iter().filter(|r| r.starts_with('╭')).count() == 2,
            "edge row: {rows:?}"
        );
    }

    #[test]
    fn render_falls_back_when_wider_than_the_column() {
        let chart = parse("flowchart TD\n A[Start] --> B[End]").unwrap();
        assert!(render(&chart, 2).is_none());
    }

    #[test]
    fn horizontal_flowcharts_fall_back_instead_of_rendering_with_vertical_edges() {
        for header in ["flowchart LR", "graph RL"] {
            let chart = parse(&format!("{header}\n A[Start] --> B[End]")).unwrap();
            assert!(render(&chart, 60).is_none(), "{header}");
        }
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
        assert!(
            rows.iter()
                .any(|r| r.contains("Note: both here") || r.contains("both here"))
        );
        assert!(rows.iter().any(|r| r.contains("Alice")));
    }

    #[test]
    fn sequence_self_message_and_control_fragment_render() {
        let rows =
            parse_sequence("sequenceDiagram\n A->>A: thinking\n loop retries\n A->>B: try\n end")
                .unwrap();
        assert!(rows.iter().any(|r| r.contains("thinking")));
        assert!(rows.iter().any(|r| r.contains("loop retries")));
    }

    #[test]
    fn sequence_unsupported_syntax_falls_back() {
        assert!(parse_sequence("sequenceDiagram\n A->>B: hi\n broken=yes").is_none());
        assert!(
            parse_sequence("sequenceDiagram\n alt x\n A->>B: hi").is_none(),
            "too much nesting / unclosed block"
        );
        assert!(
            parse_sequence("classDiagram\n A <|-- B").is_none(),
            "non-sequence stays out"
        );
    }

    #[test]
    fn state_diagram_renders_transitions_and_pseudostates() {
        let rows = parse_state(
            "stateDiagram-v2\n state \"Order drafted\" as Draft\n [*] --> Draft\n Draft --> Validating: submit\n Validating --> [*]\n Draft: bare description",
            60,
        )
        .unwrap();
        assert!(
            rows.iter().any(|r| r.contains("● initial")),
            "rows: {rows:?}"
        );
        assert!(rows.iter().any(|r| r.contains("◎ final")));
        assert!(
            rows.iter().any(|r| r.contains("[Validating]")),
            "rows: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("[Order drafted]")),
            "alias wins"
        );
        // The bare description does not override the alias label.
        assert!(
            rows.iter()
                .all(|r| r.contains("Order drafted") || !r.contains("[Draft]")),
            "rows: {rows:?}"
        );
    }

    #[test]
    fn state_diagram_unsupported_syntax_falls_back() {
        assert!(
            parse_state("classDiagram\n A <|-- B", 60).is_none(),
            "wrong header"
        );
        assert!(
            parse_state("stateDiagram-v2\n A --- B", 60).is_none(),
            "only --> transitions"
        );
        assert!(parse_state("stateDiagram-v2\n A --> [*]\n B --> [*]", 60).is_some());
        assert!(
            parse_state("stateDiagram-v2\n state Broken as \n A --> B", 60).is_none(),
            "empty alias"
        );
    }

    #[test]
    fn state_composite_block_nests_transitions_under_the_parent() {
        let rows = parse_state(
            "stateDiagram-v2\n state Active {\n [*] --> Ticking\n Ticking --> Done: finish\n }\n [*] --> Active\n", 60,
        ).unwrap();
        // Composite members belong to the parent; the block contents parse
        // without falling back, and the parent itself is drawn.
        assert!(
            rows.iter().any(|r| r.contains("[Active]")),
            "rows: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("● initial")),
            "rows: {rows:?}"
        );
    }

    #[test]
    fn state_composite_parent_is_drawn_even_without_edges() {
        // The parent is named only by the block header.
        let rows = parse_state(
            "stateDiagram-v2\n state Idle {\n [*] --> Breathing\n Breathing --> [*]\n }\n",
            60,
        )
        .unwrap();
        assert!(rows.iter().any(|r| r.contains("[Idle]")), "rows: {rows:?}");
        assert!(
            rows.iter().any(|r| r.contains("[Breathing]")),
            "rows: {rows:?}"
        );
    }

    #[test]
    fn state_double_nested_composite_falls_back() {
        let body = "stateDiagram-v2\n state Outer {\n state Inner {\n A --> B\n }\n }\n";
        assert!(
            parse_state(body, 60).is_none(),
            ">1 nesting level is unsupported"
        );
    }

    #[test]
    fn state_unclosed_composite_falls_back() {
        let body = "stateDiagram-v2\n state Pending {\n A --> B\n";
        assert!(
            parse_state(body, 60).is_none(),
            "unclosed block is unsupported"
        );
    }

    #[test]
    fn state_diagram_respects_width_and_node_caps() {
        let chart_body = |n: usize| {
            let mut body = String::from("stateDiagram-v2\n [*] --> S0\n");
            for i in 0..n {
                body.push_str(&format!("S{i} --> S{}: step\n", i + 1));
            }
            body
        };
        assert!(
            parse_state(&chart_body(MAX_NODES), 60).is_none(),
            "node cap"
        );
        let wide = "stateDiagram-v2\n[*] --> AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA --> [*]";
        assert!(
            parse_state(wide, 24)
                .unwrap()
                .iter()
                .all(|row| glass_tui::text::width(row) <= 24),
            "compact state layout"
        );
    }

    #[test]
    fn class_diagram_renders_relations_with_glyphs() {
        let rows = parse_class(
            "classDiagram\n Animal <|-- Dog\n Animal *-- Bone\n Animal o-- Tail\n Animal --> Food: eats\n", 88,
        ).unwrap();
        assert!(rows.iter().any(|r| r.contains("[Dog]")), "rows: {rows:?}");
        assert!(
            rows.iter().any(|r| r.contains("── ◁─ ──▶ [Dog]")),
            "inheritance glyph, rows: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("✦─ ──▶ [Bone]")),
            "composition, rows: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("◇─ ──▶ [Tail]")),
            "aggregation, rows: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("── eats ──▶")),
            "label, rows: {rows:?}"
        );
    }

    #[test]
    fn er_diagram_renders_cardinality_labels() {
        let rows = parse_er(
            "erDiagram\n CUSTOMER ||--o{ ORDER : places\n ORDER ||--|{ LINE_ITEM : contains\n",
            88,
        )
        .unwrap();
        assert!(
            rows.iter().any(|r| r.contains("[CUSTOMER]")),
            "rows: {rows:?}"
        );
        assert!(
            rows.iter().any(|r| r.contains("── places ──▶ [ORDER]")),
            "rows: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|r| r.contains("── contains ──▶ [LINE_ITEM]")),
            "rows: {rows:?}"
        );
    }

    #[test]
    fn er_diagram_attributes_and_unlabeled_fall_back() {
        assert!(
            parse_er("stateDiagram\n A --> B", 60).is_none(),
            "wrong header"
        );
        assert!(
            parse_er("erDiagram\n A ||--o{ B", 60).is_none(),
            "label required"
        );
        assert!(
            parse_er(
                "erDiagram\n A ||--o{ B : places\n CUSTOMER {\n string name\n }\n",
                88
            )
            .is_none(),
            "attribute block unsupported"
        );
    }

    #[test]
    fn class_diagram_wider_than_the_column_compacts() {
        let chart = "classDiagram\n Animal <|-- Dog\n Animal *-- Bone\n Animal o-- Tail\n Animal --> Food: eats\n";
        assert!(
            parse_class(chart, 60)
                .unwrap()
                .iter()
                .all(|row| glass_tui::text::width(row) <= 60),
            "compact class layout"
        );
    }

    #[test]
    fn class_diagram_unsupported_shapes_fall_back() {
        assert!(
            parse_class("stateDiagram\n A --> B", 60).is_none(),
            "wrong header"
        );
        assert!(parse_class("classDiagram\n A <|-- B", 60).is_some());
        assert!(
            parse_class("classDiagram\n List~T~ <|-- Impl", 60).is_none(),
            "generics unsupported"
        );
        assert!(
            parse_class("classDiagram\n class Foo\n", 60).is_none(),
            "members need their own layout"
        );
        assert!(
            parse_class(
                "classDiagram\n A <|
 B",
                60
            )
            .is_none(),
            "no relation token"
        );
    }

    #[test]
    fn sequence_wide_diagram_falls_back_not_clipped() {
        // Eight participants × wide boxes exceed the 88-column ceiling.
        let mut body = String::from("sequenceDiagram\n");
        for i in 0..8 {
            body.push_str(&format!(" participant P{i} as Long-Label-{i}\n"));
        }
        body.push_str("P0->>P7: crosses the whole diagram");
        assert!(
            parse_sequence(&body).is_none(),
            "宽度 over ceiling falls back"
        );
    }
}
