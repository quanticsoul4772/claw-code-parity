# Mock LLM parity harness

This milestone adds a deterministic Anthropic-compatible mock service plus a reproducible CLI harness for the Rust `claw` binary.

## Artifacts

- `crates/mock-anthropic-service/` — mock `/v1/messages` service
- `crates/rusty-claude-cli/tests/mock_parity_harness.rs` — end-to-end clean-environment harness
- `scripts/run_mock_parity_harness.sh` — convenience wrapper

## Scenarios

The harness runs these scripted scenarios against a fresh workspace and isolated environment variables:

1. `streaming_text`
2. `read_file_roundtrip`
3. `grep_chunk_assembly`
4. `write_file_allowed`
5. `write_file_denied`
6. `edit_file_roundtrip`
7. `bash_timeout`
8. `hook_pre_tool_deny` (Unix only — hook deny not yet enforced)
9. `glob_search_readonly` — glob_search succeeds in read-only mode
10. `edit_file_denied_readonly` — edit_file blocked in read-only mode
11. `bash_denied_readonly` — bash blocked in read-only mode
12. `read_file_not_found` — nonexistent file returns error
13. `edit_file_old_string_missing` — wrong old_string returns error
14. `write_file_overwrite` — overwrite existing file, verify original returned

## Run

```bash
cd rust/
./scripts/run_mock_parity_harness.sh
```

## Manual mock server

```bash
cd rust/
cargo run -p mock-anthropic-service -- --bind 127.0.0.1:0
```

The server prints `MOCK_ANTHROPIC_BASE_URL=...`; point `ANTHROPIC_BASE_URL` at that URL and use any non-empty `ANTHROPIC_API_KEY`.
