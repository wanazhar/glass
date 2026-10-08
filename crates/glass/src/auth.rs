//! Glass-core owns its credentials. No other harness's tokens are imported.
use crate::{config, input::KeyReader, modal};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
    signature,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
#[derive(Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub credential: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Credential {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub id_token: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub host_id: String,
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub credentials: BTreeMap<String, Credential>,
}
pub fn load() -> Result<Store, String> {
    match std::fs::read(config::config_dir().join("auth.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "auth.json is invalid; credentials have not been overwritten".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(format!("cannot read auth.json: {e}")),
    }
}
pub fn secure_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("missing credential directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(".auth-{}.tmp", random(12)?));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut f = options.open(&tmp).map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
pub fn save(store: &Store) -> Result<(), String> {
    secure_write(
        &config::config_dir().join("auth.json"),
        &serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?,
    )
}
/// Update the store under an OS lock; a crashed process releases it automatically.
pub fn edit<T>(update: impl FnOnce(&mut Store) -> Result<T, String>) -> Result<T, String> {
    std::fs::create_dir_all(config::config_dir()).map_err(|e| e.to_string())?;
    let _lock = RefreshLock::acquire(&config::config_dir().join(".auth.lock"))?;
    let mut store = load()?;
    let result = update(&mut store)?;
    save(&store)?;
    Ok(result)
}
pub fn apply(cfg: &mut config::Config) -> Result<(), String> {
    let store = load()?;
    if let Some(c) = store
        .connections
        .iter()
        .find(|c| Some(&c.id) == store.active.as_ref())
    {
        configure(cfg, c);
    }
    Ok(())
}
pub fn configure(cfg: &mut config::Config, c: &Connection) {
    cfg.provider = c.provider.clone();
    cfg.base_url = c.base_url.clone();
    cfg.model = c.model.clone();
    cfg.api_key_env = c.api_key_env.clone();
    cfg.connection = Some(c.id.clone());
}
pub fn current<'a>(store: &'a Store, cfg: &config::Config) -> Option<&'a Connection> {
    store.connections.iter().find(|c| {
        Some(&c.id) == cfg.connection.as_ref()
            && c.provider == cfg.provider
            && c.base_url == cfg.base_url
    })
}
pub fn status(cfg: &config::Config) -> String {
    match load() {
        Err(e) => e,
        Ok(store) => {
            if let Some(id) = &cfg.connection
                && !store.connections.iter().any(|c| &c.id == id)
            {
                return "Saved connection was removed · /login to reconnect".into();
            }
            let c = current(&store, cfg);
            if let Some(cred) = c
                .and_then(|c| c.credential.as_ref())
                .and_then(|id| store.credentials.get(id))
            {
                if !cred.client_id.is_empty() {
                    return format!(
                        "ChatGPT · {} · {}",
                        cred.email,
                        if cred.refresh_token.is_empty() && cred.expires_at <= now() {
                            "sign in again"
                        } else {
                            "connected"
                        }
                    );
                }
                return format!("{} · saved API key", c.unwrap().label);
            }
            if let Some(name) = &cfg.api_key_env {
                if std::env::var(name).is_ok_and(|s| !s.trim().is_empty()) {
                    format!("Connected via {name}")
                } else {
                    format!("Not connected · {name} is missing · /login to set up")
                }
            } else {
                "Local/custom endpoint · no authentication configured".into()
            }
        }
    }
}
pub fn is_chatgpt(cfg: &config::Config) -> bool {
    if cfg.connection.is_none() {
        return false;
    }
    load()
        .ok()
        .and_then(|store| {
            current(&store, cfg)
                .and_then(|c| c.credential.as_ref())
                .and_then(|id| store.credentials.get(id))
                .map(|c| !c.client_id.is_empty())
        })
        .unwrap_or(false)
}

pub fn needs_refresh(cfg: &config::Config) -> bool {
    load()
        .ok()
        .and_then(|store| {
            current(&store, cfg)
                .and_then(|c| c.credential.as_ref())
                .and_then(|id| store.credentials.get(id))
                .map(|c| !c.client_id.is_empty() && c.expires_at <= now() + 90)
        })
        .unwrap_or(false)
}

pub fn resolve(cfg: &config::Config) -> Result<Option<String>, String> {
    if let Some(id) = &cfg.connection
        && !load()?.connections.iter().any(|c| &c.id == id)
    {
        return Err("Saved connection was removed. Use /login to reconnect.".into());
    }
    if let Some(key) = cfg.api_key() {
        return Ok(Some(key));
    }
    let store = load()?;
    if let Some(id) = current(&store, cfg).and_then(|c| c.credential.clone()) {
        return resolve_credential(&id).map(Some);
    }
    if let Some(name) = &cfg.api_key_env {
        Err(format!(
            "Not connected: {name} is missing. Open /login to choose a provider and sign in."
        ))
    } else {
        Ok(None)
    }
}
pub fn resolve_credential(id: &str) -> Result<String, String> {
    let store = load()?;
    let cred = store
        .credentials
        .get(id)
        .ok_or("Saved credentials were removed. Use /login to reconnect.")?;
    if cred.client_id.is_empty() || cred.expires_at > now() + 90 {
        return Ok(cred.access_token.clone());
    }
    let key = edit(|store| {
        let mut cred = store
            .credentials
            .get(id)
            .cloned()
            .ok_or("Connection was signed out. Use /login to reconnect.")?;
        if cred.expires_at <= now() + 90 {
            if cred.refresh_token.is_empty() {
                return Err("ChatGPT session expired. Use /login to reconnect.".into());
            }
            let token = match token_form(&[
                ("grant_type", "refresh_token"),
                ("client_id", &cred.client_id),
                ("refresh_token", &cred.refresh_token),
                ("resource", RESOURCE),
            ]) {
                Ok(token) => token,
                Err(e) if e.starts_with("ChatGPT session expired or was revoked") => {
                    cred.refresh_token.clear();
                    cred.access_token.clear();
                    store.credentials.insert(id.into(), cred);
                    return Ok(String::new());
                }
                Err(e) => return Err(e),
            };
            if let Some(id_token) = token["id_token"].as_str() {
                let (subject, email) = validate_identity(id_token, &cred.client_id, "")?;
                if subject != cred.subject {
                    return Err(
                        "Refreshed ChatGPT identity changed. Use /login to reconnect.".into(),
                    );
                }
                cred.email = email;
            }
            update_tokens(&mut cred, &token, false)?;
            store.credentials.insert(id.into(), cred.clone());
        }
        Ok(cred.access_token)
    })?;
    if key.is_empty() {
        Err("ChatGPT session expired or was revoked. Use /login to reconnect.".into())
    } else {
        Ok(key)
    }
}
struct RefreshLock(std::fs::File);
impl RefreshLock {
    fn acquire(path: &Path) -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|_| "Cannot lock the credential store".to_string())?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| {
            "Another glass-core process is updating credentials. Retry shortly.".to_string()
        })?;
        Ok(Self(file))
    }
}
impl Drop for RefreshLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn random(n: usize) -> Result<String, String> {
    let mut b = vec![0; n];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| "secure random generator failed")?;
    Ok(B64.encode(b))
}
fn host_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| "secure random generator failed")?;
    b[6] = (b[6] & 15) | 64;
    b[8] = (b[8] & 63) | 128;
    let hex: String = b.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}
pub fn http() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .http_status_as_error(false)
        .build()
        .into()
}
fn token_form(form: &[(&str, &str)]) -> Result<Value, String> {
    let mut response = http()
        .post(format!("{ISSUER}/api/accounts/oauth/token"))
        .send_form(form.iter().copied())
        .map_err(|_| {
            "Could not reach ChatGPT sign-in. Check your connection and retry.".to_string()
        })?;
    let status = response.status().as_u16();
    let v: Value = response
        .body_mut()
        .read_json()
        .map_err(|_| "Invalid sign-in server response".to_string())?;
    if !(200..300).contains(&status) {
        return Err(
            if matches!(
                v["error"].as_str(),
                Some("invalid_grant" | "invalid_client")
            ) {
                "ChatGPT session expired or was revoked. Use /login to reconnect.".into()
            } else {
                format!("ChatGPT sign-in failed (HTTP {status}). Retry /login.")
            },
        );
    }
    Ok(v)
}
fn update_tokens(c: &mut Credential, v: &Value, initial: bool) -> Result<(), String> {
    c.access_token = v["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("sign-in returned no access token")?
        .into();
    if v["token_type"]
        .as_str()
        .is_some_and(|s| !s.eq_ignore_ascii_case("bearer"))
    {
        return Err("unsupported sign-in token type".into());
    }
    if let Some(s) = v["scope"].as_str() {
        c.scopes = s.split_whitespace().map(str::to_string).collect();
    } else if initial {
        return Err("Sign-in did not grant ChatGPT plan permissions".into());
    }
    if !c.scopes.iter().any(|s| s == "chatgpt.tokens.use.direct") {
        return Err(
            "ChatGPT plan usage is not enabled. Use /login and approve plan access.".into(),
        );
    }
    if let Some(t) = v["refresh_token"].as_str() {
        c.refresh_token = t.into();
    }
    c.expires_at = now()
        + v["expires_in"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("sign-in returned no token expiry")?;
    if let Some(t) = v["id_token"].as_str() {
        c.id_token = t.into();
    }
    Ok(())
}
fn validate_identity(token: &str, client: &str, nonce: &str) -> Result<(String, String), String> {
    let mut response = http()
        .get(format!("{ISSUER}/.well-known/jwks.json"))
        .call()
        .map_err(|_| "Could not verify ChatGPT identity".to_string())?;
    if !response.status().is_success() {
        return Err("Could not retrieve ChatGPT signing keys".into());
    }
    let jwks: Value = response
        .body_mut()
        .read_json()
        .map_err(|_| "Invalid ChatGPT signing keys".to_string())?;
    validate_identity_with_jwks(token, client, nonce, &jwks)
}
fn validate_identity_with_jwks(
    token: &str,
    client: &str,
    nonce: &str,
    jwks: &Value,
) -> Result<(String, String), String> {
    let parts: Vec<_> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid ChatGPT identity token".into());
    }
    let decode = |s: &str| -> Result<Value, String> {
        serde_json::from_slice(
            &B64.decode(s)
                .map_err(|_| "Invalid identity token encoding")?,
        )
        .map_err(|_| "Invalid identity token JSON".into())
    };
    let header = decode(parts[0])?;
    let claims = decode(parts[1])?;
    if header["alg"] != "RS256" {
        return Err("Unsupported identity signature algorithm".into());
    }
    let key = jwks["keys"]
        .as_array()
        .and_then(|keys| {
            keys.iter()
                .find(|k| k["kid"] == header["kid"] && k["kty"] == "RSA")
        })
        .ok_or("Unknown ChatGPT identity signing key")?;
    let n = B64
        .decode(
            key["n"]
                .as_str()
                .ok_or("Missing identity signing modulus")?,
        )
        .map_err(|_| "Invalid signing key")?;
    let e = B64
        .decode(
            key["e"]
                .as_str()
                .ok_or("Missing identity signing exponent")?,
        )
        .map_err(|_| "Invalid signing key")?;
    let sig = B64
        .decode(parts[2])
        .map_err(|_| "Invalid identity signature")?;
    signature::RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &signature::RSA_PKCS1_2048_8192_SHA256,
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &sig,
        )
        .map_err(|_| "ChatGPT identity signature verification failed")?;
    let audience = claims["aud"] == client
        || claims["aud"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x == client));
    if claims["iss"] != ISSUER
        || !audience
        || claims["exp"].as_u64().unwrap_or(0) <= now()
        || (!nonce.is_empty() && claims["nonce"] != nonce)
    {
        return Err(
            "ChatGPT identity validation failed (issuer, audience, expiry, or nonce)".into(),
        );
    }
    if claims["nbf"].as_u64().is_some_and(|t| t > now() + 60) {
        return Err("ChatGPT identity is not valid yet".into());
    }
    let sub = claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("ChatGPT identity has no subject")?;
    Ok((
        sub.into(),
        claims["email"].as_str().unwrap_or("ChatGPT account").into(),
    ))
}
pub fn browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Ok(mut child) = result {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        true
    } else {
        false
    }
}
struct Attempt {
    listener: TcpListener,
    state: String,
    nonce: String,
    verifier: String,
    redirect: String,
    client: String,
    existing: Option<Credential>,
    pub url: String,
}
impl Attempt {
    fn start(store: &mut Store, existing: Option<Credential>) -> Result<Self, String> {
        if store.host_id.is_empty() {
            store.host_id = edit(|fresh| {
                if fresh.host_id.is_empty() {
                    fresh.host_id = host_id()?;
                }
                Ok(fresh.host_id.clone())
            })?;
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| format!("Cannot start sign-in callback: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let redirect = format!(
            "http://127.0.0.1:{}/auth/callback",
            listener.local_addr().map_err(|e| e.to_string())?.port()
        );
        let state = random(32)?;
        let nonce = random(32)?;
        let verifier = random(48)?;
        let challenge = B64.encode(digest::digest(&digest::SHA256, verifier.as_bytes()));
        let client = existing
            .as_ref()
            .map(|c| c.client_id.clone())
            .unwrap_or_else(|| "dynamic_agent_client".into());
        let mut url = url::Url::parse(&format!("{ISSUER}/api/accounts/authorize"))
            .map_err(|e| e.to_string())?;
        {
            let mut q = url.query_pairs_mut();
            for (k, v) in [
                ("client_id", client.as_str()),
                ("ext_agent_host_id", store.host_id.as_str()),
                ("response_type", "code"),
                ("redirect_uri", redirect.as_str()),
                ("scope", SCOPES),
                ("resource", RESOURCE),
                ("state", state.as_str()),
                ("nonce", nonce.as_str()),
                ("code_challenge_method", "S256"),
                ("code_challenge", challenge.as_str()),
            ] {
                q.append_pair(k, v);
            }
            if let Some(c) = &existing {
                if !c.id_token.is_empty() {
                    q.append_pair("id_token_hint", &c.id_token);
                }
                if !c.email.is_empty() {
                    q.append_pair("login_hint", &c.email);
                }
            } else {
                q.append_pair("agent_name_hint", "glass-core");
            }
        }
        Ok(Self {
            listener,
            state,
            nonce,
            verifier,
            redirect,
            client,
            existing,
            url: url.into(),
        })
    }
    fn callback(&self, value: &str) -> Result<(String, String), String> {
        let url =
            url::Url::parse(value).map_err(|_| "Paste the complete callback URL".to_string())?;
        let expected = url::Url::parse(&self.redirect).map_err(|e| e.to_string())?;
        if url.scheme() != expected.scheme()
            || url.host_str() != expected.host_str()
            || url.port() != expected.port()
            || url.path() != expected.path()
        {
            return Err("Callback address does not match this sign-in attempt".into());
        }
        let mut pairs = BTreeMap::new();
        for (k, v) in url.query_pairs().into_owned() {
            if pairs.insert(k, v).is_some() {
                return Err("Duplicate sign-in callback parameter".into());
            }
        }
        if pairs.get("state") != Some(&self.state) {
            return Err("Sign-in state did not match. Start a fresh /login.".into());
        }
        if pairs.contains_key("error") {
            return Err("Sign-in was declined. Use /login to try again.".into());
        }
        let client = pairs
            .get("client_id")
            .cloned()
            .unwrap_or_else(|| self.client.clone());
        if client == "dynamic_agent_client" || client.is_empty() {
            return Err("Registration is incomplete: no issued client ID".into());
        }
        if self.existing.is_some() && client != self.client {
            return Err("Sign-in returned a different account registration".into());
        }
        let code = pairs
            .get("code")
            .filter(|s| !s.is_empty())
            .ok_or("Callback has no authorization code")?;
        Ok((code.clone(), client))
    }
    fn exchange(self, callback: String) -> Result<Credential, String> {
        let (code, client) = self.callback(&callback)?;
        let v = token_form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client),
            ("code", &code),
            ("code_verifier", &self.verifier),
            ("redirect_uri", &self.redirect),
            ("resource", RESOURCE),
        ])?;
        let id = v["id_token"]
            .as_str()
            .ok_or("Sign-in returned no identity token")?;
        let (subject, email) = validate_identity(id, &client, &self.nonce)?;
        if self.existing.as_ref().is_some_and(|c| c.subject != subject) {
            return Err("The signed-in account does not match the selected connection".into());
        }
        let mut cred = Credential {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            client_id: client,
            subject,
            email,
            id_token: id.into(),
            scopes: Vec::new(),
        };
        update_tokens(&mut cred, &v, true)?;
        Ok(cred)
    }
}
pub fn chatgpt(
    keys: &KeyReader,
    store: &mut Store,
    existing: Option<Credential>,
) -> Result<Option<Credential>, String> {
    use glass_tui::crossterm::event::{Event, KeyCode, KeyModifiers};
    let attempt = Attempt::start(store, existing)?;
    // Never display the ID-token hint in terminal output.
    let opened = browser(&attempt.url);
    let mut screen = modal::Screen::new();
    let start = std::time::Instant::now();
    let mut safe = url::Url::parse(&attempt.url).map_err(|e| e.to_string())?;
    let pairs = safe
        .query_pairs()
        .filter(|(k, _)| k != "id_token_hint")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<Vec<_>>();
    safe.set_query(None);
    safe.query_pairs_mut().extend_pairs(pairs);
    let safe_link = safe.to_string();
    let callback = loop {
        let mut lines = vec![
            if opened {
                "Continue in your browser to authorize ChatGPT plan access.".into()
            } else {
                "The browser could not open. Press O to try again.".into()
            },
            "Waiting for sign-in…".into(),
            String::new(),
            "Remote terminal? Press P and paste the final callback URL.".into(),
        ];
        {
            lines.push("Browser link (no tokens):".into());
            let chars: Vec<_> = safe_link.chars().collect();
            let width = usize::from(
                glass_tui::crossterm::terminal::size()
                    .unwrap_or((80, 24))
                    .0
                    .saturating_sub(6),
            )
            .clamp(1, 100);
            lines.extend(chars.chunks(width).map(|s| s.iter().collect::<String>()));
        }
        let (width, height) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
        let compact = width < 50 || height < 14;
        if compact {
            lines = vec![
                "Waiting for sign-in".into(),
                "P paste callback".into(),
                "C copy login link".into(),
                "L view login link".into(),
                "O open browser".into(),
            ];
        }
        screen.draw(
            if compact {
                "ChatGPT sign-in"
            } else {
                "Continue with ChatGPT"
            },
            &lines,
            if compact {
                "Esc cancel"
            } else {
                "O browser · C copy · L link · P paste callback · Esc cancel"
            },
            0,
        );
        if start.elapsed() > Duration::from_secs(300) {
            return Err("Sign-in timed out. Open /login to start again.".into());
        }
        match attempt.listener.accept() {
            Ok((mut socket, _)) => {
                let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
                let mut bytes = [0; 8192];
                let n = socket.read(&mut bytes).unwrap_or(0);
                let request = String::from_utf8_lossy(&bytes[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|s| s.split_whitespace().nth(1))
                    .unwrap_or("");
                let url = format!(
                    "http://127.0.0.1:{}{path}",
                    attempt
                        .listener
                        .local_addr()
                        .map_err(|e| e.to_string())?
                        .port()
                );
                let result = attempt.callback(&url);
                let valid = result.is_ok();
                let belongs = url::Url::parse(&url).ok().is_some_and(|u| {
                    u.path() == "/auth/callback"
                        && u.query_pairs()
                            .any(|(k, v)| k == "state" && v == attempt.state)
                });
                let page = if valid {
                    "Sign-in received. Return to glass-core to finish."
                } else {
                    "Invalid sign-in callback. Return to glass-core and retry."
                };
                let _ = write!(
                    socket,
                    "HTTP/1.1 {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    if valid { "200 OK" } else { "400 Bad Request" },
                    page.len(),
                    page
                );
                if belongs {
                    result?;
                    break url;
                } // Unrelated requests never cancel a real attempt.
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(format!("Sign-in callback failed: {e}")),
        }
        if let Some(Event::Key(k)) = keys.next_timeout(Duration::from_millis(100)) {
            if k.code == KeyCode::Esc
                || (k.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(k.code, KeyCode::Char('c' | 'd')))
            {
                return Ok(None);
            }
            match k.code {
                KeyCode::Char('c') => {
                    let encoded =
                        base64::engine::general_purpose::STANDARD.encode(safe_link.as_bytes());
                    let _ = write!(std::io::stdout(), "\x1b]52;c;{encoded}\x07");
                    let _ = std::io::stdout().flush();
                }
                KeyCode::Char('o') => {
                    browser(&attempt.url);
                }
                KeyCode::Char('l') => {
                    drop(screen);
                    modal::info(keys, "ChatGPT login link", &safe_link);
                    screen = modal::Screen::new();
                }
                KeyCode::Char('p') => {
                    drop(screen);
                    if let Some(url) = modal::field(
                        keys,
                        "Continue with ChatGPT",
                        "Paste the final callback URL",
                        "",
                        true,
                    ) {
                        return modal::work(keys, "Verifying ChatGPT sign-in", move || {
                            attempt.exchange(url)
                        })
                        .transpose();
                    } else {
                        return Ok(None);
                    }
                }
                _ => {}
            }
        }
    };
    drop(screen);
    modal::work(keys, "Verifying ChatGPT sign-in", move || {
        attempt.exchange(callback)
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn credential() -> Credential {
        Credential {
            access_token: "test-access".into(),
            refresh_token: "test-refresh".into(),
            expires_at: 0,
            client_id: "oaiapp_test".into(),
            subject: "test-subject".into(),
            email: "test@example.invalid".into(),
            id_token: String::new(),
            scopes: vec!["chatgpt.tokens.use.direct".into()],
        }
    }
    fn attempt(existing: bool) -> Attempt {
        Attempt {
            listener: TcpListener::bind(("127.0.0.1", 0)).unwrap(),
            state: "test-state".into(),
            nonce: "test-nonce".into(),
            verifier: "test-verifier".into(),
            redirect: "http://127.0.0.1:1455/auth/callback".into(),
            client: if existing {
                "oaiapp_test"
            } else {
                "dynamic_agent_client"
            }
            .into(),
            existing: existing.then(credential),
            url: String::new(),
        }
    }
    #[test]
    fn callback_binds_state_address_and_registration() {
        let a = attempt(false);
        let valid =
            "http://127.0.0.1:1455/auth/callback?code=c&state=test-state&client_id=oaiapp_test";
        assert_eq!(
            a.callback(valid).unwrap(),
            ("c".into(), "oaiapp_test".into())
        );
        for bad in [
            valid.replace("test-state", "wrong"),
            valid.replace("127.0.0.1", "localhost"),
            valid.replace("1455", "1234"),
            valid.replace("/auth/callback", "/callback"),
            valid.replace("&client_id=oaiapp_test", ""),
            format!("{valid}&state=test-state"),
            format!("{valid}&error=access_denied"),
        ] {
            assert!(a.callback(&bad).is_err(), "{bad}");
        }
        let a = attempt(true);
        assert!(
            a.callback(&valid.replace("oaiapp_test", "oaiapp_other"))
                .is_err()
        );
        assert!(
            a.callback(&valid.replace("&client_id=oaiapp_test", ""))
                .is_ok()
        );
    }
    #[test]
    fn identity_verification_rejects_invalid_claims_and_signatures() {
        let fixture: Value =
            serde_json::from_str(include_str!("testdata/chatgpt-id-tokens.json")).unwrap();
        for name in ["valid", "multiple_audiences"] {
            assert_eq!(
                validate_identity_with_jwks(
                    fixture["tokens"][name].as_str().unwrap(),
                    "oaiapp_test",
                    "test-nonce",
                    &fixture["jwks"]
                )
                .unwrap(),
                ("test-subject".into(), "test@example.invalid".into())
            );
        }
        for name in ["expired", "issuer", "future", "missing_subject"] {
            assert!(
                validate_identity_with_jwks(
                    fixture["tokens"][name].as_str().unwrap(),
                    "oaiapp_test",
                    "test-nonce",
                    &fixture["jwks"]
                )
                .is_err()
            );
        }
        let valid = fixture["tokens"]["valid"].as_str().unwrap();
        assert!(
            validate_identity_with_jwks(valid, "wrong-client", "test-nonce", &fixture["jwks"])
                .is_err()
        );
        assert!(
            validate_identity_with_jwks(valid, "oaiapp_test", "wrong-nonce", &fixture["jwks"])
                .is_err()
        );
        let mut parts = valid.split('.').map(str::to_string).collect::<Vec<_>>();
        parts[1]=B64.encode(br#"{"iss":"https://auth.openai.com","aud":"oaiapp_test","exp":4102444800,"nonce":"test-nonce","sub":"forged"}"#);
        assert!(
            validate_identity_with_jwks(
                &parts.join("."),
                "oaiapp_test",
                "test-nonce",
                &fixture["jwks"]
            )
            .is_err()
        );
    }
    #[test]
    fn scopes_and_rotating_refresh_tokens_are_checked() {
        let mut c = credential();
        assert!(
            update_tokens(
                &mut c,
                &json!({"access_token":"new","expires_in":3600,"scope":"openid email"}),
                true
            )
            .is_err()
        );
        let mut c = credential();
        let before = now();
        update_tokens(
            &mut c,
            &json!({"access_token":"new","expires_in":3600,"refresh_token":"rotated"}),
            false,
        )
        .unwrap();
        assert_eq!(c.access_token, "new");
        assert_eq!(c.refresh_token, "rotated");
        assert!(c.expires_at >= before + 3600);
        assert!(
            update_tokens(
                &mut c,
                &json!({"access_token":"new","expires_in":3600}),
                true
            )
            .is_err()
        );
    }
    #[test]
    fn credential_writes_are_private_and_locks_release() {
        let dir = std::env::temp_dir().join(format!("glass-auth-test-{}", random(12).unwrap()));
        let path = dir.join("auth.json");
        secure_write(&path, b"test").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"test");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let lock = dir.join("refresh.lock");
        let guard = RefreshLock::acquire(&lock).unwrap();
        assert!(RefreshLock::acquire(&lock).is_err());
        drop(guard);
        assert!(RefreshLock::acquire(&lock).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn host_identity_is_a_uuid_v4() {
        let id = host_id().unwrap();
        assert!(id.starts_with("urn:uuid:"));
        assert_eq!(id.len(), 45);
        assert_eq!(&id[23..24], "4");
    }
}
