//! Bounded, non-executing LaTeX-to-terminal math. Unsupported commands retain source.
const MAX_BYTES: usize = 16 * 1024;
struct Parser<'a> {
    text: &'a str,
    at: usize,
    environments: Vec<String>,
}
impl Parser<'_> {
    fn group(&mut self, depth: usize) -> Result<String, String> {
        if depth > 16 {
            return Err("math nesting exceeds 16".into());
        }
        while self.text[self.at..].starts_with(' ') {
            self.at += 1;
        }
        if self.text[self.at..].starts_with('{') {
            self.at += 1;
            self.parse(depth + 1, true)
        } else if self.text[self.at..].starts_with('\\') {
            self.at += 1;
            let tail = &self.text[self.at..];
            let count = tail.bytes().take_while(u8::is_ascii_alphabetic).count();
            if count == 0 {
                return Err("missing math operand".into());
            }
            let command = &tail[..count];
            self.at += count;
            match command {
                "sqrt" => {
                    let root = self.root_index()?;
                    let value = self.group(depth + 1)?;
                    Ok(format!("{root}√({value})"))
                }
                "frac" | "dfrac" | "tfrac" => {
                    let a = self.group(depth + 1)?;
                    let b = self.group(depth + 1)?;
                    Ok(format!("({a})⁄({b})"))
                }
                _ => symbol(command)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("unsupported math operand \\{command}")),
            }
        } else {
            let c = self.text[self.at..]
                .chars()
                .next()
                .ok_or("missing math operand")?;
            if matches!(c, '}' | '^' | '_' | '&') {
                return Err("missing math operand".into());
            }
            self.at += c.len_utf8();
            Ok(c.to_string())
        }
    }
    fn root_index(&mut self) -> Result<String, String> {
        while self.text[self.at..].starts_with(' ') {
            self.at += 1;
        }
        if !self.text[self.at..].starts_with('[') {
            return Ok(String::new());
        }
        self.at += 1;
        let end = self.text[self.at..]
            .find(']')
            .ok_or("unclosed root index")?;
        let raw = &self.text[self.at..self.at + end];
        if raw.is_empty() || raw.len() > 16 || !raw.chars().all(|c| c.is_ascii_digit()) {
            return Err("unsupported root index".into());
        }
        self.at += end + 1;
        Ok(script(raw, true))
    }
    fn parse(&mut self, depth: usize, group: bool) -> Result<String, String> {
        if depth > 16 {
            return Err("math nesting exceeds 16".into());
        }
        let mut out = String::new();
        while self.at < self.text.len() {
            let tail = &self.text[self.at..];
            let c = tail.chars().next().unwrap();
            self.at += c.len_utf8();
            match c {
                '}' if group => return Ok(out),
                '}' => return Err("unmatched math brace".into()),
                '{' => out.push_str(&self.parse(depth + 1, true)?),
                '^' | '_' => {
                    let body = self.group(depth)?;
                    out.push_str(&script(&body, c == '^'));
                }
                '\\' => {
                    let tail = &self.text[self.at..];
                    if tail.starts_with('\\') {
                        self.at += 1;
                        out.push('\n');
                        continue;
                    }
                    let count = tail.bytes().take_while(u8::is_ascii_alphabetic).count();
                    if count == 0 {
                        let escaped = tail.chars().next().ok_or("unfinished math command")?;
                        self.at += escaped.len_utf8();
                        match escaped {
                            ',' | ';' | ' ' | ':' => out.push(' '),
                            '!' => {}
                            '{' | '}' | '%' | '$' | '_' | '#' | '&' | '|' => out.push(escaped),
                            _ => return Err(format!("unsupported math command \\{escaped}")),
                        }
                        continue;
                    }
                    let command = &tail[..count];
                    self.at += count;
                    match command {
                        "frac" | "dfrac" | "tfrac" => {
                            let a = self.group(depth)?;
                            let b = self.group(depth)?;
                            if a.chars().count() > 1 {
                                out.push('(');
                            }
                            out.push_str(&a);
                            if a.chars().count() > 1 {
                                out.push(')');
                            }
                            out.push('⁄');
                            if b.chars().count() > 1 {
                                out.push('(');
                            }
                            out.push_str(&b);
                            if b.chars().count() > 1 {
                                out.push(')');
                            }
                        }
                        "sqrt" => {
                            let index = self.root_index()?;
                            let a = self.group(depth)?;
                            out.push_str(&index);
                            out.push_str("√(");
                            out.push_str(&a);
                            out.push(')');
                        }
                        "mathbb" => {
                            let value = self.group(depth)?;
                            out.push_str(match value.as_str() {
                                "R" => "ℝ",
                                "C" => "ℂ",
                                "N" => "ℕ",
                                "Z" => "ℤ",
                                "Q" => "ℚ",
                                "H" => "ℍ",
                                "P" => "ℙ",
                                _ => &value,
                            });
                        }
                        "hat" | "widehat" | "bar" | "overline" | "vec" | "dot" | "ddot"
                        | "tilde" | "widetilde" => {
                            let value = self.group(depth)?;
                            if value.chars().count() == 1 {
                                out.push_str(&value);
                                out.push_str(match command {
                                    "hat" | "widehat" => "̂",
                                    "bar" | "overline" => "̄",
                                    "vec" => "⃗",
                                    "dot" => "̇",
                                    "ddot" => "̈",
                                    _ => "̃",
                                });
                            } else {
                                out.push_str(&format!("{command}({value})"));
                            }
                        }
                        "boxed" => {
                            out.push('⟦');
                            out.push_str(&self.group(depth)?);
                            out.push('⟧');
                        }
                        "binom" => {
                            let a = self.group(depth)?;
                            let b = self.group(depth)?;
                            out.push_str(&format!("({a} choose {b})"));
                        }
                        "text" | "textrm" | "mathrm" | "mathbf" | "mathit" | "mathsf"
                        | "mathscr" | "mathcal" | "operatorname" => {
                            out.push_str(&self.group(depth)?)
                        }
                        "left" | "right" => {
                            if self.text[self.at..].starts_with('.') {
                                self.at += 1;
                            }
                        }
                        "displaystyle" | "textstyle" | "limits" | "nolimits" => {}
                        "begin" | "end" => {
                            let name = self.group(depth)?;
                            if command == "begin" {
                                if self.environments.len() >= 16 {
                                    return Err("math environment nesting exceeds 16".into());
                                }
                                self.environments.push(name.clone());
                            } else if self.environments.pop().as_deref() != Some(&name) {
                                return Err("mismatched math environment".into());
                            }
                            match name.as_str() {
                                "aligned" | "align" | "align*" | "gathered" | "equation"
                                | "equation*" => {}
                                "matrix" => {
                                    if command == "begin" {
                                        out.push('[');
                                    } else {
                                        out.push(']');
                                    }
                                }
                                "pmatrix" => out.push(if command == "begin" { '(' } else { ')' }),
                                "bmatrix" => out.push(if command == "begin" { '[' } else { ']' }),
                                "cases" => {
                                    if command == "begin" {
                                        out.push('{');
                                    }
                                }
                                _ => return Err(format!("unsupported math environment {name}")),
                            }
                        }
                        _ => out.push_str(
                            symbol(command)
                                .ok_or_else(|| format!("unsupported math command \\{command}"))?,
                        ),
                    }
                }
                '&' => out.push_str("  "),
                '~' => out.push(' '),
                _ => out.push(c),
            }
        }
        if group {
            Err("unclosed math brace".into())
        } else {
            Ok(out)
        }
    }
}
fn script(text: &str, upper: bool) -> String {
    let from = "0123456789+-=()in";
    let to = if upper {
        "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁱⁿ"
    } else {
        "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ᵢₙ"
    };
    let result: Option<String> = text
        .chars()
        .map(|c| {
            from.chars()
                .position(|item| item == c)
                .and_then(|at| to.chars().nth(at))
        })
        .collect();
    result.unwrap_or_else(|| format!("{}({text})", if upper { '^' } else { '_' }))
}
fn symbol(command: &str) -> Option<&'static str> {
    Some(match command {
        "cdotp" => "·",
        "bullet" => "•",
        "circ" => "∘",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "perp" => "⊥",
        "parallel" => "∥",
        "angle" => "∠",
        "degree" => "°",
        "prime" => "′",
        "therefore" => "∴",
        "because" => "∵",
        "simeq" => "≃",
        "cong" => "≅",
        "ll" => "≪",
        "gg" => "≫",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "top" => "⊤",
        "bot" => "⊥",
        "setminus" => "∖",
        "not" => "¬",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "longrightarrow" => "⟶",
        "longleftrightarrow" => "⟷",
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" | "vartheta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" | "varpi" => "π",
        "rho" => "ρ",
        "sigma" | "varsigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" | "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "times" => "×",
        "cdot" => "·",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "propto" => "∝",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "iint" => "∬",
        "oint" => "∮",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "supset" => "⊃",
        "cup" => "∪",
        "cap" => "∩",
        "emptyset" | "varnothing" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "neg" | "lnot" => "¬",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "ldots" | "dots" => "…",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "lim" => "lim",
        "min" => "min",
        "max" => "max",
        "det" => "det",
        "sup" => "sup",
        "inf" => "inf",
        "quad" => "  ",
        "qquad" => "    ",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lvert" | "rvert" => "|",
        "lVert" | "rVert" => "‖",
        "ell" => "ℓ",
        "hbar" => "ℏ",
        _ => return None,
    })
}
pub fn render(text: &str) -> Result<String, String> {
    if text.len() > MAX_BYTES {
        return Err("math block exceeds 16 KiB".into());
    }
    let mut parser = Parser {
        text,
        at: 0,
        environments: Vec::new(),
    };
    let output = parser.parse(0, false)?;
    if !parser.environments.is_empty() {
        return Err("unclosed math environment".into());
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_math_is_readable_and_does_not_execute_commands() {
        assert_eq!(
            render(r"E=mc^2 + \frac{a}{b} + \alpha_i").unwrap(),
            "E=mc² + a⁄b + αᵢ"
        );
        assert_eq!(render(r"\sqrt{x+1} \leq \infty").unwrap(), "√(x+1) ≤ ∞");
        assert!(render(r"\begin{matrix}x\end{aligned}").is_err());
        assert!(render(r"\begin{matrix}x").is_err());
        assert!(render(r"\input{/etc/passwd}").is_err());
        assert!(render(r"\write18{command}").is_err());
        assert!(render(&"{".repeat(40)).is_err());
    }
}

/// Structured display math. Inline expressions still use the compact renderer.
/// Geometry is computed once for a completed block, never for each stream token.
pub fn display(text: &str, width: usize) -> Result<Vec<String>, String> {
    if text.len() > MAX_BYTES {
        return Err("math block exceeds 16 KiB".into());
    }
    let text = text.trim();
    for prefix in ["\\frac", "\\dfrac", "\\tfrac"] {
        if text.starts_with(prefix) && text[prefix.len()..].trim_start().starts_with('{') {
            let mut parser = Parser {
                text,
                at: prefix.len(),
                environments: Vec::new(),
            };
            let a = parser.group(0)?;
            let b = parser.group(0)?;
            if text[parser.at..].trim().is_empty() && !a.contains('\n') && !b.contains('\n') {
                let cells = crate::text::width(&a).max(crate::text::width(&b));
                if cells > 0 && cells <= width {
                    return Ok(vec![
                        format!("{}{}", " ".repeat((cells - crate::text::width(&a)) / 2), a),
                        "─".repeat(cells),
                        format!("{}{}", " ".repeat((cells - crate::text::width(&b)) / 2), b),
                    ]);
                }
            }
        }
    }
    if let Some(begin) = text.find("\\begin{") {
        let name_start = begin + 7;
        let name_end = name_start
            + text[name_start..]
                .find('}')
                .ok_or("unclosed math environment")?;
        let name = &text[name_start..name_end];
        if matches!(
            name,
            "matrix"
                | "pmatrix"
                | "bmatrix"
                | "Bmatrix"
                | "vmatrix"
                | "Vmatrix"
                | "cases"
                | "aligned"
                | "align"
                | "align*"
                | "gathered"
                | "array"
        ) {
            let end_tag = format!("\\end{{{name}}}");
            let end = text[name_end + 1..]
                .find(&end_tag)
                .map(|at| at + name_end + 1)
                .ok_or("unclosed math environment")?;
            let mut body = &text[name_end + 1..end];
            if name == "array" {
                body = body.trim_start();
                let alignment = body
                    .strip_prefix('{')
                    .and_then(|s| s.split_once('}'))
                    .ok_or("array column definition required")?;
                if alignment.0.len() > 32
                    || !alignment
                        .0
                        .chars()
                        .all(|c| matches!(c, 'l' | 'c' | 'r' | '|'))
                {
                    return Err("unsupported array column definition".into());
                }
                body = alignment.1;
            }
            if body.contains("\\begin{") || body.contains("\\end{") {
                return Err("nested display environments retain source".into());
            }
            let mut rows = Vec::new();
            let mut row = Vec::new();
            let mut start = 0;
            let mut at = 0;
            let mut depth = 0usize;
            while at < body.len() {
                let tail = &body[at..];
                let c = tail.chars().next().unwrap();
                if c == '\\' {
                    if depth == 0 && tail.starts_with("\\\\") {
                        row.push(render(body[start..at].trim())?);
                        rows.push(std::mem::take(&mut row));
                        at += 2;
                        start = at;
                        continue;
                    }
                    at += 1;
                    if let Some(next) = body[at..].chars().next() {
                        at += next.len_utf8();
                    }
                    continue;
                }
                match c {
                    '{' => {
                        depth += 1;
                        if depth > 16 {
                            return Err("math nesting exceeds 16".into());
                        }
                    }
                    '}' => {
                        depth = depth.checked_sub(1).ok_or("unmatched math brace")?;
                    }
                    '&' if depth == 0 => {
                        row.push(render(body[start..at].trim())?);
                        start = at + 1;
                    }
                    _ => {}
                }
                at += c.len_utf8();
            }
            if depth != 0 {
                return Err("unclosed math brace".into());
            }
            if !body[start..].trim().is_empty() || !row.is_empty() {
                row.push(render(body[start..].trim())?);
                rows.push(row);
            }
            if rows.is_empty()
                || rows.len() > 32
                || rows
                    .iter()
                    .any(|r| r.len() > 16 || r.iter().any(|cell| cell.contains('\n')))
            {
                return Err("matrix layout limit".into());
            }
            let columns = rows.iter().map(Vec::len).max().unwrap();
            let aligned = matches!(name, "aligned" | "align" | "align*" | "gathered");
            if !aligned && rows.iter().any(|r| r.len() != columns) {
                return Err("matrix rows have different column counts".into());
            }
            let mut sizes = vec![0usize; columns];
            for row in &rows {
                for (col, cell) in row.iter().enumerate() {
                    sizes[col] = sizes[col].max(crate::text::width(cell));
                }
            }
            let mut out = Vec::new();
            if !text[..begin].trim().is_empty() {
                out.push(render(text[..begin].trim())?);
            }
            let total = sizes.iter().sum::<usize>()
                + columns.saturating_sub(1) * 2
                + if aligned || name == "matrix" || name == "array" {
                    0
                } else {
                    4
                };
            if total > width {
                out.push(format!(
                    "{} · {} × {}",
                    if aligned { "Aligned" } else { "Matrix" },
                    rows.len(),
                    columns
                ));
                for (index, row) in rows.iter().enumerate() {
                    out.push(format!("{} │ {}", index + 1, row.join(" ; ")));
                }
            } else {
                for (index, row) in rows.iter().enumerate() {
                    let count = rows.len();
                    let (left, right) = match name {
                        "pmatrix" => {
                            if count == 1 {
                                ("(", ")")
                            } else if index == 0 {
                                ("⎛", "⎞")
                            } else if index + 1 == count {
                                ("⎝", "⎠")
                            } else {
                                ("⎜", "⎟")
                            }
                        }
                        "bmatrix" => {
                            if count == 1 {
                                ("[", "]")
                            } else if index == 0 {
                                ("⎡", "⎤")
                            } else if index + 1 == count {
                                ("⎣", "⎦")
                            } else {
                                ("⎢", "⎥")
                            }
                        }
                        "Bmatrix" | "cases" => {
                            if count == 1 {
                                ("{", if name == "cases" { "" } else { "}" })
                            } else if index == 0 {
                                ("⎧", if name == "cases" { "" } else { "⎫" })
                            } else if index + 1 == count {
                                ("⎩", if name == "cases" { "" } else { "⎭" })
                            } else {
                                ("⎨", if name == "cases" { "" } else { "⎬" })
                            }
                        }
                        "vmatrix" => ("|", "|"),
                        "Vmatrix" => ("‖", "‖"),
                        _ => ("", ""),
                    };
                    let mut line = String::new();
                    if !left.is_empty() {
                        line.push_str(left);
                        line.push(' ');
                    }
                    for (col, size) in sizes.iter().enumerate() {
                        if col > 0 {
                            line.push_str("  ");
                        }
                        let cell = row.get(col).map(String::as_str).unwrap_or("");
                        let padding = size.saturating_sub(crate::text::width(cell));
                        if aligned && col == 0 {
                            line.push_str(&" ".repeat(padding));
                            line.push_str(cell);
                        } else {
                            line.push_str(cell);
                            line.push_str(&" ".repeat(padding));
                        }
                    }
                    if !right.is_empty() {
                        line.push(' ');
                        line.push_str(right);
                    }
                    out.push(line.trim_end().to_owned());
                }
            }
            if !text[end + end_tag.len()..].trim().is_empty() {
                out.push(render(text[end + end_tag.len()..].trim())?);
            }
            return Ok(out);
        }
    }
    Ok(render(text)?.lines().map(str::to_owned).collect())
}

#[cfg(test)]
mod expanded_tests {
    use super::*;
    #[test]
    fn operands_roots_and_missing_operands() {
        assert_eq!(
            render(r"\sqrt[3]{x} + x^\alpha + \frac\alpha\beta").unwrap(),
            "³√(x) + x^(α) + α⁄β"
        );
        assert!(render("x^}").is_err());
        assert!(render(&format!("{}x", r"\sqrt".repeat(20))).is_err());
        assert!(render(r"\sqrt[x]{n}").is_err());
    }
    #[test]
    fn matrices_alignment_and_fractions() {
        let m = display(r"A=\begin{bmatrix}1&22\\333&4\end{bmatrix}", 40).unwrap();
        assert_eq!(m, vec!["A=", "⎡ 1    22 ⎤", "⎣ 333  4  ⎦"]);
        let a = display(r"\begin{aligned}a&=b\\long&=c\end{aligned}", 40).unwrap();
        assert_eq!(a, vec!["   a  =b", "long  =c"]);
        assert_eq!(
            display(r"\frac{x+1}{n}", 40).unwrap(),
            vec!["x+1", "───", " n"]
        );
        let m = display(
            r"\begin{bmatrix}verylongentry&secondlongentry\\a&b\end{bmatrix}",
            12,
        )
        .unwrap();
        assert!(m[0].contains("2 × 2"));
    }
    #[test]
    fn malformed_layout_keeps_source() {
        assert!(display(r"\begin{bmatrix}a&b\\c\end{bmatrix}", 40).is_err());
        assert!(display(r"\begin{pmatrix}a\end{bmatrix}", 40).is_err());
        assert!(display(r"\begin{matrix}\begin{matrix}x\end{matrix}\end{matrix}", 40).is_err());
    }
}
