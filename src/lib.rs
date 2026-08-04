pub mod runtime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("runtime is shutting down")]
    Shutdown,
}

pub type Result<T> = std::result::Result<T, Error>;
