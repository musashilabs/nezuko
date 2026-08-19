use std::{
    future::poll_fn,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::fd::AsRawFd,
    task::Poll,
};

use crate::runtime::register_io;

/// Async accept: wait for an incoming connection.
pub async fn accept(listener: &mut TcpListener) -> io::Result<(TcpStream, SocketAddr)> {
    poll_fn(|context| match listener.accept() {
        Ok((stream, addr)) => {
            stream.set_nonblocking(true)?;
            Poll::Ready(Ok((stream, addr)))
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
            register_io(listener.as_raw_fd(), libc::POLLIN, context.waker().clone());
            Poll::Pending
        }
        Err(e) => Poll::Ready(Err(e)),
    })
    .await
}

/// Async write: write all of `buf` to `stream`.
pub async fn write_all(mut buf: &[u8], stream: &mut TcpStream) -> io::Result<()> {
    poll_fn(|context| {
        while !buf.is_empty() {
            match stream.write(buf) {
                Ok(0) => {
                    return Poll::Ready(Err(io::Error::from(io::ErrorKind::WriteZero)));
                }
                Ok(n) => buf = &buf[n..],
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    register_io(stream.as_raw_fd(), libc::POLLOUT, context.waker().clone());
                    return Poll::Pending;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(()))
    })
    .await
}

/// Async read + print: copy everything from `stream` to stdout
pub async fn print_all(stream: &mut TcpStream) -> io::Result<()> {
    poll_fn(|context| {
        loop {
            let mut buf = [0; 1024];
            match stream.read(&mut buf) {
                Ok(0) => return Poll::Ready(Ok(())), // EOF
                Ok(n) => io::stdout().write_all(&buf[..n])?,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    register_io(stream.as_raw_fd(), libc::POLLIN, context.waker().clone());
                    return Poll::Pending;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    })
    .await
}
