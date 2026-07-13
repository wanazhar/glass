#![no_main]
use glass::browser::session::{Locator, WaitCondition};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(value) = std::str::from_utf8(data) {
        let _ = Locator::parse(value);
        let _ = WaitCondition::parse(value);
    }
});
