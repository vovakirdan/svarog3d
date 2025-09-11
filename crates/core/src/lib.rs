//! Core shared types and errors (renderer-agnostic).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Generic error: {0}")]
    Generic(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
