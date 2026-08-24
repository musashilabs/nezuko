//! Async TCP, using the standard library's own types.
//!
//! There is no `nezuko::TcpStream` here. These are free functions that take
//! `std::net::TcpListener` / `TcpStream` and make them await-able, which keeps
//! the crate small and lets you see exactly where the async part happens.
//!
//! **You must set the socket to non-blocking yourself** with
//! `set_nonblocking(true)` before handing it to these functions. That is what
//! makes the OS say "not ready yet" instead of parking the thread - and "not
//! ready yet" is precisely the moment these functions hand the socket to the
//! reactor and return `Pending`.
//!
//! ```no_run
//! use nezuko::{Runtime, accept, spawn, write_all};
//! use std::net::TcpListener;
//!
//! let rt = Runtime::new().unwrap();
//!
//! rt.block_on(async {
//!     let mut listener = TcpListener::bind("127.0.0.1:8080").unwrap();
//!     listener.set_nonblocking(true).unwrap();
//!
//!     loop {
//!         let (mut socket, _addr) = accept(&mut listener).await.unwrap();
//!         // One task per connection; the loop is free to take the next one.
//!         spawn(async move {
//!             write_all(b"hello\n", &mut socket).await.unwrap();
//!         });
//!     }
//! });
//! ```
//!
//! All three functions follow the same shape, so read one and you have read
//! them all: try the operation; if it succeeds you're done, and if it comes
//! back `WouldBlock`, register the fd with the reactor and return `Pending`.

use std::{
    future::poll_fn,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::fd::AsRawFd,
    task::Poll,
};

use crate::runtime::Handle;

/// Wait for someone to connect, without blocking the thread.
///
/// Gives you the new connection and the address it came from. The returned
/// stream is already set to non-blocking, so you can pass it straight to
/// [`write_all`] or [`print_all`].
///
/// `listener` must be non-blocking: call `set_nonblocking(true)` on it first.
///
/// ```no_run
/// # use nezuko::{Runtime, accept};
/// # use std::net::TcpListener;
/// # let rt = Runtime::new().unwrap();
/// # rt.block_on(async {
/// let mut listener = TcpListener::bind("127.0.0.1:9000").unwrap();
/// listener.set_nonblocking(true).unwrap();
///
/// let (stream, addr) = accept(&mut listener).await.unwrap();
/// println!("connected from {addr}");
/// # });
/// ```
pub async fn accept(listener: &mut TcpListener) -> io::Result<(TcpStream, SocketAddr)> {
    poll_fn(|context| match listener.accept() {
        Ok((stream, addr)) => {
            stream.set_nonblocking(true)?;
            Poll::Ready(Ok((stream, addr)))
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
            Handle::current().register_io(
                listener.as_raw_fd(),
                libc::POLLIN,
                context.waker().clone(),
            );
            Poll::Pending
        }
        Err(e) => Poll::Ready(Err(e)),
    })
    .await
}

/// Send every byte of `buf` down `stream`.
///
/// A single write often only takes part of the buffer, so this keeps going
/// until it is all gone, waiting whenever the socket's send buffer fills up.
///
/// ```no_run
/// # use nezuko::{Runtime, write_all};
/// # use std::net::TcpStream;
/// # let rt = Runtime::new().unwrap();
/// # rt.block_on(async {
/// let mut stream = TcpStream::connect("127.0.0.1:9000").unwrap();
/// stream.set_nonblocking(true).unwrap();
///
/// write_all(b"GET / HTTP/1.0\r\n\r\n", &mut stream).await.unwrap();
/// # });
/// ```
///
/// # Errors
///
/// Any socket error, plus [`WriteZero`](io::ErrorKind::WriteZero) if the peer
/// stops accepting data before the buffer is finished.
pub async fn write_all(mut buf: &[u8], stream: &mut TcpStream) -> io::Result<()> {
    poll_fn(|context| {
        while !buf.is_empty() {
            match stream.write(buf) {
                Ok(0) => {
                    return Poll::Ready(Err(io::Error::from(io::ErrorKind::WriteZero)));
                }
                Ok(n) => buf = &buf[n..],
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    Handle::current().register_io(
                        stream.as_raw_fd(),
                        libc::POLLOUT,
                        context.waker().clone(),
                    );
                    return Poll::Pending;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(()))
    })
    .await
}

/// Read from `stream` and print it to stdout until the other side hangs up.
///
/// Handy for a quick client or for watching what a server sends back.
///
/// ```no_run
/// # use nezuko::{Runtime, print_all};
/// # use std::net::TcpStream;
/// # let rt = Runtime::new().unwrap();
/// # rt.block_on(async {
/// let mut stream = TcpStream::connect("127.0.0.1:9000").unwrap();
/// stream.set_nonblocking(true).unwrap();
///
/// // Returns once the server closes the connection.
/// print_all(&mut stream).await.unwrap();
/// # });
/// ```
pub async fn print_all(stream: &mut TcpStream) -> io::Result<()> {
    poll_fn(|context| {
        loop {
            let mut buf = [0; 1024];
            match stream.read(&mut buf) {
                Ok(0) => return Poll::Ready(Ok(())), // EOF
                Ok(n) => io::stdout().write_all(&buf[..n])?,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    Handle::current().register_io(
                        stream.as_raw_fd(),
                        libc::POLLIN,
                        context.waker().clone(),
                    );
                    return Poll::Pending;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    })
    .await
}
