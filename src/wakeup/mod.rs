use std::io;
use std::os::fd::RawFd;

/// Wakeup that is watched by Reactor. Triggering it wakes lib::poll
pub(crate) struct Wakeup {
    read_fd: RawFd,
    write_fd: RawFd,
}

impl Wakeup {
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

    /// Get read fd: reactor should watch for POLLIN in its poll set
    pub(crate) fn read_fd(&self) -> RawFd {
        self.read_fd
    }

    /// Wake the reactor: write one byte to the pipe.
    pub(crate) fn trigger(&self) -> io::Result<()> {
        let byte = [1u8];

        let res = unsafe { libc::write(self.write_fd, byte.as_ptr() as *const _, 1) };

        if res < 0 {
            let e = io::Error::last_os_error();
            // this will come even if pipe is full but that is fine as pending byte already means wake
            if e.kind() != io::ErrorKind::WouldBlock {
                return Err(e);
            }
        }
        Ok(())
    }

    /// Drain the pipe after waking, so the next poll doesn't fire immediately.
    pub(crate) fn clear(&self) {
        let mut buf = [0u8; 128];
        loop {
            // here we are draining the bytes from pipe .. not others .. look at signature
            let res = unsafe { libc::read(self.read_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if res <= 0 {
                break;
            }
        }
    }
}

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
