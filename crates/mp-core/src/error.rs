//! Structured error types for Moneypenny operations.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MpError {
    #[error("policy denied: {reason}")]
    PolicyDenied {
        reason: String,
        policy_id: Option<String>,
    },

    #[error("not found: {resource}")]
    NotFound { resource: String },

    #[error("invalid arguments: {message}")]
    InvalidArgs { message: String },

    #[error("idempotency conflict: key {key} already used")]
    IdempotencyConflict { key: String },

    #[error("operation failed: {message}")]
    OperationFailed { message: String },

    #[error(transparent)]
    Database(#[from] rusqlite::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
