# Behavioral Divergences from Upstream

Last updated: 2026-04-05

This document separates **intentional design differences** from **not-yet-implemented features** (tracked in `PARITY.md`). An item here means "we know upstream does X, and we deliberately chose Y."

---

## Intentional Divergences

| Area | Upstream Behavior | Claw Behavior | Rationale |
|------|------------------|---------------|-----------|
| **File path boundary** | Workspace-scoped enforcement (file ops restricted to project tree) | `canonicalize()` only — resolves symlinks and `../` but does not jail to workspace | Simpler security model; workspace jailing adds complexity for marginal benefit when permission modes gate tool access |
| **Sandbox strategy** | 18 validation submodules (sedValidation, pathValidation, etc.) | Linux namespace isolation via `unshare` + `bash_validation` module (3 submodules) | Sandbox-first approach — namespace isolation provides stronger guarantees than string-based command parsing |
| **Token counting** | Tiktoken-based accurate token counting | Character approximation (`text.len() / 4 + 1`) | Avoids tiktoken dependency; sufficient for compaction threshold decisions |
| **Default permission mode** | `Prompt` (asks user for each escalation) | `DangerFullAccess` | Development convenience; will change before production release |
| **Binary name** | `claude` | `claw` | Avoids namespace collision with upstream |
| **Model aliases** | Internal lookup tables | Hardcoded alias map (`opus` -> `claude-opus-4-6`, etc.) | Simpler; no need for dynamic resolution |
| **Output redirect detection** | Shell AST parsing | Heuristic `>` / `>>` detection with quote-awareness in `bash_validation` | Good enough for warning purposes; full shell parsing is out of scope |
| **Sandbox field isolation** | Sandbox params accepted from tool_use JSON | Sandbox fields (`dangerouslyDisableSandbox`, `namespaceRestrictions`, `isolateNetwork`, `filesystemMode`, `allowedMounts`) stripped via `serde(skip_deserializing)` | LLM should not control security boundaries; only CLI flags/config set these |
| **SSRF protection** | Unknown | Private IP ranges (RFC 1918), link-local, cloud metadata endpoints blocked in WebFetch; loopback allowed for local dev | Prevents LLM from accessing internal networks via tool_use |
| **Parallel tool execution** | Sequential execution | Read-only tools execute in parallel via `std::thread::scope`; write tools stay sequential | Performance improvement; classification in `parallel_tools.rs` |

## Partially Implemented (functional but incomplete)

| Area | Upstream Scope | Claw Scope | Gap |
|------|---------------|------------|-----|
| **MCP** | Full server lifecycle (connect, list tools, call tool, disconnect) | Dispatch registered, returns stub responses | Need full MCP client implementation |
| **Plugin system** | Install, enable, disable, uninstall, tool execution | Plugin metadata model + `GlobalToolRegistry` conflict detection | Need plugin tool execution pipeline |
| **Skills** | Skill discovery, install, marketplace | Local `SKILL.md` file loading from filesystem | Need skill discovery/install flow |
| **Hooks** | Pre/Post tool use with full abort/modify semantics | Config-based hook execution with PreToolUse permission override | Need PostToolUseFailure handling |
| **Bash validation** | 9 submodules covering command classification, permissions, security, sandboxing | 3 submodules: `commandSemantics`, `readOnlyValidation`, `destructiveCommandWarning` | Need sedValidation, pathValidation, bashPermissions, bashSecurity, modeValidation, shouldUseSandbox |
| **Sub-agent** | Full agent lifecycle with tool restrictions and parallel orchestration | Agent tool with isolated session and handoff metadata | Need allowed-tools enforcement during execution |

## Not Yet Implemented

See `PARITY.md` "Blocking Gaps" column for the complete list. Major categories:

- **Task/Team/Cron tools**: All at STUB level, need runtime infrastructure
- **LSP integration**: Needs language server client
- **MCP auth**: Needs OAuth flow
- **Binary file detection**: read_file doesn't detect/handle binary content
