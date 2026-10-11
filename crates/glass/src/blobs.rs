//! Content-addressed blobs (task six-wave-d-001, OMP blob-artifacts).
//!
//! Bash spill files and image payloads are stored under the data dir as
//! `blobs/<sha256 hex>` with a `blob:<sha256>` reference the record keeps.
//! Writes are atomic (`tmp` then rename) and deduplicated: writing the same
//! content twice stores it once and both callers get the same address.
//!
//! Known honest limitation (deliberately not fixed in this slice): blobs
//! without a referencing record at startup are left alone — no garbage
//! sweep yet.

use ring::digest::{digest, SHA256};
use std::path::PathBuf;

/// Blob store root. Same isolation as the rest of the data tree.
pub fn dir() -> PathBuf {
    crate::config::data_dir().join("blobs")
}

/// Reference form kept in records: `blob:<sha256-hex>`.
pub fn reference(hex: &str) -> String {
    format!("blob:{hex}")
}

/// Lowercase hex without a third-party hex crate.
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Extract the hex digest out of a `blob:<sha256>` reference. `None` for
/// anything else — paths stay paths, this never guesses.
pub fn hex_of(reference: &str) -> Option<&str> {
    reference
        .strip_prefix("blob:")
        .filter(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Store content, returning `(reference, path)`. Dedup: identical content
/// maps to one stored object, and a second write is a no-op rename of
/// nothing — the file is already there, verified by its own name.
pub fn put(content: &[u8]) -> std::io::Result<(String, PathBuf)> {
    let hex = encode_hex(digest(&SHA256, content).as_ref());
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&hex);
    if path.exists() {
        return Ok((reference(&hex), path));
    }
    // Atomic tmp-rename; a partial file can never masquerade as a blob
    // because the name is only the content's digest once rename lands.
    let tmp = dir.join(format!(".tmp-{}-{}", std::process::id(), hex));
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let mut f = options.open(&tmp)?;
        f.write_all(content)?;
        f.flush()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok((reference(&hex), path))
}

/// Read a blob back by reference. Offset/length supported for ranged
/// re-reads from a later session or handoff.
pub fn get(reference: &str, offset: usize, length: Option<usize>) -> std::io::Result<Vec<u8>> {
    let hex = hex_of(reference)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a blob reference"))?;
    let path = dir().join(hex);
    let data = std::fs::read(path)?;
    let start = offset.min(data.len());
    let end = length.map(|l| start + l).unwrap_or(data.len()).min(data.len());
    Ok(data[start..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_is_deduplicated_and_roundtrips() {
        let content = b"blob body \xf0\x9f\x8c\x8d tail";
        let (r1, p1) = put(content).unwrap();
        let (r2, p2) = put(content).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(p1, p2, "identical content stores once");
        assert!(hex_of(&r1).is_some());
        assert_eq!(get(&r1, 0, None).unwrap(), content);
        // Ranged read.
        assert_eq!(get(&r1, 4, Some(4)).unwrap(), &content[4..8]);
    }

    #[test]
    fn non_blob_references_are_refused_not_guessed() {
        let err = get("output/out-1-2.log", 0, None).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(hex_of("0123").is_none());
    }
}
