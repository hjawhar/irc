//! IRC protocol primitives.
//!
//! Wire-level message parsing and serialization, with zero-copy byte
//! slicing via [`bytes::Bytes`]. Phase 1 ships the wire layer; the typed
//! command enum, numerics, codec, mode parsing, CTCP, and formatting
//! codes land in subsequent commits within this phase.
//!
//! The primary entry points are [`Message::parse`] and [`Message::write`].
//! See `PLAN.md` §3 for the full scope.

#![deny(missing_docs)]

pub mod error;
pub mod limits;
pub mod message;
pub mod params;
pub mod prefix;
pub mod tags;
pub mod verb;

mod util;

pub use error::ParseError;
pub use message::Message;
pub use params::Params;
pub use prefix::Prefix;
pub use tags::{Tag, TagKey, Tags};
pub use verb::Verb;
