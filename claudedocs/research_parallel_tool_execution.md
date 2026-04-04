# Research: Parallel Tool Execution for Agentic Loops

## Executive Summary

When Claude returns multiple tool_use blocks in a single response, claw-code-parity and rawcell-agent execute them **sequentially**. Read-only tools (read_file, grep_search, glob_search, WebSearch, WebFetch) are independent and could safely run in parallel. Anthropic's own multi-agent research system runs 3-5 tools in parallel per subagent, achieving significant latency reduction. This report surveys implementation patterns and proposes a safe parallel execution design.

## The Opportunity

**Current state:** claw-code-parity `conversation.rs:360` iterates `for (id, name, input) in pending_tool_uses` — purely sequential. If Claude returns 3 read_file calls, each waits for the previous to complete.

**Claude 4 models naturally return parallel tool calls.** From Anthropic's docs: "Claude 4 models excel at parallel tool execution, with a high success rate in using parallel tool calling without any prompting." When the model returns 3 tool_use blocks, it's signaling that these are independent.

**Latency impact:** If each tool call takes 200ms, 3 sequential calls = 600ms. Parallel = 200ms. For web tools (WebSearch, WebFetch) with network latency, the difference is 3-10×.

## Industry Patterns

### Anthropic Multi-Agent Research System
- Lead agent spins up **3-5 subagents in parallel**
- Subagents use **3+ tools in parallel** themselves
- Custom tools are converted to **async Python functions** to support parallel calling
- "Parallel tool calling transforms speed and performance"

### LangGraph: Send API Fan-Out / Fan-In
- **Fan-out:** Single node dispatches to multiple parallel worker nodes
- **Fan-in:** Aggregate node collects all results
- **Send API** for dynamic parallelization (runtime-determined task count)
- **max_concurrency** configuration limits parallel tasks
- **Reducers** merge updates to shared state safely

### Tokio JoinSet (Rust)
- `JoinSet` manages dynamic set of spawned tasks
- `join_next().await` collects results as they complete (unordered)
- `AbortHandle` for cancellation
- Error handling: `Ok(Ok(v))` = success, `Ok(Err(e))` = task error, `Err(e)` = task panic
- Preferred over `tokio::join!` for dynamic task counts

### Rust Concurrency Patterns for AI Agents
- **Static fan-out:** `tokio::spawn + Arc<T>` when agents don't need to communicate
- **Team model:** Shared state via `Arc<RwLock<T>>` when coordination needed
- **Decision rule:** "Do agents need to talk to each other? No → spawn. Yes → coordinate."

## Tool Safety Classification

Not all tools can safely run in parallel. Classification:

### Safe for Parallel (read-only, no side effects)
- `read_file` — reads file, no mutation
- `glob_search` — file pattern matching
- `grep_search` — content search
- `WebSearch` — network query
- `WebFetch` — network fetch
- `ToolSearch` — tool discovery
- `Sleep` — delay (independent)

### Must Stay Sequential (side effects, ordering matters)
- `write_file` — creates/overwrites files (concurrent writes = corruption)
- `edit_file` — modifies file in place (needs consistent state)
- `bash` — arbitrary commands, unknown side effects
- `NotebookEdit` — modifies notebook state
- `Agent` — spawns subagent, needs sequential context
- `TodoWrite` — updates shared state

### Conditional (depends on arguments)
- `Config` — read = safe, write = sequential
- `EnterPlanMode` / `ExitPlanMode` — state change, sequential

## Proposed Design

### Approach: Classify + Partition + JoinSet

```
Given pending_tool_uses = [(id1, "read_file", ...), (id2, "grep_search", ...), (id3, "write_file", ...), (id4, "read_file", ...)]

1. Partition into parallel-safe and sequential:
   parallel: [(id1, "read_file"), (id2, "grep_search"), (id4, "read_file")]
   sequential: [(id3, "write_file")]

2. Execute parallel batch concurrently via JoinSet:
   let mut set = JoinSet::new();
   for (id, name, input) in parallel_batch {
       set.spawn(async move { execute_tool(name, input) });
   }
   while let Some(result) = set.join_next().await { ... }

3. Then execute sequential tools one at a time (current behavior)

4. Combine all results in original order
```

### Key Design Decisions

**1. Classification is static, not dynamic.**
The tool name determines parallelizability, not the input. This is conservative but safe. A `bash` command *might* be read-only, but we can't know without analyzing the command string.

**2. Hooks run per-tool, not per-batch.**
Each tool still gets its own PreToolUse/PostToolUse hooks. Hooks run sequentially even for parallel tools (hooks may have ordering assumptions).

**3. Results preserved in original order.**
Even though parallel tools may complete out of order, the tool_result messages are pushed to the session in the order Claude requested them. This maintains conversation coherence.

**4. Permission checks happen before spawning.**
All permission/hook checks run sequentially before the parallel spawn. Only the actual tool execution is parallelized.

**5. Max concurrency is configurable.**
Default: 5 parallel tools. Prevents spawning 50 tasks from a single response.

### Implementation Sketch (claw-code-parity)

```rust
const PARALLEL_SAFE_TOOLS: &[&str] = &[
    "read_file", "glob_search", "grep_search",
    "WebSearch", "WebFetch", "ToolSearch", "Sleep",
];
const MAX_PARALLEL: usize = 5;

// In run_turn(), replace the sequential for loop:

// 1. Partition
let (parallel, sequential): (Vec<_>, Vec<_>) = pending_tool_uses
    .into_iter()
    .partition(|(_, name, _)| PARALLEL_SAFE_TOOLS.contains(&name.as_str()));

// 2. Execute parallel batch
let mut parallel_results = Vec::new();
for chunk in parallel.chunks(MAX_PARALLEL) {
    let mut set = tokio::task::JoinSet::new();
    for (id, name, input) in chunk {
        let executor = self.tool_executor.clone();  // needs Clone
        let name = name.clone();
        let input = input.clone();
        set.spawn(async move {
            (id.clone(), name.clone(), executor.execute(&name, &input))
        });
    }
    while let Some(result) = set.join_next().await {
        parallel_results.push(result??);
    }
}

// 3. Execute sequential tools (current for loop)
for (id, name, input) in sequential {
    // existing hook + permission + execute logic
}
```

### Implementation for rawcell-agent

rawcell doesn't execute tools directly — Claude Code does. But rawcell's **MCP client** (`mcp_manager.rs`) could parallelize calls to external MCP servers:

```rust
// In mcp_manager.rs, when processing multiple tool calls:
pub async fn call_tools_parallel(
    &self,
    calls: Vec<(String, Value)>,  // (qualified_name, arguments)
) -> Vec<Result<CallToolResult>> {
    let mut set = JoinSet::new();
    for (name, args) in calls {
        let mgr = self.clone();
        set.spawn(async move { mgr.call_tool(&name, args).await });
    }
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        results.push(result.unwrap_or_else(|e| Err(anyhow::anyhow!("task panic: {e}"))));
    }
    results
}
```

This enables the sidecar to call Exa + GitHub + mcp-reasoning simultaneously during sleep stages.

## Challenges

### 1. ToolExecutor trait needs Clone or Arc
Current `ToolExecutor` in claw-code is `&mut self`. Parallel execution requires shared access. Options:
- Change to `&self` (most tools are stateless)
- Wrap in `Arc<Mutex<T>>` (adds lock contention)
- Use `StaticToolExecutor` which is already `Clone`

### 2. Session message ordering
Parallel results complete out of order. Must buffer and reorder before pushing to session. Use the tool_use_id as the ordering key.

### 3. Hook interaction
Pre-hooks can modify input. If hook modifies input for a parallel tool, the modified input must be captured before spawning. Solution: run all pre-hooks sequentially, collect (id, effective_input) pairs, THEN spawn parallel execution.

### 4. Error propagation
One parallel tool failing shouldn't cancel others (they're independent). Collect all results, mark failures individually.

### 5. Resource pressure
5 parallel WebFetch calls = 5 concurrent HTTP connections. Need backpressure or concurrency limit. The `MAX_PARALLEL` constant handles this.

## Recommended Implementation Order

1. **Add tool classification** — `is_parallel_safe(tool_name) -> bool` (trivial)
2. **Partition tool uses** — split pending_tool_uses into parallel/sequential (small change)
3. **Pre-hooks sequential, execution parallel** — run hooks first, spawn execution after (medium)
4. **JoinSet with result collection** — parallel execution + reordering (medium)
5. **Concurrency limit** — `MAX_PARALLEL` config (small)
6. **rawcell MCP parallel** — `call_tools_parallel` on mcp_manager (small, independent)

## Sources

- [Anthropic: Programmatic Tool Calling](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling)
- [Anthropic: Advanced Tool Use](https://www.anthropic.com/engineering/advanced-tool-use)
- [Anthropic: Multi-Agent Research System](https://www.anthropic.com/engineering/multi-agent-research-system)
- [Claude Tool Use Overview](https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview)
- [Vadim: Two Paradigms of Multi-Agent AI — Rust Parallel Agents vs Claude Code Teams](https://vadim.blog/two-paradigms-multi-agent-ai-rust-vs-claude-teams)
- [LangGraph: Branching and Parallelization](https://www.baihezi.com/mirrors/langgraph/how-tos/branching/index.html)
- [LangGraph: Parallelization Techniques](https://deepwiki.com/langchain-ai/langchain-academy/7.2-parallelization-techniques)
- [Scaling LangGraph Agents: Parallelization and Map-Reduce](https://aipractitioner.substack.com/p/scaling-langgraph-agents-parallelization)
- [Tokio JoinSet Documentation](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html)
- [Rust Concurrency Patterns That Scale](https://dasroot.net/posts/2026/02/rust-concurrency-patterns-scale/)
- [Tokio Spawning Tutorial](https://tokio.rs/tokio/tutorial/spawning)
