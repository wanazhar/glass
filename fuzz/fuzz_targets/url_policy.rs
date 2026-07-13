#![no_main]
use glass::browser::{policy::BrowserPolicy, session::normalize_url};
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

fuzz_target!(|data: &[u8]| {
    static POLICY: OnceLock<BrowserPolicy> = OnceLock::new();
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let policy = POLICY.get_or_init(|| BrowserPolicy::hardened(".").unwrap());
    if let Ok(value) = std::str::from_utf8(data) {
        let normalized = normalize_url(value);
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
        });
        let _ = runtime.block_on(policy.require_url(&normalized));
    }
});
