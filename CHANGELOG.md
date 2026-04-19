# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-19

Initial release of the IRC Suite: a server, bouncer, and CLI client built in Rust.

### Added

#### Protocol (`irc-proto`)
- RFC 1459 / RFC 2812 wire-format parser and serializer (zero-copy, `bytes`-backed).
- Typed `Message`, `Command` enum, `ReplyCode` numerics, `Prefix` variants.
- IRCv3 message-tags with escape handling.
- Channel and user mode parser with parameterized mode support.
- CTCP encode/decode.
- mIRC formatting codes (color, bold, italic, underline, strikethrough, monospace, reverse, reset) with `StyledSpan` output.
- Casemapping-aware newtypes: `Nick`, `ChannelName`, `ServerName`, `AccountName` (`ascii`, `rfc1459`, `rfc1459-strict`).
- `tokio-util` codec with 512-byte untagged / 8191-byte tagged limits, lenient `\n` decode.
- CAP 302 primitives and ISUPPORT parser.
- Property tests (`proptest`) for parse/serialize roundtrip.
- Fuzz targets (`cargo-fuzz`): decoder, casemap, codec.

#### Server (`irc-server`)
- Tokio multi-threaded async runtime with per-connection tasks.
- TLS via `tokio-rustls` (no OpenSSL dependency).
- PROXY protocol v2 support (per-listener opt-in).
- Full command set: registration (`CAP`, `NICK`, `USER`, `PASS`, `AUTHENTICATE`, `QUIT`, `PING`/`PONG`), channels (`JOIN`, `PART`, `TOPIC`, `KICK`, `INVITE`, `NAMES`, `LIST`, `WHO`, `WHOIS`, `WHOWAS`, `MODE`), messaging (`PRIVMSG`, `NOTICE`, `TAGMSG`), info (`MOTD`, `VERSION`, `ADMIN`, `TIME`, `LUSERS`, `STATS`).
- User modes: `+i`, `+w`, `+o`, `+s`, `+Z`, `+x`, `+R`.
- Channel modes: `+o`, `+v`, `+b`, `+e`, `+I`, `+i`, `+k`, `+l`, `+m`, `+n`, `+s`, `+t`, `+p`, `+r`, `+M`.
- SQLite storage (`sqlx`, WAL mode) behind a `Store` trait.
- Account registration and verification (`REGISTER`/`VERIFY`) with SMTP email (`lettre`).
- Nick protection modes: off, warn, rename, ghost.
- SASL PLAIN (argon2id) and EXTERNAL (TLS client-cert fingerprint).
- HMAC-SHA256 IP cloaking (always-on by default); `user/<account>` cloaks for registered users; vanity cloaks via `SETCLOAK`.
- Oper system: config-driven blocks, argon2 password hashing, SASL-account-bound, hostmask-restricted, class-based privileges.
- Oper commands: `OPER`, `KILL`, `KLINE`/`UNKLINE`, `GLINE`/`UNGLINE`, `LOCKDOWN`, `SAMODE`, `REHASH`, `DIE`, `RESTART`, `SHOWHOST`.
- Audit logging: every oper action emits structured `tracing` spans with oper identity, target, and reason.
- IRCv3: CAP 302, message-tags, server-time, echo-message, away-notify, account-notify, batch, chathistory (`LATEST`/`BEFORE`/`AFTER`/`AROUND`/`BETWEEN`), MONITOR.
- Anti-abuse: per-IP connection limits (/24 IPv4, /64 IPv6 aggregation), per-IP rate limiting, global connection cap, registration deadline, per-connection token buckets (messages/sec, bytes/sec, targets/sec), penalty escalation, DNSBL integration, lockdown mode.
- Chathistory: per-channel ring buffer with optional SQLite persistence.
- Hot-reload via `REHASH` (config reload without dropping clients).
- Event bus for future TS6 server-to-server linking.
- Prometheus metrics endpoint (`/metrics`): connections, messages, bytes, auth outcomes, flood kicks, DNSBL hits, klines, oper actions, chathistory queries, store latency.
- Health endpoint (`GET /health`) for container orchestration.

#### Bouncer (`irc-bnc`)
- Persistent upstream connections via `irc-client-core`.
- Multi-user, multi-network architecture.
- Downstream protocol: SASL PLAIN or `PASS user/net:password` auth.
- Synthetic welcome, JOIN/TOPIC/NAMES replay on attach.
- Buffered message replay with original `@time=` server-time tags.
- Ring buffer per target (configurable depth) with optional SQLite persistence.
- Admin interface via `*status` pseudo-user: `addnetwork`, `delnetwork`, `connect`, `disconnect`, `listnets`, `status`, `setnick`, `setsasl`, `adduser`, `passwd`, `metrics`.
- CLI admin tool: `irc-bnc admin users create|delete|listnets|passwd`.
- Prometheus metrics: upstream/downstream gauges, buffer depth, replay bytes, reconnect counters.
- Health endpoint.

#### Client Core (`irc-client-core`)
- Headless client library consumed by GUI and bouncer.
- Multi-network connection manager with exponential-backoff reconnection.
- Full IRCv3 capability negotiation (cap-notify, account-notify, away-notify, extended-join, server-time, message-tags, echo-message, batch, chathistory, sasl, multi-prefix, userhost-in-names, invite-notify, setname).
- Per-window scrollback ring buffers.
- Per-window log files with daily rotation.
- SASL PLAIN and EXTERNAL support.
- Account registration client flow (`REGISTER`/`VERIFY`).

#### Client GUI (`irc-client-gui`)
- mIRC-style layout: treebar, window stack, nick list, topic bar, input line, status bar.
- Tab completion (nicks, channels, commands).
- mIRC formatting code rendering and input (Ctrl+K/B/U/I/R).
- TOML themes (classic mIRC + modern dark shipped).
- Channel browser (`/list` with sortable grid).
- Registration wizard UI.
- SASL credential manager (keychain-backed where available).
- Desktop notifications for highlights, PMs, watched-nick joins.
- Keyboard shortcuts: Ctrl+Tab/Shift+Tab, Ctrl+W, Ctrl+F, Alt+number, F1, F2.

#### CLI Client (`irc-cli`)
- Ratatui TUI built on `irc-client-core`.

#### Scripting (Rhai)
- Script loading from `~/.config/irc-suite/scripts/*.rhai`.
- Identifiers: `nick()`, `chan()`, `network()`, `now()`, `me()`, `topic()`, `version()`, `account()`.
- Actions: `send_msg`, `send_raw`, `join`, `part`, `set_mode`, `open_window`, `echo`.
- Event hooks with filters, aliases, timers (`after`/`every`).
- Async dialog integration (`prompt`, `confirm`, `form`).
- Instruction cap per event for runaway loop prevention.

#### Test Kit (`irc-testkit`)
- `TestServer` / `TestBnc` builders with ephemeral ports and shutdown guards.
- `ScriptedClient` with fluent DSL for wire-level assertions.
- `SmtpSink` for email verification testing.
- `ClockOverride` for deterministic time-dependent tests.
- `CaptureStore` for call recording and invariant checks.
- Conformance, integration, and E2E test scenarios.

#### Operations
- Multi-stage Dockerfiles with `cargo-chef` caching and distroless runtime images.
- Container hardening: non-root UID, read-only rootfs, `cap_drop: ALL`, `no-new-privileges`.
- Docker Compose dev stack: server + bouncer + Prometheus + Grafana + MailHog.
- Grafana dashboards: server overview, bouncer, protocol.
- Prometheus scrape configuration.
- GitHub Actions CI: fmt, clippy, nextest, cargo-deny.
- GitHub Actions release workflow: multi-arch binaries (x86_64-linux, aarch64-linux, aarch64-darwin), multi-arch Docker images (linux/amd64, linux/arm64) published to GHCR, GitHub Release with auto-generated notes.

#### Documentation
- `docs/architecture.md` — system design overview.
- `docs/protocol-notes.md` — IRC protocol implementation notes.
- `docs/scripting.md` — Rhai scripting guide.
- `docs/accounts-and-cloaks.md` — account system and IP cloaking.
- `docs/ops-and-admins.md` — operator guide and SQLite tuning.
- `docs/security-and-abuse.md` — threat model and anti-abuse layers.
- `docs/metrics.md` — Prometheus metrics reference.
- `docs/testing.md` — test strategy and running tests.

[0.1.0]: https://github.com/owner/irc/releases/tag/v0.1.0
