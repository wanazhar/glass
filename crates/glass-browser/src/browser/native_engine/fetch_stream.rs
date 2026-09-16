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
pub(crate) const MAX_NATIVE_FETCH_UPLOAD_CHUNKS: usize = MAX_NATIVE_EFFECTS * 4;

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

/// Commands sent by the serialized JavaScript owner to a request-body
/// producer. A producer emits one demand before each command so the HTTP
/// client never reads ahead of the page's stream consumer.
pub(crate) enum NativeFetchUploadCommand {
    Chunk { data: Vec<u8> },
    End,
    Error { message: String },
    Cancel,
}

pub(crate) enum NativeFetchUploadEvent {
    Demand,
}

pub(crate) struct NativeFetchUploadConnection {
    pub(crate) commands: mpsc::Sender<NativeFetchUploadCommand>,
    pub(crate) events: mpsc::Receiver<NativeFetchUploadEvent>,
    pub(crate) demand_pending: bool,
    pub(crate) chunk_count: usize,
    pub(crate) total_bytes: usize,
}

/// The receiving half of a page- or worker-owned request-body stream.
///
/// A network request turns this source into a `reqwest::Body`; a controlled
/// Service Worker request instead consumes it into one bounded replayable
/// body before entering the worker's normal FetchEvent bridge. Keeping the
/// source separate prevents either path from inventing a second producer
/// protocol.
pub(crate) struct NativeFetchUploadSource {
    commands: mpsc::Receiver<NativeFetchUploadCommand>,
    events: mpsc::Sender<NativeFetchUploadEvent>,
}

impl NativeFetchUploadSource {
    pub(crate) fn into_body(self) -> reqwest::Body {
        let stream =
            futures_util::stream::unfold((self, false), |(mut source, terminated)| async move {
                if terminated
                    || source
                        .events
                        .send(NativeFetchUploadEvent::Demand)
                        .await
                        .is_err()
                {
                    return None;
                }
                let next = match source.commands.recv().await {
                    Some(NativeFetchUploadCommand::Chunk { data }) => (Ok(data), false),
                    Some(NativeFetchUploadCommand::End) | None => return None,
                    Some(NativeFetchUploadCommand::Error { message }) => {
                        (Err(std::io::Error::other(message)), true)
                    }
                    Some(NativeFetchUploadCommand::Cancel) => (
                        Err(std::io::Error::other(
                            "native fetch request body was cancelled",
                        )),
                        true,
                    ),
                };
                Some((next.0, (source, next.1)))
            });
        reqwest::Body::wrap_stream(stream)
    }

    /// Consume a request body for a controlled Service Worker handoff.
    ///
    /// The content process already validates each command before forwarding
    /// it here. These limits remain at the receiving boundary as a second
    /// line of defense if another producer is added later.
    pub(crate) async fn collect(mut self, max_bytes: usize) -> Result<Vec<u8>, String> {
        let mut body = Vec::new();
        let mut chunk_count = 0usize;
        loop {
            self.events
                .send(NativeFetchUploadEvent::Demand)
                .await
                .map_err(|_| "native fetch request upload consumer is unavailable".to_owned())?;
            match self.commands.recv().await {
                Some(NativeFetchUploadCommand::Chunk { data }) => {
                    chunk_count = chunk_count.saturating_add(1);
                    if chunk_count > MAX_NATIVE_FETCH_UPLOAD_CHUNKS {
                        return Err("native fetch request upload chunk limit exceeded".into());
                    }
                    let next_len = body.len().saturating_add(data.len());
                    if next_len > max_bytes {
                        return Err("native fetch request upload body exceeds its limit".into());
                    }
                    body.extend_from_slice(&data);
                }
                Some(NativeFetchUploadCommand::End) => return Ok(body),
                Some(NativeFetchUploadCommand::Error { message }) => return Err(message),
                Some(NativeFetchUploadCommand::Cancel) => {
                    return Err("native fetch request body was cancelled".into());
                }
                None => {
                    return Err("native fetch request upload producer closed unexpectedly".into());
                }
            }
        }
    }
}

pub(crate) fn spawn_native_fetch_upload_source()
-> (NativeFetchUploadConnection, NativeFetchUploadSource) {
    let (command_sender, command_receiver) = mpsc::channel(MAX_NATIVE_FETCH_STREAM_CONNECTIONS);
    let (event_sender, event_receiver) = mpsc::channel(1);
    (
        NativeFetchUploadConnection {
            commands: command_sender,
            events: event_receiver,
            demand_pending: false,
            chunk_count: 0,
            total_bytes: 0,
        },
        NativeFetchUploadSource {
            commands: command_receiver,
            events: event_sender,
        },
    )
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

/// Create the same demand-driven transport for a bounded local response.
/// Fixture-owned documents do not have a reqwest response, but they still
/// need to exercise the exact stream ownership, chunking, and cancellation
/// contract used by the process-backed network path.
pub(crate) fn spawn_native_fetch_bytes_stream(
    body: Vec<u8>,
    max_response_bytes: usize,
) -> NativeFetchStreamConnection {
    let (command_sender, command_receiver) = mpsc::channel(MAX_NATIVE_FETCH_STREAM_CONNECTIONS);
    let (event_sender, event_receiver) = mpsc::channel(MAX_NATIVE_FETCH_STREAM_CONNECTIONS);
    tokio::spawn(run_native_fetch_bytes_stream(
        body,
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

/// Create a bounded request body whose next byte chunk is admitted by the
/// content process. The body is intentionally an ordinary reqwest stream so
/// redirects, timeouts, and transport errors remain owned by the HTTP client.
pub(crate) fn spawn_native_fetch_upload_stream() -> (NativeFetchUploadConnection, reqwest::Body) {
    let (connection, source) = spawn_native_fetch_upload_source();
    (connection, source.into_body())
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

async fn run_native_fetch_bytes_stream(
    body: Vec<u8>,
    max_response_bytes: usize,
    mut commands: mpsc::Receiver<NativeFetchStreamCommand>,
    events: mpsc::Sender<NativeFetchStreamEvent>,
) {
    if body.len() > max_response_bytes {
        let _ = events
            .send(NativeFetchStreamEvent::Error {
                message: "native fetch response stream exceeds its limit".into(),
            })
            .await;
        return;
    }
    let mut pending_parts = body
        .chunks(MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES)
        .map(|part| part.to_vec())
        .collect::<VecDeque<_>>();
    loop {
        match commands.recv().await {
            Some(NativeFetchStreamCommand::Read) => {}
            Some(NativeFetchStreamCommand::Cancel) | None => return,
        }
        if let Some(part) = pending_parts.pop_front() {
            if events
                .send(NativeFetchStreamEvent::Chunk { data: part })
                .await
                .is_err()
            {
                return;
            }
            continue;
        }
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
