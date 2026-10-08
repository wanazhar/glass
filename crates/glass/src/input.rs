//! A background key reader, so the UI stays live during a turn.
//!
//! `crossterm::event::read()` blocks, which is fine while idle and useless
//! while a model is streaming: the user could not type a steer, could not open
//! the transcript, and Ctrl-C would not interrupt. So one thread owns stdin and
//! pushes key events into a channel; every other consumer drains that channel
//! non-blockingly.
//!
//! Only one thread may read stdin at a time. The main loop never does, which is
//! the whole reason this works.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use glass_tui::crossterm::event::Event;

/// Handle to the reader thread.
pub struct KeyReader {
    rx: Receiver<Event>,
    stop: Arc<AtomicBool>,
    /// Kept so the thread outlives every clone of this struct.
    _handle: Option<std::thread::JoinHandle<()>>,
}

impl KeyReader {
    /// Spawn the reader. Should be called once, after raw mode is enabled.
    pub fn spawn() -> Self {
        let (tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            loop {
                if stop_thread.load(Ordering::Relaxed) {
                    return;
                }
                // `poll` with a timeout rather than a blocking read, so the stop
                // flag is observed promptly instead of only after the next key.
                match glass_tui::crossterm::event::poll(std::time::Duration::from_millis(120)) {
                    Ok(true) => match glass_tui::crossterm::event::read() {
                        Ok(ev) => {
                            if tx.send(ev).is_err() {
                                return; // receiver dropped, reader is useless
                            }
                        }
                        Err(e) => {
                            let _ =
                                writeln!(std::io::stderr(), "\r\x1b[31minput error: {e}\x1b[0m");
                            return;
                        }
                    },
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
        });

        Self {
            rx,
            stop,
            _handle: Some(handle),
        }
    }

    /// Take a pending event without blocking.
    pub fn try_next(&self) -> Option<Event> {
        match self.rx.try_recv() {
            Ok(ev) => Some(ev),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn next_timeout(&self, timeout: std::time::Duration) -> Option<Event> {
        self.rx.recv_timeout(timeout).ok()
    }

    pub fn wait_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Option<Event>, std::sync::mpsc::RecvTimeoutError> {
        match self.rx.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Block until the next event. Only for the idle path, where blocking is
    /// what we want and saves a spin.
    pub fn next_blocking(&self) -> Option<Event> {
        self.rx.recv().ok()
    }

    /// Stop the reader. Called before leaving raw mode so the thread cannot
    /// read from a terminal that is about to be handed back.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self._handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for KeyReader {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_next_is_empty_when_nothing_pending() {
        let (_tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let reader = KeyReader {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            _handle: None,
        };
        assert!(reader.try_next().is_none());
    }

    #[test]
    fn try_next_drains_a_queued_event() {
        let (tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let reader = KeyReader {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            _handle: None,
        };
        tx.send(Event::FocusGained).unwrap();
        assert!(reader.try_next().is_some());
        assert!(
            reader.try_next().is_none(),
            "must not redeliver the same event"
        );
    }
}
