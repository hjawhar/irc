//! Per-connection user state.
//!
//! Every live connection owns exactly one [`User`]. The struct
//! aggregates identity (nick, user name, realname), the outbound
//! write queue, and a small mutable bag of registration flags.

use std::net::SocketAddr;

use bytes::Bytes;
use irc_proto::Message;
use parking_lot::RwLock;
use tokio::sync::mpsc;

/// Unique per-connection identifier, minted by [`crate::ServerState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserId(u64);

impl UserId {
    /// Construct from a raw `u64`. Outside of the server core, mint
    /// via [`crate::ServerState::next_user_id`] instead of this.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Underlying numeric value, for logging and debugging.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Registration-time info captured from the `USER` command.
#[derive(Debug, Clone)]
pub struct UserRegInfo {
    /// `user` parameter (historically the local username).
    pub user_name: Bytes,
    /// Realname (trailing param).
    pub realname: Bytes,
}

/// Mutable fields grouped under a single [`RwLock`] so every
/// registration-state change is observed atomically.
#[derive(Debug, Default, Clone)]
#[allow(clippy::struct_excessive_bools)] // IRC user modes are a flag bag
pub struct UserInner {
    /// Current nickname. `None` until NICK is accepted.
    pub nick: Option<Bytes>,
    /// `USER` registration info. `None` until USER is accepted.
    pub reg: Option<UserRegInfo>,
    /// Optional password supplied via `PASS` before registration.
    pub pass: Option<Bytes>,
    /// Whether the client is currently negotiating CAP. Registration
    /// holds while this is true.
    pub cap_negotiating: bool,
    /// Whether the welcome flow has been sent.
    pub registered: bool,
    /// Host string as seen on the wire (cloak applied later in
    /// Phase 3; for now it is the raw peer address).
    pub host: Bytes,
    /// `+i` — invisible mode.
    pub mode_i: bool,
    /// `+w` — wallops mode.
    pub mode_w: bool,
}

/// A connected user: identity + outbound write pipe.
#[derive(Debug)]
pub struct User {
    id: UserId,
    peer: SocketAddr,
    outgoing: mpsc::Sender<Message>,
    inner: RwLock<UserInner>,
}

/// Cheap-to-clone reference to a [`User`]. Useful for iterators that
/// must outlive a [`dashmap::DashMap`] guard.
#[derive(Debug, Clone)]
pub struct UserHandle(pub std::sync::Arc<User>);

impl UserHandle {
    /// Dereference to the wrapped [`User`].
    #[must_use]
    pub fn user(&self) -> &User {
        &self.0
    }
}

impl User {
    /// Construct a freshly-accepted user that has not yet sent NICK
    /// or USER.
    #[must_use]
    pub fn new(id: UserId, peer: SocketAddr, outgoing: mpsc::Sender<Message>) -> Self {
        let host = Bytes::from(peer.ip().to_string().into_bytes());
        Self {
            id,
            peer,
            outgoing,
            inner: RwLock::new(UserInner {
                host,
                ..UserInner::default()
            }),
        }
    }

    /// Wrap the user in an `Arc` — the form every callsite works with.
    #[must_use]
    #[allow(clippy::should_implement_trait)] // `into` clearer than From at call sites
    pub fn into(self) -> std::sync::Arc<Self> {
        std::sync::Arc::new(self)
    }

    /// User identifier.
    #[must_use]
    pub const fn id(&self) -> UserId {
        self.id
    }

    /// Remote socket address as seen by `accept()`.
    #[must_use]
    pub const fn peer(&self) -> SocketAddr {
        self.peer
    }

    /// Snapshot of the mutable fields under a read lock.
    #[must_use]
    pub fn snapshot(&self) -> UserInner {
        self.inner.read().clone()
    }

    /// Has registration completed (welcome flow sent)?
    #[must_use]
    pub fn is_registered(&self) -> bool {
        self.inner.read().registered
    }

    /// Set the nickname after a successful NICK.
    pub fn set_nick(&self, nick: Bytes) {
        self.inner.write().nick = Some(nick);
    }

    /// Stash registration info from USER.
    pub fn set_reg_info(&self, reg: UserRegInfo) {
        self.inner.write().reg = Some(reg);
    }

    /// Record a password from PASS.
    pub fn set_pass(&self, pass: Bytes) {
        self.inner.write().pass = Some(pass);
    }

    /// Flip cap-negotiating on (first CAP) or off (CAP END).
    pub fn set_cap_negotiating(&self, v: bool) {
        self.inner.write().cap_negotiating = v;
    }

    /// Mark the user as fully registered.
    pub fn mark_registered(&self) {
        self.inner.write().registered = true;
    }

    /// Obtain a write guard to the inner mutable fields.
    pub fn inner_write(&self) -> parking_lot::RwLockWriteGuard<'_, UserInner> {
        self.inner.write()
    }

    /// Fire-and-forget send to the outbound write pipe. Drops the
    /// message and returns `false` if the buffer is full; at MVP we
    /// log and drop silently, Phase 5 tightens with a kill policy.
    pub fn send(&self, msg: Message) -> bool {
        self.outgoing.try_send(msg).is_ok()
    }

    /// Wire-form `nick!user@host` prefix for messages the user
    /// originates on the server. Falls back to best-effort for
    /// partially-registered users.
    #[must_use]
    pub fn origin_prefix(&self) -> Bytes {
        let inner = self.inner.read();
        let nick = inner
            .nick
            .clone()
            .unwrap_or_else(|| Bytes::from_static(b"*"));
        let user = inner
            .reg
            .as_ref()
            .map_or_else(|| Bytes::from_static(b"*"), |r| r.user_name.clone());
        let host = inner.host.clone();
        drop(inner);
        compose_prefix(&nick, &user, &host)
    }

    /// Wire-form prefix using an explicit nickname. Useful for NICK
    /// change notifications where the *old* nick must appear in the
    /// origin even though `self` has already been updated.
    #[must_use]
    pub fn origin_prefix_with_nick(&self, nick: &[u8]) -> Bytes {
        let inner = self.inner.read();
        let user = inner
            .reg
            .as_ref()
            .map_or_else(|| Bytes::from_static(b"*"), |r| r.user_name.clone());
        let host = inner.host.clone();
        drop(inner);
        compose_prefix(nick, &user, &host)
    }
}

fn compose_prefix(nick: &[u8], user: &[u8], host: &[u8]) -> Bytes {
    let mut out = Vec::with_capacity(nick.len() + user.len() + host.len() + 2);
    out.extend_from_slice(nick);
    out.push(b'!');
    out.extend_from_slice(user);
    out.push(b'@');
    out.extend_from_slice(host);
    Bytes::from(out)
}
