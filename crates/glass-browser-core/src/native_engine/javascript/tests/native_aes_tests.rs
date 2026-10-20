use super::*;

fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn aes_block_modes_match_nist_vectors() {
    let key = hex("2b7e151628aed2a6abf7158809cf4f3c");
    let iv = hex("000102030405060708090a0b0c0d0e0f");
    let counter = hex("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    let plaintext = hex(
        "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
    );
    let expected_cbc_prefix = hex(
        "7649abac8119b246cee98e9b12e9197d5086cb9b507219ee95db113a917678b273bed6b8e3c1743b7116e69e222295163ff1caa1681fac09120eca307586e1a7",
    );
    let expected_ctr = hex(
        "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff5ae4df3edbd5d35e5b4f09020db03eab1e031dda2fbe03d1792170a0f3009cee",
    );

    let ciphertext = native_aes_cbc_bytes("encrypt", &key, &iv, &plaintext).unwrap();
    assert_eq!(ciphertext.len(), 80);
    assert_eq!(&ciphertext[..64], expected_cbc_prefix.as_slice());
    assert_eq!(
        native_aes_cbc_bytes("decrypt", &key, &iv, &ciphertext).unwrap(),
        plaintext
    );
    assert_eq!(
        native_aes_ctr_bytes(&key, &counter, 128, &plaintext).unwrap(),
        expected_ctr
    );
}
