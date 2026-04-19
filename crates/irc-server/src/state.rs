//! Shared runtime state.
//!
//! Phase 2a ships only the skeleton: a counter for minting user IDs
//! and a handle to the server [`Config`]. Registries for users,
//! nicks, and channels land in a follow-up commit.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::Config;

/// Unique per-connection identifier, minted by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserId(u64);

impl UserId {
    /// Convert to a raw `u64` for logging and debugging.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Shared server-wide state. Cloned via `Arc` into every connection
/// task; mutation is internal to the state and is serialised per its
/// own concurrency discipline.
#[derive(Debug)]
pub struct ServerState {
    config: Arc<Config>,
    next_user_id: AtomicU64,
}

impl ServerState {
    /// Construct a fresh state snapshot from a validated [`Config`].
    #[must_use]
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            next_user_id: AtomicU64::new(1),
        }
    }

    /// Access the active configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Mint the next [`UserId`]. Zero is reserved and never issued.
    pub fn next_user_id(&self) -> UserId {
        UserId(self.next_user_id.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::ServerState;
    use crate::config::Config;
    use std::sync::Arc;

    #[test]
    fn user_ids_are_monotonically_unique() {
        let cfg = Arc::new(Config::builder().build().unwrap());
        let state = ServerState::new(cfg);
        let a = state.next_user_id();
        let b = state.next_user_id();
        assert_ne!(a, b);
        assert_eq!(a.get() + 1, b.get());
    }
}
