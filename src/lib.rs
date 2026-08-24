//! A small async runtime you can read in an afternoon.
//!
//! Writing `async fn` in Rust does not actually run anything. It builds a
//! value called a **future**: a paused piece of work that knows how to make a
//! little progress every time someone asks it to. Something has to do the
//! asking, keep track of the work that is still waiting, and wake it back up
//! at the right moment. That something is a runtime, and `nezuko` is a very
//! small one.
//!
//! You give it work with [`spawn`], you wait for the result, and it takes care
//! of running everything at the same time on a handful of threads.
//!
//! # Quick start
//!
//! ```
//! use nezuko::{Runtime, sleep, spawn};
//! use std::time::Duration;
//!
//! let rt = Runtime::new().unwrap();
//!
//! rt.block_on(async {
//!     // Both tasks start right away and nap at the same time,
//!     // so this whole block takes ~50ms, not ~100ms.
//!     let a = spawn(async {
//!         sleep(Duration::from_millis(50)).await;
//!         "hello"
//!     });
//!     let b = spawn(async {
//!         sleep(Duration::from_millis(50)).await;
//!         "world"
//!     });
//!
//!     println!("{} {}", a.await, b.await);
//! });
//! ```
//!
//! # What's in the box
//!
//! - [`Runtime`] - start here. Owns the threads and hands you [`block_on`].
//! - [`spawn`] - run a future alongside the others.
//! - [`sleep`] - wait for a while without blocking a thread.
//! - [`accept`], [`write_all`], [`print_all`] - tiny async TCP helpers.
//!
//! [`block_on`]: Runtime::block_on
//!
//! # A TCP server
//!
//! The networking helpers work on the standard library's own socket types -
//! there is no `nezuko::TcpStream`. You do have to call `set_nonblocking(true)`
//! yourself: that is what makes the OS answer "not ready yet" instead of
//! parking the thread, and "not ready yet" is the moment nezuko takes over and
//! goes to run something else.
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
//!         // One task per connection, so the loop is free to take the next one.
//!         spawn(async move {
//!             write_all(b"hello\n", &mut socket).await.unwrap();
//!         });
//!     }
//! });
//! ```
//!
//! # How it fits together
//!
//! A map for anyone reading the source. Follow the modules in this order:
//!
//! | module     | its job                                                         |
//! | ---------- | --------------------------------------------------------------- |
//! | `task`     | one unit of work, plus the queue of work that is ready to run    |
//! | `runtime`  | the public entry point, the worker threads, and the shared state |
//! | `reactor`  | one thread that watches sockets and timers, and wakes tasks      |
//! | `wakeup`   | the pipe used to interrupt the reactor when something changes    |
//! | `time`     | `sleep`, built on the reactor's timer list                       |
//! | `net`      | async TCP helpers, built on the reactor's socket watching        |
//! | `channel`  | a one-shot channel used to get `block_on`'s result back          |
//!
//! The short version of the loop: a task is pushed onto the ready queue, a
//! worker thread pops it and polls it, and the task either finishes or says
//! "not yet" and leaves behind a **waker** - a callback that pushes it back
//! onto the queue. The reactor is what calls that waker once the socket is
//! readable or the timer is up.

#![allow(dead_code)]
mod error;
pub use error::Result;
mod reactor;
mod runtime;
pub use runtime::{Runtime, spawn};
mod task;
mod time;
pub use time::sleep;

mod net;
pub use net::{accept, print_all, write_all};
mod channel;
mod wakeup;
