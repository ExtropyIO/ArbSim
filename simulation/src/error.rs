//! Error types for the simulation framework.

use thiserror::Error;

/// Result type alias for simulation operations
pub type Result<T> = std::result::Result<T, Error>;

/// Simulation framework errors
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Arithmetic error (overflow, underflow, division by zero)
    #[error("Arithmetic error: {0}")]
    Arithmetic(String),

    /// Unsupported chain
    #[error("Unsupported chain ID: {0}")]
    UnsupportedChain(u64),

    /// Unsupported DEX type
    #[error("Unsupported DEX type: {0}")]
    UnsupportedDex(String),

    /// Pool not found
    #[error("Pool not found: {0}")]
    PoolNotFound(String),

    /// Insufficient liquidity (simple variant)
    #[error("Insufficient liquidity in pool")]
    InsufficientLiquidity,

    /// Insufficient liquidity (detailed variant)
    #[error("Insufficient liquidity in pool {pool}: need {needed}, have {available}")]
    InsufficientLiquidityDetailed {
        pool: String,
        needed: String,
        available: String,
    },

    /// Insufficient balance
    #[error("Insufficient balance for {token}: need {needed}, have {available}")]
    InsufficientBalance {
        token: String,
        needed: String,
        available: String,
    },

    /// Slippage exceeded
    #[error("Slippage exceeded: expected {expected}, got {actual}")]
    SlippageExceeded { expected: String, actual: String },

    /// Transaction deadline expired
    #[error("Transaction deadline expired")]
    DeadlineExpired,

    /// Bot registration error
    #[error("Bot registration error: {0}")]
    BotRegistration(String),

    /// Bot not found
    #[error("Bot not found: {0}")]
    BotNotFound(String),

    /// Simulation not running
    #[error("Simulation is not running")]
    NotRunning,

    /// Simulation already running
    #[error("Simulation is already running")]
    AlreadyRunning,

    /// State fork error
    #[error("Failed to fork state: {0}")]
    StateFork(String),

    /// RPC error
    #[error("RPC error: {0}")]
    Rpc(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Create an RPC error
    pub fn rpc(msg: impl Into<String>) -> Self {
        Self::Rpc(msg.into())
    }
}
