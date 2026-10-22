//! Small streaming lexer. No grammar loading, regex engine, or subprocess.
use crate::theme::{self, Tone};
use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
#[derive(Clone, Default)]
pub struct State {
    block_comment: bool,
    quote: Option<char>,
    delimiter: Option<(String, bool)>,
    literal_comment: bool,
}
fn style(tone: Tone) -> Style {
    Style::new().fg(theme::color(tone))
}
fn family(language: &str) -> &str {
    match language
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "py" | "python" => "python",
        "js" | "jsx" | "javascript" | "ts" | "tsx" | "typescript" | "javascriptreact"
        | "typescriptreact" | "node" => "javascript",
        "rs" | "rust" => "rust",
        "c" | "h" | "cpp" | "c++" | "cc" | "hpp" | "java" | "cs" | "csharp" | "c#" | "go"
        | "swift" | "kotlin" | "kt" | "kts" | "dart" | "scala" => "c",
        "sh" | "bash" | "zsh" | "fish" | "shell" => "shell",
        "json" | "jsonc" | "json5" => "json",
        "yml" | "yaml" | "toml" | "ini" | "properties" | "dotenv" | "env" => "config",
        "sql" => "sql",
        "html" | "xml" | "svg" | "css" | "scss" | "vue" | "svelte" => "markup",
        "rb" | "ruby" => "ruby",
        "lua" => "lua",
        "r" => "r",
        "php" => "php",
        "ps1" | "powershell" | "pwsh" => "powershell",
        "dockerfile" | "docker" => "docker",
        "makefile" | "make" => "make",
        "graphql" | "gql" => "graphql",
        "diff" | "patch" => "diff",
        "log" | "logs" | "console" => "log",
        _ => "plain",
    }
}
fn keyword(word: &str, family: &str) -> bool {
    let words = match family {
        "python" => {
            "False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield"
        }
        "javascript" => {
            "abstract as async await break case catch class const continue debugger declare default delete do else enum export extends false finally for from function get if implements import in instanceof interface let new null of private protected public readonly return satisfies set static super switch this throw true try type typeof undefined var void while yield"
        }
        "rust" => {
            "Self as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self static struct super trait true type unsafe use where while"
        }
        "c" => {
            "abstract auto bool boolean break byte case catch char class const continue default delete do double else enum extends false final finally float for func goto if implements import int interface long namespace new nil null nullptr package private protected public return short signed sizeof static string struct super switch template this throw true try typedef typename unsigned using var virtual void volatile while"
        }
        "shell" => {
            "if then else elif fi for in do done case esac function select while until export local readonly return source set unset true false"
        }
        "json" => "true false null",
        "config" => "true false null yes no on off",
        "sql" => {
            "select from where join left right inner outer on as and or not null is insert into values update set delete create table drop alter group by order having limit offset distinct union all primary key references constraint begin commit rollback case when then else end"
        }
        "ruby" => {
            "BEGIN END alias and begin break case class def defined do else elsif end ensure false for if in module next nil not or redo rescue retry return self super then true undef unless until when while yield"
        }
        "lua" => {
            "and break do else elseif end false for function goto if in local nil not or repeat return then true until while"
        }
        "r" => "if else repeat while function for in next break TRUE FALSE NULL NA NaN Inf",
        "php" => {
            "abstract and array as break callable case catch class clone const continue declare default die do echo else elseif empty endfor endforeach endif endswitch endwhile eval exit extends final finally fn for foreach function global goto if implements include include_once instanceof interface isset list match namespace new null or print private protected public readonly require require_once return static switch throw trait try unset use var while xor yield true false"
        }
        "powershell" => {
            "begin break catch class continue data do dynamicparam else elseif end enum exit filter finally for foreach from function if in param process return switch throw trap try until using var while workflow true false null"
        }
        "docker" => {
            "FROM RUN CMD LABEL MAINTAINER EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG ONBUILD STOPSIGNAL HEALTHCHECK SHELL"
        }
        "make" => {
            "include define endef ifdef ifndef ifeq ifneq else endif export unexport override private vpath"
        }
        "graphql" => {
            "query mutation subscription fragment on schema type interface union enum input scalar extend implements directive true false null"
        }
        _ => "",
    };
    words.split_ascii_whitespace().any(|keyword| {
        keyword.eq_ignore_ascii_case(word)
            && (matches!(family, "sql" | "powershell" | "docker") || keyword == word)
    })
}
/// Withhold only the last unfinished identifier so streaming cannot commit a
/// keyword before its boundary. Strings/comments keep bounded lexical state.
pub fn stable_prefix(text: &str) -> &str {
    if matches!(text.trim(), "+" | "++" | "-" | "--" | "@") {
        return "";
    }
    let markers = ["<!--", "--[[", "/*", "//", "--", "[["];
    if markers.iter().any(|marker| text.ends_with(marker)) {
        return text;
    }
    for marker in markers {
        for size in 1..marker.len() {
            let prefix = &marker[..size];
            if let Some(stripped) = text.strip_suffix(prefix) {
                return stripped;
            }
        }
    }
    let hashes = text.bytes().rev().take_while(|c| *c == b'#').count();
    if (1..=16).contains(&hashes) {
        let end = text.len() - hashes;
        if text[..end].ends_with('r') {
            let mut start = end - 1;
            if text[..start].ends_with('b') {
                start -= 1;
            }
            if text[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
            {
                return &text[..start];
            }
        }
    }
    let trimmed = text.trim_end_matches([' ', '\t']);
    let end = trimmed
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map_or(text.len(), |(at, _)| at);
    &text[..end]
}
pub fn line(text: &str, language: &str, state: &State) -> (Vec<Span<'static>>, State) {
    let family = family(language);
    if family == "diff" {
        let tone = if text.starts_with("@@") {
            Tone::Secondary
        } else if text.starts_with("+++")
            || text.starts_with("---")
            || text.starts_with("diff ")
            || text.starts_with("index ")
        {
            Tone::Accent
        } else if text.starts_with('+') {
            Tone::Success
        } else if text.starts_with('-') {
            Tone::Error
        } else {
            Tone::Body
        };
        return (
            vec![Span::styled(text.to_owned(), style(tone))],
            State::default(),
        );
    }
    if family == "log" {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut at = 0;
        while at < text.len() {
            let alphabetic = text[at..].chars().next().unwrap().is_ascii_alphabetic();
            let end = text[at..]
                .char_indices()
                .take_while(|(_, c)| c.is_ascii_alphabetic() == alphabetic)
                .last()
                .map(|(i, c)| at + i + c.len_utf8())
                .unwrap();
            let word = &text[at..end];
            let tone = if ["error", "fatal", "panic"]
                .iter()
                .any(|s| word.eq_ignore_ascii_case(s))
            {
                Tone::Error
            } else if ["warn", "warning"]
                .iter()
                .any(|s| word.eq_ignore_ascii_case(s))
            {
                Tone::Warning
            } else if word.eq_ignore_ascii_case("info") {
                Tone::Secondary
            } else if word.eq_ignore_ascii_case("debug") || word.eq_ignore_ascii_case("trace") {
                Tone::Muted
            } else {
                Tone::Body
            };
            let style = style(tone);
            if let Some(last) = spans.last_mut().filter(|last| last.style == style) {
                last.content.to_mut().push_str(word);
            } else {
                spans.push(Span::styled(word.to_owned(), style));
            }
            at = end;
        }
        return (spans, State::default());
    }
    if family == "plain" {
        return (
            vec![Span::styled(text.to_owned(), style(Tone::Body))],
            State::default(),
        );
    }
    let mut state = state.clone();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let tail = &text[at..];
        let c = tail.chars().next().unwrap();
        if let Some((delimiter, escapes)) = state.delimiter.clone() {
            let mut scan = 0;
            let mut end = None;
            while let Some(found) = tail[scan..].find(&delimiter) {
                let at = scan + found;
                if !escapes || tail[..at].bytes().rev().take_while(|c| *c == b'\\').count() % 2 == 0
                {
                    end = Some(at + delimiter.len());
                    break;
                }
                scan = at + delimiter.len();
            }
            let length = end.unwrap_or(tail.len());
            spans.push(Span::styled(
                tail[..length].to_owned(),
                style(if state.literal_comment {
                    Tone::Muted
                } else {
                    Tone::Success
                }),
            ));
            at += length;
            if end.is_some() {
                state.delimiter = None;
                state.literal_comment = false;
            }
            continue;
        }
        if state.block_comment {
            let end = tail.find("*/").map(|end| end + 2).unwrap_or(tail.len());
            spans.push(Span::styled(
                tail[..end].to_owned(),
                style(Tone::Muted).add_modifier(Modifier::ITALIC),
            ));
            state.block_comment = !tail[..end].ends_with("*/");
            at += end;
            continue;
        }
        if let Some(quote) = state.quote {
            let mut escaped = false;
            let mut end = tail.len();
            for (i, ch) in tail.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == quote {
                    end = i + ch.len_utf8();
                    state.quote = None;
                    break;
                }
            }
            spans.push(Span::styled(tail[..end].to_owned(), style(Tone::Success)));
            at += end;
            continue;
        }
        if family == "lua" && tail.starts_with("--[[") {
            spans.push(Span::styled("--[[", style(Tone::Muted)));
            state.delimiter = Some(("]]".into(), false));
            state.literal_comment = true;
            at += 4;
            continue;
        }
        if (matches!(
            family,
            "python" | "shell" | "config" | "ruby" | "r" | "powershell" | "docker" | "make"
        ) && c == '#')
            || (matches!(
                family,
                "javascript" | "rust" | "c" | "json" | "markup" | "php"
            ) && tail.starts_with("//"))
            || (matches!(family, "sql" | "lua") && tail.starts_with("--"))
        {
            spans.push(Span::styled(
                tail.to_owned(),
                style(Tone::Muted).add_modifier(Modifier::ITALIC),
            ));
            break;
        }
        if matches!(
            family,
            "javascript" | "rust" | "c" | "json" | "markup" | "php"
        ) && tail.starts_with("/*")
        {
            spans.push(Span::styled("/*", style(Tone::Muted)));
            state.block_comment = true;
            at += 2;
            continue;
        }
        let multiline = if (family == "python"
            && (tail.starts_with("\"\"\"") || tail.starts_with("'''")))
            || (language
                .split_whitespace()
                .next()
                .is_some_and(|tag| matches!(tag, "java" | "kotlin" | "kt" | "swift" | "scala"))
                && tail.starts_with("\"\"\""))
        {
            Some((tail[..3].to_owned(), 3, true))
        } else if family == "lua" && tail.starts_with("[[") {
            Some(("]]".into(), 2, false))
        } else if family == "markup" && tail.starts_with("<!--") {
            Some(("-->".into(), 4, false))
        } else if family == "rust" && (tail.starts_with('r') || tail.starts_with("br")) {
            let prefix = if tail.starts_with("br") { 2 } else { 1 };
            let hashes = tail[prefix..].bytes().take_while(|c| *c == b'#').count();
            if hashes <= 16 && tail[prefix + hashes..].starts_with('"') {
                Some((
                    format!("\"{}", "#".repeat(hashes)),
                    prefix + hashes + 1,
                    false,
                ))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((delimiter, length, escapes)) = multiline {
            spans.push(Span::styled(
                tail[..length].to_owned(),
                style(if family == "markup" {
                    Tone::Muted
                } else {
                    Tone::Success
                }),
            ));
            state.literal_comment = family == "markup";
            state.delimiter = Some((delimiter, escapes));
            at += length;
            continue;
        }
        if matches!(c, '\'' | '"' | '`') {
            // Rust lifetimes are identifiers, not unterminated character strings.
            if family == "rust" && c == '\'' && !tail[1..].chars().take(3).any(|c| c == '\'') {
                spans.push(Span::styled("'", style(Tone::Body)));
                at += 1;
                continue;
            }
            spans.push(Span::styled(c.to_string(), style(Tone::Success)));
            state.quote = Some(c);
            at += c.len_utf8();
            continue;
        }
        if c.is_ascii_digit() {
            let end = tail
                .char_indices()
                .take_while(|(_, c)| c.is_ascii_alphanumeric() || matches!(c, '.' | '_'))
                .last()
                .map_or(1, |(i, c)| i + c.len_utf8());
            spans.push(Span::styled(tail[..end].to_owned(), style(Tone::Secondary)));
            at += end;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let end = tail
                .char_indices()
                .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
                .last()
                .map_or(c.len_utf8(), |(i, c)| i + c.len_utf8());
            let word = &tail[..end];
            let token_style = if keyword(word, family) {
                style(Tone::Accent).add_modifier(Modifier::BOLD)
            } else if matches!(family, "config" | "make")
                && tail[end..].trim_start().starts_with([':', '='])
            {
                style(Tone::Accent)
            } else if family == "markup" && text[..at].trim_end().ends_with(['<', '/']) {
                style(Tone::Accent).add_modifier(Modifier::BOLD)
            } else if (family == "markup" && tail[end..].trim_start().starts_with('='))
                || tail[end..].trim_start().starts_with('(')
            {
                style(Tone::Secondary)
            } else {
                style(Tone::Body)
            };
            spans.push(Span::styled(word.to_owned(), token_style));
            at += end;
            continue;
        }
        let mut end = c.len_utf8();
        for (i, ch) in tail[end..].char_indices() {
            if ch.is_alphanumeric()
                || matches!(ch, '_' | '#' | '/' | '\'' | '"' | '`' | '-' | '[' | '<')
            {
                break;
            }
            end = c.len_utf8() + i + ch.len_utf8();
        }
        spans.push(Span::styled(tail[..end].to_owned(), style(Tone::Body)));
        at += end;
    }
    // Ordinary single/double quotes cannot continue across lines in most formats.
    if !(family == "javascript" && state.quote == Some('`'))
        && !matches!(family, "shell" | "ruby" | "sql")
    {
        state.quote = None;
    }
    (spans, state)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_tokens_preserve_source_and_have_distinct_colors() {
        for (language, text) in [
            ("python", "def greet(name): # hello"),
            ("javascript", "const n = 42; console.log(\"hello\");"),
            ("rust", "pub fn main() { let n = 1; }"),
            ("json", "{\"name\": true, \"n\": 3}"),
            ("java", "public class Greeter { return \"Hi\"; }"),
        ] {
            let (spans, _) = line(text, language, &State::default());
            assert_eq!(
                spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
                text
            );
            assert!(
                spans
                    .iter()
                    .any(|s| s.style.add_modifier.contains(Modifier::BOLD)),
                "{language}"
            );
            assert!(
                spans
                    .iter()
                    .any(|s| !s.style.add_modifier.contains(Modifier::BOLD)),
                "{language}"
            );
        }
    }
    #[test]
    fn multiline_comments_and_unknown_languages_are_safe() {
        let (_, state) = line("/* comment", "rust", &State::default());
        assert!(state.block_comment);
        let (spans, state) = line("still */ let x = 2;", "rust", &state);
        assert!(!state.block_comment);
        assert!(spans.iter().any(|s| s.content == "let"));
        assert_eq!(
            line("raw <tag>", "unknown", &state).0[0].content,
            "raw <tag>"
        );
    }
}

#[cfg(test)]
mod expanded_tests {
    use super::*;
    #[test]
    fn more_languages_and_multiline_literals_preserve_source() {
        for lang in ["c", "cpp", "c++", "c#"] {
            let (_, state) = line("const char *s = \"\"\";", lang, &State::default());
            assert!(state.delimiter.is_none(), "{lang}");
        }
        let (_, state) = line("String s = \"\"\"", "java", &State::default());
        assert!(state.delimiter.is_some());
        for lang in [
            "ruby",
            "lua",
            "r",
            "php",
            "powershell",
            "dockerfile",
            "makefile",
            "graphql",
            "diff",
            "log",
            "yaml",
            "c#",
        ] {
            let source = "return value = 123 # comment";
            let (spans, _) = line(source, lang, &State::default());
            assert_eq!(
                spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
                source
            );
        }
        for (lang, first, last) in [
            ("python", "value = \"\"\"start", "end\"\"\" + 1"),
            ("rust", "let s = r##\"start", "end\"##;"),
            ("lua", "local s = [[start", "end]]"),
        ] {
            let (_, state) = line(first, lang, &State::default());
            assert!(state.delimiter.is_some(), "{lang}");
            let (spans, state) = line(last, lang, &state);
            assert!(state.delimiter.is_none());
            assert_eq!(
                spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
                last
            );
        }
    }
    #[test]
    fn streaming_prefix_is_unicode_safe() {
        assert_eq!(stable_prefix("　r##"), "　");
        assert_eq!(stable_prefix("call "), "");
        assert_eq!(stable_prefix("r##\"body"), "r##\"");
        assert_eq!(stable_prefix("let x=r##"), "let x=");
        assert_eq!(stable_prefix("x /*"), "x /*");
    }
}
