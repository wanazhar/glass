use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();
/// Path-only syntax selection for terminal source and diff rendering.
///
/// Known aliases use the closest bundled grammar when syntect lacks a native
/// grammar. Unknown paths deliberately return no highlighter so callers retain
/// the deterministic plain-text fallback.
pub(crate) struct SyntaxHighlighter {
    syntax_set: &'static SyntaxSet,
    #[cfg(test)]
    syntax_name: Option<String>,
    highlighter: Option<HighlightLines<'static>>,
}

impl SyntaxHighlighter {
    pub(crate) fn for_path(path: &str) -> Self {
        let syntax_set = syntax_set();
        let syntax = syntax_for_path(syntax_set, path);
        #[cfg(test)]
        let syntax_name =
            logical_syntax_name(path).or_else(|| syntax.map(|syntax| syntax.name.clone()));
        let highlighter = syntax.map(|syntax| HighlightLines::new(syntax, theme()));
        Self {
            syntax_set,
            #[cfg(test)]
            syntax_name,
            highlighter,
        }
    }

    pub(crate) fn for_token(token: &str) -> Self {
        let syntax_set = syntax_set();
        let syntax = syntax_for_token(syntax_set, token);
        #[cfg(test)]
        let syntax_name = syntax.map(|syntax| syntax.name.clone());
        let highlighter = syntax.map(|syntax| HighlightLines::new(syntax, theme()));
        Self {
            syntax_set,
            #[cfg(test)]
            syntax_name,
            highlighter,
        }
    }

    pub(crate) fn highlight(
        &mut self,
        line: &str,
        background: Option<Color>,
    ) -> Option<Vec<Span<'static>>> {
        let highlighter = self.highlighter.as_mut()?;
        let regions = highlighter.highlight_line(line, self.syntax_set).ok()?;
        Some(
            regions
                .into_iter()
                .map(|(style, text)| syntax_span(style, text, background))
                .collect(),
        )
    }

    #[cfg(test)]
    pub(crate) fn syntax_name(&self) -> Option<&str> {
        self.syntax_name.as_deref()
    }
}

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_nonewlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let mut themes = ThemeSet::load_defaults().themes;
        themes
            .remove("base16-ocean.dark")
            .or_else(|| themes.into_values().next())
            .expect("syntect default theme set is not empty")
    })
}
#[cfg(test)]
fn logical_syntax_name(path: &str) -> Option<String> {
    let path = Path::new(path);
    let file_name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let named = if file_name == "dockerfile" || file_name.starts_with("dockerfile.") {
        Some("Dockerfile")
    } else if file_name == "requirements.txt" {
        Some("Plain Text")
    } else {
        None
    };
    if let Some(name) = named {
        return Some(name.to_string());
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let name = match extension.as_str() {
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "swift" => "Swift",
        "kt" | "kts" => "Kotlin",
        "dart" => "Dart",
        "sql" => "SQL",
        _ => return None,
    };
    Some(name.to_string())
}

fn syntax_for_path<'a>(syntax_set: &'a SyntaxSet, path: &str) -> Option<&'a SyntaxReference> {
    let path = Path::new(path);
    let file_name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let named_syntax = match file_name.as_str() {
        "dockerfile" | "makefile" | "gnumakefile" | "cmakelists.txt" => Some("Shell-Unix-Generic"),
        "gemfile" | "rakefile" | "guardfile" => Some("Ruby"),
        "vagrantfile" => Some("Ruby"),
        "justfile" => Some("Makefile"),
        ".env" | ".env.example" | ".env.local" => Some("Shell-Unix-Generic"),
        "requirements.txt" => Some("Plain Text"),
        _ if file_name.starts_with("dockerfile.") => Some("Shell-Unix-Generic"),
        _ => None,
    };
    if let Some(name) = named_syntax
        && let Some(syntax) = syntax_set.find_syntax_by_name(name)
    {
        return Some(syntax);
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let aliased_syntax = match extension.as_str() {
        "ts" | "tsx" | "mts" | "cts" => Some("JavaScript"),
        "swift" | "dart" | "m" | "mm" => Some("C++"),
        "kt" | "kts" => Some("Java"),
        _ => None,
    };
    if let Some(name) = aliased_syntax
        && let Some(syntax) = syntax_set.find_syntax_by_name(name)
    {
        return Some(syntax);
    }
    syntax_set.find_syntax_by_extension(&extension)
}

fn syntax_for_token<'a>(syntax_set: &'a SyntaxSet, token: &str) -> Option<&'a SyntaxReference> {
    let token = token.trim().to_ascii_lowercase();
    let aliased_syntax = match token.as_str() {
        "ts" | "tsx" | "mts" | "cts" | "typescript" => Some("JavaScript"),
        "js" | "jsx" | "mjs" | "cjs" | "javascript" => Some("JavaScript"),
        "swift" | "dart" | "objective-c" | "objc" => Some("C++"),
        "kotlin" | "kt" | "kts" => Some("Java"),
        _ => None,
    };
    aliased_syntax
        .and_then(|name| syntax_set.find_syntax_by_name(name))
        .or_else(|| syntax_set.find_syntax_by_token(&token))
}
fn syntax_span(style: SyntectStyle, text: &str, background: Option<Color>) -> Span<'static> {
    let mut terminal_style = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if let Some(background) = background {
        terminal_style = terminal_style.bg(background);
    }
    if style.font_style.contains(FontStyle::BOLD) {
        terminal_style = terminal_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        terminal_style = terminal_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        terminal_style = terminal_style.add_modifier(Modifier::UNDERLINED);
    }
    Span::styled(text.to_string(), terminal_style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_broad_editor_language_families_by_path_only() {
        let paths = [
            ("src/lib.rs", "Rust"),
            ("web/app.tsx", "TypeScript"),
            ("web/worker.mts", "TypeScript"),
            ("server.rb", "Ruby"),
            ("service.swift", "Swift"),
            ("android/MainActivity.kt", "Kotlin"),
            ("tools/build.dart", "Dart"),
            ("query.sql", "SQL"),
            ("infra/Dockerfile.production", "Dockerfile"),
            ("config/requirements.txt", "Plain Text"),
        ];
        for (path, expected) in paths {
            let highlighter = SyntaxHighlighter::for_path(path);
            assert_eq!(highlighter.syntax_name(), Some(expected), "{path}");
        }
    }

    #[test]
    fn highlights_without_content_based_detection() {
        let mut highlighter = SyntaxHighlighter::for_path("src/lib.rs");
        let spans = highlighter.highlight("fn main() {}", None).unwrap();
        assert!(spans.iter().any(|span| span.content == "fn"));

        let unknown = SyntaxHighlighter::for_path("file.unknown-glass-format");
        assert_eq!(unknown.syntax_name(), None);
    }
    #[test]
    fn resolves_markdown_fence_aliases_without_content_detection() {
        for (token, source) in [
            ("typescript", "const value = 1;"),
            ("swift", "let value = 1"),
            ("kotlin", "val value = 1"),
            ("dart", "final value = 1;"),
        ] {
            let mut highlighter = SyntaxHighlighter::for_token(token);
            assert!(
                highlighter.highlight(source, None).is_some(),
                "{token} should use a bundled fallback grammar"
            );
        }
    }
}
