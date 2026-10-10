//! Offline shell completion. Generated scripts never contact a provider.
pub const COMMANDS: &[&str] = &[
    "help",
    "resume",
    "run",
    "sessions",
    "list",
    "search",
    "context",
    "requests",
    "review",
    "details",
    "profiles",
    "extensions",
    "init",
    "export",
    "plugin",
    "worktree",
    "send",
    "serve",
    "attach",
    "stop",
    "respawn",
    "logs",
    "acp",
    "agent",
    "file",
    "status",
    "doctor",
    "completions",
    "workspace",
    "policy",
    "inspect",
    "contract",
    "history",
    "attention",
    "usage",
    "handoff",
    "diagnostics",
    "memory",
    "skill",
];
pub const FLAGS: &[&str] = &[
    "--help",
    "--json",
    "--model",
    "--connection",
    "--resume",
    "--attach",
    "--budget",
    "--trust",
    "--plan",
    "--retry",
    "--output-schema",
];
pub fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str){
        Some("candidates")=>{
            let kind=args.get(1).map(String::as_str).unwrap_or("commands");
            let candidates=match kind{
                "commands"=>COMMANDS.iter().map(|s|s.to_string()).collect::<Vec<_>>(),
                "flags"=>FLAGS.iter().map(|s|s.to_string()).collect(),
                "models"=>{let cfg=crate::config::load(&crate::config::default_path()).map_err(|_|"Cannot read model configuration")?;vec![cfg.model]},
                "sessions"=>crate::resume_picker::index(&crate::config::data_dir(),&std::env::current_dir().map_err(|e|e.to_string())?).into_iter().take(100).map(|s|s.id).collect(),
                _=>return Err("Candidates: commands, flags, models or sessions".into())
            };
            for value in candidates{if value.chars().all(|c|c.is_ascii_alphanumeric()||"-_.:/".contains(c)){println!("{value}");}}
        }
        Some("bash")=>println!("{}", BASH),Some("zsh")=>println!("{}",ZSH),Some("fish")=>println!("{}",FISH),
        _=>return Err("Usage: gc completions bash|zsh|fish\nInstall the generated script in your shell's completion directory.".into())
    }
    Ok(())
}
const BASH: &str = r#"_gc_complete() {
  local kind=commands prev="${COMP_WORDS[COMP_CWORD-1]}" cur="${COMP_WORDS[COMP_CWORD]}"
  case "$prev" in --model) kind=models;; --resume|resume) kind=sessions;; esac
  [[ "$cur" == --* ]] && kind=flags
  local candidates
  candidates="$(command gc completions candidates "$kind" 2>/dev/null)"
  COMPREPLY=( $(compgen -W "$candidates" -- "$cur") )
}
complete -F _gc_complete gc glass-core"#;
const ZSH: &str = r#"#compdef gc glass-core
_gc_complete() {
  local kind=commands
  case "$words[CURRENT-1]" in --model) kind=models;; --resume|resume) kind=sessions;; esac
  [[ "$words[CURRENT]" == --* ]] && kind=flags
  local -a choices
  choices=("${(@f)$(command gc completions candidates "$kind" 2>/dev/null)}")
  compadd -- "${choices[@]}"
}
compdef _gc_complete gc glass-core"#;
const FISH: &str = r#"for cmd in gc glass-core
  complete -c $cmd -f -n '__fish_use_subcommand' -a '(command gc completions candidates commands 2>/dev/null)'
  complete -c $cmd -l model -r -a '(command gc completions candidates models 2>/dev/null)'
  complete -c $cmd -l resume -r -a '(command gc completions candidates sessions 2>/dev/null)'
  complete -c $cmd -n '__fish_seen_subcommand_from resume' -a '(command gc completions candidates sessions 2>/dev/null)'
  complete -c $cmd -l help
  complete -c $cmd -l json
end"#;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_has_unique_safe_names() {
        let mut names = std::collections::HashSet::new();
        for name in COMMANDS {
            assert!(names.insert(name));
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        }
    }
}
