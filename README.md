# IRC Suite

A modern, modular IRC stack in Rust: a standards-compliant server, a mIRC-style GUI client, and a 24/7 bouncer — all built from a shared protocol crate.

> **Status**: Pre-release. In planning / early implementation. See [`PLAN.md`](PLAN.md) for the full scope, phases, and exit criteria.

## What's in the box

| Component | Crate | What it does |
|---|---|---|
| **Server** | `irc-server` | Single-node IRC daemon. RFC 1459/2812 + IRCv3 baseline (CAP 302, SASL, server-time, echo-message, batch, chathistory). Account registration with email verification, HMAC cloaks, class-based operators, layered flood control, DNSBL, PROXY protocol v2. SQLite-backed. |
| **GUI client** | `irc-client-gui` | mIRC-style desktop client (iced). Treebar, MDI-ish window stack, nick list, context menus, themes, tab completion, Rhai scripting (aliases, event hooks, identifiers, dialogs). |
| **Bouncer** | `irc-bnc` | Persistent 24/7 IRC connection per user/network. Replays missed traffic with IRCv3 `server-time`. Multi-user, multi-network, `*status` admin pseudo-user. |
| **Client core** | `irc-client-core` | Reusable library underneath the GUI and bouncer. |
| **CLI client** | `irc-cli` | Headless TUI (ratatui) for smoke tests and ops. |
| **Protocol** | `irc-proto` | Parser, serializer, codec, numerics, modes, CTCP, color codes. Hand-written, zero-copy, fuzzed. |
| **Test kit** | `irc-testkit` | In-process servers, scripted clients, SMTP sink, clock override, scenario DSL. |

## Architecture at a glance

```
┌──────────────┐      ┌──────────────┐      ┌──────────────┐
│ irc-client-  │      │ irc-bnc      │      │ irc-server   │
│ gui (iced)   │◀────▶│ (bouncer)    │◀────▶│ (daemon)     │
└──────┬───────┘      └──────┬───────┘      └──────┬───────┘
       │                     │                     │
       └─ irc-client-core ───┴──── irc-proto ──────┘
                 │                        │
                 └─ irc-testkit ◀─────────┘
                            │
                            └─ /metrics → Prometheus → Grafana
```

## Requirements

- **Rust** 1.80+ (pinned via `rust-toolchain.toml`)
- **Docker** 24+ and **Docker Compose v2** (optional; for the reference stack)
- **Linux/macOS** for server + bouncer; **Linux/macOS/Windows** for the GUI client
- For the GUI: a working display server (X11/Wayland on Linux, native on macOS/Windows)

## Quick start — Docker (recommended)

Bring up the full reference stack (server + bouncer + Prometheus + Grafana + MailHog):

```bash
git clone https://github.com/<owner>/irc.git
cd irc
cp -r ops/compose/examples/server-config ops/compose/server-config
cp -r ops/compose/examples/bnc-config    ops/compose/bnc-config
docker compose -f ops/compose/docker-compose.yml up --build
```

You now have:

| Service      | URL / address                              |
|--------------|--------------------------------------------|
| IRC server   | `localhost:6667` (plain), `:6697` (TLS)    |
| Bouncer      | `localhost:6668` (plain), `:6699` (TLS)    |
| Server metrics | `http://localhost:9772/metrics`          |
| Bouncer metrics | `http://localhost:9773/metrics`         |
| Prometheus   | `http://localhost:9090`                    |
| Grafana      | `http://localhost:3000` (admin/admin)      |
| MailHog      | `http://localhost:8025` (verification inbox) |

Connect with any IRC client and register an account:

```
/server localhost 6697
/nick alice
/join #general
/register alice you@example.com hunter2hunter2
# fetch the token from MailHog at http://localhost:8025
/verify alice <token>
```

Stop the stack with `docker compose -f ops/compose/docker-compose.yml down` (add `-v` to wipe volumes).

## Quick start — native

Build every binary in the workspace:

```bash
cargo build --release --workspace
```

Start the server:

```bash
cp examples/server-config.toml ./config.toml
./target/release/irc-server --config ./config.toml
```

Start the bouncer (optional, in another terminal):

```bash
cp examples/bnc-config.toml ./bnc.toml
./target/release/irc-bnc --config ./bnc.toml
```

Launch the GUI client:

```bash
cargo run --release --bin irc-client-gui
```

## Installation (end users, post-1.0)

Coming with release 0.1:

- **Prebuilt binaries** on the GitHub Releases page (Linux, macOS, Windows).
- **OCI images**: `ghcr.io/<owner>/irc-server:<tag>`, `ghcr.io/<owner>/irc-bnc:<tag>`, `ghcr.io/<owner>/irc-cli:<tag>` (multi-arch, cosign-signed, SBOM attached).
- **Cargo**: `cargo install irc-server irc-bnc irc-client-gui`.
- **Homebrew / Flatpak / MSI**: planned.

## Configuration

Configs are TOML. Full reference in [`docs/ops-and-admins.md`](docs/ops-and-admins.md).

Minimal server config (`examples/server-config.toml`):

```toml
server_name   = "irc.example.net"
network_name  = "ExampleNet"
motd_path     = "/etc/irc-server/motd.txt"
# HMAC key for cloaks — generate once, keep secret, rotate with a migration
cloak_secret_file = "/var/lib/irc-server/cloak.secret"

[[listener]]
bind = "0.0.0.0:6667"
tls  = false

[[listener]]
bind = "0.0.0.0:6697"
tls  = true
cert = "/etc/irc-server/tls/fullchain.pem"
key  = "/etc/irc-server/tls/privkey.pem"

[metrics]
bind = "127.0.0.1:9772"

[storage]
sqlite_path = "/var/lib/irc-server/state.db"

[smtp]
host           = "mail.example.com"
port           = 587
username       = "irc-server"
password_file  = "/etc/irc-server/smtp.password"
from           = "IRC <noreply@example.net>"

[limits]
per_ip_max_connections         = 5
per_ip_connect_rate_per_minute = 3
registration_deadline_seconds  = 10
messages_per_second            = 2
messages_burst                 = 6

[[oper]]
name            = "alice"
password_hash   = "$argon2id$v=19$..."
require_account = "alice"
allowed_hosts   = ["*!*@192.0.2.0/24"]
class           = "netadmin"

[oper_class.netadmin]
privileges = ["kline", "kill", "samode", "rehash", "see-realhost", "lockdown"]
```

Starter configs live in `examples/` and are the same files the compose stack seeds from.

## Development

First-time setup:

```bash
rustup show                               # picks up pinned toolchain
cargo install cargo-nextest cargo-deny cargo-llvm-cov sqlx-cli cargo-chef
```

Daily loop:

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace             # unit + property + conformance + integration
cargo deny check                          # licenses, advisories, duplicate majors
```

Focused tests:

```bash
cargo nextest run -p irc-proto
cargo nextest run -p irc-server -- registration_happy_path
```

Fuzzing the protocol parser:

```bash
cargo fuzz run decoder -- -max_total_time=60
```

Database schema changes:

```bash
cd crates/irc-server
sqlx migrate add <description>
cargo sqlx prepare                         # refresh .sqlx/ for offline builds
```

More in [`docs/testing.md`](docs/testing.md) and [`AGENTS.md`](AGENTS.md).

## Documentation map

| File | Topic |
|---|---|
| [`PLAN.md`](PLAN.md) | Full plan, phases, exit criteria — source of truth |
| [`AGENTS.md`](AGENTS.md) | Working-on-this-repo guide (humans + AI) |
| [`CLAUDE.md`](CLAUDE.md) | Claude-specific notes (points at AGENTS.md) |
| [`docs/architecture.md`](docs/architecture.md) | How the pieces fit |
| [`docs/accounts-and-cloaks.md`](docs/accounts-and-cloaks.md) | Registration, verification, cloak engine |
| [`docs/ops-and-admins.md`](docs/ops-and-admins.md) | Server ops, oper blocks, config reference |
| [`docs/security-and-abuse.md`](docs/security-and-abuse.md) | Anti-flood, DDoS posture, bans |
| [`docs/metrics.md`](docs/metrics.md) | Prometheus metrics, Grafana dashboards |
| [`docs/scripting.md`](docs/scripting.md) | Rhai API for the client |
| [`docs/testing.md`](docs/testing.md) | Testkit, scenarios, running tests |
| [`docs/protocol-notes.md`](docs/protocol-notes.md) | RFC quirks, casemapping, IRCv3 interop |
| [`docs/bnc-admin.md`](docs/bnc-admin.md) | Bouncer management |

## Roadmap

Tracked in [`PLAN.md` §13](PLAN.md#13-phased-delivery). Summary:

- **Phase 0–1** — workspace + `irc-proto`
- **Phase 2–6** — server (MVP → accounts → opers → IRCv3 → anti-abuse)
- **Phase 7** — observability + Docker
- **Phase 8–9** — client (core → GUI)
- **Phase 10** — Rhai scripting
- **Phase 11** — bouncer
- **Phase 12** — DCC + polish
- **Phase 13** — hardening, security review, release 0.1
- **Phase 14 (opt)** — TS6 server-to-server federation

## License

MIT. See [`LICENSE-MIT`](LICENSE-MIT).

## Contributing

Pre-release; external contributions not accepted yet. Design feedback welcome on the issue tracker.
