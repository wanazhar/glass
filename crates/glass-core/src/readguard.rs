//! Read-before-write guard.
//!
//! Every high-scoring harness has some version of this, and it is the one
//! guardrail that needs no extra tool: refuse to let the model overwrite a file
//! it has never looked at.
//!
//! The failure it prevents is specific and expensive. `sed -i 's/x/y/' f.rs`
//! against a file the model has never read will happily succeed, and the model
//! will only discover the damage when the build fails several turns later — or
//! not at all. Requiring a read first costs nothing when the model was going to
//! read anyway, which is nearly always, and turns a silent corruption into an
//! immediate, legible error.
//!
//! Two rules keep it from becoming an obstacle:
//!
//! 1. **Only files that already exist.** Creating a new file is not a blind
//!    overwrite, so `cat > new.rs` is never blocked.
//! 2. **Any read counts.** Not `cat` specifically — a `rg` that matched inside
//!    the file, a `git diff` that showed it, a build that compiled it. The
//!    model has seen the contents; the mechanism does not matter.

use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Tool-call-age past which an edit tool result reminds the model to reread.
/// A read 25 tool calls ago is stale for editing purposes even when the
/// revision guard would still accept the write: the model's memory of the
/// file is old even when the file is not.
pub const STALE_READ_CALLS: u64 = 25;

/// Files the model has demonstrably seen the contents of.
#[derive(Default)]
pub struct ReadSet {
    seen: HashSet<String>,
    native_revisions: HashMap<String, String>,
    /// Tool-call index of the most recent read per canonical path, for the
    /// staleness hint. Absent means never read (but `seen` is the authority).
    last_read: HashMap<String, u64>,
}

impl ReadSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember only successful native results, before extension/mod rewriting.
    pub fn observe_native(
        &mut self,
        call: &glass_mod_api::ToolCall,
        body: &str,
        workspace: &Path,
        call_index: u64,
    ) {
        if !matches!(call.name.as_str(), "read" | "edit" | "write")
            || call.arg_raw("preview").as_deref() == Some("true")
        {
            return;
        }
        let Some(path) = call.arg_str("path") else {
            return;
        };
        if call.name == "read" || !self.contains(&workspace.join(&path)) {
            self.mark_at(&workspace.join(&path), call_index);
        }
        let Some(header) = body
            .lines()
            .next()
            .filter(|line| line.starts_with("[file "))
        else {
            return;
        };
        let Some((_, rest)) = header.rsplit_once(" · revision ") else {
            return;
        };
        let revision: String = rest.chars().take(64).collect();
        if revision.len() == 64 && revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            self.native_revisions.insert(
                super::concurrency::canonical_key(&workspace.join(path)),
                revision,
            );
        }
    }

    /// Supply the last observed revision, never a fresh disk revision. Explicit
    /// revisions retain their original semantics; absent writes are create-only.
    pub fn native_call(
        &self,
        call: &glass_mod_api::ToolCall,
        workspace: &Path,
    ) -> glass_mod_api::ToolCall {
        let mut result = call.clone();
        if !matches!(call.name.as_str(), "edit" | "write") {
            return result;
        }
        let Ok(mut args) = serde_json::from_str::<serde_json::Value>(&call.arguments) else {
            return result;
        };
        if args.get("expected").is_some() {
            return result;
        }
        if let Some(path) = call.arg_str("path") {
            let revision = self
                .native_revisions
                .get(&super::concurrency::canonical_key(&workspace.join(path)));
            if let Some(revision) = revision {
                args["expected"] = serde_json::json!(revision);
            } else if call.name == "write" {
                args["expected"] = serde_json::Value::Null;
            }
        }
        result.arguments = args.to_string();
        result
    }

    /// Record a file the model has read, without an index (fresh by definition).
    /// Retained for tests and callers that have no call counter.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn mark(&mut self, path: &Path) {
        self.mark_at(path, 0);
    }
    /// Record a read that happened at the given tool-call index.
    pub fn mark_at(&mut self, path: &Path, call_index: u64) {
        let key = super::concurrency::canonical_key(path);
        self.seen.insert(key.clone());
        let entry = self.last_read.entry(key).or_insert(call_index);
        if call_index > *entry {
            *entry = call_index;
        }
    }

    /// Tool-call index of the most recent read, or `None` when never read.
    pub fn last_read_at(&self, path: &Path) -> Option<u64> {
        self.last_read
            .get(&super::concurrency::canonical_key(path))
            .copied()
    }

    /// The staleness hint for an edit result, or `None` when the read is
    /// fresh enough or the file was never read (the revision guard already
    /// covers that case with its own error).
    pub fn staleness_hint(&self, path: &Path, current_call: u64) -> Option<String> {
        let last = self.last_read_at(path)?;
        let age = current_call.saturating_sub(last);
        (age >= STALE_READ_CALLS).then(|| {
            format!(
                "note: `{}` was last read {age} tool calls ago; reread it before further edits.",
                path.display()
            )
        })
    }

    /// Whether the model has read this file.
    pub fn contains(&self, path: &Path) -> bool {
        self.seen.contains(&super::concurrency::canonical_key(path))
    }

    /// Paths recorded as read, for tests and diagnostics.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn marked(&self) -> &HashSet<String> {
        &self.seen
    }

    /// Inspect a bash command for reads and record them.
    ///
    /// Recognize explicit reads without executing or expanding shell text.
    /// Unknown shell syntax may require a native read before a later write.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn observe(&mut self, command: &str, output: &str, workspace: &Path) {
        self.observe_at(command, output, workspace, 0)
    }

    /// Same scan, recording reads at the given tool-call index so the
    /// staleness hint can age reads later. Callers without an index (tests,
    /// recovery replay) use 0, which is "oldest possible" — a hint on age 0
    /// would fire immediately, so 0 means fresh-enough by construction: the
    /// age check only fires when `current_call - last >= STALE_READ_CALLS`.
    pub fn observe_at(&mut self, command: &str, output: &str, workspace: &Path, call_index: u64) {
        let cmd = command.trim();
        if cmd.is_empty() {
            return;
        }

        for path in reader_paths(cmd, output, workspace) {
            self.mark_at(&path, call_index);
        }

        // Only positional search operands count, never option values or patterns.
        for path in search_paths(cmd, workspace) {
            self.mark_at(&path, call_index);
        }

        // A compiler or test run read and validated every file it touched, and
        // the failure text names them, so anything mentioned in a non-empty
        // result from a build tool counts as read.
        if is_build_command(cmd) && !output.trim().is_empty() {
            for line in output.lines() {
                for tok in line.split_whitespace() {
                    let t = tok.trim_matches(|c: char| {
                        !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-'
                    });
                    if looks_like_path(t) {
                        self.mark_at(&abs(t, workspace), call_index);
                    }
                }
            }
        }
    }

    /// Check a set of write targets, returning those that must not be written.
    ///
    /// Nonexistent files are always allowed: creating is not overwriting.
    pub fn blocked<'a>(&self, targets: &'a [std::path::PathBuf]) -> Vec<&'a std::path::PathBuf> {
        targets
            .iter()
            .filter(|p| p.is_file() && !self.contains(p))
            .collect()
    }
}

fn is_build_command(command: &str) -> bool {
    const BUILD_TOOLS: &[&str] = &[
        "cargo", "go", "npm", "make", "tsc", "pytest", "gcc", "clang", "gradle", "gradlew", "mvn",
        "dotnet", "yarn", "pnpm", "pip", "pip3", "poetry",
    ];
    let Some(tokens) = shell_tokens(command) else {
        return false;
    };
    let Some((executable, _)) = tokens.into_iter().find(|(_, operator)| !operator) else {
        return false;
    };
    let name = Path::new(&executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&executable);
    BUILD_TOOLS.contains(&name)
}

// Small lexical scan, never shell execution. Operators terminate commands;
// quotes preserve literal file names and redirection targets are not reads.
fn shell_tokens(command: &str) -> Option<Vec<(String, bool)>> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    for ch in command.chars() {
        if comment {
            if ch != '\n' {
                continue;
            }
            comment = false;
        }
        if escaped {
            word.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(end) = quote {
            if ch == end {
                quote = None;
            } else {
                word.push(ch);
            }
        } else if ch == '#' && word.is_empty() {
            comment = true;
        } else if matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if matches!(ch, ';' | '|' | '&' | '<' | '>' | '\n') {
            if !word.is_empty() {
                if matches!(ch, '<' | '>') && word.bytes().all(|byte| byte.is_ascii_digit()) {
                    word.clear(); // An adjacent numeric descriptor is not a file operand.
                } else {
                    tokens.push((std::mem::take(&mut word), false));
                }
            }
            tokens.push((ch.to_string(), true));
        } else if ch.is_whitespace() {
            if !word.is_empty() {
                tokens.push((std::mem::take(&mut word), false));
            }
        } else {
            word.push(ch);
        }
    }
    if quote.is_some() || escaped {
        return None;
    }
    if !word.is_empty() {
        tokens.push((word, false));
    }
    Some(tokens)
}

fn reader_paths(command: &str, output: &str, workspace: &Path) -> Vec<std::path::PathBuf> {
    let Some(tokens) = shell_tokens(command) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    let mut segment = Vec::new();
    let mut output_hidden = false;
    let mut index = 0;
    while index < tokens.len() {
        let (token, operator) = &tokens[index];
        let next_is_same = tokens
            .get(index + 1)
            .is_some_and(|(next, next_operator)| *next_operator && next == token);
        if *operator && matches!(token.as_str(), ";" | "\n") {
            if !output_hidden {
                paths.extend(reader_paths_in_command(&segment, output, workspace));
            }
            segment.clear();
            output_hidden = false;
        } else if *operator && matches!(token.as_str(), "&" | "|") && next_is_same {
            if !output_hidden {
                paths.extend(reader_paths_in_command(&segment, output, workspace));
            }
            segment.clear();
            output_hidden = false;
            index += 1;
        } else if *operator && matches!(token.as_str(), "|" | "&") {
            // Pipeline/background output is not reliably present in the
            // foreground tool result. Suppress the whole connected segment.
            output_hidden = true;
            segment.push((token.clone(), true));
        } else if *operator && token == ">" {
            // Redirection hides the reader's standard output from the model.
            output_hidden = true;
            segment.push((token.clone(), true));
        } else {
            segment.push((token.clone(), *operator));
        }
        index += 1;
    }
    if !output_hidden {
        paths.extend(reader_paths_in_command(&segment, output, workspace));
    }
    paths
}

fn reader_paths_in_command(
    tokens: &[(String, bool)],
    output: &str,
    workspace: &Path,
) -> Vec<std::path::PathBuf> {
    if let Some(words) = command_words(tokens) {
        let name = words
            .first()
            .and_then(|word| Path::new(word).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("");
        match name {
            "git" => {
                return git_show_read_path(&words, output, workspace)
                    .into_iter()
                    .collect();
            }
            "sed" => return sed_printed_paths(&words, output, workspace),
            "awk" => return awk_printed_paths(&words, output, workspace),
            "perl" => return perl_printed_paths(&words, output, workspace),
            _ => {}
        }
    }
    reader_paths_from_shell(tokens, workspace)
}

fn reader_paths_from_shell(tokens: &[(String, bool)], workspace: &Path) -> Vec<std::path::PathBuf> {
    let mut command_word = true;
    let mut reader = String::new();
    let mut redirect = None;
    let mut inputs = Vec::new();
    let mut option_value = false;
    let mut options = true;
    let mut paths = Vec::new();
    for (token, operator) in tokens.iter().cloned() {
        if operator {
            if matches!(token.as_str(), ">" | "<") {
                redirect = if redirect == Some('!') || token == "<" && redirect == Some('<') {
                    Some('!')
                } else {
                    token.chars().next()
                };
            } else {
                command_word = true;
                reader.clear();
                redirect = None;
                inputs.clear();
                option_value = false;
                options = true;
            }
            continue;
        }
        if let Some(kind) = redirect.take() {
            if kind == '<' && !token.contains(['$', '`', '*', '?']) {
                let path = abs(&token, workspace);
                if path.is_file() {
                    if !reader.is_empty() {
                        paths.push(path);
                    } else if command_word {
                        inputs.push(path);
                    }
                }
            }
            continue;
        }
        if command_word {
            command_word = false;
            let name = Path::new(&token)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if ["cat", "head", "tail", "less", "bat", "nl"].contains(&name) {
                reader = name.into();
                paths.append(&mut inputs);
            }
            continue;
        }
        if reader.is_empty() {
            continue;
        }
        if option_value {
            option_value = false;
            continue;
        }
        if options && token == "--" {
            options = false;
            continue;
        }
        if options && token.starts_with('-') {
            option_value = matches!(
                (reader.as_str(), token.as_str()),
                ("head" | "tail", "-n" | "-c" | "--lines" | "--bytes")
                    | ("stat", "-c" | "--format" | "--printf")
                    | ("bat", "-l" | "--language" | "--theme" | "--style")
                    | ("nl", "-b" | "-n" | "-s" | "-w")
            );
            continue;
        }
        if token == "-" {
            continue;
        }
        if token.contains(['$', '`', '*', '?']) {
            continue;
        }
        let path = abs(&token, workspace);
        if path.is_file() {
            paths.push(path);
        }
    }
    paths
}

fn command_words(tokens: &[(String, bool)]) -> Option<Vec<String>> {
    tokens
        .iter()
        .map(|(token, operator)| (!operator).then(|| token.clone()))
        .collect()
}

fn explicit_file_operands(
    words: &[String],
    start: usize,
    workspace: &Path,
) -> Vec<std::path::PathBuf> {
    let mut options = true;
    let mut paths = Vec::new();
    for word in words.iter().skip(start) {
        if options && word == "--" {
            options = false;
            continue;
        }
        if options && word.starts_with('-') {
            return Vec::new();
        }
        if word.contains(['$', '`', '*', '?']) {
            return Vec::new();
        }
        let path = abs(word, workspace);
        if path.is_file() {
            paths.push(path);
        }
    }
    paths
}

fn sed_printed_paths(words: &[String], output: &str, workspace: &Path) -> Vec<std::path::PathBuf> {
    let mut script_index = 1;
    if !words
        .get(script_index)
        .is_some_and(|option| matches!(option.as_str(), "-n" | "--quiet" | "--silent"))
    {
        return Vec::new();
    }
    script_index += 1;
    if words.get(script_index).is_some_and(|option| option == "-e") {
        script_index += 1;
    }
    let Some(script) = words.get(script_index) else {
        return Vec::new();
    };
    if !plain_sed_print_script(script) {
        return Vec::new();
    }
    explicit_file_operands(words, script_index + 1, workspace)
        .into_iter()
        .filter(|path| output_contains_source_lines(output, path))
        .collect()
}

fn plain_sed_print_script(script: &str) -> bool {
    let Some(address) = script.strip_suffix('p') else {
        return false;
    };
    if address.is_empty() || address == "$" || address.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    let mut addresses = address.split(',');
    let first = addresses.next().unwrap_or_default();
    let second = addresses.next();
    second.is_some_and(|last| {
        matches_plain_sed_address(first)
            && matches_plain_sed_address(last)
            && addresses.next().is_none()
    })
}

fn matches_plain_sed_address(address: &str) -> bool {
    address == "$" || (!address.is_empty() && address.bytes().all(|byte| byte.is_ascii_digit()))
}

fn awk_printed_paths(words: &[String], output: &str, workspace: &Path) -> Vec<std::path::PathBuf> {
    let Some(script) = words.get(1) else {
        return Vec::new();
    };
    let normalized = script.split_whitespace().collect::<String>();
    if !matches!(
        normalized.as_str(),
        "1" | "print" | "{print}" | "{print$0}" | "1{print}" | "1{print$0}"
    ) {
        return Vec::new();
    }
    explicit_file_operands(words, 2, workspace)
        .into_iter()
        .filter(|path| output_contains_source_lines(output, path))
        .collect()
}

fn perl_printed_paths(words: &[String], output: &str, workspace: &Path) -> Vec<std::path::PathBuf> {
    let (script_index, operands_index) = match words.get(1..3) {
        Some([option, script]) if matches!(option.as_str(), "-ne" | "-en") => (2, 3),
        _ if words.get(1).is_some_and(|option| option == "-n")
            && words.get(2).is_some_and(|option| option == "-e") =>
        {
            (3, 4)
        }
        _ => return Vec::new(),
    };
    let Some(script) = words.get(script_index) else {
        return Vec::new();
    };
    if !matches!(script.trim(), "print" | "print $_" | "print $_;") {
        return Vec::new();
    }
    explicit_file_operands(words, operands_index, workspace)
        .into_iter()
        .filter(|path| output_matches_file(output, path))
        .collect()
}

fn git_show_read_path(
    words: &[String],
    output: &str,
    workspace: &Path,
) -> Option<std::path::PathBuf> {
    if words.len() != 3 || words[1] != "show" || output.len() > 1024 * 1024 {
        return None;
    }
    let (revision, relative) = words[2].split_once(':')?;
    if revision.is_empty() || relative.is_empty() || relative.starts_with('/') {
        return None;
    }
    let relative = Path::new(relative);
    if relative.components().any(|component| {
        !matches!(
            component,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) || relative.to_string_lossy().contains(['$', '`', '*', '?'])
    {
        return None;
    }
    let root = workspace.canonicalize().ok()?;
    let path = abs(&relative.to_string_lossy(), workspace)
        .canonicalize()
        .ok()?;
    if !path.starts_with(root) || !output_matches_file(output, &path) {
        return None;
    }
    Some(path)
}

fn output_contains_source_lines(output: &str, path: &Path) -> bool {
    if output.is_empty() || output.len() > 1024 * 1024 {
        return false;
    }
    let Some(bytes) = read_file_bounded(path, 4 * 1024 * 1024) else {
        return false;
    };
    let Ok(source) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let source_lines = source.lines().collect::<HashSet<_>>();
    let output_lines = output.lines().collect::<Vec<_>>();
    !output_lines.is_empty() && output_lines.iter().all(|line| source_lines.contains(line))
}

fn output_matches_file(output: &str, path: &Path) -> bool {
    output.len() <= 1024 * 1024
        && read_file_bounded(path, 1024 * 1024)
            .is_some_and(|source| source.as_slice() == output.as_bytes())
}

fn read_file_bounded(path: &Path, max_bytes: usize) -> Option<Vec<u8>> {
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= max_bytes).then_some(bytes)
}

// Conservative parsing: unsupported shell syntax/options require a native read.
fn search_paths(command: &str, workspace: &Path) -> Vec<std::path::PathBuf> {
    let Some(tokens) = shell_tokens(command) else {
        return Vec::new();
    };
    if tokens.iter().any(|(_, operator)| *operator) {
        return Vec::new();
    }
    let mut words = tokens.into_iter().map(|(word, _)| word);
    let Some(tool) = words.next() else {
        return Vec::new();
    };
    if !matches!(
        Path::new(&tool).file_name().and_then(|n| n.to_str()),
        Some("rg" | "grep")
    ) {
        return Vec::new();
    }
    let mut pattern = false;
    let mut options = true;
    let mut paths = Vec::new();
    while let Some(word) = words.next() {
        if options && word == "--" {
            options = false;
            continue;
        }
        if options && word.starts_with('-') {
            let (flag, attached) = word
                .split_once('=')
                .map_or((word.as_str(), false), |(f, _)| (f, true));
            match flag {
                "-e" | "--regexp" | "-f" | "--file" => {
                    pattern = true;
                    if !attached && words.next().is_none() {
                        return Vec::new();
                    }
                }
                "-g" | "--glob" | "--iglob" | "-t" | "--type" | "-T" | "--type-not"
                | "--include" | "--exclude" | "--exclude-dir" | "-m" | "--max-count" | "-A"
                | "-B" | "-C" | "--after-context" | "--before-context" | "--context"
                | "--encoding" | "--color" => {
                    if !attached && words.next().is_none() {
                        return Vec::new();
                    }
                }
                "-n" | "--line-number" | "-i" | "--ignore-case" | "-S" | "--smart-case" | "-s"
                | "--no-messages" | "-F" | "--fixed-strings" | "-w" | "--word-regexp" | "-r"
                | "-R" | "--recursive" | "-H" | "--with-filename" | "-h" | "--no-filename"
                | "--hidden" | "--no-ignore" | "-v" | "--invert-match" => {}
                _ => return Vec::new(),
            }
            continue;
        }
        if !pattern {
            pattern = true;
            continue;
        }
        if word == "-" {
            continue;
        }
        if word.contains(['$', '`', '*', '?']) {
            continue;
        }
        let path = abs(&word, workspace);
        if path.is_file() {
            paths.push(path)
        }
    }
    paths
}

fn looks_like_path(t: &str) -> bool {
    t.contains('/') || t.contains('.')
}

fn abs(p: &str, workspace: &Path) -> std::path::PathBuf {
    let raw = Path::new(p);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        workspace.join(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-rbw-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn metadata_and_count_commands_do_not_authorize_content_edits() {
        let dir = scratch("metadata-only");
        let file = dir.join("a.rs");
        std::fs::write(&file, "private contents").unwrap();
        for command in ["wc -l a.rs", "stat a.rs", "file a.rs", "source a.rs"] {
            let mut reads = ReadSet::new();
            reads.observe(command, "metadata", &dir);
            assert!(!reads.contains(&file), "{command}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cat_marks_the_file_read() {
        let d = scratch("cat");
        let ws = d.clone();
        let f = d.join("a.rs");
        std::fs::write(&f, "fn main() {}").unwrap();

        let mut rs = ReadSet::new();
        rs.observe(&format!("cat {}", f.display()), "fn main() {}", &ws);
        assert!(rs.contains(&f), "cat must count as a read");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn reader_options_quotes_and_multiple_files_count_as_reads() {
        let dir = scratch("reader-options");
        for name in ["a.rs", "space name.rs", "README"] {
            std::fs::write(dir.join(name), "contents").unwrap();
        }
        for command in [
            "cat -n a.rs",
            "head -n 5 a.rs",
            "head -n5 a.rs",
            "tail --bytes 8 a.rs",
            "/bin/cat -- a.rs",
            "cat a.rs README",
            "cat 'space name.rs'",
        ] {
            let mut reads = ReadSet::new();
            reads.observe_at(command, "contents", &dir, 7);
            let name = if command.contains("space name") {
                "space name.rs"
            } else {
                "a.rs"
            };
            assert_eq!(reads.last_read_at(&dir.join(name)), Some(7), "{command}");
            if command.ends_with("README") {
                assert!(reads.contains(&dir.join("README")));
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reader_arguments_do_not_mark_redirects_comments_or_other_commands() {
        let dir = scratch("reader-boundaries");
        for name in ["a.rs", "b.rs"] {
            std::fs::write(dir.join(name), "contents").unwrap();
        }
        for command in ["echo cat b.rs", "cat a.rs > b.rs", "cat a.rs | echo b.rs"] {
            let mut reads = ReadSet::new();
            reads.observe(command, "contents", &dir);
            assert!(
                !reads.contains(&dir.join("a.rs")) && !reads.contains(&dir.join("b.rs")),
                "output not visible to the model must not authorize either path: {command}"
            );
        }
        let mut reads = ReadSet::new();
        reads.observe("cat a.rs; echo b.rs", "contents", &dir);
        assert!(reads.contains(&dir.join("a.rs")));
        assert!(!reads.contains(&dir.join("b.rs")));

        let mut reads = ReadSet::new();
        reads.observe("cat a.rs # b.rs", "contents", &dir);
        assert!(reads.contains(&dir.join("a.rs")));
        assert!(!reads.contains(&dir.join("b.rs")));

        let mut reads = ReadSet::new();
        reads.observe("cat a.rs; cat b.rs > /dev/null", "contents", &dir);
        assert!(reads.contains(&dir.join("a.rs")));
        assert!(!reads.contains(&dir.join("b.rs")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn common_readers_only_earn_credit_when_source_text_is_emitted() {
        let dir = scratch("extra-readers");
        let path = dir.join("a.rs");
        let contents = "line one\nline two\n";
        std::fs::write(&path, contents).unwrap();
        let cases = [
            ("git show HEAD:a.rs", contents, true),
            ("git show HEAD:a.rs", "old file version\n", false),
            ("sed -n '1,2p' a.rs", contents, true),
            ("sed -n '1p' a.rs", "line one\n", true),
            ("sed -i 's/line/new/' a.rs", contents, false),
            ("sed -n '1,2p' a.rs > /dev/null", contents, false),
            ("awk '{print $0}' a.rs", contents, true),
            (
                "awk 'BEGIN { print \"line one\" }' a.rs",
                "line one\n",
                false,
            ),
            ("awk '{print $0}' a.rs | cat", contents, false),
            ("perl -ne 'print' a.rs", contents, true),
            (
                "perl -ne 'system(\"echo unsafe\"); print' a.rs",
                contents,
                false,
            ),
        ];
        for (command, output, expected) in cases {
            let mut reads = ReadSet::new();
            reads.observe(command, output, &dir);
            assert_eq!(
                reads.contains(&path),
                expected,
                "unexpected read credit for {command:?} with output {output:?}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn input_redirection_is_read_but_here_documents_and_output_targets_are_not() {
        let dir = scratch("input-redirection");
        let path = dir.join("a.rs");
        std::fs::write(&path, "contents").unwrap();
        for command in ["cat < a.rs", "head -n 2 <a.rs", "< a.rs cat"] {
            let mut reads = ReadSet::new();
            reads.observe(command, "contents", &dir);
            assert!(reads.contains(&path), "{command}");
        }
        for command in ["echo > a.rs", "cat << a.rs", "cat <<< a.rs"] {
            let mut reads = ReadSet::new();
            reads.observe(command, "contents", &dir);
            assert!(!reads.contains(&path), "{command}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn relative_paths_resolve_against_the_workspace() {
        let d = scratch("rel");
        let f = d.join("src/lib.rs");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "x").unwrap();

        let mut rs = ReadSet::new();
        rs.observe("cat src/lib.rs", "x", &d);
        assert!(
            rs.contains(&f),
            "a relative read must resolve to the same key as an absolute one"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn bare_recursive_search_is_not_a_read() {
        let d = scratch("rg");
        let mut rs = ReadSet::new();
        // `rg foo .` says nothing about any individual file's contents.
        rs.observe("rg foo .", "src/a.rs:1:foo", &d);
        assert!(rs.marked().is_empty(), "a tree-wide grep is not a read");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn grep_naming_a_file_is_a_read() {
        let d = scratch("grepfile");
        let f = d.join("a.rs");
        std::fs::write(&f, "foo").unwrap();
        let mut rs = ReadSet::new();
        rs.observe(&format!("rg foo {}", f.display()), "1:foo", &d);
        assert!(rs.contains(&f));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn search_options_and_patterns_never_authorize_writes() {
        let dir = scratch("search-options");
        let file = dir.join("config.rs");
        std::fs::write(&file, "secret contents").unwrap();
        for command in [
            "rg --glob config.rs no_such_match .",
            "grep --include config.rs missing .",
            "rg -e config.rs .",
            "rg --files config.rs",
            "rg -l pattern config.rs",
            "rgfoo pattern config.rs",
            "rg pattern .; echo config.rs",
            "echo < config.rs",
        ] {
            let mut reads = ReadSet::new();
            reads.observe(command, "", &dir);
            assert!(!reads.contains(&file), "{command}");
        }
        for command in [
            "rg -n --glob '*.rs' secret config.rs",
            "grep -e secret -- config.rs",
            "rg secret 'config.rs'",
        ] {
            let mut reads = ReadSet::new();
            reads.observe(command, "secret contents", &dir);
            assert!(reads.contains(&file), "{command}");
        }
        std::fs::write(dir.join("-"), "literal dash").unwrap();
        for command in ["rg -- secret -", "grep -- secret -", "cat -- -"] {
            let mut reads = ReadSet::new();
            reads.observe(command, "stdin contents", &dir);
            assert!(!reads.contains(&dir.join("-")), "{command}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unwritten_file_is_blocked() {
        let d = scratch("block");
        let f = d.join("a.rs");
        std::fs::write(&f, "old").unwrap();
        let rs = ReadSet::new();
        let targets = [f];
        assert_eq!(
            rs.blocked(&targets).len(),
            1,
            "an unread existing file must be blocked"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn read_file_is_allowed() {
        let d = scratch("allow");
        let f = d.join("a.rs");
        std::fs::write(&f, "old").unwrap();
        let mut rs = ReadSet::new();
        rs.mark(&f);
        let targets = [f];
        assert!(rs.blocked(&targets).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn new_file_is_never_blocked() {
        // Creating is not overwriting.
        let d = scratch("new");
        let f = d.join("brand_new.rs");
        assert!(!f.exists());
        let rs = ReadSet::new();
        let targets = [f];
        assert!(
            rs.blocked(&targets).is_empty(),
            "a nonexistent file must be writable"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn different_spellings_of_one_path_are_the_same_key() {
        let d = scratch("spell");
        let f = d.join("a.rs");
        std::fs::write(&f, "x").unwrap();
        let mut rs = ReadSet::new();
        rs.mark(&f);
        assert!(
            rs.contains(&d.join("./a.rs")),
            "the same file must not look unread because of spelling"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn build_output_marks_named_files_read() {
        let d = scratch("build");
        let f = d.join("a.rs");
        std::fs::write(&f, "x").unwrap();
        for command in [
            "cargo build",
            "/usr/bin/cargo build",
            "gradle test",
            "./gradlew test",
            "mvn test",
            "dotnet test",
            "yarn build",
            "pnpm build",
            "pip install package",
            "pip3 install package",
            "poetry build",
        ] {
            let mut reads = ReadSet::new();
            reads.observe(command, &format!("error[E0425]: {}", f.display()), &d);
            assert!(
                reads.contains(&f),
                "a recognized build tool that named the file clearly read it: {command}"
            );
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn build_output_from_a_prefix_collision_does_not_mark_files_read() {
        let d = scratch("build-prefix");
        let f = d.join("a.rs");
        std::fs::write(&f, "x").unwrap();
        for command in [
            "cargo-wrapper build",
            "npm-preview test",
            "maven-helper test",
        ] {
            let mut reads = ReadSet::new();
            reads.observe(command, &format!("diagnostic mentions {}", f.display()), &d);
            assert!(
                !reads.contains(&f),
                "an unrelated executable must not earn build-read credit: {command}"
            );
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn writes_do_not_mark_a_file_read() {
        // Writing is not reading. A command that overwrites a file must not
        // thereby earn permission to overwrite it again.
        let d = scratch("write");
        let f = d.join("a.rs");
        std::fs::write(&f, "old").unwrap();
        let mut rs = ReadSet::new();
        rs.observe(&format!("echo new > {}", f.display()), "", &d);
        assert!(!rs.contains(&f), "writing must not count as reading");
        std::fs::remove_dir_all(&d).ok();
    }
}

#[cfg(test)]
mod native_revision_tests {
    use super::*;
    use glass_mod_api::ToolCall;
    use serde_json::json;
    fn call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "revision-test".into(),
            name: name.into(),
            arguments: args.to_string(),
        }
    }
    #[test]
    fn revisions_are_observed_not_resampled_and_explicit_values_win() {
        let root = Path::new("/tmp");
        let mut reads = ReadSet::new();
        let read = call("read", json!({"path":"gc-revision-test"}));
        let rev = "a".repeat(64);
        reads.observe_native(
            &read,
            &format!("[file gc-revision-test · revision {rev} · lines 1/1]\n1|hello"),
            root,
            0,
        );
        let edit = call(
            "edit",
            json!({"path":"gc-revision-test","edits":[{"old":"hello","new":"bye"}]}),
        );
        assert_eq!(
            reads.native_call(&edit, root).arg_str("expected"),
            Some(rev.clone())
        );
        let explicit = call(
            "write",
            json!({"path":"gc-revision-test","expected":null,"text":"new"}),
        );
        assert_eq!(
            reads
                .native_call(&explicit, root)
                .arg_raw("expected")
                .as_deref(),
            Some("null")
        );
        let preview = call("edit", json!({"path":"gc-revision-test","preview":true}));
        reads.observe_native(
            &preview,
            &format!(
                "[file gc-revision-test · preview · revision {}]",
                "b".repeat(64)
            ),
            root,
            0,
        );
        assert_eq!(
            reads.native_call(&edit, root).arg_str("expected"),
            Some(rev)
        );
        assert!(
            ReadSet::new()
                .native_call(&edit, root)
                .arg_raw("expected")
                .is_none()
        );
        let create = call("write", json!({"path":"unseen","text":"new"}));
        assert_eq!(
            reads
                .native_call(&create, root)
                .arg_raw("expected")
                .as_deref(),
            Some("null")
        );
    }
}
