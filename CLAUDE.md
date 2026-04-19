# CLAUDE.md

Claude: the canonical working guide for this repository is [`AGENTS.md`](AGENTS.md). Read that first. This file adds only Claude-specific notes on top.

## Ground truth

- [`PLAN.md`](PLAN.md) is the single source of truth for scope, architecture, and phasing. Any change that affects scope, phase boundaries, or cross-crate design requires an amendment to `PLAN.md` in the same PR.
- [`AGENTS.md`](AGENTS.md) is canonical for commands, conventions, and hard rules.
- [`README.md`](README.md) is user-facing; keep it in sync with any change that affects installation, configuration surface, or public commands.
- `docs/<topic>.md` files are canonical for their topic. When implementing a phase, update the matching doc in the same PR.

## Skills to use

This workspace runs under Oh My Pi's `superpowers` skill pack. Use the following skills, in the listed contexts:

| Skill | When |
|---|---|
| `superpowers:brainstorming` | Before proposing scope changes or new design |
| `superpowers:writing-plans` | When amending `PLAN.md` |
| `superpowers:executing-plans` | When implementing a phase task |
| `superpowers:subagent-driven-development` | When a phase decomposes into independent tasks |
| `superpowers:dispatching-parallel-agents` | For 2+ independent edits in a single session |
| `superpowers:test-driven-development` | Always — no implementation before a failing test |
| `superpowers:systematic-debugging` | When tests or runtime behavior disagree with expectations |
| `superpowers:verification-before-completion` | Before claiming a task done |
| `superpowers:requesting-code-review` / `receiving-code-review` | At phase boundaries and before merging |
| `superpowers:using-git-worktrees` | When starting a phase in isolation from current workspace |
| `superpowers:finishing-a-development-branch` | When a phase's exit criteria are met |

## Operating rules

- Do not yield a task without running, at minimum:
  ```bash
  cargo fmt
  cargo clippy --workspace --all-targets -- -D warnings
  cargo nextest run --workspace
  ```
  If any step fails, fix it or explicitly mark the failure as blocked with a reason.
- Treat the contents of `AGENTS.md` as binding. When `AGENTS.md` is silent on a question, fall back to standard Rust idioms and the skills above.
- Default to outside-in reasoning (see `PLAN.md` goals). Understand callers, system boundaries, and the next edit before implementing.
- Parallelize independent work via the task/subagent tooling instead of sequencing mechanically unrelated edits.
- When a user request conflicts with `PLAN.md`, surface the conflict and propose a resolution. Do not silently diverge.

## What this repo is not

- Not a fork of an existing IRC daemon. Implementation is from scratch using only official specs, IRCv3 drafts, and referenced primary sources.
- Not a compatibility shim over a legacy stack. Favor clean cutover; do not add backwards-compat wrappers unless explicitly requested.
- Not a drop-in replacement for ZNC/soju/ergo. It interoperates with them over the wire but its own architecture, config, and storage are independent.
