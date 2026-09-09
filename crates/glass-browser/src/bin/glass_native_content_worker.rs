use serde_json::{Value, json};
use std::io::{self, Read, Write};

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const PROTOCOL_VERSION: u64 = 1;

fn main() {
    if run().is_err() {
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut running = false;
    loop {
        let Some(payload) = read_frame(&mut stdin)? else {
            return Ok(());
        };
        let request: Value = serde_json::from_slice(&payload)?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let kind = request
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let response = match kind {
            "ping" if request.get("protocol").and_then(Value::as_u64) == Some(PROTOCOL_VERSION) => {
                json!({"kind":"pong","id":id,"protocol":PROTOCOL_VERSION})
            }
            "start" if !running => {
                running = true;
                json!({"kind":"started","id":id})
            }
            "commit" if running => json!({"kind":"committed","id":id}),
            "close" => {
                write_frame(&mut stdout, &json!({"kind":"closed","id":id}))?;
                return Ok(());
            }
            _ => json!({"kind":"error","id":id}),
        };
        write_frame(&mut stdout, &response)?;
    }
}

fn read_frame(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds bound",
        ));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_frame(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(io::Error::other)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds bound",
        ));
    }
    let length = u32::try_from(payload.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "frame length exceeds wire bounds",
        )
    })?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}
