//! Ownership-aware self-update support for Cargo-installed Glass executables.

use crate::browser::session::BrowserResult;
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const FULL_PACKAGE: &str = "glass-dev";
const CORE_PACKAGE: &str = "glass-browser";
const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";
const CRATES_IO_SPARSE_SOURCE: &str = "sparse+https://index.crates.io/";

#[derive(Debug)]
pub(crate) struct UpdateOptions {
    pub dry_run: bool,
    pub version: Option<String>,
    pub force: bool,
    pub registry: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstalledPackage {
    name: String,
    version: String,
    bins: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePlan {
    executable: String,
    package: String,
    current_version: String,
    source: String,
    install_root: PathBuf,
    command: Vec<String>,
    waits_for_completion: bool,
}

pub(crate) fn run(options: UpdateOptions) -> BrowserResult<()> {
    validate_options(&options)?;
    let executable = env::current_exe()
        .map_err(|error| format!("could not resolve the running Glass executable: {error}"))?;
    let (binary, root) = installed_binary_and_root(&executable)?;
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let packages = cargo_install_list(&cargo, &root)?;
    let owner = resolve_owner(&packages, &binary)?;
    let source = match installed_source(&root, owner) {
        Ok(source) => source,
        Err(_) if options.registry.is_some() => "unknown (explicit registry selected)".to_owned(),
        Err(error) => return Err(error),
    };
    validate_source(&source, options.registry.as_deref())?;
    reject_owner_conflict(&packages, owner)?;

    let arguments = install_arguments(owner, &root, &options);
    let plan = UpdatePlan {
        executable: binary,
        package: owner.name.clone(),
        current_version: owner.version.clone(),
        source,
        install_root: root,
        command: display_command(&cargo, &arguments),
        waits_for_completion: !cfg!(windows),
    };

    if options.dry_run {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    eprintln!(
        "updating {} v{} in {}",
        plan.package,
        plan.current_version,
        plan.install_root.display()
    );
    let mut command = Command::new(&cargo);
    command
        .args(&arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    #[cfg(windows)]
    {
        let child = command.spawn().map_err(|error| {
            format!(
                "could not start Cargo updater `{}`: {error}",
                plan.command.join(" ")
            )
        })?;
        eprintln!(
            "Cargo updater started as process {}; it will finish after Glass exits",
            child.id()
        );
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let status = command.status().map_err(|error| {
            format!(
                "could not run Cargo updater `{}`: {error}",
                plan.command.join(" ")
            )
        })?;
        if !status.success() {
            return Err(format!("Cargo updater failed with {status}").into());
        }
        println!("updated {} successfully", plan.package);
        Ok(())
    }
}

fn validate_options(options: &UpdateOptions) -> BrowserResult<()> {
    for (label, value) in [
        ("version", options.version.as_deref()),
        ("registry", options.registry.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(format!("--{label} cannot be empty").into());
        }
    }
    Ok(())
}

fn installed_binary_and_root(executable: &Path) -> BrowserResult<(String, PathBuf)> {
    let binary = executable
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or("the running executable name is not valid UTF-8")?;
    if !matches!(binary, "glass" | "glass-browser") {
        return Err(format!(
            "cannot update unmanaged executable `{}`; install `glass-dev` or `glass-browser` with Cargo",
            executable.display()
        )
        .into());
    }
    let bin_dir = executable
        .parent()
        .ok_or("the running executable has no parent directory")?;
    if bin_dir.file_name() != Some(OsStr::new("bin")) {
        return Err(format!(
            "cannot update source or unmanaged build `{}`; use `cargo install --path ...` from its source or install a published Glass package",
            executable.display()
        )
        .into());
    }
    let root = bin_dir
        .parent()
        .ok_or("the Cargo bin directory has no install root")?;
    Ok((binary.to_owned(), root.to_path_buf()))
}

fn cargo_install_list(cargo: &OsStr, root: &Path) -> BrowserResult<Vec<InstalledPackage>> {
    let output = Command::new(cargo)
        .args([
            OsStr::new("install"),
            OsStr::new("--list"),
            OsStr::new("--root"),
        ])
        .arg(root)
        .output()
        .map_err(|error| format!("could not query Cargo-installed packages: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "`cargo install --list --root {}` failed: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    parse_install_list(&String::from_utf8(output.stdout)?)
}

fn parse_install_list(input: &str) -> BrowserResult<Vec<InstalledPackage>> {
    let mut packages: Vec<InstalledPackage> = Vec::new();
    for line in input.lines() {
        if !line.starts_with(char::is_whitespace) && line.ends_with(':') {
            let header = line.trim_end_matches(':');
            let mut fields = header.split_whitespace();
            let name = fields
                .next()
                .ok_or_else(|| format!("unrecognized Cargo install record `{line}`"))?;
            let version = fields
                .next()
                .and_then(|value| value.strip_prefix('v'))
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("unrecognized Cargo install record `{line}`"))?;
            packages.push(InstalledPackage {
                name: name.to_owned(),
                version: version.to_owned(),
                bins: Vec::new(),
            });
        } else if let Some(package) = packages.last_mut() {
            let bin = line.trim();
            if !bin.is_empty() {
                package.bins.push(bin.to_owned());
            }
        }
    }
    Ok(packages)
}

fn resolve_owner<'a>(
    packages: &'a [InstalledPackage],
    binary: &str,
) -> BrowserResult<&'a InstalledPackage> {
    let owners: Vec<_> = packages
        .iter()
        .filter(|package| {
            matches!(package.name.as_str(), FULL_PACKAGE | CORE_PACKAGE)
                && package.bins.iter().any(|candidate| candidate == binary)
        })
        .collect();
    match owners.as_slice() {
        [owner] if binary != "glass" || owner.name == FULL_PACKAGE => Ok(owner),
        [] => Err(format!(
            "Cargo does not record an owner for `{binary}` in this install root; refusing to guess an update channel"
        )
        .into()),
        _ => Err(format!(
            "multiple or invalid Cargo package owners were found for `{binary}`; repair the install with `cargo uninstall glass-dev` and `cargo uninstall glass-browser`"
        )
        .into()),
    }
}

fn reject_owner_conflict(
    packages: &[InstalledPackage],
    owner: &InstalledPackage,
) -> BrowserResult<()> {
    let other = if owner.name == FULL_PACKAGE {
        CORE_PACKAGE
    } else {
        FULL_PACKAGE
    };
    if packages.iter().any(|package| package.name == other) {
        return Err(format!(
            "both `{}` and `{other}` are recorded in this Cargo root; choose one owner and uninstall the other before updating",
            owner.name
        )
        .into());
    }
    Ok(())
}

fn installed_source(root: &Path, owner: &InstalledPackage) -> BrowserResult<String> {
    let path = root.join(".crates2.json");
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "cannot verify the installed package source from {}: {error}; pass --registry NAME only if changing to that registry is intentional",
            path.display()
        )
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "cannot read Cargo install provenance from {}: {error}; pass --registry NAME only if changing to that registry is intentional",
            path.display()
        )
    })?;
    let installs = value
        .get("installs")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!(
                "Cargo install provenance in {} has an unsupported shape",
                path.display()
            )
        })?;
    let prefix = format!("{} {} (", owner.name, owner.version);
    let matches: Vec<_> = installs
        .keys()
        .filter_map(|key| key.strip_prefix(&prefix)?.strip_suffix(')'))
        .collect();
    match matches.as_slice() {
        [source] => Ok((*source).to_owned()),
        [] => Err(format!(
            "Cargo provenance has no source for {} v{}; pass --registry NAME only if changing registries is intentional",
            owner.name, owner.version
        )
        .into()),
        _ => Err(format!(
            "Cargo provenance has multiple sources for {} v{}; refusing to choose one",
            owner.name, owner.version
        )
        .into()),
    }
}

fn validate_source(source: &str, registry: Option<&str>) -> BrowserResult<()> {
    if source == CRATES_IO_SOURCE || source == CRATES_IO_SPARSE_SOURCE || registry.is_some() {
        return Ok(());
    }
    Err(format!(
        "installed source `{source}` is not the crates.io registry; pass --registry NAME to intentionally select a configured Cargo registry, or update from the original source manually"
    )
    .into())
}

fn install_arguments(
    owner: &InstalledPackage,
    root: &Path,
    options: &UpdateOptions,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("install"),
        OsString::from(&owner.name),
        OsString::from("--locked"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
    ];
    if let Some(version) = &options.version {
        arguments.push(OsString::from("--version"));
        arguments.push(OsString::from(version));
    }
    if options.force {
        arguments.push(OsString::from("--force"));
    }
    if let Some(registry) = &options.registry {
        arguments.push(OsString::from("--registry"));
        arguments.push(OsString::from(registry));
    }
    arguments
}

fn display_command(cargo: &OsStr, arguments: &[OsString]) -> Vec<String> {
    std::iter::once(cargo)
        .chain(arguments.iter().map(OsString::as_os_str))
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, version: &str, bins: &[&str]) -> InstalledPackage {
        InstalledPackage {
            name: name.to_owned(),
            version: version.to_owned(),
            bins: bins.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[test]
    fn parses_cargo_install_ownership() {
        let packages = parse_install_list(
            "glass-dev v0.3.3 (/checkout/crates/glass-dev):\n    glass\n    glass-browser\nserde-cli v1.0.0:\n    serde\n",
        )
        .unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(
            packages[0],
            package("glass-dev", "0.3.3", &["glass", "glass-browser"])
        );
    }

    #[test]
    fn resolves_each_supported_package_owner() {
        let full = vec![package(FULL_PACKAGE, "0.3.3", &["glass", "glass-browser"])];
        assert_eq!(resolve_owner(&full, "glass").unwrap().name, FULL_PACKAGE);
        assert_eq!(
            resolve_owner(&full, "glass-browser").unwrap().name,
            FULL_PACKAGE
        );

        let core = vec![package(CORE_PACKAGE, "0.3.3", &["glass-browser"])];
        assert_eq!(
            resolve_owner(&core, "glass-browser").unwrap().name,
            CORE_PACKAGE
        );
        assert!(resolve_owner(&core, "glass").is_err());
    }

    #[test]
    fn rejects_conflicting_package_records() {
        let packages = vec![
            package(FULL_PACKAGE, "0.3.3", &["glass", "glass-browser"]),
            package(CORE_PACKAGE, "0.3.2", &["glass-browser"]),
        ];
        assert!(resolve_owner(&packages, "glass-browser").is_err());
        assert!(reject_owner_conflict(&packages, &packages[0]).is_err());
    }

    #[test]
    fn builds_a_root_preserving_locked_install() {
        let owner = package(FULL_PACKAGE, "0.3.3", &["glass"]);
        let arguments = install_arguments(
            &owner,
            Path::new("/opt/glass"),
            &UpdateOptions {
                dry_run: false,
                version: Some("0.3.4".to_owned()),
                force: true,
                registry: Some("crates-io".to_owned()),
            },
        );
        let rendered: Vec<_> = arguments
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            rendered,
            [
                "install",
                FULL_PACKAGE,
                "--locked",
                "--root",
                "/opt/glass",
                "--version",
                "0.3.4",
                "--force",
                "--registry",
                "crates-io"
            ]
        );
    }

    #[test]
    fn requires_explicit_registry_for_source_change() {
        assert!(validate_source("git+https://example.test/glass", None).is_err());
        assert!(validate_source("git+https://example.test/glass", Some("internal")).is_ok());
        assert!(validate_source(CRATES_IO_SOURCE, None).is_ok());
    }
}
