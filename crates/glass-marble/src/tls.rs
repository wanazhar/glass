//! TLS on the same port as plain HTTP: the server sniffs each connection's
//! first bytes and either does a rustls handshake or serves HTTP. The
//! `tls-cert` subcommand provisions the identity, preferring a real Tailscale
//! cert and falling back to a self-signed one made by the openssl CLI.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// The certificate chain and the hash clients pin against: SHA-256 over the
/// leaf DER, the same bytes a client sees in the handshake.
pub struct TlsIdentity {
    pub config: Arc<ServerConfig>,
    pub leaf_sha256: [u8; 32],
}

impl TlsIdentity {
    pub fn leaf_sha256_hex(&self) -> String {
        hex(&self.leaf_sha256)
    }
}

/// Load a PEM cert + key into a rustls server identity.
pub fn load_identity(cert_path: &Path, key_path: &Path) -> Result<TlsIdentity, String> {
    let cert_text = std::fs::read_to_string(cert_path)
        .map_err(|error| format!("cannot read {}: {error}", cert_path.display()))?;
    let key_text = std::fs::read_to_string(key_path)
        .map_err(|error| format!("cannot read {}: {error}", key_path.display()))?;
    let certificates: Vec<CertificateDer<'static>> = pem_blocks(&cert_text, "CERTIFICATE")
        .map_err(|error| format!("{}: {error}", cert_path.display()))?
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    if certificates.is_empty() {
        return Err(format!("{} has no CERTIFICATE block", cert_path.display()));
    }
    let leaf_sha256 = sha256(certificates[0].as_ref());
    let key = private_key(&key_text).map_err(|error| format!("{}: {error}", key_path.display()))?;
    let provider = rustls::crypto::ring::default_provider();
    let config = ServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .map_err(|error| format!("TLS protocol versions: {error}"))?
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|error| format!("certificate and key do not load together: {error}"))?;
    Ok(TlsIdentity {
        config: Arc::new(config),
        leaf_sha256,
    })
}

/// What `tls-cert` produced, for the operator and the report.
pub struct MadeIdentity {
    pub kind: &'static str,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub leaf_sha256: String,
}

/// Provision a cert in `dir`: a real Tailscale cert for this machine's
/// MagicDNS name when `tailscale cert` can make one, otherwise a self-signed
/// P-256 cert from openssl with the tailnet IP and DNS name as SANs.
pub fn make_identity(dir: &Path) -> Result<MadeIdentity, String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    chmod(dir, 0o700)?;
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    let dns_name = tailscale_dns_name();
    let kind = if let Some(name) = dns_name.as_deref() {
        let status = Command::new("tailscale")
            .arg("cert")
            .arg("--cert-file")
            .arg(&cert)
            .arg("--key-file")
            .arg(&key)
            .arg(name)
            .output();
        match status {
            Ok(output) if output.status.success() => Some("tailscale"),
            Ok(output) => {
                tracing::warn!(
                    stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                    "tailscale cert failed; falling back to self-signed"
                );
                None
            }
            Err(error) => {
                tracing::warn!("cannot run tailscale cert: {error}; falling back to self-signed");
                None
            }
        }
    } else {
        None
    };
    let kind = match kind {
        Some(kind) => kind,
        None => {
            self_signed(&cert, &key, dns_name.as_deref())?;
            "self-signed"
        }
    };
    chmod(&cert, 0o600)?;
    chmod(&key, 0o600)?;
    let leaf_sha256 = leaf_fingerprint(&cert)?;
    Ok(MadeIdentity {
        kind,
        cert,
        key,
        leaf_sha256,
    })
}

/// SHA-256 of the leaf certificate in a PEM file, for fingerprints clients pin.
pub fn leaf_fingerprint(cert_path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(cert_path)
        .map_err(|error| format!("cannot read {}: {error}", cert_path.display()))?;
    let leaf = pem_blocks(&text, "CERTIFICATE")
        .map_err(|error| format!("{}: {error}", cert_path.display()))?
        .into_iter()
        .next()
        .ok_or_else(|| format!("{} has no CERTIFICATE block", cert_path.display()))?;
    Ok(hex(&sha256(&leaf)))
}

/// A self-signed P-256 cert via the openssl CLI (825 days, SANs for the
/// tailnet IPv4 and MagicDNS name when known).
fn self_signed(cert: &Path, key: &Path, dns_name: Option<&str>) -> Result<(), String> {
    let name = dns_name
        .map(str::to_string)
        .or_else(tailscale_ipv4)
        .unwrap_or_else(|| "localhost".to_string());
    let mut san = Vec::new();
    if let Some(name) = dns_name {
        san.push(format!("DNS:{name}"));
    }
    if let Some(ip) = tailscale_ipv4() {
        san.push(format!("IP:{ip}"));
    }
    let san = if san.is_empty() {
        format!("DNS:{name}")
    } else {
        san.join(",")
    };
    let output = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-keyout",
        ])
        .arg(key)
        .arg("-out")
        .arg(cert)
        .args(["-days", "825", "-nodes", "-subj", &format!("/CN={name}")])
        .arg("-addext")
        .arg(format!("subjectAltName={san}"))
        .output()
        .map_err(|error| format!("cannot run openssl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "openssl req failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// This machine's MagicDNS name, from `tailscale status --json`.
fn tailscale_dns_name() -> Option<String> {
    let output = Command::new("tailscale")
        .arg("status")
        .arg("--json")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let name = status.pointer("/Self/DNSName")?.as_str()?;
    Some(name.trim_end_matches('.').to_string())
}

fn tailscale_ipv4() -> Option<String> {
    let output = Command::new("tailscale")
        .arg("ip")
        .arg("-4")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// The DER bytes of every `-----BEGIN <label>-----` block in a PEM text.
fn pem_blocks(text: &str, label: &str) -> Result<Vec<Vec<u8>>, String> {
    use base64::Engine;
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut blocks = Vec::new();
    let mut inside = false;
    let mut body = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line == begin {
            inside = true;
            body.clear();
        } else if line == end && inside {
            let der = base64::engine::general_purpose::STANDARD
                .decode(body.as_bytes())
                .map_err(|error| format!("bad base64 in {label} block: {error}"))?;
            blocks.push(der);
            inside = false;
        } else if inside {
            body.push_str(line);
        }
    }
    Ok(blocks)
}

/// Any PEM private-key block: PKCS8, SEC1 EC, or PKCS1 RSA.
fn private_key(text: &str) -> Result<PrivateKeyDer<'static>, String> {
    for label in ["PRIVATE KEY", "EC PRIVATE KEY", "RSA PRIVATE KEY"] {
        if let Some(der) = pem_blocks(text, label)?.into_iter().next() {
            return Ok(match label {
                "EC PRIVATE KEY" => PrivateKeyDer::Sec1(der.into()),
                "RSA PRIVATE KEY" => PrivateKeyDer::Pkcs1(der.into()),
                _ => PrivateKeyDer::Pkcs8(der.into()),
            });
        }
    }
    Err("no private-key PEM block".to_string())
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn chmod(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| format!("chmod {}: {error}", path.display()))
}

/// Hex-encoded HMAC-SHA256, used by /api/tls-proof so a client holding the
/// token can verify the leaf it actually saw.
pub fn tls_proof(key_token: &str, nonce: &str, cert_sha256_hex: &str) -> String {
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key_token.as_bytes());
    let mut message = Vec::new();
    message.extend_from_slice(b"marble-tls-pin-v1\0");
    message.extend_from_slice(nonce.as_bytes());
    message.push(0);
    message.extend_from_slice(cert_sha256_hex.as_bytes());
    hex(ring::hmac::sign(&key, &message).as_ref())
}

/// Print what `tls-cert` made: which kind and the pin fingerprint.
pub fn describe(made: &MadeIdentity, out: &mut impl Write) -> std::io::Result<()> {
    writeln!(out, "kind={}", made.kind)?;
    writeln!(out, "cert={}", made.cert.display())?;
    writeln!(out, "key={}", made.key.display())?;
    writeln!(out, "certSha256={}", made.leaf_sha256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_proof_is_deterministic_and_nonce_bound() {
        let a = tls_proof("token", "aa".repeat(16).as_str(), "00".repeat(32).as_str());
        let b = tls_proof("token", "aa".repeat(16).as_str(), "00".repeat(32).as_str());
        let c = tls_proof("token", "bb".repeat(16).as_str(), "00".repeat(32).as_str());
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn pem_blocks_decodes_certificates() {
        use base64::Engine;
        let der = base64::engine::general_purpose::STANDARD.encode(b"fake-der");
        let text = format!("-----BEGIN CERTIFICATE-----\n{der}\n-----END CERTIFICATE-----\n");
        assert_eq!(pem_blocks(&text, "CERTIFICATE").unwrap(), vec![b"fake-der"]);
    }
}
