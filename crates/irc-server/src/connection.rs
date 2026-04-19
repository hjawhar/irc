//! Per-connection driver.
//!
//! Phase 2a handles exactly one responsibility: wrap an accepted
//! socket in the [`irc_proto::IrcCodec`], read [`Message`]s off the
//! wire, and log them at `debug` level. No state is mutated yet; the
//! registration state machine arrives in the next commit.

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_util::codec::FramedRead;
use tracing::{Instrument, debug, info, info_span, warn};

use irc_proto::{IrcCodec, Message};

use crate::state::{ServerState, UserId};

/// Drive a single accepted connection to completion.
///
/// The function returns when the remote closes the socket or a codec
/// error is surfaced. Runtime errors are logged inside — callers don't
/// need to propagate them.
pub async fn handle_connection(state: Arc<ServerState>, stream: TcpStream, peer: SocketAddr) {
    let user_id = state.next_user_id();
    let span = info_span!("conn", user = user_id.get(), %peer);
    async move {
        info!("accepted");
        run(state.clone(), stream, user_id, peer).await;
        info!("closed");
    }
    .instrument(span)
    .await;
}

async fn run(_state: Arc<ServerState>, stream: TcpStream, _user_id: UserId, _peer: SocketAddr) {
    // For now we only read. Writes land with the registration handler.
    let (read_half, _write_half) = stream.into_split();
    let mut reader = FramedRead::new(read_half, IrcCodec::new());
    while let Some(frame) = reader.next().await {
        match frame {
            Ok(message) => on_message(&message),
            Err(e) => {
                warn!(error = %e, "read error; dropping connection");
                return;
            }
        }
    }
}

fn on_message(message: &Message) {
    debug!(?message.verb, params = message.params.len(), "recv");
}
