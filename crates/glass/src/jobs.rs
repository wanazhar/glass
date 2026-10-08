//! Explicitly owned background process groups; output is kept on disk.
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
pub struct Job {
    child: Child,
    command: String,
    output: PathBuf,
}
#[derive(Default)]
pub struct Jobs {
    jobs: BTreeMap<u32, Job>,
    next: u32,
}
impl Jobs {
    pub fn start(&mut self, command: &str, workspace: &Path, dir: &Path) -> Result<String, String> {
        if command.trim().is_empty() {
            return Err("background command is empty".into());
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        self.next += 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let output = dir.join(format!("job-{}-{timestamp}.log", self.next));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|e| e.to_string())?;
        if let Some(reason) = crate::sandbox::refusal() {
            return Err(format!(
                "sandbox = strict is not available ({reason}); refusing to run jobs unsandboxed"
            ));
        }
        let mut cmd = crate::sandbox::shell(command);
        cmd.current_dir(workspace)
            .stdin(Stdio::null())
            .stdout(file.try_clone().map_err(|e| e.to_string())?)
            .stderr(file);
        #[cfg(unix)]
        cmd.process_group(0);
        crate::sandbox::harden(&mut cmd);
        let child = cmd.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        self.jobs.insert(
            self.next,
            Job {
                child,
                command: command.into(),
                output: output.clone(),
            },
        );
        Ok(format!("job {} pid {pid}: {}", self.next, output.display()))
    }
    pub fn list(&mut self) -> String {
        self.jobs
            .iter_mut()
            .map(|(id, j)| {
                let status = match j.child.try_wait() {
                    Ok(Some(s)) => s.to_string(),
                    Ok(None) => "running".into(),
                    Err(e) => e.to_string(),
                };
                format!("{id}: {status} — {} — {}", j.command, j.output.display())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn output(&self, id: u32) -> Result<String, String> {
        use std::io::{Read, Seek, SeekFrom};
        let j = self.jobs.get(&id).ok_or("unknown job")?;
        let mut f = std::fs::File::open(&j.output).map_err(|e| e.to_string())?;
        let size = f.metadata().map_err(|e| e.to_string())?.len();
        f.seek(SeekFrom::Start(size.saturating_sub(60_000)))
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        f.take(60_000)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    pub fn stop(&mut self, id: u32) -> Result<String, String> {
        let j = self.jobs.get_mut(&id).ok_or("unknown job")?;
        if j.child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Ok(format!("job {id} already exited"));
        }
        #[cfg(unix)]
        {
            let result = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", j.child.id())])
                .output()
                .map_err(|e| e.to_string())?;
            if !result.status.success() && j.child.try_wait().map_err(|e| e.to_string())?.is_none()
            {
                return Err(String::from_utf8_lossy(&result.stderr).into());
            }
        }
        #[cfg(not(unix))]
        j.child.kill().map_err(|e| e.to_string())?;
        let _ = j.child.wait();
        Ok(format!("stopped job {id}"))
    }
}
impl Drop for Jobs {
    fn drop(&mut self) {
        let ids: Vec<_> = self.jobs.keys().copied().collect();
        for id in ids {
            let _ = self.stop(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_output_is_available_and_stop_reaps_the_owned_process() {
        let dir = std::env::temp_dir().join(format!("glass-jobs-{}", std::process::id()));
        let mut jobs = Jobs::default();
        jobs.start("echo ready; sleep 30", &std::env::temp_dir(), &dir)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !jobs.output(1).unwrap().contains("ready") {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(jobs.list().contains("running"));
        jobs.stop(1).unwrap();
        assert!(
            jobs.jobs
                .get_mut(&1)
                .unwrap()
                .child
                .try_wait()
                .unwrap()
                .is_some()
        );
        assert!(jobs.output(1).unwrap().contains("ready"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
