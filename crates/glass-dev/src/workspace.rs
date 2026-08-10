//! Resident development workspace ownership.

use glass_browser::development::{DevelopmentResult, ProjectWorkspace};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// One resident software-development workspace owned by Glass Dev.
///
/// The handle is deliberately distinct from the browser workspace. It remains
/// valid while browser sessions connect, fail, or are replaced and is the
/// ownership root for services moved into `glass-dev` during the 0.3.4 cycle.
pub struct DevelopmentWorkspace {
    root: PathBuf,
    project: ProjectWorkspace,
    generation: u64,
}

impl DevelopmentWorkspace {
    /// Open a project and establish generation one of its resident state.
    pub fn open(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let project = ProjectWorkspace::open(root)?;
        let root = project.root().to_path_buf();
        Ok(Self {
            root,
            project,
            generation: 1,
        })
    }

    /// Canonical project root confining all resident development services.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Current durable workspace generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Existing project runtime while its implementation is migrated into
    /// this crate service by service.
    pub fn project(&self) -> &ProjectWorkspace {
        &self.project
    }

    /// Mutable access for governed resident service operations.
    pub fn project_mut(&mut self) -> &mut ProjectWorkspace {
        &mut self.project
    }

    /// Advance the generation after replacing resident service ownership.
    pub fn advance_generation(&mut self) -> DevelopmentResult<u64> {
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            glass_browser::development::DevelopmentError::Conflict(
                "development workspace generation overflowed".into(),
            )
        })?;
        Ok(self.generation)
    }
}

/// Thread-safe resident handle shared by CLI, TUI, MCP and daemon clients.
#[derive(Clone)]
pub struct SharedDevelopmentWorkspace {
    inner: Arc<Mutex<DevelopmentWorkspace>>,
}

impl SharedDevelopmentWorkspace {
    pub fn open(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        Ok(Self {
            inner: Arc::new(Mutex::new(DevelopmentWorkspace::open(root)?)),
        })
    }

    pub fn lock(&self) -> DevelopmentResult<MutexGuard<'_, DevelopmentWorkspace>> {
        self.inner.lock().map_err(|_| {
            glass_browser::development::DevelopmentError::Conflict(
                "development workspace lock was poisoned".into(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn test_root() -> PathBuf {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "glass-dev-workspace-{}-{sequence}",
            std::process::id()
        ))
    }

    #[test]
    fn shared_workspace_preserves_project_identity_across_generations() {
        let root = test_root();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )
        .unwrap();

        let workspace = SharedDevelopmentWorkspace::open(&root).unwrap();
        let clone = workspace.clone();
        {
            let mut state = workspace.lock().unwrap();
            assert_eq!(state.generation(), 1);
            assert_eq!(state.advance_generation().unwrap(), 2);
        }
        let state = clone.lock().unwrap();
        assert_eq!(state.generation(), 2);
        assert_eq!(state.root(), std::fs::canonicalize(&root).unwrap());

        std::fs::remove_dir_all(root).unwrap();
    }
}
