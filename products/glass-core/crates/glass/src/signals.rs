//! Headless interruption: the handler only stores an atomic signal number.
use std::sync::atomic::{AtomicI32, Ordering};
static SIGNAL: AtomicI32 = AtomicI32::new(0);
extern "C" fn received(signal: i32) {
    SIGNAL.store(signal, Ordering::Relaxed);
}
pub struct Guard {
    interrupt: libc::sighandler_t,
    terminate: libc::sighandler_t,
}
impl Guard {
    pub fn install() -> Self {
        SIGNAL.store(0, Ordering::Relaxed);
        // libc's signal handler signature is compatible with received; no allocation or I/O in it.
        unsafe {
            Self {
                interrupt: libc::signal(libc::SIGINT, received as *const () as libc::sighandler_t),
                terminate: libc::signal(libc::SIGTERM, received as *const () as libc::sighandler_t),
            }
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            libc::signal(libc::SIGINT, self.interrupt);
            libc::signal(libc::SIGTERM, self.terminate);
        }
    }
}
pub fn interrupted() -> bool {
    SIGNAL.load(Ordering::Relaxed) != 0
}
pub fn exit_code() -> Option<i32> {
    let n = SIGNAL.load(Ordering::Relaxed);
    (n != 0).then_some(128 + n)
}
