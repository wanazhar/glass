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
const MAX_AUTH_STORE_BYTES: usize = 1024 * 1024;
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
/// OpenAI's Codex public client, used for the device-code sign-in method.
const DEVICE_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEVICE_VERIFICATION_URI: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const DEVICE_FLOW: &str = "device_code";

pub(crate) fn is_codex_endpoint(base_url: &str) -> bool {
    url::Url::parse(base_url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("chatgpt.com")
            && url.port_or_known_default() == Some(443)
            && url.path().trim_end_matches('/') == "/backend-api/codex"
    })
}

pub(crate) fn chatgpt_account_id(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let claims: Value = serde_json::from_slice(&B64.decode(payload).ok()?).ok()?;
    claims["https://api.openai.com/auth"]["chatgpt_account_id"]
        .as_str()
        .map(str::to_owned)
}

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
    /// Empty for the browser dynamic-client flow, `device_code` for the
    /// Codex device flow. The two flows refresh against different endpoints.
    #[serde(default)]
    pub flow: String,
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
#[cfg(any(target_os = "macos", test))]
pub(crate) mod broker;

pub fn load() -> Result<Store, String> {
    #[cfg(target_os = "macos")]
    if let Some(response) = broker::request(broker::Request::Load) {
        return response;
    }
    load_from(&config::config_dir().join("auth.json"))
}
fn load_from(path: &Path) -> Result<Store, String> {
    match crate::requests::bounded_optional(path, MAX_AUTH_STORE_BYTES)
        .map_err(|e| format!("cannot read auth.json: {e}"))?
    {
        Some(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "auth.json is invalid; credentials have not been overwritten".into()),
        None => Ok(Store::default()),
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
fn save_to(path: &Path, store: &Store) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_AUTH_STORE_BYTES {
        return Err("auth.json exceeds the 1048576 byte limit".into());
    }
    secure_write(path, &bytes)
}
/// Update the store under an OS lock; a crashed process releases it automatically.
pub fn edit<T>(update: impl FnOnce(&mut Store) -> Result<T, String>) -> Result<T, String> {
    #[cfg(target_os = "macos")]
    if broker::connected() {
        return Err("Sign in from the host to update saved connections".into());
    }
    edit_at(&config::config_dir().join("auth.json"), update)
}

fn edit_at<T>(
    path: &Path,
    update: impl FnOnce(&mut Store) -> Result<T, String>,
) -> Result<T, String> {
    let directory = path.parent().ok_or("missing credential directory")?;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let _lock = RefreshLock::acquire(&directory.join(".auth.lock"))?;
    let mut store = load_from(path)?;
    let result = update(&mut store)?;
    save_to(path, &store)?;
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
    #[cfg(target_os = "macos")]
    if let Some(response) = broker::request(broker::Request::Resolve(id.into())) {
        return response;
    }
    resolve_credential_at(
        &config::config_dir().join("auth.json"),
        id,
        exchange_refresh_token,
    )
}

fn exchange_refresh_token(cred: &Credential) -> Result<Value, String> {
    if cred.flow == DEVICE_FLOW {
        oauth_token_form(&[
            ("grant_type", "refresh_token"),
            ("client_id", &cred.client_id),
            ("refresh_token", &cred.refresh_token),
        ])
    } else {
        token_form(&[
            ("grant_type", "refresh_token"),
            ("client_id", &cred.client_id),
            ("refresh_token", &cred.refresh_token),
            ("resource", RESOURCE),
        ])
    }
}

fn resolve_credential_at(
    path: &Path,
    id: &str,
    exchange: impl FnOnce(&Credential) -> Result<Value, String>,
) -> Result<String, String> {
    let store = load_from(path)?;
    let cred = store
        .credentials
        .get(id)
        .ok_or("Saved credentials were removed. Use /login to reconnect.")?;
    if cred.client_id.is_empty() || cred.expires_at > now() + 90 {
        return Ok(cred.access_token.clone());
    }
    let key = edit_at(path, |store| {
        let mut cred = store
            .credentials
            .get(id)
            .cloned()
            .ok_or("Connection was signed out. Use /login to reconnect.")?;
        if cred.expires_at <= now() + 90 {
            if cred.refresh_token.is_empty() {
                return Err("ChatGPT session expired. Use /login to reconnect.".into());
            }
            let token = match exchange(&cred) {
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
            let plan_required = cred.flow != DEVICE_FLOW;
            update_tokens_with(&mut cred, &token, false, plan_required)?;
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
    update_tokens_with(c, v, initial, true)
}
fn update_tokens_with(
    c: &mut Credential,
    v: &Value,
    initial: bool,
    plan_required: bool,
) -> Result<(), String> {
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
    } else if initial && plan_required {
        return Err("Sign-in did not grant ChatGPT plan permissions".into());
    }
    if plan_required && !c.scopes.iter().any(|s| s == "chatgpt.tokens.use.direct") {
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
/// Browsers may show either spelling of the loopback host after a copy.
fn loopback_host(host: &str) -> &str {
    if host == "localhost" {
        "127.0.0.1"
    } else {
        host
    }
}
pub(crate) struct Attempt {
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
    pub(crate) fn start(store: &mut Store, existing: Option<Credential>) -> Result<Self, String> {
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
        // A browser on another device (phone PWA or an SSH session) cannot
        // reach this host's loopback listener, so the failed page's URL is
        // pasted back. Accept the full callback URL, its query string,
        // `code#state`, or a bare authorization code.
        let value = value.trim();
        if value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control) {
            return Err("Paste the complete callback URL or authorization code".into());
        }
        let expected = url::Url::parse(&self.redirect).map_err(|e| e.to_string())?;
        let mut pairs = BTreeMap::new();
        let mut from_url = false;
        if let Ok(url) = url::Url::parse(value)
            && matches!(url.scheme(), "http" | "https")
        {
            from_url = true;
            if url.scheme() != expected.scheme()
                || url.host_str().map(loopback_host) != expected.host_str().map(loopback_host)
                || url.port() != expected.port()
                || url.path() != expected.path()
            {
                return Err("Callback address does not match this sign-in attempt".into());
            }
            for (k, v) in url.query_pairs().into_owned() {
                if pairs.insert(k, v).is_some() {
                    return Err("Duplicate sign-in callback parameter".into());
                }
            }
        }
        if !from_url {
            if let Some((code, state)) = value.split_once('#') {
                pairs.insert("code".into(), code.trim().to_string());
                pairs.insert("state".into(), state.trim().to_string());
            } else {
                let query = value.split_once('?').map_or(value, |(_, query)| query);
                let query = query.strip_prefix('?').unwrap_or(query);
                if query.contains("code=") {
                    for (k, v) in url::form_urlencoded::parse(query.as_bytes()).into_owned() {
                        if pairs.insert(k, v).is_some() {
                            return Err("Duplicate sign-in callback parameter".into());
                        }
                    }
                } else {
                    // A bare code stays bound to this attempt by its ID and
                    // PKCE verifier, so it does not need a state value.
                    pairs.insert("code".into(), value.to_string());
                }
            }
        }
        match pairs.get("state") {
            Some(state) if state == &self.state => {}
            Some(_) => return Err("Sign-in state did not match. Start a fresh /login.".into()),
            // A full callback URL must carry state; a manually pasted code is
            // already bound to the pending attempt and its PKCE verifier.
            None if from_url => {
                return Err("Sign-in callback is missing state. Start a fresh /login.".into());
            }
            None => {}
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
    pub(crate) fn exchange(self, callback: String) -> Result<Credential, String> {
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
        check_reconnected_account(self.existing.as_ref(), &subject)?;
        let mut cred = Credential {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            client_id: client,
            subject,
            email,
            id_token: id.into(),
            scopes: Vec::new(),
            flow: String::new(),
        };
        update_tokens(&mut cred, &v, true)?;
        Ok(cred)
    }
}
/// A pending Codex device-code sign-in. The browser never redirects to this
/// host, so it works from a phone web UI or an SSH session.
#[derive(Clone)]
pub struct DeviceAttempt {
    device_auth_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_at: u64,
}
fn device_json(url: &str, body: Value) -> Result<(u16, Value), String> {
    let mut response = http()
        .post(url)
        .header("Content-Type", "application/json")
        .send_json(body)
        .map_err(|_| {
            "Could not reach ChatGPT sign-in. Check your connection and retry.".to_string()
        })?;
    let status = response.status().as_u16();
    let value = response.body_mut().read_json().unwrap_or(Value::Null);
    Ok((status, value))
}
fn oauth_token_form(form: &[(&str, &str)]) -> Result<Value, String> {
    let mut response = http()
        .post(format!("{ISSUER}/oauth/token"))
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
        return Err(if matches!(v["error"].as_str(), Some("invalid_grant")) {
            "ChatGPT session expired or was revoked. Start sign-in again.".into()
        } else {
            format!("ChatGPT device sign-in failed (HTTP {status}). Retry.")
        });
    }
    Ok(v)
}
fn parse_device_start(v: &Value) -> Result<DeviceAttempt, String> {
    let device_auth_id = v["device_auth_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Device sign-in returned no identifier")?;
    let user_code = v["user_code"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Device sign-in returned no code")?;
    let interval = v["interval"]
        .as_u64()
        .or_else(|| v["interval"].as_str().and_then(|s| s.trim().parse().ok()))
        .unwrap_or(5)
        .max(1);
    let expires_at = v["expires_at"].as_u64().unwrap_or_else(|| now() + 900);
    Ok(DeviceAttempt {
        device_auth_id: device_auth_id.into(),
        user_code: user_code.into(),
        verification_uri: DEVICE_VERIFICATION_URI.into(),
        interval,
        expires_at,
    })
}
/// `Some((authorization_code, code_verifier))` only once approved.
fn device_poll_result(status: u16, v: &Value) -> Result<Option<(String, String)>, String> {
    if (200..300).contains(&status) {
        let code = v["authorization_code"].as_str().filter(|s| !s.is_empty());
        let verifier = v["code_verifier"].as_str().filter(|s| !s.is_empty());
        return match (code, verifier) {
            (Some(code), Some(verifier)) => Ok(Some((code.into(), verifier.into()))),
            _ => Err("Device sign-in returned an incomplete authorization".into()),
        };
    }
    let code = v["error"]
        .as_str()
        .map(str::to_string)
        .or_else(|| v["error"]["code"].as_str().map(str::to_string));
    if status == 403
        || status == 404
        || matches!(
            code.as_deref(),
            Some("deviceauth_authorization_pending" | "authorization_pending" | "slow_down")
        )
    {
        return Ok(None);
    }
    Err(format!("Device sign-in failed (HTTP {status}). Retry."))
}
impl DeviceAttempt {
    pub fn start() -> Result<Self, String> {
        let (status, v) = device_json(
            &format!("{ISSUER}/api/accounts/deviceauth/usercode"),
            serde_json::json!({"client_id": DEVICE_CLIENT_ID}),
        )?;
        if status == 404 {
            return Err(
                "Device sign-in is not enabled for this account. Enable device code authorization in ChatGPT settings, or use browser sign-in."
                    .into(),
            );
        }
        if !(200..300).contains(&status) {
            return Err(format!(
                "Could not start device sign-in (HTTP {status}). Retry."
            ));
        }
        parse_device_start(&v)
    }
    /// One poll. `None` means the user has not approved yet.
    pub fn poll(&self) -> Result<Option<Credential>, String> {
        if now() >= self.expires_at {
            return Err("Device sign-in expired. Start again.".into());
        }
        let (status, v) = device_json(
            &format!("{ISSUER}/api/accounts/deviceauth/token"),
            serde_json::json!({
                "device_auth_id": self.device_auth_id,
                "user_code": self.user_code
            }),
        )?;
        match device_poll_result(status, &v)? {
            None => Ok(None),
            Some((code, verifier)) => self.finish(&code, &verifier).map(Some),
        }
    }
    fn finish(&self, code: &str, verifier: &str) -> Result<Credential, String> {
        let v = oauth_token_form(&[
            ("grant_type", "authorization_code"),
            ("client_id", DEVICE_CLIENT_ID),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", DEVICE_REDIRECT_URI),
        ])?;
        let id = v["id_token"]
            .as_str()
            .ok_or("Device sign-in returned no identity token")?;
        let (subject, email) = validate_identity(id, DEVICE_CLIENT_ID, "")?;
        let mut cred = Credential {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            client_id: DEVICE_CLIENT_ID.into(),
            subject,
            email,
            id_token: id.into(),
            scopes: Vec::new(),
            flow: DEVICE_FLOW.into(),
        };
        update_tokens_with(&mut cred, &v, true, false)?;
        Ok(cred)
    }
}
fn use_device_sign_in(existing: Option<&Credential>) -> bool {
    existing.is_some_and(|credential| credential.flow == DEVICE_FLOW)
}

fn check_reconnected_account(existing: Option<&Credential>, subject: &str) -> Result<(), String> {
    if existing.is_some_and(|credential| credential.subject != subject) {
        return Err("The signed-in account does not match the selected connection".into());
    }
    Ok(())
}

fn device_sign_in_lines(verification_uri: &str, user_code: &str, size: (u16, u16)) -> Vec<String> {
    let (width, height) = size;
    let width = usize::from(width.max(1));
    let width = if width >= 24 && height >= 12 {
        width.saturating_sub(6).clamp(1, 100)
    } else {
        width
    };
    let mut lines = vec!["Open on any device:".into()];
    lines.extend(glass_tui::text::wrap(verification_uri, width));
    lines.push("Enter code:".into());
    lines.extend(glass_tui::text::wrap(user_code, width));
    lines.extend(glass_tui::text::wrap_words(
        "Approve ChatGPT access, then wait for sign-in to finish.",
        width,
    ));
    lines
}

fn chatgpt_device(
    keys: &KeyReader,
    existing: Option<&Credential>,
) -> Result<Option<Credential>, String> {
    use glass_tui::crossterm::event::{Event, KeyCode};
    let Some(attempt) = modal::work(
        keys,
        "Starting ChatGPT device sign-in",
        DeviceAttempt::start,
    )
    .transpose()?
    else {
        return Ok(None);
    };
    let screen = modal::Screen::new();
    let mut next_poll = std::time::Instant::now() + Duration::from_secs(attempt.interval);
    loop {
        if now() >= attempt.expires_at {
            return Err("Device sign-in expired. Start again.".into());
        }
        screen.draw(
            "ChatGPT device sign-in",
            &device_sign_in_lines(
                &attempt.verification_uri,
                &attempt.user_code,
                glass_tui::crossterm::terminal::size().unwrap_or((80, 24)),
            ),
            "O open page · Esc cancel",
            0,
        );
        if std::time::Instant::now() >= next_poll {
            if let Some(credential) = attempt.poll()? {
                check_reconnected_account(existing, &credential.subject)?;
                return Ok(Some(credential));
            }
            next_poll = std::time::Instant::now() + Duration::from_secs(attempt.interval);
        }
        if let Some(Event::Key(key)) = keys.next_timeout(Duration::from_millis(100)) {
            if modal::cancelled(key.code, key.modifiers) {
                return Ok(None);
            }
            if key.code == KeyCode::Char('o') {
                browser(&attempt.verification_uri);
            }
        }
    }
}

pub fn chatgpt(
    keys: &KeyReader,
    store: &mut Store,
    existing: Option<Credential>,
) -> Result<Option<Credential>, String> {
    use glass_tui::crossterm::event::{Event, KeyCode};
    if use_device_sign_in(existing.as_ref()) {
        return chatgpt_device(keys, existing.as_ref());
    }
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
            "Remote terminal? Press P and paste the callback URL or code.".into(),
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
            if modal::cancelled(k.code, k.modifiers) {
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
                        "Paste the callback URL or authorization code",
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
    #[test]
    fn device_sign_in_preserves_the_url_and_code_at_40_by_24() {
        for (uri, code) in [
            (DEVICE_VERIFICATION_URI.to_string(), "ABCD-EFGH".to_string()),
            (
                format!("{DEVICE_VERIFICATION_URI}?routing={}", "abcdef".repeat(8)),
                "ABCD-EFGH-".repeat(6),
            ),
            (String::new(), String::new()),
        ] {
            let lines = device_sign_in_lines(&uri, &code, (40, 24));
            assert!(lines.len() + 3 <= 20, "all rows must fit the framed modal");
            assert!(lines.iter().all(|line| glass_tui::text::width(line) <= 34));
            let visible: Vec<_> = lines
                .iter()
                .map(|line| glass_tui::text::clip(line, 34))
                .collect();
            let code_label = visible
                .iter()
                .position(|line| line == "Enter code:")
                .unwrap();
            let approval = visible
                .iter()
                .position(|line| line.starts_with("Approve"))
                .unwrap();
            assert_eq!(visible[1..code_label].concat(), uri);
            assert_eq!(visible[code_label + 1..approval].concat(), code);
        }
    }

    #[test]
    fn reconnect_routes_device_credentials_without_reusing_the_loopback_client() {
        assert!(!use_device_sign_in(None));
        let mut cred = credential();
        assert!(!use_device_sign_in(Some(&cred)));
        cred.client_id = DEVICE_CLIENT_ID.into();
        cred.flow = DEVICE_FLOW.into();
        assert!(use_device_sign_in(Some(&cred)));
        cred.flow = "unknown".into();
        assert!(!use_device_sign_in(Some(&cred)));
        assert!(check_reconnected_account(None, "new-account").is_ok());
        assert!(check_reconnected_account(Some(&cred), &cred.subject).is_ok());
        assert!(check_reconnected_account(Some(&cred), "different-account").is_err());
    }

    #[test]
    fn codex_endpoint_requires_the_exact_https_origin_and_base_path() {
        assert!(is_codex_endpoint("https://chatgpt.com/backend-api/codex/"));
        for url in [
            "https://api.openai.com/v1",
            "https://chatgpt.com.evil.test/backend-api/codex",
            "http://chatgpt.com/backend-api/codex",
            "https://chatgpt.com:8443/backend-api/codex",
            "https://chatgpt.com/other",
            "",
        ] {
            assert!(!is_codex_endpoint(url));
        }
    }

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
            flow: String::new(),
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
    fn auth_store_reads_and_writes_are_bounded() {
        let path = std::env::temp_dir().join(format!(
            "gc-auth-bounds-{}-{}.json",
            std::process::id(),
            random(8).unwrap()
        ));
        std::fs::write(&path, vec![b' '; MAX_AUTH_STORE_BYTES + 1]).unwrap();
        let error = match load_from(&path) {
            Ok(_) => panic!("oversized auth store should be refused"),
            Err(error) => error,
        };
        assert!(error.contains("1048576 byte limit"), "{error}");
        std::fs::remove_file(&path).unwrap();

        let mut store = Store::default();
        store.credentials.insert(
            "oversized".into(),
            Credential {
                access_token: "x".repeat(MAX_AUTH_STORE_BYTES),
                refresh_token: String::new(),
                expires_at: 0,
                client_id: String::new(),
                subject: String::new(),
                email: String::new(),
                id_token: String::new(),
                scopes: vec![],
                flow: String::new(),
            },
        );
        let error = save_to(&path, &store).unwrap_err();
        assert!(error.contains("1048576 byte limit"), "{error}");
        assert!(!path.exists(), "oversized credentials are never written");
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
            valid.replace("1455", "1234"),
            valid.replace("/auth/callback", "/callback"),
            valid.replace("&client_id=oaiapp_test", ""),
            format!("{valid}&state=test-state"),
            format!("{valid}&error=access_denied"),
        ] {
            assert!(a.callback(&bad).is_err(), "{bad}");
        }
        // Remote sign-in: the browser cannot reach this host's loopback
        // listener, so the callback is pasted back. Browsers may spell the
        // loopback host either way, and the query or `code#state` may arrive
        // without the surrounding URL.
        for pasted in [
            valid.replace("127.0.0.1", "localhost"),
            "code=c&state=test-state&client_id=oaiapp_test".into(),
            "?code=c&state=test-state&client_id=oaiapp_test".into(),
            "localhost:1455/auth/callback?code=c&state=test-state&client_id=oaiapp_test".into(),
        ] {
            assert_eq!(
                a.callback(&pasted).unwrap(),
                ("c".into(), "oaiapp_test".into()),
                "{pasted}"
            );
        }
        assert!(
            a.callback("code=c&state=wrong&client_id=oaiapp_test")
                .is_err()
        );
        assert!(a.callback("").is_err());
        let a = attempt(true);
        // A saved registration keeps its issued client ID, so a bare code or
        // `code#state` is enough when only part of the callback was copied.
        assert_eq!(
            a.callback("c#test-state").unwrap(),
            ("c".into(), "oaiapp_test".into())
        );
        assert_eq!(a.callback("c").unwrap(), ("c".into(), "oaiapp_test".into()));
        assert!(a.callback("c#wrong").is_err());
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
    fn device_sign_in_parses_start_and_poll_responses() {
        let started = parse_device_start(&json!({
            "device_auth_id": "device-1",
            "user_code": "ABCD-EFGH",
            "interval": "5",
            "expires_at": 4102444800u64
        }))
        .unwrap();
        assert_eq!(started.user_code, "ABCD-EFGH");
        assert_eq!(started.interval, 5);
        assert_eq!(started.verification_uri, DEVICE_VERIFICATION_URI);
        assert!(parse_device_start(&json!({"user_code": "ABCD"})).is_err());

        let approved = device_poll_result(
            200,
            &json!({"authorization_code": "code-1", "code_verifier": "verifier-1"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(approved, ("code-1".into(), "verifier-1".into()));
        assert!(
            device_poll_result(403, &json!({"error": "deviceauth_authorization_pending"}))
                .unwrap()
                .is_none()
        );
        assert!(
            device_poll_result(400, &json!({"error": {"code": "slow_down"}}))
                .unwrap()
                .is_none()
        );
        assert!(device_poll_result(400, &json!({"error": "invalid_request"})).is_err());
        assert!(device_poll_result(200, &json!({"authorization_code": "c"})).is_err());
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
