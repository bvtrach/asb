//! Required owning-resource inspection cannot hold cancellation authority.
use super::*;
use std::io;
use std::sync::mpsc;

enum Event {
    Ready,
    Finished(Result<(), String>),
}
pub(super) struct Observer {
    stop: Arc<AtomicBool>,
    events: Option<mpsc::Receiver<Event>>,
    ready: bool,
    finished: bool,
    failure: Option<String>,
}
impl Observer {
    pub(super) fn start(
        lease: &ProducerLease,
        replacement: Option<ObservationCheck>,
    ) -> io::Result<Self> {
        let check = replacement.or_else(|| {
            lease
                ._owner
                .as_ref()
                .and_then(|owner| Arc::clone(owner).observation())
        });
        let stop = Arc::new(AtomicBool::new(false));
        let Some(mut check) = check else {
            return Ok(Self {
                stop,
                events: None,
                ready: true,
                finished: true,
                failure: None,
            });
        };
        let owned = lease.clone();
        let stopping = Arc::clone(&stop);
        // At most one Ready and one Finished event; no output-sized channel.
        let (send, events) = mpsc::sync_channel(2);
        std::thread::Builder::new()
            .name("asb-install-observer".into())
            .spawn(move || {
                let keepalive = owned;
                let mut ready = false;
                let result = loop {
                    let final_sample = stopping.load(Ordering::Acquire);
                    let observed =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut check))
                            .unwrap_or_else(|_| {
                                Err("installation resource observer panicked".into())
                            });
                    if let Err(error) = observed {
                        break Err(error);
                    }
                    if !ready {
                        ready = true;
                        if send.send(Event::Ready).is_err() {
                            break Ok(());
                        }
                    }
                    if final_sample {
                        break Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                // Release both actual-owner references BEFORE promising completion.
                // A blocked check retains them; neither parent nor supervisor joins.
                drop(check);
                drop(keepalive);
                let _ = send.send(Event::Finished(result));
            })?;
        Ok(Self {
            stop,
            events: Some(events),
            ready: false,
            finished: false,
            failure: None,
        })
    }
    pub(super) fn poll(&mut self) {
        let Some(events) = &self.events else { return };
        loop {
            match events.try_recv() {
                Ok(Event::Ready) => self.ready = true,
                Ok(Event::Finished(result)) => {
                    self.finished = true;
                    if let Err(error) = result {
                        self.failure = Some(error);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if !self.finished {
                        self.failure =
                            Some("installation resource observation completion was lost".into());
                        self.finished = true;
                    }
                    break;
                }
            }
        }
    }
    pub(super) fn ready(&self) -> bool {
        self.ready
    }
    pub(super) fn finished(&self) -> bool {
        self.finished
    }
    pub(super) fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        self.request_stop();
    }
}
