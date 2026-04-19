//! Single-node IRC daemon.
//!
//! The `irc-server` binary is a thin wrapper around the types and
//! entrypoints re-exported from this library: [`Config`], [`Server`],
//! and [`ServerError`]. Integration tests — both in this crate and in
//! `irc-testkit` downstream — drive the server via these types so the
//! test harness and the `main.rs` binary share the same code path.

#![deny(missing_docs)]

pub mod config;
pub mod connection;
pub mod error;
pub mod handler;
pub mod numeric;
pub mod runtime;
pub mod state;

pub use config::{Config, Limits, ListenerConfig};
pub use error::ServerError;
pub use runtime::{Server, ShutdownHandle};
pub use state::{ServerState, User, UserHandle, UserId};
