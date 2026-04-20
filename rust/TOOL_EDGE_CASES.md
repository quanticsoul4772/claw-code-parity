# Tool Edge-Case Checklists

Each tool's checklist enumerates the behaviors that must be tested for that tool to reach PRODUCTION parity. Items are derived from upstream submodule inventory, parameter space, and standard adversarial inputs.

**How to use**: Check off items as tests are written. The percentage of checked items determines the Edge% column in `PARITY.md`. A tool needs >= 80% for BEHAVIORAL, 100% for PRODUCTION.

---

## bash (25 items)

**Core execution:**
- [x] successful command returns stdout
- [x] failed command returns stderr + exit code
- [x] command exceeding timeout returns interrupted output
- [x] background process returns task ID
- [ ] empty command string returns error
- [ ] non-UTF-8 stdout handled gracefully
- [ ] extremely long stdout is truncated at upstream threshold
- [ ] command with quoted arguments preserves quoting
- [ ] working directory (cwd) is respected

**Upstream validation submodules:**
- [ ] `sedValidation` — validate sed commands before execution
- [ ] `pathValidation` — validate file paths in commands
- [x] `readOnlyValidation` — block writes in read-only permission mode (bash_validation::validate_for_mode)
- [x] `destructiveCommandWarning` — warn on rm -rf, git reset --hard, etc. (bash_validation::check_destructive_patterns)
- [x] `commandSemantics` — classify command intent (read vs write vs destructive) (bash_validation::classify_command)
- [ ] `bashPermissions` — permission gating per command type
- [ ] `bashSecurity` — injection and escape sequence checks
- [ ] `modeValidation` — validate against current permission mode
- [ ] `shouldUseSandbox` — sandbox decision logic based on env/config

**Permission matrix:**
- [x] allowed in danger-full-access mode (permission_matrix_covers_all_tools_and_modes)
- [x] denied in read-only mode (for write commands) (permission_matrix_covers_all_tools_and_modes)
- [x] behavior in workspace-write mode matches upstream (permission_matrix_workspace_write_prompts_for_danger_tools)

**Adversarial:**
- [ ] command injection via shell metacharacters
- [ ] signal handling (SIGTERM, SIGKILL propagation to children)
- [ ] zombie process cleanup after timeout

---

## read_file (12 items)

**Core:**
- [x] read full file returns content with line numbers
- [x] read with offset/limit returns correct slice
- [x] read past EOF returns empty content
- [x] read missing file returns error
- [ ] read empty file returns empty content
- [ ] read binary file detected and handled
- [ ] read file exceeding size limit returns truncated output or error
- [ ] read with offset=0 equivalent to no offset

**Safety:**
- [x] path traversal via ../ prevented (canonicalize in normalize_path)
- [ ] symlink following behavior matches upstream
- [x] absolute vs relative path handling

**Permission matrix:**
- [x] allowed in read-only mode (permission_matrix_covers_all_tools_and_modes)
- [x] exercised in E2E harness (read_file_roundtrip)

---

## write_file (10 items)

**Core:**
- [x] create new file in new directory
- [x] overwrite existing file (returns original content)
- [ ] write empty content creates empty file
- [x] write to path with non-existent deep directory creates parents

**Safety:**
- [x] path traversal via ../ prevented (canonicalize in normalize_path_allow_missing)
- [ ] symlink following behavior matches upstream
- [ ] size limit enforcement on content

**Permission matrix:**
- [x] allowed in workspace-write mode (E2E: write_file_allowed)
- [x] denied in read-only mode (E2E: write_file_denied)
- [x] behavior in danger-full-access mode (permission_matrix_covers_all_tools_and_modes)

---

## edit_file (10 items)

**Core:**
- [x] single replacement (old_string → new_string)
- [x] replace_all replaces every occurrence
- [x] identical old/new string returns error
- [x] old_string not found returns error
- [x] edit on non-existent file returns error
- [ ] old_string not unique (without replace_all) returns error
- [ ] edit preserving file permissions/mode

**Safety:**
- [x] path traversal via ../ prevented (canonicalize in normalize_path)

**Permission matrix:**
- [x] allowed in workspace-write mode (permission_matrix_covers_all_tools_and_modes)
- [x] denied in read-only mode (permission_matrix_covers_all_tools_and_modes)

---

## glob_search (8 items)

**Core:**
- [x] pattern matches files correctly
- [x] invalid glob pattern returns error
- [ ] pattern with no matches returns empty result
- [ ] recursive glob (**/) works
- [ ] glob respects .gitignore exclusions (if applicable)

**Safety:**
- [ ] glob in restricted directory handled

**Permission matrix:**
- [x] allowed in read-only mode (permission_matrix_covers_all_tools_and_modes)
- [ ] denied outside workspace boundary (if applicable)

---

## grep_search (10 items)

**Core:**
- [x] content mode with line numbers
- [x] count mode returns match count
- [x] invalid regex returns error
- [x] offset and head_limit pagination
- [ ] files_with_matches mode returns file paths
- [ ] multiline mode matches across lines
- [ ] context lines (-A, -B, -C) work correctly
- [ ] type filter (--type) works
- [ ] glob filter works with grep

**Permission matrix:**
- [x] allowed in read-only mode (permission_matrix_covers_all_tools_and_modes)
- [x] exercised in E2E harness (grep_chunk_assembly)

---

## WebFetch (8 items)

**Core:**
- [x] HTML page fetched and text extracted
- [x] plain text response handled
- [x] invalid URL returns error
- [x] prompt-aware summary includes title
- [ ] redirect following behavior matches upstream
- [ ] content truncation at upstream threshold
- [ ] timeout handling for slow servers
- [ ] non-200 status codes handled

---

## WebSearch (6 items)

**Core:**
- [x] search returns filtered results
- [x] generic links and invalid base URL handled
- [ ] empty query returns error or empty results
- [ ] result count limiting
- [ ] special characters in query handled
- [ ] rate limiting / error response handling

---

## TodoWrite (6 items)

**Core:**
- [x] persist todos and return previous state
- [x] empty todos rejected
- [x] blank content rejected
- [x] all-completed triggers verificationNudge
- [ ] status transitions (pending → in_progress → completed)
- [ ] concurrent access to todo store

---

## Skill (5 items)

**Core:**
- [x] loads local SKILL.md from filesystem
- [x] accepts $-prefixed skill names
- [ ] missing skill returns helpful error
- [ ] skill with no SKILL.md file returns error
- [ ] skill args passed correctly

---

## Agent (8 items)

**Core:**
- [x] persists handoff metadata to manifest file
- [x] normalizes subagent type names
- [x] normalizes explicit agent names (slugify)
- [x] spawn error surfaces as tool error
- [x] completion and failure terminal states persisted
- [x] blank description/prompt rejected
- [x] subagent tool loop with isolated session
- [ ] allowed tools subset enforced during execution

---

## ToolSearch (5 items)

**Core:**
- [x] keyword query returns relevant matches
- [x] select: query returns exact tools
- [x] alias resolution (AgentTool → Agent)
- [ ] empty query returns error or all tools
- [ ] max_results parameter respected

---

## NotebookEdit (7 items)

**Core:**
- [x] replace cell source
- [x] insert new cell (with position and append)
- [x] delete cell
- [x] non-.ipynb file rejected
- [x] insert without source rejected
- [x] delete on empty notebook rejected
- [ ] cell_id not found returns error

---

## Sleep (3 items)

- [x] positive duration sleeps and reports
- [x] zero duration succeeds immediately
- [x] excessive duration rejected with error

---

## SendUserMessage/Brief (5 items)

**Core:**
- [x] message sent with attachment metadata
- [ ] missing message field returns error
- [ ] non-existent attachment path handled
- [ ] multiple attachments handled
- [ ] status field values (normal, info, warning) handled

---

## Config (6 items)

**Core:**
- [x] get existing config value
- [x] set config value with validation
- [x] invalid config value rejected
- [x] unknown setting returns success=false
- [ ] delete/unset config value
- [ ] scope parameter (user vs project vs local) works

---

## EnterPlanMode (4 items)

- [x] enters plan mode, writes state file
- [x] preserves existing local override
- [x] creates override from empty local state
- [ ] re-entering plan mode when already in plan mode

---

## ExitPlanMode (4 items)

- [x] restores previous local override
- [x] clears override when created from empty state
- [ ] exiting when not in plan mode
- [ ] concurrent plan mode operations

---

## StructuredOutput (3 items)

- [x] echoes input payload in structured_output field
- [x] empty payload rejected
- [ ] very large payload handled (truncation or error)

---

## REPL (5 items)

- [x] executes Python code and returns output
- [x] empty code rejected
- [x] unsupported language rejected
- [x] timeout enforcement
- [ ] non-zero exit code handled

---

## PowerShell (4 items)

- [x] executes command via pwsh
- [x] background execution returns task ID
- [x] missing shell executable returns error
- [ ] timeout enforcement

---

## Output Truncation Thresholds

All tool output is subject to a global truncation limit of **100,000 bytes** (`MAX_TOOL_OUTPUT_BYTES` in `tools/src/lib.rs`). Individual tools may apply tighter limits:

| Tool | Limit | Location |
|------|-------|----------|
| All tools | 100KB output | `execute_tool` wrapper |
| glob_search | 100 files | `file_ops.rs:248` |
| grep_search | 250 lines default (configurable) | `file_ops.rs:416` |
| ToolSearch | 8 results | `lib.rs` |
| bash | No individual limit (global 100KB applies) | -- |

---

## Stub Tools (1 item each)

The following tools are at STUB level. Each needs only a dispatch smoke test to maintain STUB status. Behavioral items will be added when implementation begins.

**AskUserQuestion**: needs user I/O integration
**TaskCreate**: needs sub-agent runtime
**TaskGet**: needs task registry
**TaskList**: needs task registry
**TaskStop**: needs process management
**TaskUpdate**: needs task message passing
**TaskOutput**: needs output capture
**TeamCreate**: needs parallel task orchestration
**TeamDelete**: needs team lifecycle
**CronCreate**: needs scheduler runtime
**CronDelete**: needs cron registry
**CronList**: needs cron registry
**LSP**: needs language server client
**ListMcpResources**: needs MCP client
**ReadMcpResource**: needs MCP client
**McpAuth**: needs OAuth flow
**MCP**: needs MCP tool proxy
**RemoteTrigger**: needs HTTP client
**TestingPermission**: test-only, low priority
