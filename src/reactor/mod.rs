use std::{os::fd::AsRawFd, sync::Mutex, task::{Context, Waker}};

pub static POLL_FDS: Mutex<Vec<libc::pollfd>> = Mutex::new(Vec::new());
pub static POLL_WAKERS: Mutex<Vec<Waker>> = Mutex::new(Vec::new());


pub fn register_pollfd(context: &mut Context, fd: &impl AsRawFd, events: libc::c_short) {
    let mut poll_fds = POLL_FDS.lock().unwrap();
    let mut poll_wakers = POLL_WAKERS.lock().unwrap();
    poll_fds.push(libc::pollfd {
        fd: fd.as_raw_fd(),
        events,
        revents: 0,
    });
    poll_wakers.push(context.waker().clone());
}