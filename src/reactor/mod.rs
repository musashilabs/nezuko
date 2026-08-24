//! The reactor: the part that waits on the outside world.
//!
//! Tasks that are waiting on a socket aren't in the ready queue and aren't
//! using a thread - they are just registered here. The reactor collects all
//! those file descriptors, hands them to the OS in one `poll()` call, and
//! sleeps until the OS says one of them is ready. Then it wakes the matching
//! tasks, which puts them back on the queue.
//!
//! One `poll()` for all of them is the whole point: a thousand idle
//! connections cost one sleeping thread, not a thousand.

use crate::wakeup::Wakeup;
use std::collections::HashSet;
use std::io;
use std::os::fd::RawFd;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Waker;

/// One "wake me when this socket is ready" request.
///
/// The `id` exists because two tasks can be waiting on the same `fd`; the id
/// is what tells them apart when it is time to remove the one that fired.
struct Registration {
    id: u64,
    fd: RawFd,
    events: libc::c_short,
    waker: Waker,
}

/// Holds the current set of "wake me when..." requests and does the waiting.
pub(crate) struct Reactor {
    registrations: Mutex<Vec<Registration>>,
    next_id: AtomicU64,
    /// A self-pipe, so a thread that isn't the reactor can interrupt its sleep.
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

    /// Watch `fd` for `events` and call `waker` once it is ready.
    ///
    /// The trigger at the end matters: the reactor is probably already asleep
    /// in a `poll()` that knows nothing about this fd, so it has to be woken to
    /// rebuild its list.
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

    /// Interrupt the reactor's sleep so it takes a fresh look at its work.
    pub(crate) fn wakeup_trigger(&self) -> io::Result<()> {
        self.wakeup.trigger()
    }

    /// Sleep until something is ready (or `timeout_ms` passes), then wake the
    /// tasks waiting on whatever became ready.
    ///
    /// Pass `-1` for "no timeout, wait as long as it takes". This is the one
    /// blocking call in the whole runtime, and the reactor thread sits in it in
    /// a loop - see [`run_reactor`](crate::runtime::run_reactor).
    pub(crate) fn poll_and_wake(&self, timeout_ms: libc::c_int) -> io::Result<()> {
        // Snapshot the registrations into the array shape poll() wants, and
        // release the lock before sleeping - holding it would deadlock anyone
        // trying to register while we wait.
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

            // Always watch the wakeup pipe too, so we can be interrupted.
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
            // A signal cut the wait short. Nothing is wrong; the caller's loop
            // will just come back around.
            if err.kind() == io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(err);
        }

        // drain wakeup fd so the next poll does not return immediately
        self.wakeup.clear();

        // poll() writes what happened back into revents; anything non-zero is
        // an fd that is now ready. Note the zip drops the wakeup pipe we pushed
        // on the end, since ids is one shorter.
        let ready: HashSet<u64> = ids
            .iter()
            .zip(poll_fds.iter())
            .filter(|(_, poll_fd)| poll_fd.revents != 0)
            .map(|(id, _)| *id)
            .collect();

        if ready.is_empty() {
            return Ok(());
        }

        // Registrations are one-shot: drop the ones that fired and keep the
        // rest. A future that still isn't done registers again next poll.
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
