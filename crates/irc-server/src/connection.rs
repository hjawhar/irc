//! Per-connection driver.
//!
//! Each accepted socket is split into a read half and a write half.
//! The write task drains a bounded `mpsc::Receiver<Message>` fed by
//! the rest of the server; the read task decodes incoming messages
//! and hands them to the dispatcher. When the dispatcher says
//! `Outcome::Disconnect`, the connection tears down cleanly,
//! broadcasting a QUIT to every peer that shares a channel.

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use irc_proto::{IrcCodec, Message, Params, Prefix, Tags, Verb};
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
        cleanup(&state, user_id);
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

    // Drop the user so the mpsc sender goes away. The write task sees
    // its receiver close and exits cleanly.
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
}

/// Broadcast QUIT to every channel peer and remove the user from
/// state. Safe to call for partially-registered users (it becomes a
/// no-op for anyone without a nick).
fn cleanup(state: &ServerState, user_id: UserId) {
    let Some(user) = state.user(user_id) else {
        return;
    };
    let peers = state.channel_peers(user_id);
    let origin = user.origin_prefix();
    drop(user);
    let _ = state.purge_user_from_channels(user_id);
    if !peers.is_empty() {
        let quit = quit_line(&origin, None);
        for uid in peers {
            if let Some(u) = state.user(uid) {
                u.send(quit.clone());
            }
        }
    }
    state.remove_user(user_id);
}

fn quit_line(origin: &Bytes, reason: Option<&Bytes>) -> Message {
    let mut params = Params::new();
    if let Some(r) = reason {
        params.push_trailing(r.clone());
    } else {
        params.push_trailing(Bytes::from_static(b"Connection closed"));
    }
    Message {
        tags: Tags::new(),
        prefix: Some(Prefix::User {
            nick: origin.clone(),
            user: None,
            host: None,
        }),
        verb: Verb::word(Bytes::from_static(b"QUIT")),
        params,
    }
}
