//! Bounded, non-executing LaTeX-to-terminal math. Unsupported commands retain source.
const MAX_BYTES: usize = 16 * 1024;
struct Parser<'a> {
    text: &'a str,
    at: usize,
    environments: Vec<String>,
}
impl Parser<'_> {
    fn group(&mut self, depth: usize) -> Result<String, String> {
        while self.text[self.at..].starts_with(' ') {
            self.at += 1;
        }
        if self.text[self.at..].starts_with('{') {
            self.at += 1;
            self.parse(depth + 1, true)
        } else {
            let c = self.text[self.at..]
                .chars()
                .next()
                .ok_or("missing math operand")?;
            self.at += c.len_utf8();
            Ok(c.to_string())
        }
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
                            let a = self.group(depth)?;
                            out.push_str("√(");
                            out.push_str(&a);
                            out.push(')');
                        }
                        "text" | "textrm" | "mathrm" | "mathbf" | "mathit" | "mathsf"
                        | "mathcal" | "operatorname" => out.push_str(&self.group(depth)?),
                        "left" | "right" | "displaystyle" | "textstyle" | "limits" | "nolimits" => {
                        }
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
