use crate::wakeup::Wakeup;
use std::collections::HashSet;
use std::io;
use std::os::fd::RawFd;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Waker;

struct Registration {
    id: u64,
    fd: RawFd,
    events: libc::c_short,
    waker: Waker,
}

pub(crate) struct Reactor {
    registrations: Mutex<Vec<Registration>>,
    next_id: AtomicU64,
    wakeup: Wakeup,
}

impl Reactor {
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Reactor {
            registrations: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(0),
            wakeup: Wakeup::new()?,
        })
    }

    pub(crate) fn register(&self, fd: RawFd, events: libc::c_short, waker: Waker) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.registrations.lock().unwrap().push(Registration {
            id,
            fd,
            events,
            waker,
        });

        let _ = self.wakeup.trigger();
    }

    pub(crate) fn wakeup_trigger(&self) -> io::Result<()> {
        self.wakeup.trigger()
    }

    pub(crate) fn poll_and_wake(&self, timeout_ms: libc::c_int) -> io::Result<()> {
        let (ids, mut poll_fds) = {
            let registrations = self.registrations.lock().unwrap();
            let ids: Vec<u64> = registrations.iter().map(|reg| reg.id).collect();
            let mut poll_fds: Vec<libc::pollfd> = registrations
                .iter()
                .map(|reg| libc::pollfd {
                    fd: reg.fd,
                    events: reg.events,
                    revents: 0,
                })
                .collect();

            poll_fds.push(libc::pollfd {
                fd: self.wakeup.read_fd(),
                events: libc::POLLIN,
                revents: 0,
            });

            (ids, poll_fds)
        };

        let poll_result = unsafe {
            libc::poll(
                poll_fds.as_mut_ptr(),
                poll_fds.len() as libc::nfds_t,
                timeout_ms,
            )
        };

        if poll_result < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(err);
        }

        // drain wakeup fd so the next poll does not return immediately
        self.wakeup.clear();

        let ready: HashSet<u64> = ids
            .iter()
            .zip(poll_fds.iter())
            .filter(|(_, poll_fd)| poll_fd.revents != 0)
            .map(|(id, _)| *id)
            .collect();

        if ready.is_empty() {
            return Ok(());
        }

        let mut wakers = Vec::new();
        self.registrations.lock().unwrap().retain(|reg| {
            if ready.contains(&reg.id) {
                wakers.push(reg.waker.clone());
                false
            } else {
                true
            }
        });

        wakers.into_iter().for_each(Waker::wake);
        Ok(())
    }
}
