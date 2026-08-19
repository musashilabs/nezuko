use std::os::fd::RawFd;
use std::task::Waker;

struct Registration {
    fd: RawFd,
    events: libc::c_short,
    waker: Waker,
}
pub(crate) struct Reactor {
    registrations: Vec<Registration>,
}

impl Reactor {
    pub(crate) fn new() -> Self {
        Reactor {
            registrations: Vec::new(),
        }
    }

    pub(crate) fn register(&mut self, fd: RawFd, events: libc::c_short, waker: Waker) {
        self.registrations.push(Registration { fd, events, waker });
    }
}
// pub static POLL_FDS: Mutex<Vec<libc::pollfd>> = Mutex::new(Vec::new());
// pub static POLL_WAKERS: Mutex<Vec<Waker>> = Mutex::new(Vec::new());

// pub fn register_pollfd(context: &mut Context, fd: &impl AsRawFd, events: libc::c_short) {
//     let mut poll_fds = POLL_FDS.lock().unwrap();
//     let mut poll_wakers = POLL_WAKERS.lock().unwrap();
//     poll_fds.push(libc::pollfd {
//         fd: fd.as_raw_fd(),
//         events,
//         revents: 0,
//     });
//     poll_wakers.push(context.waker().clone());
// }
