//! Bounded native file operations, exposed through bash rather than extra tool schemas.
use serde_json::{Value, json};
use std::{
    ffi::CString,
    fs::File,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path},
    time::{Duration, Instant},
};
const MAX_FILE: usize = 8 * 1024 * 1024;
const MAX_PREVIEW: usize = 24 * 1024;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn c(s: &std::ffi::OsStr) -> Result<CString, String> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(s.as_bytes()).map_err(error)
}
fn open_at(dir: &File, name: &CString, flags: i32, mode: u32) -> Result<File, String> {
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if fd < 0 {
        Err(error(std::io::Error::last_os_error()))
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
struct Target {
    dir: File,
    name: CString,
    label: String,
}
impl Target {
    fn new(root: &Path, path: &str) -> Result<Self, String> {
        let root = root.canonicalize().map_err(error)?;
        let path = Path::new(path);
        let path = if path.is_absolute() {
            path.strip_prefix(&root)
                .map_err(|_| "path escapes workspace")?
        } else {
            path
        };
        let parts = path
            .components()
            .filter(|p| !matches!(p, Component::CurDir))
            .collect::<Vec<_>>();
        if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
            return Err("choose a file within the workspace; '..' is not accepted".into());
        }
        let mut dir = File::open(&root).map_err(error)?;
        for part in &parts[..parts.len() - 1] {
            dir = open_at(
                &dir,
                &c(part.as_os_str())?,
                libc::O_RDONLY | libc::O_DIRECTORY,
                0,
            )?;
        }
        Ok(Self {
            dir,
            name: c(parts.last().unwrap().as_os_str())?,
            label: path.display().to_string(),
        })
    }
    fn read(&self) -> Result<Option<(Vec<u8>, std::fs::Metadata)>, String> {
        let file = match open_at(&self.dir, &self.name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
            Ok(f) => f,
            Err(e) => {
                // Only ENOENT means create-only is allowed; every other failure is fatal.
                let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
                let rc = unsafe {
                    libc::fstatat(
                        self.dir.as_raw_fd(),
                        self.name.as_ptr(),
                        stat.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                if rc < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                    return Ok(None);
                }
                return Err(e);
            }
        };
        let meta = file.metadata().map_err(error)?;
        if !meta.is_file() {
            return Err("only regular files are supported".into());
        }
        if meta.len() > MAX_FILE as u64 {
            return Err("file exceeds the 8 MiB safety bound".into());
        }
        let mut bytes = Vec::new();
        (&file)
            .take((MAX_FILE + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if bytes.len() > MAX_FILE {
            return Err("file exceeds the 8 MiB safety bound".into());
        }
        let after = file.metadata().map_err(error)?;
        if meta.len() != after.len()
            || meta.mtime() != after.mtime()
            || meta.mtime_nsec() != after.mtime_nsec()
            || meta.ctime() != after.ctime()
            || meta.ctime_nsec() != after.ctime_nsec()
        {
            return Err("file changed during read; retry".into());
        }
        Ok(Some((bytes, meta)))
    }
    fn commit(&self, old: Option<&[u8]>, bytes: &[u8], mode: u32) -> Result<(), String> {
        let now = self.read()?;
        if now.as_ref().map(|(b, _)| b.as_slice()) != old {
            return Err("stale revision; nothing written, re-read the file".into());
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let name = CString::new(format!(
            ".gc-write-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
        .unwrap();
        let result = (|| {
            let mut temp = open_at(
                &self.dir,
                &name,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )?;
            temp.write_all(bytes).map_err(error)?;
            temp.set_permissions(std::fs::Permissions::from_mode(mode & 0o777))
                .map_err(error)?;
            temp.sync_all().map_err(error)?;
            let current = self.read()?;
            if current.as_ref().map(|(b, _)| b.as_slice()) != old {
                return Err("stale revision; nothing written, re-read the file".into());
            }
            if old.is_none() {
                // A link is an atomic create-if-absent; never overwrite a racing creator.
                if unsafe {
                    libc::linkat(
                        self.dir.as_raw_fd(),
                        name.as_ptr(),
                        self.dir.as_raw_fd(),
                        self.name.as_ptr(),
                        0,
                    )
                } < 0
                {
                    return Err(error(std::io::Error::last_os_error()));
                }
            } else if unsafe {
                libc::renameat(
                    self.dir.as_raw_fd(),
                    name.as_ptr(),
                    self.dir.as_raw_fd(),
                    self.name.as_ptr(),
                )
            } < 0
            {
                return Err(error(std::io::Error::last_os_error()));
            }
            self.dir.sync_all().map_err(|e| {
                format!("write committed, but directory durability confirmation failed: {e}")
            })?;
            Ok(())
        })();
        unsafe {
            libc::unlinkat(self.dir.as_raw_fd(), name.as_ptr(), 0);
        }
        result
    }
}
fn revision(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn text(bytes: &[u8]) -> Result<&str, String> {
    if bytes.contains(&0) {
        return Err("binary file: use a binary-aware command".into());
    }
    std::str::from_utf8(bytes).map_err(|_| "file is not valid UTF-8; nothing written".into())
}
fn clip(s: &str, max: usize) -> &str {
    let mut n = s.len().min(max);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    &s[..n]
}
fn number(v: &Value, key: &str, default: usize) -> Result<usize, String> {
    match v.get(key) {
        None => Ok(default),
        Some(n) => n
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| format!("{key} must be a nonnegative integer")),
    }
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}
fn preview(old: &str, new: &str) -> String {
    let prefix = old
        .lines()
        .zip(new.lines())
        .take_while(|(a, b)| a == b)
        .count();
    let old_count = old.lines().count();
    let new_count = new.lines().count();
    let suffix = old
        .lines()
        .rev()
        .take(old_count - prefix)
        .zip(new.lines().rev().take(new_count - prefix))
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old_count - prefix - suffix;
    let added = new_count - prefix - suffix;
    let mut out = format!("+{added} -{removed}\n");
    for (mark, source, count) in [("- ", old, removed), ("+ ", new, added)] {
        for line in source.lines().skip(prefix).take(count.min(6)) {
            out.push_str(mark);
            out.push_str(clip(line, 240));
            out.push('\n');
        }
        if count > 6 {
            out.push_str("  … more changed lines\n");
        }
    }
    if old != new && removed == 0 && added == 0 {
        out.push_str("  line ending or final newline changed\n");
    }
    out
}
pub fn execute(root: &Path, action: &str, v: &Value) -> Result<Value, String> {
    let target = Target::new(root, required(v, "path")?)?;
    let mutation = matches!(action, "edit" | "write");
    if mutation {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match fs2::FileExt::try_lock_exclusive(&target.dir) {
                Ok(()) => break,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => return Err(format!("file writer busy: {e}")),
            }
        }
    }
    let existing = target.read()?;
    let bytes = existing
        .as_ref()
        .map(|(b, _)| b.as_slice())
        .unwrap_or_default();
    let source = text(bytes)?;
    if action == "read" {
        if existing.is_none() {
            return Err("file does not exist".into());
        }
        let offset = number(v, "offset", 1)?;
        let limit = number(v, "limit", 200)?.clamp(1, 1000);
        if offset == 0 {
            return Err("offset is 1-indexed".into());
        }
        let total = source.lines().count();
        let mut body = String::new();
        let mut shown = 0;
        let mut partial = false;
        for (i, line) in source.lines().enumerate().skip(offset - 1).take(limit) {
            let row = format!("{}|{}\n", i + 1, line);
            let room = MAX_PREVIEW.saturating_sub(body.len());
            if row.len() > room {
                body.push_str(clip(&row, room));
                partial = true;
                break;
            }
            body.push_str(&row);
            shown += 1;
        }
        return Ok(
            json!({"kind":"file_read","path":target.label,"revision":revision(bytes),"offset":offset,"shown":shown,"total_lines":total,"truncated":partial||offset.saturating_add(shown)<=total,"next_offset":if !partial && offset.saturating_add(shown)<=total {Some(offset+shown)} else {None},"partial_line":partial,"text":body}),
        );
    }
    if action == "check" {
        if existing.is_none() {
            return Err("file does not exist".into());
        }
        return Ok(crate::diagnostics::check(&target.label, source));
    }
    if !mutation {
        return Err("action must be read, edit, write, or check".into());
    }
    if existing.as_ref().is_some_and(|(_, m)| m.nlink() > 1) {
        return Err("refusing to replace a hard-linked file".into());
    }
    let expected = v
        .get("expected")
        .ok_or("expected is required: use the read revision, or null for create-only")?;
    if existing.is_some() {
        if expected.as_str() != Some(revision(bytes).as_str()) {
            return Err("stale revision; nothing written, re-read the file".into());
        }
    } else if !expected.is_null() {
        return Err("file does not exist; use expected:null for create-only".into());
    }
    let result = if action == "write" {
        required(v, "text")?.to_string()
    } else {
        if existing.is_none() {
            return Err("edit needs an existing file".into());
        }
        let ops = v
            .get("edits")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty() && a.len() <= 128)
            .ok_or("edits must contain 1..128 operations")?;
        let mut spans = Vec::new();
        let mut offsets = vec![0];
        let needs_ranges = ops.iter().any(|op| op.get("old").is_none());
        for (i, _) in source.match_indices('\n').filter(|_| needs_ranges) {
            if offsets.len() > 1_000_000 {
                return Err(
                    "line-range edits limited to one million lines; use exact replacements".into(),
                );
            }
            offsets.push(i + 1);
        }
        if offsets.last().copied() != Some(source.len()) {
            offsets.push(source.len());
        }
        let line_count = offsets.len() - 1;
        let ending = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        for op in ops {
            if let Some(old) = op.get("old").and_then(Value::as_str) {
                if old.is_empty() {
                    return Err("old must not be empty; use a line insertion".into());
                }
                let first = source.find(old);
                if first.is_none()
                    || first.is_some_and(|at| {
                        source[at + source[at..].chars().next().unwrap().len_utf8()..].contains(old)
                    })
                {
                    return Err(
                        "old must match exactly once; use line ranges for repeated text".into(),
                    );
                }
                let start = first.unwrap();
                spans.push((start, start + old.len(), required(op, "new")?.to_string()));
            } else {
                let start = number(op, "start", 0)?;
                let end = number(op, "end", usize::MAX)?;
                if start == 0 || start > line_count + 1 || end < start - 1 || end > line_count {
                    return Err(
                        "invalid original-file line range; insertion uses end=start-1".into(),
                    );
                }
                let mut replacement = required(op, "text")?
                    .replace("\r\n", "\n")
                    .replace('\n', ending);
                let from = offsets[start - 1];
                let mut to = offsets[end];
                if from == 0 && to == 0 && source.starts_with('\u{feff}') {
                    to = 3;
                }
                if from == to
                    && from == source.len()
                    && from > 0
                    && !source.ends_with('\n')
                    && !replacement.is_empty()
                {
                    replacement.insert_str(0, ending);
                }
                if !replacement.is_empty()
                    && (to < source.len()
                        || source[from..to].ends_with('\n')
                        || from == to && to == source.len() && source.ends_with('\n'))
                    && !replacement.ends_with('\n')
                {
                    replacement.push_str(ending)
                }
                // Keep an existing BOM on a first-line range replacement.
                if from == 0
                    && source.starts_with('\u{feff}')
                    && !replacement.starts_with('\u{feff}')
                {
                    replacement.insert(0, '\u{feff}');
                }
                spans.push((from, to, replacement));
            }
        }
        spans.sort_by_key(|s| (s.0, s.1));
        for pair in spans.windows(2) {
            if pair[1].0 < pair[0].1 || pair[1].0 == pair[0].0 {
                return Err("overlapping edits; nothing written".into());
            }
        }
        let size = spans
            .iter()
            .try_fold(source.len(), |n, (a, b, t)| {
                n.checked_sub(b - a).and_then(|n| n.checked_add(t.len()))
            })
            .ok_or("edit size overflow")?;
        if size > MAX_FILE {
            return Err("result exceeds 8 MiB".into());
        }
        let mut result = String::with_capacity(size);
        let mut at = 0;
        for (a, b, t) in spans {
            result.push_str(&source[at..a]);
            result.push_str(&t);
            at = b;
        }
        result.push_str(&source[at..]);
        result
    };
    if result.len() > MAX_FILE {
        return Err("result exceeds 8 MiB".into());
    }
    text(result.as_bytes())?;
    if v.get("validate").is_some_and(|v| !v.is_boolean()) {
        return Err("validate must be a boolean".into());
    }
    let diagnostics = if v.get("validate").and_then(Value::as_bool).unwrap_or(false)
        || target.label.ends_with(".json")
    {
        crate::diagnostics::check(&target.label, &result)
    } else {
        json!({"status":"not_requested","diagnostics":[],"message":"Use file check or validate:true for a syntax check"})
    };
    if v.get("validate").and_then(Value::as_bool).unwrap_or(false) && diagnostics["status"] != "ok"
    {
        return Err(format!(
            "validation did not pass; nothing written\n{}",
            crate::diagnostics::summary(&diagnostics)
        ));
    }
    let diff = preview(source, &result);
    let mode = existing.as_ref().map_or(0o600, |(_, m)| m.mode());
    if bytes != result.as_bytes() || existing.is_none() {
        target.commit(
            existing.as_ref().map(|(b, _)| b.as_slice()),
            result.as_bytes(),
            mode,
        )?;
    }
    Ok(
        json!({"kind":"file_change","path":target.label,"revision":revision(result.as_bytes()),"bytes":result.len(),"changed":bytes!=result.as_bytes(),"preview":diff,"diagnostics":diagnostics}),
    )
}
pub fn command(args: &[String]) -> Result<(), String> {
    let json_output =
        args.iter().any(|s| s == "--json") || std::env::var_os("GC_FILE_JSON").is_some();
    let filtered = args
        .iter()
        .filter(|s| s.as_str() != "--json")
        .cloned()
        .collect::<Vec<_>>();
    let args = &filtered;
    if args.is_empty() || args[0] == "--help" {
        crate::cli::text("Files",
            "Usage:\n  gc file [--json] read PATH [OFFSET LIMIT] | check PATH | edit|write < JSON\n\nEdit request:\n {\"path\":\"a.rs\",\"expected\":\"SHA256 from read\",\"edits\":[{\"start\":2,\"end\":3,\"text\":\"replacement\"}]}\nExact edit: {\"old\":\"unique text\",\"new\":\"replacement\"}; insertion: end=start-1.\n\nWrite request:\n {\"path\":\"a.rs\",\"expected\":null,\"text\":\"new file\"} (null=create-only). Optional validate:true requires a passing supported syntax check.\n\nSafety:\nUTF-8 regular files only, 8 MiB maximum; workspace-relative paths; no symlinks. JSON results are bounded; revisions cover the entire file."
        );
        return Ok(());
    }
    let action = &args[0];
    if !matches!(action.as_str(), "read" | "check" | "edit" | "write") {
        return Err("action must be read, edit, write, or check; see file --help".into());
    }
    let v = if matches!(action.as_str(), "read" | "check") {
        let path = args.get(1).ok_or("path required")?;
        let offset = args
            .get(2)
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(error)?
            .unwrap_or(1);
        let limit = args
            .get(3)
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(error)?
            .unwrap_or(200);
        json!({"path":path,"offset":offset,"limit":limit})
    } else {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take((MAX_FILE + 65537) as u64)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if bytes.len() > MAX_FILE + 65536 {
            return Err("JSON request too large".into());
        }
        serde_json::from_slice(&bytes).map_err(error)?
    };
    let out = execute(&std::env::current_dir().map_err(error)?, action, &v)?;
    if json_output {
        crate::emit_json(out.clone());
    } else {
        let display = match out["kind"].as_str() {
            Some("file_read") => format!(
                "[file {} · revision {} · lines {}/{}]\n{}{}",
                out["path"].as_str().unwrap(),
                out["revision"].as_str().unwrap(),
                out["shown"],
                out["total_lines"],
                out["text"].as_str().unwrap(),
                if out["truncated"] == true {
                    format!(
                        "\n[partial read; next_offset={}, partial_line={}]\n",
                        out["next_offset"], out["partial_line"]
                    )
                } else {
                    String::new()
                }
            ),
            Some("file_change") => format!(
                "[file {} · revision {}]\n{}{}",
                out["path"].as_str().unwrap(),
                out["revision"].as_str().unwrap(),
                crate::diagnostics::summary(&out["diagnostics"]),
                out["preview"].as_str().unwrap()
            ),
            _ => crate::diagnostics::summary(&out),
        };
        crate::cli::literal("Files", &glass_tui::markdown::sanitize(&display));
    }
    if action == "check" && out["status"] != "ok" {
        return Err(format!(
            "syntax check {}",
            out["status"].as_str().unwrap_or("failed")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Dir(std::path::PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn dir() -> Dir {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "gc-files-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Dir(p)
    }
    fn write(d: &Dir, name: &str, text: &str) {
        std::fs::write(d.0.join(name), text).unwrap();
    }
    fn read(d: &Dir, name: &str) -> Value {
        execute(&d.0, "read", &json!({"path":name})).unwrap()
    }
    #[test]
    fn batch_ranges_and_exact_edits() {
        let d = dir();
        write(&d, "a", "one\ntwo\nthree\nfour");
        let r = read(&d, "a");
        let out=execute(&d.0,"edit",&json!({"path":"a","expected":r["revision"],"edits":[{"start":2,"end":2,"text":"TWO"},{"old":"four","new":"FOUR"}]})).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.0.join("a")).unwrap(),
            "one\nTWO\nthree\nFOUR"
        );
        assert!(out["preview"].as_str().unwrap().contains("+ TWO"));
    }
    #[test]
    fn stale_and_overlaps_leave_original() {
        let d = dir();
        write(&d, "a", "one\ntwo\n");
        let r = read(&d, "a");
        for edits in [
            json!([{"start":1,"end":2,"text":"x"},{"start":2,"end":2,"text":"y"}]),
            json!([{"old":"absent","new":"x"}]),
            json!([{"start":0,"end":1,"text":"x"}]),
        ] {
            assert!(
                execute(
                    &d.0,
                    "edit",
                    &json!({"path":"a","expected":r["revision"],"edits":edits})
                )
                .is_err()
            );
            assert_eq!(
                std::fs::read_to_string(d.0.join("a")).unwrap(),
                "one\ntwo\n"
            );
        }
        write(&d, "a", "external");
        assert!(
            execute(
                &d.0,
                "write",
                &json!({"path":"a","expected":r["revision"],"text":"lost"})
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(d.0.join("a")).unwrap(), "external");
    }
    #[test]
    fn create_only_modes_and_validation() {
        let d = dir();
        execute(
            &d.0,
            "write",
            &json!({"path":"a.json","expected":null,"text":"{}","validate":true}),
        )
        .unwrap();
        assert_eq!(
            std::fs::metadata(d.0.join("a.json")).unwrap().mode() & 0o777,
            0o600
        );
        assert!(
            execute(
                &d.0,
                "write",
                &json!({"path":"a.json","expected":null,"text":"oops"})
            )
            .is_err()
        );
        let r = read(&d, "a.json");
        assert!(
            execute(
                &d.0,
                "write",
                &json!({"path":"a.json","expected":r["revision"],"text":"broken","validate":true})
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(d.0.join("a.json")).unwrap(), "{}");
        assert!(std::fs::read_dir(&d.0).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".gc-write")
        }));
    }
    #[test]
    fn crlf_bom_and_eof() {
        let d = dir();
        write(&d, "a", "\u{feff}one\r\ntwo");
        std::fs::set_permissions(d.0.join("a"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let r = read(&d, "a");
        execute(&d.0,"edit",&json!({"path":"a","expected":r["revision"],"edits":[{"start":1,"end":1,"text":"ONE"},{"start":2,"end":2,"text":"TWO"}]})).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.0.join("a")).unwrap(),
            "\u{feff}ONE\r\nTWO"
        );
        assert_eq!(
            std::fs::metadata(d.0.join("a")).unwrap().mode() & 0o777,
            0o755
        );
        let r = read(&d, "a");
        execute(&d.0,"edit",&json!({"path":"a","expected":r["revision"],"edits":[{"start":1,"end":0,"text":"prefix"}]})).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.0.join("a")).unwrap(),
            "\u{feff}prefix\r\nONE\r\nTWO"
        );
    }
    #[test]
    fn symlinks_hardlinks_and_binary_rejected() {
        use std::os::unix::fs::symlink;
        let d = dir();
        write(&d, "a", "safe");
        symlink("a", d.0.join("link")).unwrap();
        symlink(".", d.0.join("dirlink")).unwrap();
        for p in ["link", "dirlink/a", "../a"] {
            assert!(execute(&d.0, "read", &json!({"path":p})).is_err());
        }
        std::fs::hard_link(d.0.join("a"), d.0.join("hard")).unwrap();
        assert!(
            execute(
                &d.0,
                "write",
                &json!({"path":"hard","expected":revision(b"safe"),"text":"x"})
            )
            .is_err()
        );
        write(&d, "binary", "a\0b");
        assert!(execute(&d.0, "read", &json!({"path":"binary"})).is_err());
        assert!(
            execute(
                &d.0,
                "write",
                &json!({"path":"new","expected":null,"text":"a\0b"})
            )
            .is_err()
        );
    }
    #[test]
    fn eof_insertions_and_overlapping_text_matches() {
        let d = dir();
        for (original, expected) in [("one", "one\ntwo"), ("one\n", "one\ntwo\n"), ("", "two")] {
            write(&d, "a", original);
            let r = read(&d, "a");
            let start = original.lines().count() + 1;
            execute(&d.0,"edit",&json!({"path":"a","expected":r["revision"],"edits":[{"start":start,"end":start-1,"text":"two"}]})).unwrap();
            assert_eq!(std::fs::read_to_string(d.0.join("a")).unwrap(), expected);
        }
        write(&d, "a", "aaa");
        let r = read(&d, "a");
        assert!(
            execute(
                &d.0,
                "edit",
                &json!({"path":"a","expected":r["revision"],"edits":[{"old":"aa","new":"b"}]})
            )
            .is_err()
        );
    }
    #[test]
    fn bounded_read_and_paging() {
        let d = dir();
        write(&d, "a", &"界\n".repeat(1500));
        let v = execute(&d.0, "read", &json!({"path":"a","offset":2,"limit":3})).unwrap();
        assert_eq!(v["shown"], 3);
        assert_eq!(v["next_offset"], 5);
        write(&d, "long", &"界".repeat(20_000));
        let v = read(&d, "long");
        assert!(v["text"].as_str().unwrap().len() <= MAX_PREVIEW);
        assert_eq!(v["partial_line"], true);
        assert_eq!(v["next_offset"], Value::Null);
    }
    #[test]
    fn special_files_oversize_and_many_lines_are_bounded() {
        let d = dir();
        let fifo = c(d.0.join("fifo").as_os_str()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let start = Instant::now();
        assert!(execute(&d.0, "read", &json!({"path":"fifo"})).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        let big = File::create(d.0.join("big")).unwrap();
        big.set_len((MAX_FILE + 1) as u64).unwrap();
        assert!(execute(&d.0, "read", &json!({"path":"big"})).is_err());
        write(&d, "lines", &format!("unique{}", "\n".repeat(1_000_002)));
        let r = read(&d, "lines");
        execute(&d.0,"edit",&json!({"path":"lines","expected":r["revision"],"edits":[{"old":"unique","new":"replacement"}]})).unwrap();
    }
    #[test]
    fn cooperative_racing_writers_only_one_commits() {
        let d = dir();
        write(&d, "a", "old");
        let rev = read(&d, "a")["revision"].clone();
        let root = d.0.clone();
        let rev2 = rev.clone();
        let handle = std::thread::spawn(move || {
            execute(
                &root,
                "write",
                &json!({"path":"a","expected":rev2,"text":"first"}),
            )
        });
        let other = execute(
            &d.0,
            "write",
            &json!({"path":"a","expected":rev,"text":"second"}),
        );
        assert_ne!(handle.join().unwrap().is_ok(), other.is_ok());
    }
}
