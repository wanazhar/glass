//! One instance per data directory.
//!
//! Marble keeps its durable document in memory and writes the whole revision on
//! every change, so two processes serving the same data directory would each
//! overwrite the other's work. The service definition restarts Marble
//! automatically, and a person can start one by hand, so this is a real failure
//! mode rather than a theoretical one: the second process is refused instead of
//! silently forking the history.
//!
//! The lock is a file with this process's pid. A pid that is no longer running
//! is a crash left behind, not an owner, so a stale lock is taken over rather
//! than requiring the operator to delete a file by hand. `restore` takes the
//! same lock, which is how it refuses to write underneath a running service.

use crate::MarbleError;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Lock file name inside the data directory.
pub const LOCK_FILE: &str = "marble.lock";

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// Whether `pid` still names a running process.
///
/// `kill -0` only answers "is this pid taken", which is exactly the question.
/// A platform without `kill` reports the pid as gone, so the worst case is that
/// a stale lock is taken over; the common case on a host that runs Marble is a
/// Unix-like system where the answer is accurate.
fn pid_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn read_pid(path: &Path) -> Option<u32> {
    let contents = fs::read_to_string(path).ok()?;
    contents
        .lines()
        .next()
        .and_then(|line| line.trim().strip_prefix("pid="))
        .and_then(|pid| pid.trim().parse::<u32>().ok())
}

/// An acquired claim on one data directory, released when dropped.
#[derive(Debug)]
pub struct DataDirLock {
    path: PathBuf,
    pid: u32,
}

impl DataDirLock {
    /// Claim `data_dir` for this process.
    ///
    /// A held lock is an error that names the owning pid; a lock whose owner is
    /// gone is replaced. If another process wins the replacement race, that is
    /// reported as held too, never overwritten.
    pub fn acquire(data_dir: &Path) -> Result<Self, MarbleError> {
        fs::create_dir_all(data_dir).map_err(MarbleError::Io)?;
        let path = data_dir.join(LOCK_FILE);
        let pid = std::process::id();
        match Self::claim(&path, pid) {
            Ok(()) => Ok(Self { path, pid }),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let owner = read_pid(&path);
                // A live owner holds it, including this same process asking a
                // second time: two servers in one process would still race.
                if let Some(owner) = owner
                    && pid_is_alive(owner)
                {
                    return Err(MarbleError::Protocol(format!(
                        "another glass-marble is already serving this data directory (pid {owner}, lock {}). \
                         Stop it first, or point --data-dir somewhere else.",
                        path.display()
                    )));
                }
                tracing::warn!(
                    lock = %path.display(),
                    pid = owner.unwrap_or_default(),
                    "taking over a lock left by a process that is no longer running"
                );
                let _ = fs::remove_file(&path);
                Self::claim(&path, pid).map_err(|error| {
                    MarbleError::Protocol(format!(
                        "cannot claim {}: {error}; another glass-marble may have started at the same time",
                        path.display()
                    ))
                })?;
                Ok(Self { path, pid })
            }
            Err(error) => Err(MarbleError::Io(error)),
        }
    }

    fn claim(path: &Path, pid: u32) -> std::io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        writeln!(file, "pid={pid}")?;
        writeln!(file, "started={}", unix_millis())?;
        file.flush()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Whether this process still owns the lock file it created.
    pub fn held(&self) -> bool {
        read_pid(&self.path) == Some(self.pid)
    }
}

impl Drop for DataDirLock {
    fn drop(&mut self) {
        // Only remove the lock this process wrote: a lock that was taken over by
        // a newer instance must not be deleted on the way out.
        if self.held() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("glass-marble-lock-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_second_instance_is_refused_while_the_first_holds_the_lock() {
        let dir = data_dir("held");
        let held = DataDirLock::acquire(&dir).expect("the first claim succeeds");
        assert!(held.held());
        let refused = DataDirLock::acquire(&dir);
        assert!(refused.is_err(), "a second instance must not start");
        let message = refused.unwrap_err().to_string();
        assert!(
            message.contains(&std::process::id().to_string()),
            "the refusal names the owner: {message}"
        );
        drop(held);
        let again = DataDirLock::acquire(&dir).expect("the lock is free again");
        drop(again);
        assert!(!dir.join(LOCK_FILE).exists(), "the lock is cleaned up");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lock_left_by_a_dead_process_is_taken_over() {
        let dir = data_dir("stale");
        let path = dir.join(LOCK_FILE);
        // No process is likely to own this pid, and `kill -0` will say so.
        fs::write(&path, "pid=4294967290\nstarted=1\n").unwrap();
        let lock = DataDirLock::acquire(&dir).expect("a stale lock is not an owner");
        assert!(lock.held());
        assert_eq!(read_pid(&path), Some(std::process::id()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_lock_file_is_replaced_rather_than_trusted() {
        let dir = data_dir("unreadable");
        fs::write(dir.join(LOCK_FILE), "not a lock file").unwrap();
        let lock = DataDirLock::acquire(&dir).expect("an ownerless lock is stale");
        assert!(lock.held());
        let _ = fs::remove_dir_all(&dir);
    }
}
