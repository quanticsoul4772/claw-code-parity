# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A clean-room rewrite of the Claude Code agent harness. Two parallel implementations exist:

- **`rust/`** — The primary Rust implementation (~20K LOC), the active development focus. Binary name: `claw`.
- **`src/`** — A Python porting workspace that mirrors the archived TypeScript structure for parity analysis. Not a standalone runtime.
- **`tests/`** — Python tests for the `src/` workspace.

See `PARITY.md` for current tool/command parity status vs upstream Claude Code.

## Build & Test Commands

### Rust (primary — run from `rust/`)

```bash
cd rust/
cargo build --release          # build the CLI binary
cargo fmt --all --check        # check formatting
cargo clippy --workspace --all-targets -- -D warnings  # lint
cargo test --workspace         # run all tests
cargo test -p rusty-claude-cli # test just the CLI crate
cargo test -p runtime          # test just the runtime crate
```

Run the mock parity harness (deterministic end-to-end CLI tests):
```bash
cd rust/
./scripts/run_mock_parity_harness.sh
```

### Python (parity workspace — run from repo root)

```bash
python3 -m src.main summary        # render porting summary
python3 -m src.main parity-audit   # compare against local TS archive
python3 -m unittest discover -s tests -v  # run all Python tests
```

### CI

GitHub Actions runs these jobs on pushes to `main`, `feature/**`, `gaebal/**`, and `omx-issue-*` branches (only when `rust/` files change):

- `cargo fmt --all --check` — formatting
- `cargo clippy --workspace --all-targets -- -D warnings` — linting
- `cargo test --workspace` — all tests
- `cargo audit` — dependency vulnerability scanning (non-blocking)
- `cargo tarpaulin --workspace` — coverage report (uploaded as artifact)

## Rust Workspace Architecture

```
rust/crates/
├── api/                  # Anthropic HTTP client, SSE streaming, auth (API key + OAuth)
├── commands/             # Slash-command registry and help text
├── compat-harness/       # Extracts tool/prompt manifests from upstream TS source
├── mock-anthropic-service/ # Deterministic local /v1/messages mock for parity tests
├── plugins/              # Plugin system (bundled plugin examples)
├── runtime/              # Core agentic loop, config hierarchy, session persistence,
│                         #   permissions, MCP client, system prompt assembly, usage tracking
├── rusty-claude-cli/     # Main CLI binary — REPL, one-shot prompt, streaming display,
│                         #   tool call rendering, arg parsing
├── telemetry/            # Telemetry/metrics collection
└── tools/                # Built-in tool implementations (bash, file ops, web, agent, etc.)
```

Key dependency flow: `rusty-claude-cli` → `runtime` → `api` + `tools` + `commands`

The runtime crate owns `ConversationRuntime` (the agentic loop), `ConfigLoader` (config merge hierarchy), `Session` (persistence), and the permission policy system.

## Python Workspace Architecture

`src/` mirrors the upstream TypeScript subsystem structure. Each subdirectory (e.g., `src/assistant/`, `src/bridge/`, `src/utils/`) exposes archive metadata (`MODULE_COUNT`, `SAMPLE_FILES`). Key modules:

- `src/main.py` — CLI entrypoint with subcommands (summary, parity-audit, bootstrap, route, turn-loop, etc.)
- `src/runtime.py` — `PortRuntime` bootstrap session and turn loop
- `src/execution_registry.py` — Unified command/tool execution registry
- `src/reference_data/` — JSON snapshots of upstream commands, tools, and subsystem metadata

## Conventions

- Rust workspace enforces `unsafe_code = "forbid"` and clippy pedantic warnings.
- Update `src/` and `tests/` together when Python workspace behavior changes.
- Keep `PARITY.md` honest — it tracks tool surface (40/40), slash commands (67/141), and behavioral gaps.
- The only allowed `#[ignore]` test is `live_stream_smoke_test`.
- Config lives in `.claude.json` (shared defaults); `.claude/settings.local.json` is for machine-local overrides.
- Branch naming: `gaebal/**` and `omx-issue-*` are CI-enabled prefixes.
