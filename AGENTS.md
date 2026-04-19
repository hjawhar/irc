# Agent & contributor guide

Working reference for anyone (human or AI) modifying this repository. Follows the [agents.md](https://agents.md/) convention. Read alongside [`README.md`](README.md).

## Status

Pre-implementation. The repository currently contains planning artifacts (`PLAN.md`, `README.md`, this file, `CLAUDE.md`). Before writing code, read `PLAN.md` end to end.

## Project layout

```
irc/
├── PLAN.md                 # canonical plan; source of truth for scope + design
├── README.md               # user-facing overview + quickstart
├── AGENTS.md               # this file
├── CLAUDE.md               # Claude-specific pointer to AGENTS.md
├── Cargo.toml              # workspace
├── rust-toolchain.toml
├── rustfmt.toml clippy.toml deny.toml
├── crates/
│   ├── irc-proto/          # protocol primitives — parser, codec, types
│   ├── irc-server/         # daemon
│   ├── irc-client-core/    # headless client library
│   ├── irc-client-gui/     # iced GUI
│   ├── irc-bnc/            # bouncer
│   ├── irc-cli/            # ratatui CLI (optional frontend)
│   └── irc-testkit/        # test harness
├── ops/
│   ├── docker/             # per-binary Dockerfiles
│   ├── compose/            # docker-compose dev stack
│   ├── prometheus/         # scrape config
│   └── grafana/            # dashboards JSON
├── docs/                   # architecture, scripting, ops, testing, protocol notes
├── examples/               # example configs
└── .github/workflows/      # CI
```

## Dev setup

```bash
rustup show                  # picks up pinned toolchain from rust-toolchain.toml
cargo install cargo-nextest cargo-deny cargo-llvm-cov sqlx-cli cargo-chef
```

## Commands

### Daily loop

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo deny check
```

### Per-crate tests

```bash
cargo nextest run -p irc-proto
cargo nextest run -p irc-server
cargo nextest run -p irc-testkit
```

### Running binaries

```bash
cargo run -p irc-server     -- --config examples/server-config.toml
cargo run -p irc-bnc        -- --config examples/bnc-config.toml
cargo run -p irc-client-gui
cargo run -p irc-cli        -- --connect localhost:6697
```

### Containers

```bash
docker compose -f ops/compose/docker-compose.yml up --build
docker build -f ops/docker/irc-server.Dockerfile -t irc-server .
docker build -f ops/docker/irc-bnc.Dockerfile    -t irc-bnc    .
```

### Database

```bash
cd crates/irc-server
sqlx migrate add <description>
sqlx migrate run
cargo sqlx prepare           # refresh .sqlx/ for offline builds (commit the result)
```

### Fuzzing

```bash
cargo fuzz run decoder -- -max_total_time=60
cargo fuzz run modes   -- -max_total_time=60
```

## Coding standards

Enforced in CI. Violations fail the build.

- `#![forbid(unsafe_code)]` on every crate.
- `#![warn(clippy::pedantic, clippy::nursery)]`; targeted `#[allow]` with a comment explaining why.
- No `.unwrap()` / `.expect()` outside `#[cfg(test)]` and the `main` bootstrap.
- Every protocol identifier is a newtype (`Nick`, `ChannelName`, `AccountName`, `CloakHost`, `RealHost`). Do not pass raw `String` across crate boundaries.
- `#![deny(missing_docs)]` on `irc-proto` and `irc-client-core`.
- All network I/O behind timeouts. All queues bounded.
- `thiserror` in libraries, `anyhow` in binaries. Errors carry enough context to explain what was attempted.
- `tracing` spans on every connection and command dispatch with structured fields (`user.id`, `nick`, `chan`, `cmd`).
- Oper / admin actions emit events on the `audit` tracing target — never log them only at `info`.
- `sqlx` queries use `query!` / `query_as!` (compile-time checked). Commit `.sqlx/` for offline builds.
- No backwards-compatibility shims unless the user explicitly asks. Favor clean cutover.

## Testing expectations

Every PR that touches behavior must land:

1. A unit or property test covering the new branch.
2. An `irc-testkit` scenario if the change is observable over the wire.
3. An `insta` snapshot update if the change affects emitted numerics.
4. A regression test with a comment linking the issue when fixing a bug.

Layers (see [`PLAN.md` §12](PLAN.md#12-testing-strategy) and [`docs/testing.md`](docs/testing.md)):

| Layer | Location |
|---|---|
| Unit | `#[cfg(test)]` inside each crate |
| Property | `crates/irc-proto/tests/prop/` |
| Fuzz | `crates/irc-proto/fuzz/` |
| Conformance | `crates/irc-testkit/tests/conformance/` |
| Integration | in-crate `tests/` using `irc-testkit` |
| E2E | `crates/irc-testkit/tests/e2e/` |

Run `cargo nextest run --workspace` before opening a PR. CI runs the full matrix.

## Adding a feature — where things touch

| Change | What you touch |
|---|---|
| New IRC command | `irc-proto::Command` + parser + serializer + numerics; `irc-server` handler; conformance scenario; insta snapshot for numerics |
| New IRCv3 cap | `irc-proto` CAP table; `irc-server` negotiation; `irc-client-core` offer + handling; conformance test |
| New oper privilege | `irc-server::opers::Privilege` enum; class-config validator; audit event; integration test |
| New anti-abuse limiter | `irc-server::limits`; metric registration; config schema; scenario test |
| New metric | `docs/metrics.md`; exporter registration; Grafana dashboard panel |
| New Rhai binding | `irc-client-core::scripting`; `docs/scripting.md`; example script; unit test |
| DB schema change | `sqlx migrate add`; `Store` trait update; SQLite impl; `cargo sqlx prepare`; testkit scenario |
| New Dockerfile / compose change | `ops/docker/` or `ops/compose/`; bump CI; document in README |

## Branching + PR flow

- Feature branches per phase task; name `phase-<N>/<short-desc>`.
- Commits follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`).
- Each PR links the `PLAN.md` phase/task it addresses.
- PRs merge only when CI is green (fmt, clippy, nextest, deny, fuzz-smoke) and at least one review is in.

## Hard rules

- Do **not** add dependencies without updating `deny.toml` and justifying the addition in the PR description.
- Do **not** weaken lints to silence warnings.
- Do **not** introduce backwards-compatibility shims unless explicitly requested.
- Do **not** log oper actions only at `info` — they must also emit on `audit`.
- Do **not** store secrets as plaintext in config when a `*_file` variant exists.
- Do **not** add `.unwrap()` / `.expect()` to production paths to unblock a PR.
- Do **not** commit changes to `.sqlx/` without running `cargo sqlx prepare` fresh.

## Where to ask

- Implementation questions → reread `PLAN.md` and the relevant `docs/<topic>.md`.
- Design questions → open a discussion; scope changes require a `PLAN.md` amendment PR.
- Bugs → issue with a minimal repro using `irc-testkit`.

## For AI agents specifically

- `PLAN.md` is canonical. If a request conflicts with it, surface the conflict rather than silently diverging.
- Before making changes, run `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo nextest run --workspace`. Report failures before adding new code.
- Prefer editing existing files to creating new ones. New files must earn their existence.
- When a request is underspecified, propose the minimal reasonable interpretation and state what you assumed.
- When touching protocol code, always add a `proptest` or `insta` snapshot covering the change.
- Never commit `.sqlx/` blindly — regenerate with `cargo sqlx prepare` and confirm offline builds still pass.
- When in doubt about an architectural decision, cross-reference `PLAN.md` §§4, 5, 9, 10, 11 (server, storage, observability, security, deployment).
