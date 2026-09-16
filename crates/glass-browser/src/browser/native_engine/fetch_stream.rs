//! Demand-driven response streams shared by page and worker realms.
//!
//! The content owner keeps the network task behind a bounded command/event
//! channel. The JavaScript realm controls when the next network read is
//! admitted, so a slow script consumer cannot make the native host buffer an
//! unbounded response.

use futures_util::StreamExt;
use std::collections::VecDeque;
use tokio::sync::mpsc;

use super::interaction::MAX_NATIVE_EFFECTS;

pub(crate) const MAX_NATIVE_FETCH_STREAM_CONNECTIONS: usize = MAX_NATIVE_EFFECTS;
pub(crate) const MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES: usize = 8 * 1024;

pub(crate) enum NativeFetchStreamCommand {
    Read,
    Cancel,
}

pub(crate) enum NativeFetchStreamEvent {
    Chunk { data: Vec<u8> },
    End,
    Error { message: String },
}

pub(crate) struct NativeFetchStreamConnection {
    pub(crate) commands: mpsc::Sender<NativeFetchStreamCommand>,
    pub(crate) events: mpsc::Receiver<NativeFetchStreamEvent>,
    pub(crate) read_pending: bool,
}

pub(crate) fn spawn_native_fetch_stream(
    response: reqwest::Response,
    max_response_bytes: usize,
) -> NativeFetchStreamConnection {
    let (command_sender, command_receiver) = mpsc::channel(MAX_NATIVE_FETCH_STREAM_CONNECTIONS);
    let (event_sender, event_receiver) = mpsc::channel(MAX_NATIVE_FETCH_STREAM_CONNECTIONS);
    tokio::spawn(run_native_fetch_stream(
        response,
        max_response_bytes,
        command_receiver,
        event_sender,
    ));
    NativeFetchStreamConnection {
        commands: command_sender,
        events: event_receiver,
        read_pending: false,
    }
}

async fn run_native_fetch_stream(
    response: reqwest::Response,
    max_response_bytes: usize,
    mut commands: mpsc::Receiver<NativeFetchStreamCommand>,
    events: mpsc::Sender<NativeFetchStreamEvent>,
) {
    let mut stream = response.bytes_stream();
    let mut total_bytes = 0usize;
    let mut pending_parts = VecDeque::new();
    let mut read_requested = false;
    loop {
        if !read_requested {
            match commands.recv().await {
                Some(NativeFetchStreamCommand::Read) => read_requested = true,
                Some(NativeFetchStreamCommand::Cancel) | None => return,
            }
        }
        if let Some(part) = pending_parts.pop_front() {
            read_requested = false;
            if events
                .send(NativeFetchStreamEvent::Chunk { data: part })
                .await
                .is_err()
            {
                return;
            }
            continue;
        }
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(NativeFetchStreamCommand::Read) => {}
                    Some(NativeFetchStreamCommand::Cancel) | None => return,
                }
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(chunk)) => {
                        let next_total = total_bytes.saturating_add(chunk.len());
                        if next_total > max_response_bytes {
                            let _ = events.send(NativeFetchStreamEvent::Error {
                                message: "native fetch response stream exceeds its limit".into(),
                            }).await;
                            return;
                        }
                        total_bytes = next_total;
                        pending_parts.extend(
                            chunk
                                .chunks(MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES)
                                .map(|part| part.to_vec()),
                        );
                    }
                    Some(Err(error)) => {
                        let mut message = error.to_string();
                        if message.len() > crate::browser_backend::MAX_TEXT_BYTES {
                            message.truncate(crate::browser_backend::MAX_TEXT_BYTES);
                        }
                        if events
                            .send(NativeFetchStreamEvent::Error { message })
                            .await
                            .is_err()
                        {
                            return;
                        }
                        while let Some(command) = commands.recv().await {
                            if matches!(command, NativeFetchStreamCommand::Cancel) {
                                return;
                            }
                        }
                        return;
                    }
                    None => {
                        if events.send(NativeFetchStreamEvent::End).await.is_err() {
                            return;
                        }
                        while let Some(command) = commands.recv().await {
                            if matches!(command, NativeFetchStreamCommand::Cancel) {
                                return;
                            }
                        }
                        return;
                    }
                }
            }
        }
    }
}
