//! Bounded client for Herdr's experimental owned pane-graphics stream.

use std::io;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;
use std::time::Duration;

const FRAME_QUEUE_CAPACITY: usize = 1;
const MAX_ENV_VALUE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdrEnvironment {
    pub socket_path: String,
    pub pane_id: String,
}

impl HerdrEnvironment {
    pub fn from_process() -> Option<Self> {
        if std::env::var("HERDR_ENV").ok().as_deref() != Some("1") {
            return None;
        }
        let socket_path = std::env::var("HERDR_SOCKET_PATH").ok()?;
        let pane_id = std::env::var("HERDR_PANE_ID").ok()?;
        if socket_path.is_empty()
            || pane_id.is_empty()
            || socket_path.len() > MAX_ENV_VALUE_BYTES
            || pane_id.len() > 256
        {
            return None;
        }
        Some(Self {
            socket_path,
            pane_id,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HerdrFrame {
    pub png: Vec<u8>,
    pub image_width: u32,
    pub image_height: u32,
    pub viewport_col: i32,
    pub viewport_row: i32,
    pub grid_cols: u32,
    pub grid_rows: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HerdrEvent {
    Connected,
    Failed(String),
    Stopped,
}

pub struct HerdrGraphicsWorker {
    frames: Option<SyncSender<HerdrFrame>>,
    events: Receiver<HerdrEvent>,
    join: Option<thread::JoinHandle<()>>,
}

impl HerdrGraphicsWorker {
    pub fn spawn(environment: HerdrEnvironment) -> Self {
        let (frame_tx, frame_rx) = mpsc::sync_channel(FRAME_QUEUE_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel();
        let join = thread::spawn(move || {
            if let Err(error) = run_stream(&environment, frame_rx, &event_tx) {
                let _ = event_tx.send(HerdrEvent::Failed(error.to_string()));
            }
            let _ = event_tx.send(HerdrEvent::Stopped);
        });
        Self {
            frames: Some(frame_tx),
            events: event_rx,
            join: Some(join),
        }
    }

    /// Queue only the newest frame. `false` means the previous frame is still
    /// waiting for the local Herdr socket and this frame was dropped.
    pub fn try_send(&self, frame: HerdrFrame) -> bool {
        let Some(frames) = self.frames.as_ref() else {
            return false;
        };
        match frames.try_send(frame) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn try_event(&self) -> Option<HerdrEvent> {
        self.events.try_recv().ok()
    }

    pub fn stop(&mut self) -> io::Result<()> {
        self.frames.take();
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| io::Error::other("Herdr graphics worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for HerdrGraphicsWorker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(unix)]
fn run_stream(
    environment: &HerdrEnvironment,
    frames: Receiver<HerdrFrame>,
    events: &mpsc::Sender<HerdrEvent>,
) -> io::Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(&environment.socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = serde_json::json!({
        "id": "glass_live",
        "method": "pane.graphics.stream",
        "params": {"pane_id": environment.pane_id}
    });
    serde_json::to_writer(&mut stream, &request)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut response = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut response)?;
    let response: serde_json::Value = serde_json::from_str(response.trim()).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Herdr response: {error}"),
        )
    })?;
    if response.get("result").is_none() || response.get("error").is_some() {
        let message = response
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Herdr rejected pane.graphics.stream");
        return Err(io::Error::other(message));
    }
    let _ = events.send(HerdrEvent::Connected);

    while let Ok(frame) = frames.recv() {
        if frame.png.is_empty() {
            continue;
        }
        let header = serde_json::json!({
            "format": "png",
            "image_width": frame.image_width,
            "image_height": frame.image_height,
            "data_length": frame.png.len(),
            "placement": {
                "viewport_col": frame.viewport_col,
                "viewport_row": frame.viewport_row,
                "grid_cols": frame.grid_cols,
                "grid_rows": frame.grid_rows
            }
        });
        serde_json::to_writer(&mut stream, &header)?;
        stream.write_all(b"\n")?;
        stream.write_all(&frame.png)?;
        stream.flush()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn run_stream(
    _environment: &HerdrEnvironment,
    _frames: Receiver<HerdrFrame>,
    _events: &mpsc::Sender<HerdrEvent>,
) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Herdr pane graphics currently require a Unix local socket",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixListener;

    #[test]
    fn streams_raw_png_frame_and_closes_cleanly() {
        let directory = tempfile_dir();
        let socket = directory.join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert!(request.contains("pane.graphics.stream"));
            stream
                .write_all(b"{\"id\":\"glass_live\",\"result\":{}}\n")
                .unwrap();
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            let header: serde_json::Value = serde_json::from_str(&header).unwrap();
            assert_eq!(header["data_length"], 3);
            let mut png = [0; 3];
            reader.read_exact(&mut png).unwrap();
            assert_eq!(&png, b"png");
        });
        let mut worker = HerdrGraphicsWorker::spawn(HerdrEnvironment {
            socket_path: socket.display().to_string(),
            pane_id: "w1:p1".into(),
        });
        assert_eq!(
            worker.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            HerdrEvent::Connected
        );
        assert!(worker.try_send(HerdrFrame {
            png: b"png".to_vec(),
            image_width: 1,
            image_height: 1,
            viewport_col: 0,
            viewport_row: 0,
            grid_cols: 1,
            grid_rows: 1,
        }));
        worker.stop().unwrap();
        server.join().unwrap();
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let path = std::path::PathBuf::from(format!("/tmp/gh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
