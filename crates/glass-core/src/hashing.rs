//! Shared lowercase digest encoding with one allocation per result.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 15) as usize] as char);
    }
    output
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoding_matches_all_byte_values_and_sha256_vectors() {
        let bytes: Vec<u8> = (0..=255).collect();
        let previous: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(hex(&bytes), previous);
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(hex(&[]).is_empty());
    }
}
