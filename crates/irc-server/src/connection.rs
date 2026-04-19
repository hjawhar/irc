//! Per-connection driver.
//!
//! Each accepted socket is split into a read half and a write half.
//! The write task drains a bounded `mpsc::Receiver<Message>` fed by
//! the rest of the server; the read task decodes incoming messages
//! and hands them to the dispatcher. When the dispatcher says
//! `Outcome::Disconnect`, the connection tears down cleanly.

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use irc_proto::{IrcCodec, Message};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{Instrument, debug, info, info_span, warn};

use crate::handler::{Outcome, dispatch};
use crate::state::{ServerState, User, UserId};

/// Bounded capacity for the per-connection outbound queue.
/// Overflow drops the message — Phase 5 tightens this with a policy.
const OUTBOUND_QUEUE: usize = 256;

/// Drive a single accepted connection to completion.
pub async fn handle_connection(state: Arc<ServerState>, stream: TcpStream, peer: SocketAddr) {
    let user_id = state.next_user_id();
    let span = info_span!("conn", user = user_id.get(), %peer);
    async move {
        info!("accepted");
        run(state.clone(), stream, user_id, peer).await;
        // Clean up state — idempotent.
        state.remove_user(user_id);
        info!("closed");
    }
    .instrument(span)
    .await;
}

async fn run(state: Arc<ServerState>, stream: TcpStream, user_id: UserId, peer: SocketAddr) {
    let _ = stream.set_nodelay(true);
    let (read_half, write_half) = stream.into_split();

    let (out_tx, out_rx) = mpsc::channel::<Message>(OUTBOUND_QUEUE);
    let user = Arc::new(User::new(user_id, peer, out_tx));
    state.insert_user(user.clone());

    let writer = tokio::spawn(write_loop(write_half, out_rx));
    read_loop(state.clone(), user.clone(), read_half).await;

    // Closing the user's write queue signals the write task to drain
    // and exit. The user is still in the state map until
    // handle_connection removes it.
    drop(user);
    let _ = writer.await;
}

async fn read_loop(
    state: Arc<ServerState>,
    user: Arc<User>,
    read_half: tokio::net::tcp::OwnedReadHalf,
) {
    let mut reader = FramedRead::new(read_half, IrcCodec::new());
    while let Some(frame) = reader.next().await {
        match frame {
            Ok(message) => {
                debug!(verb = ?message.verb, params = message.params.len(), "recv");
                match dispatch(&state, &user, message).await {
                    Outcome::Continue => {}
                    Outcome::Disconnect => {
                        debug!("handler requested disconnect");
                        return;
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "read error");
                return;
            }
        }
    }
}

async fn write_loop(write_half: tokio::net::tcp::OwnedWriteHalf, mut rx: mpsc::Receiver<Message>) {
    let mut writer = FramedWrite::new(write_half, IrcCodec::new());
    while let Some(msg) = rx.recv().await {
        if let Err(e) = writer.send(msg).await {
            warn!(error = %e, "write error");
            return;
        }
    }
    // Channel closed — read loop has ended. Flush and exit.
    let _ = writer.flush().await;
}
