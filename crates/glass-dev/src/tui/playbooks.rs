//! Per-surface instructions injected into Pi context on every send.

use super::state::DevSurface;

pub fn playbook_name(surface: DevSurface) -> &'static str {
    match surface {
        DevSurface::Code => "editor",
        DevSurface::App => "browser",
        DevSurface::Git => "git",
        DevSurface::Debug => "debug",
        DevSurface::Terminal => "process",
        DevSurface::Tasks => "todo",
        DevSurface::More => "workspace",
        DevSurface::Trust => "trust",
        DevSurface::Agent => "agent",
    }
}

pub fn playbook(surface: DevSurface) -> &'static str {
    match surface {
        DevSurface::Code => {
            "Editor playbook: call glass.editor.selection and glass.editor.buffers before changing source. Address open comments with glass.editor.proposal.create. Do not overwrite unsaved human buffers with glass.file.write. Use glass.editor.fim for ghosts. Save only after the human accepts."
        }
        DevSurface::App => {
            "App playbook: glass.browser.observe before describing the page. glass.browser.act only with the current browserRevision and selected entity. glass.browser.verify for prove-it. Never invent selectors or claim a click without a Glass result."
        }
        DevSurface::Git => {
            "Git playbook: glass.git.status, glass.git.diff, and glass.git.conflicts before mutations. Prefer context.git.selectedPath. Stage/unstage/commit/push/merge/rebase/pull/discard through glass.git.* and glass.github.review / glass.github.ship. Do not bash git. Review remote PRs with glass.github.review before merge or push."
        }
        DevSurface::Debug => {
            "Debug playbook: glass.debug.threads and glass.debug.stack before describing a pause. Use context.debug.session / threadId / frameId. Load glass.debug.scopes / variables for the selected frame. Set breakpoints with glass.debug.breakpoint.set on the focused path. glass.debug.continue / step / pause only after a DAP event. Jump the human to a frame by reading its source path."
        }
        DevSurface::Terminal => {
            "Process playbook: glass.process.list / logs / restart for named resident processes. Prefer context.process.name. Use bash only for one-shot commands, never to replace process.start."
        }
        DevSurface::Tasks => {
            "Todo playbook: update glass.todo.write on every multi-step Agent turn. Keep at most one active item. glass.todo.complete when a step is done. glass.task.crew only when the human asks for overnight work."
        }
        DevSurface::More => {
            "Workspace playbook: glass.daemon.doctor and cockpit tools for host health. Kernels and experiments stay named glass.eval.* / glass.experiment.*."
        }
        DevSurface::Trust => "Trust playbook: do not mutate until the human trusts the workspace.",
        DevSurface::Agent => {
            "Agent playbook: inspect, write session todos, edit via proposals, test, then glass.browser.verify when UI is in scope. Use the surface playbook from context.surface when the human is on Code, App, Git, Debug, or Terminal."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_playbook_covers_review_merge_and_push() {
        let text = playbook(DevSurface::Git);
        assert!(text.contains("glass.github.review"));
        assert!(text.contains("merge"));
        assert!(text.contains("push"));
        assert!(text.contains("Do not bash git"));
    }

    #[test]
    fn editor_browser_debug_and_process_playbooks_name_glass_tools() {
        assert!(playbook(DevSurface::Code).contains("glass.editor.selection"));
        assert!(playbook(DevSurface::App).contains("glass.browser.observe"));
        assert!(playbook(DevSurface::Debug).contains("glass.debug.threads"));
        assert!(playbook(DevSurface::Terminal).contains("glass.process.list"));
        assert!(playbook(DevSurface::Tasks).contains("glass.todo.write"));
        assert!(playbook(DevSurface::Debug).contains("glass.debug.scopes"));
    }
}
