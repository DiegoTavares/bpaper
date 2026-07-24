//! BYO-agent rails (V5 spec §6.1–6.2): the launch command for the user's own
//! CLI agent, where it is stored, and how a kickoff prompt becomes argv.
//!
//! BreadPaper never speaks to a model. Everything here reduces to: what
//! command, what working directory, and what first argument.

use anyhow::{Context as _, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::vault::Vault;

/// Optional placeholder in a launch command that is replaced by the kickoff
/// prompt. Without it, the kickoff is appended as one final argument.
pub const PROMPT_PLACEHOLDER: &str = "{prompt}";

/// CLI agents the connect flow scans PATH for.
pub const KNOWN_AGENTS: &[KnownAgent] = &[
    KnownAgent {
        program: "claude",
        display_name: "Claude Code",
    },
    KnownAgent {
        program: "gemini",
        display_name: "Gemini CLI",
    },
    KnownAgent {
        program: "codex",
        display_name: "Codex",
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnownAgent {
    pub program: &'static str,
    pub display_name: &'static str,
}

/// The known agents currently resolvable on PATH. Blocking I/O — call from a
/// background thread.
pub fn detect_installed_agents() -> Vec<KnownAgent> {
    KNOWN_AGENTS
        .iter()
        .copied()
        .filter(|agent| which::which(agent.program).is_ok())
        .collect()
}

/// Where a resolved launch command came from; the vault override wins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandSource {
    Vault,
    Global,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectedAgent {
    pub command: String,
    pub source: CommandSource,
}

/// Resolves the launch command: the vault's `[agent] command` override if set,
/// otherwise the user-level default. `None` means the user isn't connected.
/// Reads the global settings file — call from a background thread when
/// latency matters.
pub fn resolved_command(vault: Option<&Vault>) -> Option<ConnectedAgent> {
    if let Some(command) = vault.and_then(|vault| vault.config.agent.command.clone()) {
        return Some(ConnectedAgent {
            command,
            source: CommandSource::Vault,
        });
    }
    load_global_command().map(|command| ConnectedAgent {
        command,
        source: CommandSource::Global,
    })
}

/// A parsed launch, ready to spawn directly (not through a shell).
#[derive(Debug, Clone, PartialEq)]
pub struct AgentLaunch {
    pub program: String,
    pub args: Vec<String>,
}

/// Parses a shell-style command line into argv and weaves in the kickoff
/// prompt: `{prompt}` tokens are substituted (or dropped entirely when there
/// is no kickoff — the ad-hoc "New conversation" case); without a placeholder
/// the kickoff is appended as one final argument. The kickoff itself is one
/// argv element, so it needs no quoting.
pub fn build_launch(command_line: &str, kickoff: Option<&str>) -> Result<AgentLaunch> {
    let tokens = shlex::split(command_line).with_context(|| {
        format!("couldn't parse the agent command {command_line:?} (unbalanced quotes?)")
    })?;
    let mut argv = Vec::new();
    let mut used_placeholder = false;
    for token in tokens {
        if token.contains(PROMPT_PLACEHOLDER) {
            used_placeholder = true;
            if let Some(kickoff) = kickoff {
                argv.push(token.replace(PROMPT_PLACEHOLDER, kickoff));
            }
        } else {
            argv.push(token);
        }
    }
    if let Some(kickoff) = kickoff
        && !used_placeholder
    {
        argv.push(kickoff.to_string());
    }
    let mut argv = argv.into_iter();
    let program = argv
        .next()
        .with_context(|| format!("the agent command {command_line:?} is empty"))?;
    Ok(AgentLaunch {
        program,
        args: argv.collect(),
    })
}

/// The kickoff prompt for running a skill file. Deliberately agent-agnostic
/// and a pointer, not an inline copy — the agent reads the live, user-editable
/// file (spec §5.2).
pub fn run_skill_kickoff(vault_relative_path: &str) -> String {
    format!("Read and execute {vault_relative_path}")
}

/// The user-level BreadPaper settings file holding the global agent default.
/// Vault-independent state, so it lives in the user's config dir rather than
/// any vault. Handled as a raw TOML table — never a typed schema — so fields
/// written by newer builds survive both reads and saves by older ones.
pub fn global_settings_path() -> PathBuf {
    util::paths::home_dir()
        .join(".config")
        .join("breadpaper")
        .join("settings.toml")
}

fn load_global_settings(path: &Path) -> Result<toml::Table> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(toml::Table::new());
        }
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

fn load_global_command_from(path: &Path) -> Result<Option<String>> {
    Ok(load_global_settings(path)?
        .get("agent")
        .and_then(|agent| agent.as_table())
        .and_then(|agent| agent.get("command"))
        .and_then(|command| command.as_str())
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(String::from))
}

/// The user-level default launch command, if configured. Read errors are
/// logged and treated as not-connected.
pub fn load_global_command() -> Option<String> {
    load_global_command_from(&global_settings_path())
        .map_err(|error| log::error!("BreadPaper: couldn't read the agent settings: {error:?}"))
        .ok()
        .flatten()
}

fn save_global_command_to(path: &Path, command: &str) -> Result<()> {
    let mut settings = load_global_settings(path)?;
    let agent = settings
        .entry("agent")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !agent.is_table() {
        // `agent = "..."` (not a table) is malformed; replace it.
        *agent = toml::Value::Table(toml::Table::new());
    }
    if let Some(agent) = agent.as_table_mut() {
        agent.insert(
            "command".to_string(),
            toml::Value::String(command.to_string()),
        );
    }
    let serialized = toml::to_string_pretty(&settings).context("serializing agent settings")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, serialized).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Persists `command` as the user-level default. Blocking I/O — call from a
/// background thread.
pub fn save_global_command(command: &str) -> Result<()> {
    save_global_command_to(&global_settings_path(), command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_launch_appends_kickoff() {
        let launch = build_launch("claude", Some("Read and execute skills/x.md")).unwrap();
        assert_eq!(launch.program, "claude");
        assert_eq!(launch.args, vec!["Read and execute skills/x.md"]);
    }

    #[test]
    fn build_launch_with_extra_args() {
        let launch = build_launch("my-agent --profile personal", Some("go")).unwrap();
        assert_eq!(launch.program, "my-agent");
        assert_eq!(launch.args, vec!["--profile", "personal", "go"]);
    }

    #[test]
    fn build_launch_substitutes_placeholder() {
        let launch = build_launch("my-agent --prompt={prompt} --yes", Some("do it")).unwrap();
        assert_eq!(launch.program, "my-agent");
        assert_eq!(launch.args, vec!["--prompt=do it", "--yes"]);
    }

    #[test]
    fn build_launch_without_kickoff_drops_placeholder() {
        let launch = build_launch("my-agent {prompt} --yes", None).unwrap();
        assert_eq!(launch.program, "my-agent");
        assert_eq!(launch.args, vec!["--yes"]);
    }

    #[test]
    fn build_launch_without_kickoff_appends_nothing() {
        let launch = build_launch("claude --continue", None).unwrap();
        assert_eq!(launch.program, "claude");
        assert_eq!(launch.args, vec!["--continue"]);
    }

    #[test]
    fn build_launch_quoted_command() {
        let launch = build_launch(r#"claude --append-system-prompt "be kind""#, None).unwrap();
        assert_eq!(launch.args, vec!["--append-system-prompt", "be kind"]);
    }

    #[test]
    fn build_launch_rejects_empty_and_unparseable() {
        assert!(build_launch("", None).is_err());
        assert!(build_launch("   ", None).is_err());
        assert!(build_launch("claude \"unbalanced", None).is_err());
        // A command that is only a placeholder resolves to nothing without a
        // kickoff.
        assert!(build_launch("{prompt}", None).is_err());
    }

    #[test]
    fn global_command_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        assert_eq!(load_global_command_from(&path).unwrap(), None);

        save_global_command_to(&path, "claude").unwrap();
        assert_eq!(
            load_global_command_from(&path).unwrap(),
            Some("claude".to_string())
        );

        save_global_command_to(&path, "gemini --yolo").unwrap();
        assert_eq!(
            load_global_command_from(&path).unwrap(),
            Some("gemini --yolo".to_string())
        );
    }

    #[test]
    fn global_settings_preserve_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(
            &path,
            "top_level = true\n\n[future_section]\nkey = 1\n\n[agent]\ncommand = \"claude\"\nfuture_key = \"x\"\n",
        )
        .unwrap();
        assert_eq!(
            load_global_command_from(&path).unwrap(),
            Some("claude".to_string())
        );

        // A save by this (possibly older) build must not erase fields written
        // by a newer one.
        save_global_command_to(&path, "gemini").unwrap();
        assert_eq!(
            load_global_command_from(&path).unwrap(),
            Some("gemini".to_string())
        );
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("future_section"), "section erased: {raw}");
        assert!(raw.contains("future_key"), "agent key erased: {raw}");
        assert!(raw.contains("top_level"), "top-level key erased: {raw}");
    }

    #[test]
    fn blank_global_command_means_not_connected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "[agent]\ncommand = \"  \"\n").unwrap();
        assert_eq!(load_global_command_from(&path).unwrap(), None);
    }

    #[test]
    fn run_skill_kickoff_is_a_pointer() {
        assert_eq!(
            run_skill_kickoff("skills/timeline/wrap-today.md"),
            "Read and execute skills/timeline/wrap-today.md"
        );
    }
}
