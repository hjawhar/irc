//! Text messaging: PRIVMSG, NOTICE.

use std::sync::Arc;

use bytes::Bytes;
use irc_proto::{Message, Params, Prefix, ReplyCode, Tags, Verb};

use crate::handler::Outcome;
use crate::handler::channel::broadcast;
use crate::numeric::{Target, numeric, numeric_text};
use crate::state::{ServerState, User};

/// Deliver `PRIVMSG <targets> :<text>`.
///
/// Errors on missing targets are surfaced as `ERR_NOSUCHNICK` /
/// `ERR_NOSUCHCHANNEL`. Each target is routed independently.
pub fn handle_privmsg(
    state: &Arc<ServerState>,
    user: &Arc<User>,
    targets: Vec<Bytes>,
    text: &Bytes,
) -> Outcome {
    deliver(state, user, targets, text, b"PRIVMSG", true)
}

/// Deliver `NOTICE <targets> :<text>` — the no-auto-reply variant.
///
/// Per RFC, NOTICE never generates automatic server errors (the
/// receiving client is supposed to never reply). We therefore skip
/// the error numerics even when a target is missing.
pub fn handle_notice(
    state: &Arc<ServerState>,
    user: &Arc<User>,
    targets: Vec<Bytes>,
    text: &Bytes,
) -> Outcome {
    deliver(state, user, targets, text, b"NOTICE", false)
}

fn deliver(
    state: &Arc<ServerState>,
    user: &Arc<User>,
    targets: Vec<Bytes>,
    text: &Bytes,
    verb: &'static [u8],
    emit_errors: bool,
) -> Outcome {
    if !user.is_registered() {
        if emit_errors {
            user.send(numeric_text(
                state,
                Target::UNREGISTERED,
                ReplyCode::ERR_NOTREGISTERED,
                "You have not registered",
            ));
        }
        return Outcome::Continue;
    }
    let origin = user.origin_prefix();
    for target in targets {
        let is_channel = target
            .first()
            .is_some_and(|b| matches!(*b, b'#' | b'&' | b'+' | b'!'));
        if is_channel {
            deliver_to_channel(state, user, &origin, &target, text, verb, emit_errors);
        } else {
            deliver_to_user(state, user, &origin, &target, text, verb, emit_errors);
        }
    }
    Outcome::Continue
}

fn deliver_to_channel(
    state: &Arc<ServerState>,
    from_user: &Arc<User>,
    origin: &Bytes,
    channel: &Bytes,
    text: &Bytes,
    verb: &'static [u8],
    emit_errors: bool,
) {
    let Some(chan) = state.channel(channel.as_ref()) else {
        if emit_errors {
            from_user.send(numeric(
                state,
                Target::UNREGISTERED,
                ReplyCode::ERR_NOSUCHCHANNEL,
                [channel.clone()],
                Some("No such channel".into()),
            ));
        }
        return;
    };
    let guard = chan.read();
    if !guard.has_member(from_user.id()) {
        // `+n` no-external-messages is implicit at MVP.
        let canonical = guard.name.clone();
        drop(guard);
        if emit_errors {
            from_user.send(numeric(
                state,
                Target::UNREGISTERED,
                ReplyCode::ERR_CANNOTSENDTOCHAN,
                [canonical],
                Some("Cannot send to channel".into()),
            ));
        }
        return;
    }
    let canonical = guard.name.clone();
    // Exclude the sender unless IRCv3 `echo-message` is negotiated
    // (Phase 5). MVP: skip the sender.
    let recipients: Vec<_> = guard
        .members
        .keys()
        .copied()
        .filter(|uid| *uid != from_user.id())
        .collect();
    drop(guard);
    let msg = message_line(origin, verb, &canonical, text);
    broadcast(state, &recipients, &msg);
}

fn deliver_to_user(
    state: &Arc<ServerState>,
    from_user: &Arc<User>,
    origin: &Bytes,
    target_nick: &Bytes,
    text: &Bytes,
    verb: &'static [u8],
    emit_errors: bool,
) {
    let Some(recipient) = state.user_by_nick(target_nick.as_ref()) else {
        if emit_errors {
            from_user.send(numeric(
                state,
                Target::UNREGISTERED,
                ReplyCode::ERR_NOSUCHNICK,
                [target_nick.clone()],
                Some("No such nick/channel".into()),
            ));
        }
        return;
    };
    // Use the recipient's *original-case* nick as the target so the
    // peer sees their preferred capitalisation echoed back.
    let canonical = recipient
        .snapshot()
        .nick
        .unwrap_or_else(|| target_nick.clone());
    let msg = message_line(origin, verb, &canonical, text);
    recipient.send(msg);
}

fn message_line(origin: &Bytes, verb: &'static [u8], target: &Bytes, text: &Bytes) -> Message {
    let mut params = Params::new();
    params.push(target.clone());
    params.push_trailing(text.clone());
    Message {
        tags: Tags::new(),
        prefix: Some(Prefix::User {
            nick: origin.clone(),
            user: None,
            host: None,
        }),
        verb: Verb::word(Bytes::copy_from_slice(verb)),
        params,
    }
}
