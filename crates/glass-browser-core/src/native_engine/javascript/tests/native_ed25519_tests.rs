use super::*;

fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn ed25519_matches_rfc8032_vector() {
    let private_key = hex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
    let public_key = hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let signature = hex(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    );
    assert_eq!(
        native_ed25519_public_bytes(&private_key).unwrap(),
        public_key
    );
    assert_eq!(native_ed25519_sign(&private_key, &[]).unwrap(), signature);
    assert!(native_ed25519_verify(&public_key, &signature, &[]).unwrap());
}
