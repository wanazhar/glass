//! Plugin authoring and opt-in sidecar profiling; no sidecar runs for validation.
use serde_json::json;
use std::{io::Write, path::Path};
pub fn record(name: &str, kind: &str, elapsed: u128, replied: bool) {
    let path = crate::config::data_dir().join("extension-events.jsonl");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    if let Ok(mut file) = options.open(path)
        && file.metadata().is_ok_and(|m| m.is_file())
    {
        let row = json!({"extension":glass_tui::text::clip(name,96),"event":kind,"elapsed_ms":elapsed,"replied":replied});
        let _ = writeln!(file, "{row}");
    }
}
pub fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("GC_EXTENSION_TRACE").as_deref() == Ok("1"))
}
pub fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("scaffold") => {
            let name = args.get(1).ok_or("gc plugin scaffold NAME DIRECTORY")?;
            if !crate::plugins::valid_name(name) {
                return Err("Invalid plugin name".into());
            }
            let directory = Path::new(args.get(2).ok_or("gc plugin scaffold NAME DIRECTORY")?);
            std::fs::create_dir(directory)
                .map_err(|_| "Choose a new directory; existing paths are never overwritten")?;
            std::fs::create_dir_all(directory.join("skills/example")).map_err(|e| e.to_string())?;
            crate::auth::secure_write(&directory.join("plugin.json"),&serde_json::to_vec_pretty(&json!({"$schema":crate::plugins::PLUGIN_SCHEMA,"name":name,"version":"0.1.0","description":"A local workflow plugin"})).map_err(|e|e.to_string())?)?;
            crate::auth::secure_write(&directory.join("skills/example/SKILL.md"),b"---\nname: example\ndescription: A reviewed project workflow.\n---\n\n1. Inspect the requested scope.\n2. Make the agreed changes.\n3. Verify and report actual results.\n")?;
            crate::cli::text(
                "Plugin scaffold",
                &crate::cli::present_command_references(&format!(
                    "Created {}\nValidate: gc plugin validate {}\nNo code was executed or installed.",
                    directory.display(),
                    directory.display()
                )),
            );
        }
        Some("validate") => {
            let directory = Path::new(args.get(1).ok_or("gc plugin validate DIRECTORY")?);
            let mut notices = Vec::new();
            let plugin = crate::plugins::load_one(directory, &mut notices)
                .ok_or_else(|| format!("Plugin rejected: {}", notices.join("; ")))?;
            let value = json!({"name":plugin.name,"version":plugin.version,"skills":plugin.skills.iter().map(|s|&s.name).collect::<Vec<_>>(),"mcp":plugin.mcp.is_some(),"notices":notices,"execution":"none; static validation only"});
            if args.iter().any(|a| a == "--json") {
                crate::emit_json(value);
            } else {
                crate::cli::report("Plugin validation", &value);
            }
        }
        Some("trace") => {
            let rows =
                crate::requests::tail(&crate::config::data_dir().join("extension-events.jsonl"))?;
            let value = json!({"events":rows,"enable":"Launch gc with GC_EXTENSION_TRACE=1. Metadata only; payloads and tool arguments are never recorded. Last 100 events shown."});
            if args.iter().any(|a| a == "--json") {
                crate::emit_json(value);
            } else {
                crate::cli::report("Plugin events", &value);
            }
        }
        _ => {
            return Err(
                "gc plugin scaffold NAME DIRECTORY | validate DIRECTORY [--json] | trace [--json]"
                    .into(),
            );
        }
    }
    Ok(())
}
