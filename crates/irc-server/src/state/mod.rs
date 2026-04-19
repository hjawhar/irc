//! Shared runtime state.
//!
//! Phase 2b wires the user registry and nickname index. Channel state
//! lands in the next commit; the surface here is deliberately narrow
//! so every mutation goes through a method that updates both the user
//! map and the nickname map under the same lock discipline.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use dashmap::DashMap;
use irc_proto::Casemap;

use crate::config::Config;

pub mod user;

pub use user::{User, UserHandle, UserId, UserRegInfo};

/// Errors returned when mutating the [`ServerState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// A nickname collision: the requested nick is already owned by a
    /// different [`UserId`].
    NickInUse,
    /// The caller referenced a [`UserId`] that is no longer present.
    UnknownUser,
}

/// Shared server-wide state. Cloned via `Arc` into every connection
/// task.
#[derive(Debug)]
pub struct ServerState {
    config: Arc<Config>,
    users: DashMap<UserId, Arc<User>>,
    /// Casemap-folded nickname → owning [`UserId`].
    nicks: DashMap<Bytes, UserId>,
    casemap: Casemap,
    next_user_id: AtomicU64,
}

impl ServerState {
    /// Construct a fresh state snapshot from a validated [`Config`].
    #[must_use]
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            users: DashMap::new(),
            nicks: DashMap::new(),
            casemap: Casemap::Rfc1459,
            next_user_id: AtomicU64::new(1),
        }
    }

    /// Access the active configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Active casemap for nickname and channel comparisons.
    #[must_use]
    pub const fn casemap(&self) -> Casemap {
        self.casemap
    }

    /// Mint the next [`UserId`]. Zero is reserved and never issued.
    pub fn next_user_id(&self) -> UserId {
        UserId::new(self.next_user_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Insert a freshly-accepted (pre-registration) user.
    pub fn insert_user(&self, user: Arc<User>) {
        self.users.insert(user.id(), user);
    }

    /// Remove a user and release any nickname it held. Used when a
    /// connection closes. Idempotent.
    pub fn remove_user(&self, id: UserId) -> Option<Arc<User>> {
        let user = self.users.remove(&id).map(|(_, v)| v)?;
        if let Some(nick) = user.snapshot().nick {
            let folded = self.casemap.fold(nick.as_ref());
            // Only evict the nick entry if it still points at this user
            // (a NICK change could have already claimed the slot).
            if let Some(entry) = self.nicks.get_mut(&folded) {
                if *entry.value() == id {
                    drop(entry);
                    self.nicks.remove(&folded);
                }
            }
        }
        Some(user)
    }

    /// Look up a live [`User`] by id.
    #[must_use]
    pub fn user(&self, id: UserId) -> Option<Arc<User>> {
        self.users.get(&id).map(|r| r.value().clone())
    }

    /// Look up a live [`User`] by nickname using the active casemap.
    #[must_use]
    pub fn user_by_nick(&self, nick: &[u8]) -> Option<Arc<User>> {
        let folded = self.casemap.fold(nick);
        let id = *self.nicks.get(&folded)?.value();
        self.user(id)
    }

    /// Reserve `nick` for `user_id`. Fails if the nickname is already
    /// owned by a different user. Releases any prior nickname the user
    /// held.
    pub fn claim_nick(&self, user_id: UserId, nick: &[u8]) -> Result<(), StateError> {
        let folded = self.casemap.fold(nick);
        // If the slot exists and belongs to somebody else, refuse.
        if let Some(entry) = self.nicks.get(&folded) {
            if *entry.value() != user_id {
                return Err(StateError::NickInUse);
            }
            // Same user re-claiming its own nick — no-op.
            return Ok(());
        }
        // Release the prior nickname, if any.
        let prior = self
            .user(user_id)
            .ok_or(StateError::UnknownUser)?
            .snapshot()
            .nick;
        if let Some(old) = prior {
            let old_folded = self.casemap.fold(old.as_ref());
            if let Some(entry) = self.nicks.get(&old_folded) {
                if *entry.value() == user_id {
                    drop(entry);
                    self.nicks.remove(&old_folded);
                }
            }
        }
        self.nicks.insert(folded, user_id);
        Ok(())
    }

    /// Count currently-registered users. O(n); call sparingly.
    pub fn registered_count(&self) -> usize {
        self.users
            .iter()
            .filter(|e| e.value().is_registered())
            .count()
    }

    /// Iterate live [`UserHandle`]s. Cheap: each handle is a clone of
    /// an `Arc<User>`.
    pub fn users(&self) -> Vec<UserHandle> {
        self.users
            .iter()
            .map(|e| UserHandle(e.value().clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{ServerState, StateError, User, UserId};
    use crate::config::Config;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn mk_user(id: UserId) -> Arc<User> {
        let (tx, _rx) = mpsc::channel(32);
        User::new(id, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1), tx).into()
    }

    fn mk_state() -> ServerState {
        ServerState::new(Arc::new(Config::builder().build().unwrap()))
    }

    #[test]
    fn user_ids_are_monotonically_unique() {
        let state = mk_state();
        let a = state.next_user_id();
        let b = state.next_user_id();
        assert_ne!(a, b);
        assert_eq!(a.get() + 1, b.get());
    }

    #[test]
    fn claim_then_release_nick() {
        let state = mk_state();
        let id = state.next_user_id();
        state.insert_user(mk_user(id));
        state.claim_nick(id, b"alice").unwrap();
        assert!(state.user_by_nick(b"ALICE").is_some(), "casemap-aware");
        state.remove_user(id);
        assert!(state.user_by_nick(b"alice").is_none());
    }

    #[test]
    fn nick_collision_rejected() {
        let state = mk_state();
        let a = state.next_user_id();
        let b = state.next_user_id();
        state.insert_user(mk_user(a));
        state.insert_user(mk_user(b));
        state.claim_nick(a, b"alice").unwrap();
        assert_eq!(state.claim_nick(b, b"Alice"), Err(StateError::NickInUse));
    }

    #[test]
    fn reclaiming_own_nick_is_noop() {
        let state = mk_state();
        let id = state.next_user_id();
        state.insert_user(mk_user(id));
        state.claim_nick(id, b"alice").unwrap();
        state.claim_nick(id, b"alice").unwrap();
    }

    #[test]
    fn nick_change_releases_prior_slot() {
        let state = mk_state();
        let id = state.next_user_id();
        let user = mk_user(id);
        state.insert_user(user.clone());
        state.claim_nick(id, b"alice").unwrap();
        user.set_nick(bytes::Bytes::from_static(b"alice"));
        state.claim_nick(id, b"alice2").unwrap();
        user.set_nick(bytes::Bytes::from_static(b"alice2"));
        // Another user can now take "alice".
        let other = state.next_user_id();
        state.insert_user(mk_user(other));
        state.claim_nick(other, b"alice").unwrap();
    }
}
