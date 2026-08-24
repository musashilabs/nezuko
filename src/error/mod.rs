//! What can go wrong.

/// Anything the runtime can fail with.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Something at the OS level failed - opening the wakeup pipe, a socket
    /// call, and so on.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The runtime is on its way down and won't take new work.
    #[error("runtime is shutting down")]
    Shutdown,
}

/// `Result` with `Error` already filled in, so you can write
/// `nezuko::Result<T>` instead of the long form.
pub type Result<T> = std::result::Result<T, Error>;
