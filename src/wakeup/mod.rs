//! A doorbell for the reactor thread.
//!
//! The reactor spends its life asleep inside `poll()`, waiting on a fixed list
//! of file descriptors. If another thread adds a socket or a timer, the reactor
//! has no idea - it is still waiting on the old list. So we give it one extra
//! thing to watch: a pipe we own both ends of. Writing a byte into that pipe
//! makes `poll()` return, and the reactor starts over with the new list.
//!
//! This is the classic "self-pipe trick", and it is the only way to interrupt a
//! blocking `poll()` from outside.

use std::io;
use std::os::fd::RawFd;

/// A pipe used purely as a signal - the bytes in it mean nothing.
pub(crate) struct Wakeup {
    read_fd: RawFd,
    write_fd: RawFd,
}

impl Wakeup {
    /// Open the pipe. Both ends are made non-blocking so that neither
    /// [`trigger`](Self::trigger) nor [`clear`](Self::clear) can ever get stuck.
    pub(crate) fn new() -> io::Result<Self> {
        let mut fds = [0; 2];
        // libc::pipe fills fd[0] = read end and fd[1] = write_end
        let res = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        // so that the trigger/clear never block
        for fd in fds {
            set_nonblocking(fd)?;
        }
        Ok(Wakeup {
            read_fd: fds[0],
            write_fd: fds[1],
        })
    }

    /// The end the reactor watches. It adds this to its poll set with
    /// `POLLIN`, so a byte arriving counts as "something to do".
    pub(crate) fn read_fd(&self) -> RawFd {
        self.read_fd
    }

    /// Ring the doorbell: write one byte, which makes the reactor's `poll()`
    /// return.
    ///
    /// Safe to call from any thread, as often as you like.
    pub(crate) fn trigger(&self) -> io::Result<()> {
        let byte = [1u8];

        let res = unsafe { libc::write(self.write_fd, byte.as_ptr() as *const _, 1) };

        if res < 0 {
            let e = io::Error::last_os_error();
            // "Would block" means the pipe is already full of unread bytes -
            // which means a wakeup is already pending. Nothing to do.
            if e.kind() != io::ErrorKind::WouldBlock {
                return Err(e);
            }
        }
        Ok(())
    }

    /// Empty the pipe after waking up.
    ///
    /// Without this the leftover byte would keep the pipe readable, and the
    /// next `poll()` would return instantly forever - a busy loop.
    pub(crate) fn clear(&self) {
        let mut buf = [0u8; 128];
        loop {
            // Reads only from our own pipe, never a watched socket - the fd is
            // right there in the call.
            let res = unsafe { libc::read(self.read_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if res <= 0 {
                break;
            }
        }
    }
}

/// Flip an fd into non-blocking mode: reads and writes return "would block"
/// instead of parking the thread.
fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    let res = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if res < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
