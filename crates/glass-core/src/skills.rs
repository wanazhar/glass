//! Skills: markdown files the model can load on demand.
//!
//! This is the cheapest possible extension mechanism. There is no ABI, no
//! dynamic dispatch, and no crash surface: a skill is prose, and loading one
//! means appending its text to the system prompt for the turn. Only the skill
//! names and descriptions cost tokens up front, which is what makes the set
//! effectively free until something matches.
//!
//! A skill file is YAML-ish front matter followed by a markdown body:
//!
//! ```text
//! ---
//! name: release-check
//! description: Steps to cut a release. Use when asked to prepare or ship a version.
//! ---
//! Run the full test suite, then tag, then push.
//! ```

use std::path::{Path, PathBuf};

pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

/// Scan `dir` for `*.md` skills, sorted by name.
///
/// Unparseable files are skipped rather than fatal: a typo in someone's skill
/// should not stop the harness from starting.
pub fn load_dir(dir: &Path) -> Vec<Skill> {
    // Cached walk (fs_scan): the same directory is listed several times
    // per session, and any failed match invalidates and re-walks.
    let Some(entries) = crate::fs_scan::read_dir(dir) else {
        return Vec::new();
    };

    let mut out: Vec<Skill> = entries
        .into_iter()
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .filter_map(|e| {
            let path = e.path();
            load_metadata(path).ok()
        })
        .collect();

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Discovery reads only a bounded prefix; skill bodies are loaded on demand.
/// Front matter must finish within 64 KiB. Large bodies do not affect discovery.
pub(crate) fn load_metadata(path: PathBuf) -> Result<Skill, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK)
        .open(&path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("skill must be a regular file".into());
    }
    let mut prefix = Vec::new();
    file.take(64 * 1024).read_to_end(&mut prefix).map_err(|e| e.to_string())?;
    parse(&String::from_utf8_lossy(&prefix), path)
}

/// Parse one skill file. The name falls back to the file stem.
///
/// The body is deliberately discarded: the model reads the file with `bash` on
/// demand, so holding every skill's prose in the harness's heap would be a cost
/// with no benefit. `pub(crate)` so the Agent Plugins loader can reuse it for
/// `skills/<name>/SKILL.md` files.
pub(crate) fn parse(text: &str, path: PathBuf) -> Result<Skill, String> {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "skill".into());

    let trimmed = text.trim_start();
    if !trimmed.starts_with("---") {
        // No front matter: the file is all body, so there is no description for
        // the catalogue and the model cannot tell what it is for.
        return Ok(Skill {
            name: stem,
            description: String::new(),
            path,
        });
    }

    let rest = trimmed.strip_prefix("---").ok_or("bad front matter")?;
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))
        .unwrap_or(rest);
    let (front, _body) = rest
        .split_once("\n---")
        .ok_or_else(|| format!("unterminated front matter in {}", path.display()))?;

    let mut name = stem;
    let mut description = String::new();
    for line in front.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        match k.trim() {
            "name" => name = v.trim().to_string(),
            "description" => description = v.trim().to_string(),
            _ => {}
        }
    }

    Ok(Skill {
        name,
        description,
        path,
    })
}

/// Render the catalogue injected into the system prompt.
///
/// Only names and descriptions appear here. The bodies are loaded on demand,
/// so a large skill library costs almost nothing until it is actually used.
pub fn catalogue(skills: &[Skill]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let mut out = String::from(
        "You can load additional instructions by reading these files with `bash` \
         when the task calls for one. Read the file, then follow it:\n\n",
    );
    for s in skills {
        out.push_str(&format!(
            "- {} ({}): {}\n",
            s.path.display(),
            s.name,
            s.description
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-skill-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn metadata_discovery_bounds_body_reads_and_rejects_unclosed_headers() {
        let dir = scratch("large-body");
        let path = dir.join("large.md");
        let mut file = std::fs::File::create(&path).unwrap();
        use std::io::Write;
        file.write_all(b"---\nname: bounded\ndescription: large body\n---\n").unwrap();
        file.set_len(64 * 1024 * 1024).unwrap();
        let skill = load_metadata(path.clone()).unwrap();
        assert_eq!(skill.name, "bounded");
        assert_eq!(load_dir(&dir).len(), 1);
        std::fs::write(&path, format!("---\n{}", "x".repeat(70_000))).unwrap();
        assert!(load_metadata(path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parses_front_matter() {
        let text = "---\nname: release\ndescription: Cut a release\n---\n\n1. test\n2. tag\n";
        let s = parse(text, PathBuf::from("/x/release.md")).unwrap();
        assert_eq!(s.name, "release");
        assert_eq!(s.description, "Cut a release");
    }

    #[test]
    fn file_without_front_matter_falls_back_to_stem() {
        let s = parse("just prose here", PathBuf::from("/x/plain.md")).unwrap();
        assert_eq!(s.name, "plain");
        assert_eq!(s.description, "");
    }

    #[test]
    fn unterminated_front_matter_is_rejected() {
        assert!(parse("---\nname: x\nno end", PathBuf::from("/x/bad.md")).is_err());
    }

    #[test]
    fn loads_and_sorts_by_name() {
        let d = scratch("sort");
        std::fs::write(
            d.join("zeta.md"),
            "---\nname: zeta\ndescription: z\n---\nz body",
        )
        .unwrap();
        std::fs::write(
            d.join("alpha.md"),
            "---\nname: alpha\ndescription: a\n---\na body",
        )
        .unwrap();
        std::fs::write(d.join("notes.txt"), "ignored").unwrap();

        let skills = load_dir(&d);
        assert_eq!(skills.len(), 2, "only .md files load");
        assert_eq!(skills[0].name, "alpha");
        assert_eq!(skills[1].name, "zeta");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn unparseable_file_does_not_stop_the_scan() {
        let d = scratch("bad");
        std::fs::write(
            d.join("good.md"),
            "---\nname: good\ndescription: ok\n---\nbody",
        )
        .unwrap();
        std::fs::write(d.join("broken.md"), "---\nname: broken\nnever ends").unwrap();
        let skills = load_dir(&d);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "good");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn missing_directory_yields_nothing() {
        assert!(load_dir(Path::new("/definitely/not/here")).is_empty());
    }

    #[test]
    fn catalogue_lists_paths_and_descriptions_only() {
        let d = scratch("cat");
        std::fs::write(
            d.join("s.md"),
            "---\nname: s\ndescription: does things\n---\nSECRET BODY MARKER",
        )
        .unwrap();
        let skills = load_dir(&d);
        let c = catalogue(&skills).unwrap();
        assert!(c.contains("does things"));
        assert!(c.contains("s.md"));
        assert!(
            !c.contains("SECRET BODY MARKER"),
            "bodies must not be in the catalogue"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn empty_skill_set_has_no_catalogue() {
        assert!(catalogue(&[]).is_none());
    }
}
