//! Scoped runtimes resolve credentials over an inherited socket. The host
//! process owns the canonical store and its refresh lock.
use super::*;
#[cfg(target_os = "macos")]
use std::io::BufReader;
use std::io::{BufRead, Read, Write};
#[cfg(target_os = "macos")]
use std::os::unix::net::UnixStream;

#[derive(Serialize, Deserialize)]
pub(crate) enum Request {
    Load,
    Resolve(String),
}

fn frame<T: serde::de::DeserializeOwned>(mut reader: impl BufRead) -> Result<Option<T>, String> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((MAX_AUTH_STORE_BYTES + 65536) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "Credential broker connection failed")?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.last() != Some(&b'\n') {
        return Err("Invalid credential broker frame".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "Invalid credential broker response".into())
}

fn send(mut writer: impl Write, value: &impl Serialize) -> Result<(), String> {
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| "Cannot write credential broker frame")?;
    writer
        .write_all(b"\n")
        .and_then(|()| writer.flush())
        .map_err(|_| "Credential broker connection failed".into())
}

fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    path: &Path,
    exchange: impl Fn(&Credential) -> Result<Value, String>,
) -> Result<(), String> {
    while let Some(request) = frame(&mut input)? {
        match request {
            Request::Load => {
                let store = load_from(path).map(|mut store| {
                    for entry in store.credentials.values_mut() {
                        entry.access_token.clear();
                        entry.id_token.clear();
                        if !entry.refresh_token.is_empty() {
                            entry.refresh_token = "present".into();
                        }
                    }
                    store
                });
                send(&mut output, &store)?;
            }
            Request::Resolve(id) => {
                send(&mut output, &resolve_credential_at(path, &id, &exchange))?;
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
static CLIENT: std::sync::OnceLock<std::sync::Mutex<BufReader<UnixStream>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "macos")]
pub(crate) fn install() -> Result<(), String> {
    use std::os::fd::FromRawFd;
    let Some(fd) = std::env::var_os("GLASS_CORE_AUTH_FD") else {
        return Ok(());
    };
    let fd = fd
        .to_str()
        .and_then(|fd| fd.parse::<i32>().ok())
        .filter(|fd| *fd > 2)
        .ok_or("Invalid credential broker descriptor")?;
    // Marble passes an owned socket endpoint. Mark it close-on-exec before
    // the runtime starts so tool and extension processes cannot use it.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err("Cannot protect credential broker descriptor".into());
    }
    let socket = unsafe { UnixStream::from_raw_fd(fd) };
    CLIENT
        .set(std::sync::Mutex::new(BufReader::new(socket)))
        .map_err(|_| "Credential broker already installed".into())
}

#[cfg(target_os = "macos")]
pub(crate) fn connected() -> bool {
    CLIENT.get().is_some()
}

#[cfg(target_os = "macos")]
pub(crate) fn request<T: serde::de::DeserializeOwned>(
    request: Request,
) -> Option<Result<T, String>> {
    let stream = CLIENT.get()?;
    Some((|| {
        let mut stream = stream
            .lock()
            .map_err(|_| "Credential broker is unavailable")?;
        send(stream.get_mut(), &request)?;
        frame(&mut *stream)?.ok_or_else(|| "Credential broker disconnected".to_string())?
    })())
}

#[cfg(target_os = "macos")]
pub(crate) fn run() -> Result<(), String> {
    serve(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        &config::config_dir().join("auth.json"),
        exchange_refresh_token,
    )
}

#[cfg(test)]
#[test]
fn scoped_rotation_survives_the_broker_connection() {
    let dir = std::env::temp_dir().join(format!("glass-auth-broker-{}", random(12).unwrap()));
    let canonical = dir.join("auth.json");
    let mut store = Store::default();
    store.credentials.insert(
        "chatgpt".into(),
        Credential {
            access_token: "expired".into(),
            refresh_token: "old-refresh".into(),
            expires_at: 0,
            client_id: "fixture".into(),
            subject: String::new(),
            email: String::new(),
            id_token: String::new(),
            scopes: Vec::new(),
            flow: DEVICE_FLOW.into(),
        },
    );
    save_to(&canonical, &store).unwrap();

    let mut requests = Vec::new();
    send(&mut requests, &Request::Load).unwrap();
    send(&mut requests, &Request::Resolve("chatgpt".into())).unwrap();
    let mut response = Vec::new();
    serve(requests.as_slice(), &mut response, &canonical, |credential| {
        assert_eq!(credential.refresh_token, "old-refresh");
        Ok(serde_json::json!({
            "access_token": "fresh-access", "refresh_token": "rotated-refresh", "expires_in": 3600
        }))
    })
    .unwrap();
    let mut response = response.as_slice();
    let metadata: Result<Store, String> = frame(&mut response).unwrap().unwrap();
    assert!(
        metadata.unwrap().credentials["chatgpt"]
            .access_token
            .is_empty()
    );
    let token: Result<String, String> = frame(&mut response).unwrap().unwrap();
    assert_eq!(token.unwrap(), "fresh-access");

    let shared = load_from(&canonical).unwrap();
    assert_eq!(
        shared.credentials["chatgpt"].refresh_token,
        "rotated-refresh"
    );
    assert_eq!(
        resolve_credential_at(&canonical, "chatgpt", |_| panic!(
            "token was already refreshed"
        ))
        .unwrap(),
        "fresh-access"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
