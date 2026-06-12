use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("history is empty")]
    EmptyHistory,
    #[error("item not found: {0}")]
    NotFound(uuid::Uuid),
    #[error("text length {len} is outside allowed bounds ({min}..={max})")]
    TextSizeOutOfBounds { len: usize, min: usize, max: usize },
    #[error("navigation at boundary (index {index}, len {len})")]
    NavigationBoundary { index: usize, len: usize },
    #[error("active index {index} out of range for history length {len}")]
    ActiveIndexOutOfRange { index: usize, len: usize },
}

pub type Result<T> = std::result::Result<T, Error>;