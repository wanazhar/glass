//! Bounded native file operations shared by model tools and CLI helpers.
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
thread_local! { static APPROVED_CALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
fn approved() -> bool { APPROVED_CALL.with(|value|value.get()) }
pub fn tool_cancellable_approved(call:&glass_mod_api::ToolCall, root:&Path, cancel:&std::sync::atomic::AtomicBool, granted:bool) -> crate::tools::Outcome {
    struct Restore(bool);
    impl Drop for Restore { fn drop(&mut self) {APPROVED_CALL.with(|value|value.set(self.0));} }
    let _restore=Restore(APPROVED_CALL.with(|value|value.replace(granted)));
    tool_cancellable(call,root,cancel)
}

struct Target {
    dir: File,
    name: CString,
    label: String,
}
impl Target {
    fn new(root: &Path, path: &str, mutating: bool) -> Result<Self, String> {
        Self::with_parents(root, path, mutating, false)
    }
    fn with_parents(root: &Path, path: &str, mutating: bool, create: bool) -> Result<Self, String> {
        let workspace = root.canonicalize().map_err(error)?;
        let requested = Path::new(path);
        let unrestricted = approved() || (!mutating && crate::sandbox::policy()!=crate::sandbox::Policy::Strict)
            || (crate::sandbox::permission_profile_label() == "unrestricted"
            && crate::sandbox::policy() == crate::sandbox::Policy::Off);
        let absolute = if requested.is_absolute() { requested.to_path_buf() } else { workspace.join(requested) };
        // Normalize lexically, then retain descriptor-relative NOFOLLOW traversal.
        let mut normalized = std::path::PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::ParentDir => { normalized.pop(); },
                Component::CurDir => {},
                other => normalized.push(other.as_os_str()),
            }
        }
        let root = if unrestricted { Path::new("/").to_path_buf() } else { workspace.clone() };
        let path = normalized.strip_prefix(&root)
            .map_err(|_| "permission profile confines native files to the workspace; choose an allowed path or ask the user to change permissions")?;
        let parts = path
            .components()
            .filter(|p| !matches!(p, Component::CurDir))
            .collect::<Vec<_>>();
        if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
            return Err("choose a file within the workspace; '..' is not accepted".into());
        }
        let mut dir = File::open(&root).map_err(error)?;
        for part in &parts[..parts.len() - 1] {
            let name = c(part.as_os_str())?;
            if create {
                // Descriptor-relative mkdir and NOFOLLOW reopen retain the same
                // confinement against concurrent symlink substitution.
                let rc = unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o755) };
                if rc < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
                    return Err(error(std::io::Error::last_os_error()));
                }
            }
            dir = open_at(&dir, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        }
        Ok(Self {
            dir,
            name: c(parts.last().unwrap().as_os_str())?,
            label: normalized.strip_prefix(&workspace).unwrap_or(&normalized).display().to_string(),
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
        let mut bytes = Vec::with_capacity(meta.len() as usize + 1);
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
pub(crate) fn revision(bytes: &[u8]) -> String {
    crate::hashing::sha256(bytes)
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
    execute_cancellable(root, action, v, &std::sync::atomic::AtomicBool::new(false))
}
fn execute_cancellable(
    root: &Path,
    action: &str,
    v: &Value,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value, String> {
    let cancelled = || cancel.load(std::sync::atomic::Ordering::Acquire);
    if cancelled() {
        return Err("File operation cancelled before execution".into());
    }
    if !matches!(action, "read" | "check" | "edit" | "write" | "view_image" | "display_image") {
        return Err("action must be read, edit, write, or check".into());
    }
    if !approved() && let Some(reason) = crate::sandbox::refusal() { return Err(reason.to_string()); }
    if matches!(action, "edit" | "write") && v["preview"] != true
        && crate::sandbox::permission_profile_label() == "read-only" && !approved() {
        return Err("Read-only permission profile blocks file mutations; preview:true remains available".into());
    }
    let create_parents = action == "write" && v["preview"] != true && v.get("expected").is_some_and(Value::is_null);
    if create_parents {
        let content = required(v, "text")?;
        if content.len() > MAX_FILE { return Err("result exceeds 8 MiB".into()); }
        text(content.as_bytes())?;
        if v.get("preview").is_some_and(|value| !value.is_boolean())
            || v.get("validate").is_some_and(|value| !value.is_boolean()) {
            return Err("preview and validate must be booleans".into());
        }
    }
    let target = Target::with_parents(root, required(v, "path")?, matches!(action,"edit"|"write") && v["preview"]!=true, create_parents)?;
    let mutation = matches!(action, "edit" | "write");
    if mutation {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if cancelled() {
                return Err("File operation cancelled while waiting for writer lock".into());
            }
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
    if action == "view_image" || action == "display_image" {
        if action == "display_image" {
            if existing.is_none() { return Err("image file does not exist".into()); }
            return crate::images::share(bytes, &target.label);
        }
        if existing.is_none() { return Err("image file does not exist".into()); }
        return crate::images::prepare(bytes, &target.label, cancelled);
    }
    let source = text(bytes)?;
    if action == "read" {
        if existing.is_none() {
            return Err("file does not exist".into());
        }
        let mut offset = number(v, "offset", 1)?;
        let limit = number(v, "limit", 200)?.clamp(1, 1000);
        if offset == 0 {
            return Err("offset is 1-indexed".into());
        }
        let rev = revision(bytes);
        if let Some(expected) = v.get("expected") {
            let expected = expected
                .as_str()
                .ok_or("expected must be a revision string for reads")?;
            if expected != rev {
                return Err("stale read continuation; file changed, restart the read to obtain a new revision".into());
            }
        }
        let start = if v.get("byte_offset").is_some() {
            if v.get("expected").is_none() {
                return Err("byte_offset requires expected from the previous read revision; restart with a line read if unavailable".into());
            }
            if v.get("offset").is_some() && offset != 1 {
                return Err("use offset or byte_offset, not both".into());
            }
            let start = number(v, "byte_offset", 0)?;
            if start > source.len() || !source.is_char_boundary(start) {
                return Err(
                    "byte_offset must be within the file at a UTF-8 character boundary".into(),
                );
            }
            offset = source[..start].bytes().filter(|b| *b == b'\n').count() + 1;
            start
        } else {
            source
                .split_inclusive('\n')
                .take(offset - 1)
                .map(str::len)
                .sum()
        };
        let total = source.lines().count();
        let mut body = String::new();
        let mut shown = 0;
        let mut partial = false;
        let mut end = start;
        for (i, segment) in source[start..]
            .split_inclusive('\n')
            .take(limit)
            .enumerate()
        {
            let line = segment.strip_suffix('\n').unwrap_or(segment);
            let line = if segment.ends_with('\n') {
                line.strip_suffix('\r').unwrap_or(line)
            } else {
                line
            };
            let prefix = format!("{}|", offset + i);
            let room = MAX_PREVIEW.saturating_sub(body.len());
            if prefix.len() + line.len() + 1 > room {
                if room > prefix.len() {
                    let fragment = clip(line, room - prefix.len());
                    body.push_str(&prefix);
                    body.push_str(fragment);
                    end += fragment.len();
                }
                partial = true;
                break;
            }
            body.push_str(&prefix);
            body.push_str(line);
            body.push('\n');
            end += segment.len();
            shown += 1;
        }
        let truncated = end < bytes.len();
        let next = truncated
            .then(|| json!({"path":target.label,"byte_offset":end,"expected":rev,"limit":limit}));
        return Ok(
            json!({"kind":"file_read","path":target.label,"revision":rev,"offset":offset,"shown":shown,"total_lines":total,"bytes":bytes.len(),"byte_offset":start,"end_byte_offset":end,"truncated":truncated,"next_offset":if !partial && truncated {Some(offset+shown)} else {None},"next_read":next,"partial_line":partial,"text":body}),
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
    let base_revision = existing.as_ref().map(|_| revision(bytes));
    if existing.is_some() {
        if expected.as_str() != base_revision.as_deref() {
            return Err("stale revision; nothing written, re-read the file".into());
        }
    } else if !expected.is_null() {
        return Err("file does not exist; use expected:null for create-only".into());
    }
    let dry_run = match v.get("preview") {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => return Err("preview must be a boolean".into()),
    };
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
        for (index, op) in ops.iter().enumerate() {
            let fields = op
                .as_object()
                .ok_or_else(|| format!("edit {} must be an object; nothing written", index + 1))?;
            let exact = fields.contains_key("old");
            let allowed = if exact {
                &["old", "new"][..]
            } else {
                &["start", "end", "text"][..]
            };
            if let Some(unknown) = fields.keys().find(|key| !allowed.contains(&key.as_str())) {
                return Err(format!(
                    "edit {}: unexpected field {unknown}; use old/new or start/end/text, not both; nothing written",
                    index + 1
                ));
            }
            if exact {
                let old = required(op, "old")?;
                if old.is_empty() {
                    return Err("old must not be empty; use a line insertion".into());
                }
                // Reads display CRLF as LF. Match that representation without
                // normalizing unrelated bytes or weakening uniqueness checks.
                let normalized;
                let needle;
                let (haystack, old_match) = if source.contains(old) {
                    (source, old.to_string())
                } else {
                    normalized = source.replace("\r\n", "\n");
                    needle = old.replace("\r\n", "\n");
                    (normalized.as_str(), needle)
                };
                let Some(start) = haystack.find(&old_match) else {
                    return Err(format!(
                        "edit {}: old text was not found; nothing written. Re-read the target range with the current revision",
                        index + 1
                    ));
                };
                if haystack[start + haystack[start..].chars().next().unwrap().len_utf8()..].contains(&old_match) {
                    return Err(format!(
                        "edit {}: old text matches more than once; nothing written. Use original-file line ranges for repeated text",
                        index + 1
                    ));
                }
                let new = required(op, "new")?;
                if std::ptr::eq(haystack.as_ptr(), source.as_ptr()) {
                    spans.push((start, start + old.len(), new.to_string()));
                } else {
                    let original_offset = |offset: usize| {
                        let mut original = 0;
                        let mut remaining = offset;
                        while remaining > 0 {
                            if source.as_bytes()[original..].starts_with(b"\r\n") { original += 1; }
                            original += 1;
                            remaining -= 1;
                        }
                        original
                    };
                    spans.push((original_offset(start), original_offset(start + old_match.len()),
                        new.replace("\r\n", "\n").replace('\n', ending)));
                }
            } else {
                let start = number(op, "start", 0)?;
                let end = number(op, "end", usize::MAX)?;
                if start == 0 || start > line_count + 1 || end < start - 1 || end > line_count {
                    return Err(
                        "invalid original-file line range; insertion uses end=start-1".into(),
                    );
                }
                let from = offsets[start - 1];
                let mut to = offsets[end];
                // Choose the target's local line style, not another line's style.
                let ending = source[from..]
                    .find('\n')
                    .map(|at| {
                        if at > 0 && source.as_bytes()[from + at - 1] == b'\r' {
                            "\r\n"
                        } else {
                            "\n"
                        }
                    })
                    .unwrap_or(ending);
                let terminal_ending = if from < to && source[from..to].ends_with("\r\n") {
                    "\r\n"
                } else if from < to && source[from..to].ends_with('\n') {
                    "\n"
                } else {
                    ending
                };
                let mut replacement = required(op, "text")?
                    .replace("\r\n", "\n")
                    .replace('\n', ending);
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
                    replacement.push_str(terminal_ending)
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
    let created = existing.is_none();
    let changed = bytes != result.as_bytes() || created;
    if cancelled() {
        return Err("File operation cancelled before atomic commit; nothing written".into());
    }
    if changed && !dry_run {
        target.commit(
            existing.as_ref().map(|(b, _)| b.as_slice()),
            result.as_bytes(),
            mode,
        )?;
    }
    Ok(
        json!({"kind":"file_change","path":target.label,"operation":action,"status":if dry_run {"proposed"} else if changed {"applied"} else {"unchanged"},"applied":!dry_run && changed,"created":created,"base_revision":base_revision,"revision":if changed { revision(result.as_bytes()) } else { base_revision.clone().unwrap_or_else(||revision(result.as_bytes())) },"bytes":result.len(),"changed":changed,"preview":diff,"diagnostics":diagnostics}),
    )
}
pub(crate) fn snapshot_text(root: &Path, path: &str) -> Result<Option<String>, String> {
    let target = Target::new(root, path, false)?;
    target
        .read()?
        .map(|(bytes, _)| text(&bytes).map(str::to_owned))
        .transpose()
}
pub(crate) fn remove_created(root: &Path, path: &str, expected: &str) -> Result<(), String> {
    if crate::sandbox::permission_profile_label()=="read-only" && !approved() {return Err("Read-only permission profile blocks recovery mutations".into());}
    let target = Target::new(root, path, true)?;
    fs2::FileExt::try_lock_exclusive(&target.dir).map_err(error)?;
    let Some((bytes, _)) = target.read()? else {
        return Ok(());
    };
    if revision(&bytes) != expected {
        return Err("Recovery refuses a changed created file".into());
    }
    if unsafe { libc::unlinkat(target.dir.as_raw_fd(), target.name.as_ptr(), 0) } < 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    target.dir.sync_all().map_err(error)
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
        crate::cli::text(
            "Files",
            "Usage:\n  gc file [--json] image|display PATH | read PATH [OFFSET LIMIT] [--byte-offset N --expected SHA256] | check PATH | edit|write < JSON\n\nEdit request:\n {\"path\":\"a.rs\",\"expected\":\"SHA256 from read\",\"edits\":[{\"start\":2,\"end\":3,\"text\":\"replacement\"}]}\nExact edit: {\"old\":\"unique text\",\"new\":\"replacement\"}; insertion: end=start-1.\n\nWrite request:\n {\"path\":\"a.rs\",\"expected\":null,\"text\":\"new file\"} (null=create-only). Optional validate:true requires a passing supported syntax check. preview:true validates and returns a proposed result without writing.\n\nSafety:\nText operations: UTF-8 regular files only, 8 MiB maximum; image prepares PNG/JPEG/GIF/WebP with bounded dimensions and a private JPEG artifact; Paths follow the active permission profile: unrestricted permits absolute outside-workspace paths; restrictive profiles confine native files. No symlinks. JSON results are bounded; revisions cover the entire file. Truncated reads include next_read with a byte cursor and expected revision. Byte continuation requires --expected; restart a line read when stale. Atomic replacement preserves mode bits, not ownership or extended attributes; locks coordinate GC writers, not arbitrary external writers.",
        );
        return Ok(());
    }
    let action = if args[0] == "image" { "view_image".to_string() } else if args[0] == "display" { "display_image".to_string() } else { args[0].clone() };
    if !matches!(action.as_str(), "read" | "check" | "edit" | "write" | "view_image" | "display_image") {
        return Err("action must be read, edit, write, or check; see file --help".into());
    }
    let v = if matches!(action.as_str(), "read" | "check" | "view_image" | "display_image") {
        let path = args.get(1).ok_or("path required")?;
        let mut request = json!({"path":path});
        let mut positional = 0;
        let mut cursor = 2;
        while cursor < args.len() {
            let arg = &args[cursor];
            if matches!(arg.as_str(), "--byte-offset" | "--expected") && action == "read" {
                let value = args
                    .get(cursor + 1)
                    .ok_or_else(|| format!("{arg} requires a value"))?;
                let key = if arg == "--byte-offset" {
                    "byte_offset"
                } else {
                    "expected"
                };
                if request.get(key).is_some() {
                    return Err(format!("{arg} specified more than once"));
                }
                request[key] = if key == "byte_offset" {
                    json!(value.parse::<usize>().map_err(error)?)
                } else {
                    json!(value)
                };
                cursor += 2;
            } else if action == "read" && positional < 2 && !arg.starts_with('-') {
                let key = if positional == 0 { "offset" } else { "limit" };
                request[key] = json!(arg.parse::<usize>().map_err(error)?);
                positional += 1;
                cursor += 1;
            } else {
                return Err(format!("unexpected file argument: {arg}; see file --help"));
            }
        }
        request
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
    let mut cfg = crate::config::load(&crate::config::default_path()).map_err(error)?;
    crate::profiles::apply_active(&mut cfg)?;
    let requirements = crate::requirements::load(&crate::config::config_dir())?;
    crate::requirements::apply(&requirements, &mut cfg);
    for warning in crate::sandbox::init(&cfg) { eprintln!("glass-core: {warning}"); }
    let out = execute(&std::env::current_dir().map_err(error)?, &action, &v)?;
    if json_output {
        crate::emit_json(out.clone());
    } else {
        let display = display(&out);
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

pub(crate) fn read_workspace_bytes(root: &Path, path: &str) -> Result<Vec<u8>, String> {
    Target::new(root, path, false)?.read()?.map(|(bytes, _)| bytes).ok_or_else(|| "file does not exist".into())
}

pub fn is_tool(name: &str) -> bool {
    matches!(name, "read" | "edit" | "write" | "view_image" | "display_image")
}

pub fn schemas() -> Vec<(String, String, String)> {
    let read = json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{
        "path":{"type":"string"},"offset":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":1000},
        "byte_offset":{"type":"integer","minimum":0},"expected":{"type":"string"}}});
    let mut mutation = json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{
        "path":{"type":"string"},"expected":{"type":["string","null"]},"preview":{"type":"boolean"},"validate":{"type":"boolean"}}});
    mutation["properties"]["permission"]=json!({"type":"string","enum":["use_default","require_escalated"]});
    mutation["properties"]["justification"]=json!({"type":"string","maxLength":2048});
    let mut write = mutation.clone();
    write["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("text"));
    write["properties"]["text"] = json!({"type":"string"});
    mutation["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("edits"));
    mutation["properties"]["edits"] = json!({"type":"array","minItems":1,"maxItems":128,"items":{"anyOf":[
        {"type":"object","additionalProperties":false,"required":["old","new"],"properties":{"old":{"type":"string","minLength":1},"new":{"type":"string"}}},
        {"type":"object","additionalProperties":false,"required":["start","end","text"],"properties":{"start":{"type":"integer","minimum":1},"end":{"type":"integer","minimum":0},"text":{"type":"string"}}}
    ]}});
    vec![
        ("display_image".into(), "Show a local PNG/JPEG/GIF/WebP to the user in web chat without sending pixels to the model. Returns dimensions and Markdown only; does not inspect visual contents. For a supplied filename, try this tool directly relative to the workspace before searching. Only search after not-found; scope searches to likely folders and exclude generated/build/benchmark trees. Prefer this for sharing, including text-only models. No symlinks; max 8 MiB, 32 million pixels.".into(), json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{"path":{"type":"string"}}}).to_string()),
        ("view_image".into(), "Inspect a PNG, JPEG, GIF or WebP as an image, including browser screenshots. Returns source, dimensions and a reusable image reference; image pixels reach vision-capable models. No symlinks; input max 8 MiB, 32 million pixels; bounded resize and metadata removal.".into(), json!({"type":"object","additionalProperties":false,"required":["path"],"properties":{"path":{"type":"string"}}}).to_string()),
        ("read".into(),"Read a UTF-8 file allowed by the active permission profile with line numbers and whole-file revision. Defaults: offset=1, limit=200. Follow next_read for truncated output; byte_offset requires expected. No symlinks; max 8 MiB.".into(),read.to_string()),
        ("edit".into(),"Edit a previously read file. Omit expected to use the session’s last native read/write revision; explicit expected overrides it. edits use unique old/new text or original 1-based start/end/text ranges (insert: end=start-1). Atomic; preview returns a diff without writing; validate requires supported syntax checks.".into(),mutation.to_string()),
        ("write".into(),"Atomically write a UTF-8 file allowed by the active permission profile. Omit expected to use the session’s last native read/write revision (otherwise create-only); expected:null forces create-only. preview returns a diff without writing; validate requires supported syntax checks. Creates missing parent directories for create-only writes. No symlinks; max 8 MiB.".into(),write.to_string())
    ]
}

pub fn tool(call: &glass_mod_api::ToolCall, root: &Path) -> crate::tools::Outcome {
    tool_cancellable(call, root, &std::sync::atomic::AtomicBool::new(false))
}
pub fn tool_cancellable(
    call: &glass_mod_api::ToolCall,
    root: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> crate::tools::Outcome {
    if !approved() && let Some(reason) = crate::sandbox::refusal() {
        return crate::tools::Outcome::err(reason);
    }
    let mut value: Value = match serde_json::from_str(&call.arguments) {
        Ok(v) => v,
        Err(e) => return crate::tools::Outcome::err(e.to_string()),
    };
    if matches!(call.name.as_str(), "edit" | "write")
        && value["preview"] != true
        && crate::sandbox::permission_profile_label() == "read-only" && !approved()
    {
        return crate::tools::Outcome::err(
            "Read-only permission profile blocks file mutations; preview:true remains available",
        );
    }
    if let Some(object)=value.as_object_mut() {object.remove("permission");object.remove("justification");}
    match execute_cancellable(root, &call.name, &value, cancel) {
        Ok(value) => {
            let mut body = display_inner(&value, false);
            if let Some(next) = value.get("next_read").filter(|v| !v.is_null()) {
                body.push_str(&format!("\nnext_read: {next}"));
            }
            crate::tools::Outcome::ok(body)
        }
        Err(error) => crate::tools::Outcome::err(error),
    }
}

/// Shared readable rendering for CLI and native model results.
pub fn display(out: &Value) -> String {
    display_inner(out, true)
}
fn display_inner(out: &Value, cli_continuation: bool) -> String {
    match out["kind"].as_str() {
        Some("image_view") => crate::images::display(out),
        Some("image_display") => crate::images::display_shared(out),
        Some("file_read") => format!(
            "[file {} · revision {} · lines {}/{}]\n{}{}",
            out["path"].as_str().unwrap(),
            out["revision"].as_str().unwrap(),
            out["shown"],
            out["total_lines"],
            out["text"].as_str().unwrap(),
            if cli_continuation && out["truncated"] == true {
                format!(
                    "\n[partial read; continue with file read PATH --byte-offset {} --expected {}]\n",
                    out["end_byte_offset"],
                    out["revision"].as_str().unwrap()
                )
            } else {
                String::new()
            }
        ),
        Some("file_change") => format!(
            "[file {} · {} · revision {}]\n{}{}",
            out["path"].as_str().unwrap(),
            out["status"].as_str().unwrap(),
            out["revision"].as_str().unwrap(),
            crate::diagnostics::summary(&out["diagnostics"]),
            out["preview"].as_str().unwrap()
        ),
        _ => crate::diagnostics::summary(out),
    }
}

#[cfg(test)]
mod direct_tool_tests {
    use super::*;
    #[test]
    fn schemas_validate_native_shapes() {
        for (name, _, schema) in schemas() {
            let schema: Value = serde_json::from_str(&schema).unwrap();
            let args = match name.as_str() {
                "read" => json!({"path":"a","limit":20}),
                "view_image" | "display_image" => json!({"path":"a"}),
                "write" => json!({"path":"a","expected":null,"text":"x"}),
                _ => json!({"path":"a","expected":"revision","edits":[{"old":"x","new":"y"}]}),
            };
            let call = glass_mod_api::ToolCall {
                id: "test".into(),
                name: name.clone(),
                arguments: args.to_string(),
            };
            let tools = vec![(name.clone(), schema.clone())];
            assert!(crate::tool_repair::validate(&call, &tools).is_ok());
            let invalid = glass_mod_api::ToolCall {
                id: "bad".into(),
                name,
                arguments: json!({"path":"a","typo":true}).to_string(),
            };
            assert!(crate::tool_repair::validate(&invalid, &tools).is_err());
        }
    }
    #[test]
    fn cancellation_prevents_a_new_file() {
        let root = std::env::temp_dir().join(format!("gc-native-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let cancelled = std::sync::atomic::AtomicBool::new(true);
        assert!(
            execute_cancellable(
                &root,
                "write",
                &json!({"path":"never","expected":null,"text":"x"}),
                &cancelled
            )
            .is_err()
        );
        assert!(!root.join("never").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
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
    fn normalized_exact_edits_still_reject_ambiguity_and_overlap() {
        let d = dir();
        std::fs::write(d.0.join("a"), "one\r\ntwo\r\none\r\ntwo\r\n").unwrap();
        let rev = read(&d, "a")["revision"].clone();
        assert!(execute(&d.0, "edit", &json!({"path":"a","expected":rev,"edits":[{"old":"one\ntwo","new":"lost"}]})).unwrap_err().contains("more than once"));
        std::fs::write(d.0.join("a"), "one\r\ntwo\r\nthree\r\n").unwrap();
        let rev = read(&d, "a")["revision"].clone();
        assert!(execute(&d.0, "edit", &json!({"path":"a","expected":rev,"edits":[{"old":"one\ntwo","new":"first"},{"old":"two\nthree","new":"last"}]})).unwrap_err().contains("overlapping"));
        assert_eq!(std::fs::read(d.0.join("a")).unwrap(), b"one\r\ntwo\r\nthree\r\n");
    }

    #[test]
    fn nested_create_and_crlf_exact_edit_preserve_guards() {
        let d = dir();
        let created = execute(&d.0, "write", &json!({"path":"nested/deep/a.txt","expected":null,"text":"é\r\none\r\ntwo\r\nend\r\n"})).unwrap();
        execute(&d.0, "edit", &json!({"path":"nested/deep/a.txt","expected":created["revision"],"edits":[{"old":"one\ntwo","new":"ONE\nTWO"}]})).unwrap();
        assert_eq!(std::fs::read(d.0.join("nested/deep/a.txt")).unwrap(), "é\r\nONE\r\nTWO\r\nend\r\n".as_bytes());
        assert!(execute(&d.0, "write", &json!({"path":"nested/deep/a.txt","expected":null,"text":"lost"})).is_err());
        assert!(execute(&d.0, "write", &json!({"path":"preview/new.txt","expected":null,"text":"test","preview":true})).is_err());
        assert!(!d.0.join("preview").exists());
        std::os::unix::fs::symlink(d.0.join("nested"), d.0.join("link")).unwrap();
        assert!(execute(&d.0,"write",&json!({"path":"link/escape/x","expected":null,"text":"lost"})).is_err());
        assert!(!d.0.join("nested/escape").exists());
    }

    #[test]
    fn native_partial_reads_keep_one_tool_continuation_and_cli_keeps_its_hint() {
        let d = dir();
        write(&d, "a", &"one line\n".repeat(300));
        let value = read(&d, "a");
        assert!(display(&value).contains("continue with file read"));
        let call = glass_mod_api::ToolCall { id:"read".into(), name:"read".into(), arguments:r#"{"path":"a"}"#.into() };
        let output = tool(&call, &d.0);
        assert!(!output.body.contains("continue with file read"));
        assert_eq!(output.body.matches("next_read:").count(), 1);
        let next:Value = serde_json::from_str(output.body.split("next_read: ").nth(1).unwrap()).unwrap();
        assert!(execute(&d.0, "read", &next).is_ok());
        assert_eq!(output.body.matches(value["revision"].as_str().unwrap()).count(), 2);
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
            json!([{"old":42,"start":1,"end":1,"text":"x"}]),
            json!([{"old":"one","new":"ONE","start":1}]),
            json!([{"start":1,"end":1,"text":"x","new":"ignored"}]),
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
    fn mixed_line_endings_preserve_target_terminators() {
        let d = dir();
        write(&d, "a", "one\r\ntwo\nthree\r\nfour\n");
        let r = read(&d, "a");
        execute(&d.0,"edit",&json!({"path":"a","expected":r["revision"],"edits":[{"start":2,"end":2,"text":"TWO"}]})).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.0.join("a")).unwrap(),
            "one\r\nTWO\nthree\r\nfour\n"
        );
        let r = read(&d, "a");
        execute(&d.0,"edit",&json!({"path":"a","expected":r["revision"],"edits":[{"start":3,"end":4,"text":"THREE\nFOUR"}]})).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.0.join("a")).unwrap(),
            "one\r\nTWO\nTHREE\r\nFOUR\n"
        );
    }
    #[test]
    fn unrestricted_reads_and_revision_guarded_writes_outside_workspace() {
        let workspace = dir();
        let external = dir();
        write(&external, "outside", "original");
        let path = external.0.join("outside").display().to_string();
        let read = execute(&workspace.0, "read", &json!({"path":path})).unwrap();
        assert_eq!(read["revision"], revision(b"original"));
        execute(&workspace.0, "write", &json!({"path":path,"expected":revision(b"original"),"text":"changed"})).unwrap();
        assert_eq!(std::fs::read_to_string(external.0.join("outside")).unwrap(), "changed");
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
        assert!(v["next_read"]["byte_offset"].as_u64().unwrap() > 0);
    }
    #[test]
    fn long_unicode_read_continues_without_losing_content() {
        let d = dir();
        let original = format!("first\r\n{}\r\nlast", "界".repeat(20_000));
        write(&d, "a", &original);
        let mut request = json!({"path":"a"});
        let mut cursor = 0;
        let mut pages = 0;
        loop {
            let page = execute(&d.0, "read", &request).unwrap();
            assert_eq!(page["byte_offset"], cursor);
            let end = page["end_byte_offset"].as_u64().unwrap() as usize;
            assert!(end > cursor);
            assert!(original.is_char_boundary(end));
            assert!(page["text"].as_str().unwrap().len() <= MAX_PREVIEW);
            cursor = end;
            pages += 1;
            if page["next_read"].is_null() {
                break;
            }
            request = page["next_read"].clone();
            assert!(pages < 10);
        }
        assert_eq!(cursor, original.len());
        assert!(pages >= 3);
        write(&d, "a", "changed");
        assert!(
            execute(&d.0, "read", &request)
                .unwrap_err()
                .contains("stale read")
        );
    }
    #[test]
    fn read_cursor_and_expected_revision_are_validated() {
        let d = dir();
        write(&d, "a", "界\n");
        let rev = revision("界\n".as_bytes());
        for request in [
            json!({"path":"a","byte_offset":1,"expected":rev}),
            json!({"path":"a","byte_offset":100,"expected":rev}),
            json!({"path":"a","byte_offset":-1,"expected":rev}),
            json!({"path":"a","byte_offset":0,"offset":2,"expected":rev}),
            json!({"path":"a","byte_offset":0}),
            json!({"path":"a","expected":null}),
        ] {
            assert!(execute(&d.0, "read", &request).is_err(), "{request}");
        }
        let empty = execute(
            &d.0,
            "read",
            &json!({"path":"a","byte_offset":4,"expected":rev}),
        )
        .unwrap();
        assert_eq!(empty["truncated"], false);
        assert_eq!(empty["next_read"], Value::Null);
    }
    #[test]
    fn mutation_previews_do_not_write_and_empty_creation_is_a_change() {
        let d = dir();
        let proposed = execute(
            &d.0,
            "write",
            &json!({"path":"empty","expected":null,"text":"","preview":true}),
        )
        .unwrap();
        assert_eq!(proposed["status"], "proposed");
        assert_eq!(proposed["applied"], false);
        assert_eq!(proposed["created"], true);
        assert_eq!(proposed["changed"], true);
        assert!(!d.0.join("empty").exists());
        let created = execute(
            &d.0,
            "write",
            &json!({"path":"empty","expected":null,"text":""}),
        )
        .unwrap();
        assert_eq!(created["status"], "applied");
        assert_eq!(created["changed"], true);
        let r = read(&d, "empty");
        let request = json!({"path":"empty","expected":r["revision"],"edits":[{"start":1,"end":0,"text":"hello"}],"preview":true});
        let edited = execute(&d.0, "edit", &request).unwrap();
        assert_eq!(edited["base_revision"], r["revision"]);
        assert_eq!(std::fs::read_to_string(d.0.join("empty")).unwrap(), "");
        let unchanged = execute(
            &d.0,
            "write",
            &json!({"path":"empty","expected":r["revision"],"text":""}),
        )
        .unwrap();
        assert_eq!(unchanged["status"], "unchanged");
        assert_eq!(unchanged["applied"], false);
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
