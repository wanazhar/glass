//! Small private account records coordinate separate GC processes on this host.
use super::{Error, pacing::State};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    version: u8,
    last: u64,
    previous: u64,
    cooldown: u64,
    #[serde(default)]
    server_cooldown: u64,
    gap: u64,
    healthy: u8,
}
pub(super) struct Shared {
    file: File,
    record: Record,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl Shared {
    pub(super) fn open(key: [u8; 32]) -> Result<Option<Self>, Error> {
        // Unit tests use deterministic in-memory state; binary tests exercise locks.
        if cfg!(test) {
            return Ok(None);
        }
        let operation = || -> std::io::Result<Self> {
            let dir = crate::config::data_dir().join("provider-pacing");
            std::fs::create_dir_all(&dir)?;
            if std::fs::symlink_metadata(&dir)?.file_type().is_symlink() {
                return Err(std::io::Error::other(
                    "pacing directory must not be a symlink",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let name: String = crate::hashing::hex(&key);
            let path = dir.join(format!("{name}.json"));
            if !path.exists() {
                let mut count = 0;
                for entry in std::fs::read_dir(&dir)?.take(257) {
                    let entry = entry?;
                    if entry.file_type()?.is_file()
                        && entry.path().extension().is_some_and(|e| e == "json")
                    {
                        if entry.metadata()?.modified()?.elapsed().unwrap_or_default()
                            > Duration::from_secs(86400)
                        {
                            let _ = std::fs::remove_file(entry.path());
                        } else {
                            count += 1;
                        }
                    }
                }
                if count >= 128 {
                    return Err(std::io::Error::other("shared pacing account limit reached"));
                }
            }
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let mut file = options.open(path)?;
            if !file.metadata()?.is_file() {
                return Err(std::io::Error::other(
                    "pacing record must be a regular file",
                ));
            }
            loop {
                match file.try_lock_exclusive() {
                    Ok(()) => break,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if super::cancellable::is_cancelled() {
                            return Err(std::io::Error::other("provider pacing cancelled"));
                        }
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Err(error) => return Err(error),
                }
            }
            let mut bytes = Vec::new();
            (&mut file).take(4096).read_to_end(&mut bytes)?;
            let record = serde_json::from_slice(&bytes).unwrap_or_default();
            Ok(Self { file, record })
        };
        operation().map(Some).map_err(|error| {
            if super::cancellable::is_cancelled() {
                Error::Transport("provider pacing cancelled".into())
            } else {
                Error::Config(format!("provider pacing state: {error}"))
            }
        })
    }
    pub(super) fn load(&self, state: &mut State) {
        let now = Instant::now();
        let wall = now_ms();
        let past = |value: u64| {
            (value > 0 && value <= wall)
                .then(|| {
                    now.checked_sub(Duration::from_millis(
                        wall.saturating_sub(value).min(86400000),
                    ))
                })
                .flatten()
        };
        state.last = past(self.record.last);
        state.previous = past(self.record.previous);
        state.cooldown = (self.record.cooldown > wall && self.record.cooldown - wall <= 86400000)
            .then(|| now + Duration::from_millis((self.record.cooldown - wall).min(86400000)));
        let server = if self.record.version == 0 {
            self.record.cooldown.max(self.record.server_cooldown)
        } else {
            self.record.server_cooldown
        };
        state.server_cooldown = (server > wall && server - wall <= 86400000)
            .then(|| now + Duration::from_millis(server - wall));
        state.gap = Duration::from_millis(self.record.gap.min(60000));
        state.healthy = self.record.healthy.min(7);
    }
    pub(super) fn save(&mut self, state: &State) -> Result<(), Error> {
        let wall = now_ms();
        let now = Instant::now();
        let past = |value: Option<Instant>| {
            value
                .map(|t| wall.saturating_sub(now.saturating_duration_since(t).as_millis() as u64))
                .unwrap_or(0)
        };
        self.record = Record {
            version: 1,
            last: past(state.last),
            previous: past(state.previous),
            cooldown: state
                .cooldown
                .map(|t| wall + t.saturating_duration_since(now).as_millis() as u64)
                .unwrap_or(0),
            server_cooldown: state
                .server_cooldown
                .map(|t| wall + t.saturating_duration_since(now).as_millis() as u64)
                .unwrap_or(0),
            gap: state.gap.as_millis() as u64,
            healthy: state.healthy,
        };
        let write = || -> std::io::Result<()> {
            self.file.seek(SeekFrom::Start(0))?;
            self.file.set_len(0)?;
            serde_json::to_writer(&mut self.file, &self.record)?;
            self.file.flush()
        };
        let mut write = write;
        write().map_err(|error| Error::Config(format!("provider pacing state: {error}")))
    }
}
