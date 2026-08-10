use super::state::{DevSurface, DevTuiState};
use crate::agents::AgentSpec;
use crate::debugger::DebugAdapterConfig;
use crate::kernels::KernelKind;
use std::time::Duration;

pub fn execute(state: &mut DevTuiState, input: &str) -> Result<String, String> {
    let mut parts = input.split_whitespace();
    let Some(command) = parts.next() else {
        return Ok("Command palette closed".into());
    };
    match command {
        "help" | "?" => Ok("Routes: view SURFACE · agent spawn|prompt|steer|follow-up|cancel · debug start|stop · kernel start|reset|stop · git · tests · replay · quit".into()),
        "quit" | "q" => {
            state.quit = true;
            Ok("Closing Glass Dev".into())
        }
        "view" => {
            let name = parts.next().ok_or("view requires a surface")?;
            state.surface = parse_surface(name).ok_or("unknown surface")?;
            Ok(format!("Opened {}", state.surface.label()))
        }
        "agent" => execute_agent(state, parts.collect()),
        "debug" => execute_debug(state, parts.collect()),
        "kernel" => execute_kernel(state, parts.collect()),
        "git" => {
            state.surface = DevSurface::Git;
            Ok("Git status refreshed".into())
        }
        "tests" | "test" => {
            state.surface = DevSurface::Tests;
            Ok("Test results refreshed".into())
        }
        "replay" => {
            state.surface = DevSurface::Replay;
            Ok("Observable replay refreshed".into())
        }
        _ => Err(format!("unknown command {command}; use help")),
    }
}

fn execute_agent(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Agent;
        return Ok("Opened Glass Agent".into());
    };
    match action {
        "spawn" => {
            let role = parts.get(1).ok_or("agent spawn requires ROLE TASK")?;
            let task = parts.get(2..).unwrap_or_default().join(" ");
            if task.is_empty() {
                return Err("agent spawn requires ROLE TASK".into());
            }
            let id = state
                .workspace
                .agents()
                .create(AgentSpec::new(*role, task))
                .map_err(|error| error.to_string())?;
            state.surface = DevSurface::Agents;
            Ok(format!("Spawned {}", id.as_str()))
        }
        "prompt" | "steer" | "follow-up" => {
            let id = parts.get(1).ok_or("agent action requires ID TEXT")?;
            let text = parts.get(2..).unwrap_or_default().join(" ");
            let agent = find_agent(state, id)?;
            match action {
                "prompt" => state.workspace.agents().prompt(&agent, text),
                "steer" => state.workspace.agents().steer(&agent, text),
                _ => state.workspace.agents().follow_up(&agent, text),
            }
            .map_err(|error| error.to_string())?;
            state.surface = DevSurface::Agent;
            Ok(format!("Queued {action} for {id}"))
        }
        "cancel" => {
            let id = parts.get(1).ok_or("agent cancel requires ID")?;
            let agent = find_agent(state, id)?;
            state
                .workspace
                .agents()
                .cancel(&agent)
                .map_err(|error| error.to_string())?;
            Ok(format!("Cancelled {id}"))
        }
        _ => Err("agent actions: spawn, prompt, steer, follow-up, cancel".into()),
    }
}

fn find_agent(state: &mut DevTuiState, id: &str) -> Result<crate::AgentId, String> {
    state
        .workspace
        .agents()
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|agent| agent.id.as_str() == id)
        .map(|agent| agent.id)
        .ok_or_else(|| "unknown agent".into())
}

fn execute_debug(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied() {
        None => {
            state.surface = DevSurface::Debugger;
            Ok("Opened debugger".into())
        }
        Some("start") => {
            let name = parts.get(1).ok_or("debug start requires NAME COMMAND")?;
            let command = parts.get(2).ok_or("debug start requires NAME COMMAND")?;
            let arguments = parts
                .get(3..)
                .unwrap_or_default()
                .iter()
                .map(|value| (*value).to_string());
            state
                .workspace
                .start_debugger(
                    name,
                    &DebugAdapterConfig::new(command, arguments),
                    Duration::from_secs(30),
                )
                .map_err(|error| error.to_string())?;
            state.surface = DevSurface::Debugger;
            Ok(format!("Started debugger {name}"))
        }
        Some("stop") => {
            let name = parts.get(1).ok_or("debug stop requires NAME")?;
            state
                .workspace
                .stop_debugger(name)
                .map_err(|error| error.to_string())?;
            Ok(format!("Stopped debugger {name}"))
        }
        _ => Err("debug actions: start, stop".into()),
    }
}

fn execute_kernel(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let action = parts
        .first()
        .copied()
        .ok_or("kernel requires start, reset, or stop")?;
    let name = parts.get(1).ok_or("kernel action requires NAME")?;
    match action {
        "start" => {
            let kind = match parts.get(2).copied().ok_or("kernel start requires KIND")? {
                "python" => KernelKind::Python,
                "javascript" => KernelKind::JavaScript,
                "shell" => KernelKind::Shell,
                "sql" => KernelKind::Sql,
                _ => return Err("kernel kind must be python, javascript, shell, or sql".into()),
            };
            state
                .workspace
                .kernels_mut()
                .start(name, kind, "human")
                .map_err(|error| error.to_string())?;
        }
        "reset" => state
            .workspace
            .kernels_mut()
            .reset(name, "human")
            .map_err(|error| error.to_string())?,
        "stop" => {
            state
                .workspace
                .kernels_mut()
                .stop(name)
                .map_err(|error| error.to_string())?;
        }
        _ => return Err("kernel actions: start, reset, stop".into()),
    }
    Ok(format!("Kernel {name}: {action}"))
}

fn parse_surface(name: &str) -> Option<DevSurface> {
    DevSurface::ALL
        .into_iter()
        .find(|surface| surface.label().eq_ignore_ascii_case(name))
        .or(match name {
            "home" => Some(DevSurface::Dashboard),
            "agent" => Some(DevSurface::Agent),
            "process" => Some(DevSurface::Processes),
            "debug" => Some(DevSurface::Debugger),
            "test" => Some(DevSurface::Tests),
            "experiment" => Some(DevSurface::Experiments),
            "browser" => Some(DevSurface::Browser),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_major_surface_has_a_palette_route() {
        for surface in DevSurface::ALL {
            assert_eq!(parse_surface(surface.label()), Some(surface));
        }
    }
}
