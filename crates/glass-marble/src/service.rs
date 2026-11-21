//! Keeping Marble running after the terminal closes.
//!
//! An assistant that only lives while a shell is open is not always on. This
//! module renders a login service for the host it runs on — a systemd user unit
//! on Linux, a LaunchAgent on macOS — with the same arguments the operator used
//! interactively. Rendering is separate from writing so `--print` is always a
//! safe, inspectable operation.

use std::path::{Path, PathBuf};

/// Login service flavour for one platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServicePlatform {
    Systemd,
    Launchd,
}

impl ServicePlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Systemd => "systemd",
            Self::Launchd => "launchd",
        }
    }

    /// Label used by launchd; also the systemd unit file name stem.
    pub const LABEL: &'static str = "dev.glass.marble";
}

/// The platform this binary was built for.
pub fn current_platform() -> ServicePlatform {
    if cfg!(target_os = "macos") {
        ServicePlatform::Launchd
    } else {
        ServicePlatform::Systemd
    }
}

/// Everything the service needs to reproduce one `serve` invocation.
#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub binary: PathBuf,
    pub cwd: PathBuf,
    pub data_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub profile: String,
    pub token_file: Option<PathBuf>,
    pub log_file: PathBuf,
}

impl ServiceSpec {
    /// Arguments after the binary: global options, then `serve`.
    pub fn arguments(&self) -> Vec<String> {
        let mut arguments = vec![
            "--cwd".to_string(),
            self.cwd.display().to_string(),
            "--data-dir".to_string(),
            self.data_dir.display().to_string(),
            "serve".to_string(),
            "--port".to_string(),
            self.port.to_string(),
            "--host".to_string(),
            self.host.clone(),
            "--profile".to_string(),
            self.profile.clone(),
        ];
        if let Some(token_file) = &self.token_file {
            arguments.push("--token-file".to_string());
            arguments.push(token_file.display().to_string());
        }
        arguments
    }

    fn command_line(&self) -> String {
        let mut line = quote(&self.binary.display().to_string());
        for argument in self.arguments() {
            line.push(' ');
            line.push_str(&quote(&argument));
        }
        line
    }
}

/// Quote one argument so systemd parses it as a single word.
fn quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/._-:@=+".contains(character))
    {
        return value.to_string();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        if matches!(character, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('"');
    quoted
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Where the unit file belongs for the current user.
pub fn target_path(platform: ServicePlatform) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set, so the service location is unknown".to_string())?;
    Ok(match platform {
        ServicePlatform::Systemd => {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            config
                .join("systemd/user")
                .join(format!("{}.service", ServicePlatform::LABEL))
        }
        ServicePlatform::Launchd => home
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", ServicePlatform::LABEL)),
    })
}

/// Render the service definition for one platform.
pub fn render(platform: ServicePlatform, spec: &ServiceSpec) -> String {
    match platform {
        ServicePlatform::Systemd => {
            let mut unit = format!(
                "# Glass Marble — the always-on personal assistant.\n\
                 [Unit]\n\
                 Description=Glass Marble personal assistant\n\
                 After=network-online.target\n\
                 Wants=network-online.target\n\
                 \n\
                 [Service]\n\
                 Type=simple\n\
                 WorkingDirectory={}\n\
                 ExecStart={}\n\
                 Environment=GLASS_MARBLE_DATA_DIR={}\n\
                 Restart=always\n\
                 RestartSec=3\n\
                 StandardOutput=append:{}\n\
                 StandardError=append:{}\n",
                quote(&spec.cwd.display().to_string()),
                spec.command_line(),
                quote(&spec.data_dir.display().to_string()),
                spec.log_file.display(),
                spec.log_file.display(),
            );
            unit.push_str("\n[Install]\nWantedBy=default.target\n");
            unit
        }
        ServicePlatform::Launchd => {
            let mut arguments = String::new();
            arguments.push_str(&format!(
                "        <string>{}</string>\n",
                escape_xml(&spec.binary.display().to_string())
            ));
            for argument in spec.arguments() {
                arguments.push_str(&format!(
                    "        <string>{}</string>\n",
                    escape_xml(&argument)
                ));
            }
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
                 \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\">\n\
                 <dict>\n\
                 \x20   <key>Label</key>\n\
                 \x20   <string>{label}</string>\n\
                 \x20   <key>ProgramArguments</key>\n\
                 \x20   <array>\n{arguments}\x20   </array>\n\
                 \x20   <key>WorkingDirectory</key>\n\
                 \x20   <string>{cwd}</string>\n\
                 \x20   <key>EnvironmentVariables</key>\n\
                 \x20   <dict>\n\
                 \x20       <key>GLASS_MARBLE_DATA_DIR</key>\n\
                 \x20       <string>{data}</string>\n\
                 \x20   </dict>\n\
                 \x20   <key>RunAtLoad</key>\n\
                 \x20   <true/>\n\
                 \x20   <key>KeepAlive</key>\n\
                 \x20   <true/>\n\
                 \x20   <key>ProcessType</key>\n\
                 \x20   <string>Interactive</string>\n\
                 \x20   <key>StandardOutPath</key>\n\
                 \x20   <string>{log}</string>\n\
                 \x20   <key>StandardErrorPath</key>\n\
                 \x20   <string>{log}</string>\n\
                 </dict>\n\
                 </plist>\n",
                label = ServicePlatform::LABEL,
                arguments = arguments,
                cwd = escape_xml(&spec.cwd.display().to_string()),
                data = escape_xml(&spec.data_dir.display().to_string()),
                log = escape_xml(&spec.log_file.display().to_string()),
            )
        }
    }
}

/// Result of installing, or of rendering for review.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub platform: ServicePlatform,
    pub path: PathBuf,
    pub rendered: String,
    pub written: bool,
    pub instructions: Vec<String>,
}

/// Render the unit, and write it when `write` is set.
pub fn install(
    platform: ServicePlatform,
    spec: &ServiceSpec,
    write: bool,
) -> Result<InstallOutcome, String> {
    let path = target_path(platform)?;
    let rendered = render(platform, spec);
    let instructions = match platform {
        ServicePlatform::Systemd => vec![
            "systemctl --user daemon-reload".to_string(),
            format!("systemctl --user enable --now {}", ServicePlatform::LABEL),
            "loginctl enable-linger \"$USER\"   # keep it running without a login session"
                .to_string(),
            format!("journalctl --user -u {} -f", ServicePlatform::LABEL),
            // An always-on assistant writes a log forever; an unbounded file is
            // the one thing that fills a small host's disk without warning.
            format!(
                "rotate the log: add a logrotate rule for {} (daily, rotate 7, copytruncate)",
                spec.log_file.display()
            ),
        ],
        ServicePlatform::Launchd => vec![
            format!("launchctl unload -w {} 2>/dev/null || true", path.display()),
            format!("launchctl load -w {}", path.display()),
            format!("tail -f {}", spec.log_file.display()),
            format!(
                "rotate the log with newsyslog or a periodic trim: {}",
                spec.log_file.display()
            ),
        ],
    };
    if write {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, &rendered)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(InstallOutcome {
        platform,
        path,
        rendered,
        written: write,
        instructions,
    })
}

/// Default log location inside the data directory.
pub fn default_log_file(data_dir: &Path) -> PathBuf {
    data_dir.join("marble.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ServiceSpec {
        ServiceSpec {
            binary: PathBuf::from("/usr/local/bin/glass-marble"),
            cwd: PathBuf::from("/home/azhar/project"),
            data_dir: PathBuf::from("/home/azhar/.local/share/glass-marble"),
            host: "100.64.0.5".into(),
            port: 4783,
            profile: "workspace".into(),
            token_file: Some(PathBuf::from("/home/azhar/.marble-token")),
            log_file: PathBuf::from("/home/azhar/.local/share/glass-marble/marble.log"),
        }
    }

    #[test]
    fn a_systemd_unit_restarts_the_assistant_with_the_same_arguments() {
        let unit = render(ServicePlatform::Systemd, &spec());
        assert!(unit.contains("ExecStart=/usr/local/bin/glass-marble --cwd /home/azhar/project"));
        assert!(unit.contains("serve --port 4783 --host 100.64.0.5 --profile workspace"));
        assert!(unit.contains("--token-file /home/azhar/.marble-token"));
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("Environment=GLASS_MARBLE_DATA_DIR="));
    }

    #[test]
    fn a_launch_agent_is_a_plist_that_keeps_running() {
        let plist = render(ServicePlatform::Launchd, &spec());
        assert!(plist.starts_with("<?xml version=\"1.0\""));
        assert!(plist.contains("<key>Label</key>"));
        assert!(plist.contains(ServicePlatform::LABEL));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("/usr/local/bin/glass-marble"));
        assert!(plist.trim_end().ends_with("</plist>"));
    }

    #[test]
    fn rendering_is_safe_without_writing() {
        let outcome = install(ServicePlatform::Systemd, &spec(), false).unwrap();
        assert!(!outcome.written);
        assert!(outcome.rendered.contains("ExecStart="));
        assert!(
            outcome.path.ends_with("dev.glass.marble.service")
                || outcome.path.ends_with(".service")
        );
        assert!(
            outcome
                .instructions
                .iter()
                .any(|line| line.contains("systemctl --user enable"))
        );
    }

    #[test]
    fn paths_with_spaces_are_quoted_and_xml_escaped() {
        let mut spec = spec();
        spec.cwd = PathBuf::from("/home/azhar/My Project");
        let unit = render(ServicePlatform::Systemd, &spec);
        assert!(
            unit.contains("WorkingDirectory=\"/home/azhar/My Project\""),
            "{unit}"
        );
        spec.cwd = PathBuf::from("/home/azhar/a&b");
        let plist = render(ServicePlatform::Launchd, &spec);
        assert!(plist.contains("a&amp;b"), "{plist}");
    }

    #[test]
    fn the_default_log_lives_in_the_data_directory() {
        assert_eq!(
            default_log_file(Path::new("/tmp/marble")),
            PathBuf::from("/tmp/marble/marble.log")
        );
    }
}
