//! Per-connection command dispatch.

use std::sync::Arc;

use irc_proto::{Command, CommandError, Message, ReplyCode};
use tracing::{debug, warn};

use crate::numeric::Target;
use crate::state::{ServerState, User};

pub mod channel;
pub mod keepalive;
pub mod messaging;
pub mod mode;
pub mod registration;

/// Outcome of a command dispatch. Returned by handlers so the
/// connection driver can decide whether to keep reading.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep the connection open.
    Continue,
    /// Close the connection after flushing any queued writes.
    Disconnect,
}

/// Top-level dispatcher: parse the wire [`Message`] into a typed
/// [`Command`] and hand it to the right handler.
pub async fn dispatch(state: &Arc<ServerState>, user: &Arc<User>, msg: Message) -> Outcome {
    match Command::parse(&msg) {
        Ok(cmd) => dispatch_typed(state, user, cmd),
        Err(CommandError::MissingParam { command, .. }) => {
            debug!(command, "missing param");
            send_need_more_params(state, user, command);
            Outcome::Continue
        }
    }
}

fn dispatch_typed(state: &Arc<ServerState>, user: &Arc<User>, cmd: Command) -> Outcome {
    match cmd {
        Command::Cap { subcommand, args } => {
            registration::handle_cap(state, user, &subcommand, &args)
        }
        Command::Nick { nick } => registration::handle_nick(state, user, nick),
        Command::User {
            user: u,
            mode,
            realname,
        } => registration::handle_user(state, user, u, mode, realname),
        Command::Pass { password } => registration::handle_pass(state, user, password),
        Command::Ping { token, .. } => keepalive::handle_ping(state, user, token),
        Command::Pong { .. } => Outcome::Continue,
        Command::Quit { reason } => keepalive::handle_quit(state, user, reason),
        Command::Join { channels, keys } => channel::handle_join(state, user, channels, &keys),
        Command::Part { channels, reason } => {
            channel::handle_part(state, user, channels, reason.as_ref())
        }
        Command::Topic { channel, topic } => channel::handle_topic(state, user, channel, topic),
        Command::Privmsg { targets, text } => {
            messaging::handle_privmsg(state, user, targets, &text)
        }
        Command::Notice { targets, text } => messaging::handle_notice(state, user, targets, &text),
        Command::Unknown { verb, .. } => {
            warn!(verb = ?verb, "unknown command");
            send_unknown_command(state, user, &verb);
            Outcome::Continue
        }
        Command::Mode {
            target,
            changes,
            args,
        } => mode::handle_mode(state, user, &target, changes, &args),
        // Commands handled by later phases acknowledge silently so
        // the connection keeps moving forward.
        _ => {
            debug!("command not implemented in Phase 2b");
            Outcome::Continue
        }
    }
}

fn send_need_more_params(state: &Arc<ServerState>, user: &Arc<User>, command: &str) {
    let msg = crate::numeric::numeric_one(
        state,
        Target::UNREGISTERED,
        ReplyCode::ERR_NEEDMOREPARAMS,
        bytes::Bytes::copy_from_slice(command.as_bytes()),
        "Not enough parameters",
    );
    user.send(msg);
}

fn send_unknown_command(state: &Arc<ServerState>, user: &Arc<User>, verb: &[u8]) {
    let msg = crate::numeric::numeric_one(
        state,
        Target::UNREGISTERED,
        ReplyCode::ERR_UNKNOWNCOMMAND,
        bytes::Bytes::copy_from_slice(verb),
        "Unknown command",
    );
    user.send(msg);
}
