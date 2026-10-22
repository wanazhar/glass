//! Cancellation-aware response waits for pumped provider requests.
//! Socket reads use short waits internally; ordinary model pauses keep their
//! original deadline and never trigger another HTTP request.
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{
        Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
        time::Duration as WireDuration,
    },
};

thread_local! {
    static CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

pub(crate) fn scoped<T>(cancel: Arc<AtomicBool>, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<AtomicBool>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CANCEL.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CANCEL.with(|slot| slot.replace(Some(cancel))));
    f()
}

pub(super) fn is_cancelled() -> bool {
    CANCEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    })
}

pub(super) fn agent(config: ureq::config::Config) -> ureq::Agent {
    match CANCEL.with(|slot| slot.borrow().clone()) {
        Some(cancel) => ureq::Agent::with_parts(
            config,
            CancelConnector {
                inner: DefaultConnector::default(),
                cancel,
            },
            DefaultResolver::default(),
        ),
        None => config.new_agent(),
    }
}

#[derive(Debug)]
struct CancelConnector {
    inner: DefaultConnector,
    cancel: Arc<AtomicBool>,
}
impl Connector for CancelConnector {
    type Out = Box<dyn Transport>;
    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(cancelled());
        }
        self.inner.connect(details, chained).map(|stream| {
            stream.map(|inner| {
                Box::new(CancelTransport {
                    inner,
                    cancel: self.cancel.clone(),
                }) as Box<dyn Transport>
            })
        })
    }
}
fn cancelled() -> ureq::Error {
    ureq::Error::Io(std::io::Error::other("provider request cancelled"))
}
#[derive(Debug)]
struct CancelTransport {
    inner: Box<dyn Transport>,
    cancel: Arc<AtomicBool>,
}
impl Transport for CancelTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(cancelled());
        }
        self.inner.transmit_output(amount, timeout)
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let deadline = if timeout.after.is_not_happening() {
            None
        } else {
            Some(Instant::now() + *timeout.after)
        };
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(cancelled());
            }
            let remaining = deadline.map(|end| end.saturating_duration_since(Instant::now()));
            if remaining == Some(Duration::ZERO) {
                return Err(ureq::Error::Timeout(timeout.reason));
            }
            let slice = remaining
                .unwrap_or(Duration::from_millis(150))
                .min(Duration::from_millis(150));
            match self.inner.await_input(NextTimeout {
                after: WireDuration::Exact(slice),
                reason: timeout.reason,
            }) {
                Err(ureq::Error::Timeout(_)) => continue,
                Err(ureq::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }
    fn is_open(&mut self) -> bool {
        !self.cancel.load(Ordering::Acquire) && self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn stalled_reads_release_on_cancel_and_quiet_reads_do_not_replay() {
        for cancel_request in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = [0; 2048];
                assert!(socket.read(&mut request).unwrap() > 0);
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n")
                    .unwrap();
                ready_tx.send(()).unwrap();
                if cancel_request {
                    match socket.read(&mut request) {
                        Ok(0) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
                        result => panic!("cancelled socket must close: {result:?}"),
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(400));
                    socket.write_all(b"X").unwrap();
                }
            });
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let client = std::thread::spawn(move || {
                let result = scoped(worker_cancel, || {
                    let mut response = super::super::http_agent()
                        .get(url)
                        .call()
                        .map_err(|e| e.to_string())?;
                    let mut bytes = Vec::new();
                    response
                        .body_mut()
                        .as_reader()
                        .read_to_end(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>(bytes)
                });
                done_tx.send(result).unwrap();
            });
            ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            if cancel_request {
                cancel.store(true, Ordering::Release);
            }
            let result = done_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("provider reader remained blocked");
            if cancel_request {
                assert!(result.unwrap_err().contains("cancelled"));
            } else {
                assert_eq!(result.unwrap(), b"X");
            }
            client.join().unwrap();
            server.join().unwrap();
        }
    }
}
