# Research: Improving the Agentic Loop State Machine

## Executive Summary

The agentic conversation loop in claw-code-parity (`runtime/src/conversation.rs:286-460`) is a simple but effective bounded iteration loop: stream from LLM → extract tool uses → execute sequentially → push results → repeat until no tools. It lacks tool budgets, parallel execution, conditional branching, and progress detection. This report surveys patterns from OpenHands, LangGraph, Anthropic's own multi-agent research system, and production agentic frameworks, then proposes concrete improvements for both claw-code-parity and rawcell-agent.

## Current Architecture (claw-code-parity)

```
run_turn(user_input):
  push user message to session
  iterations = 0
  loop:
    iterations++
    if iterations > max_iterations: ERROR
    stream(system_prompt + messages) → events
    build_assistant_message(events) → message + usage
    extract pending_tool_uses from message blocks
    push assistant message to session
    if no tool_uses: break (DONE)
    for each (id, name, input) in tool_uses:  // SEQUENTIAL
      pre_hook → permission check → execute → post_hook
      push tool_result to session
  maybe_auto_compact()
  return TurnSummary
```

**Strengths:**
- Clean separation: loop logic vs tool execution vs permissions vs hooks
- Streaming-agnostic (events consumed before tool discovery)
- Atomic multi-tool batching (all tools in one response processed together)

**Gaps:**
- **No tool budgets** — only iteration count limit, no per-tool-type caps
- **Sequential execution** — tools processed one at a time, even when independent
- **No progress detection** — can't detect "stuck" (same tool called repeatedly with no effect)
- **No branching** — can't fork on ambiguous results or explore alternatives
- **No cost awareness** — doesn't factor token cost into loop decisions

## Patterns from the Industry

### OpenHands: Event-Sourced Stateless Loop
OpenHands uses an **Action–Execution–Observation triad** with typed events flowing through a central hub. All state is in a single conversation state object. Actions are validated via Pydantic schemas before execution. The key insight: **statelessness enables reliable recovery** — any crash can be replayed from the event log.

- Agent produces Actions (CmdRunAction, FileWriteAction, etc.)
- Runtime executes in isolated sandbox, returns Observations
- EventStream is the single source of truth
- Achieves 72% on SWE-Bench Verified with Claude Sonnet 4.5

### LangGraph: Directed Cyclic Graph with State
LangGraph models execution as a **directed cyclic graph** with:
- **Conditional edges** — dynamic routing based on state predicates
- **Parallel branches** — `Send()` API for concurrent execution with reducer annotations for safe merges
- **Shared AgentState** — all agents communicate through typed state dict, no direct calls
- **Checkpointing** — persistent state snapshots for recovery and human-in-the-loop

The key insight: **treating the loop as a graph enables branching, merging, and parallel paths** that a simple `loop { }` can't express.

### Anthropic Multi-Agent Research: Parallel Subagents
Anthropic's own multi-agent research system uses:
- **Lead agent spins up 3-5 subagents in parallel** for research tasks
- **Subagents use 3+ tools in parallel** themselves
- **Token budgets per agent run** prevent runaway spending
- Agents use ~4× more tokens than chat; multi-agent uses ~15×

The key insight: **distributing work across agents with separate context windows adds capacity for parallel reasoning** without hitting context limits.

### Production Guardrails (Oracle, StackAI, PromptEngineering.org)
Production agentic systems in 2026 universally implement:
- **Maximum iteration limits** (hard cap)
- **No-progress detection** (exit when repeated iterations produce no new information)
- **Token/cost budgets** as hard guardrails per workflow execution
- **Intelligent model routing** to cheaper models for simple subtasks
- **Aggressive caching** of common queries

## Proposed Improvements

### Improvement 1: Tool Budgets

Add per-tool-type call limits and a global tool budget.

```rust
pub struct ToolBudget {
    /// Maximum total tool calls per turn.
    pub max_total_calls: usize,
    /// Per-tool-type limits (e.g., {"bash": 10, "write_file": 20}).
    pub per_tool_limits: BTreeMap<String, usize>,
    /// Maximum total input tokens spent on tool results.
    pub max_tool_result_tokens: usize,
}

// In the loop, track:
let mut tool_call_counts: BTreeMap<String, usize> = BTreeMap::new();
let mut total_tool_calls = 0;

// Before executing each tool:
total_tool_calls += 1;
*tool_call_counts.entry(tool_name.clone()).or_default() += 1;

if total_tool_calls > budget.max_total_calls {
    return Err(RuntimeError::new("tool budget exhausted"));
}
if let Some(&limit) = budget.per_tool_limits.get(&tool_name) {
    if tool_call_counts[&tool_name] > limit {
        // Return error to model instead of hard-failing:
        output = format!("Tool budget exhausted: {tool_name} called {count} times (limit: {limit})");
        is_error = true;
    }
}
```

**Value:** Prevents runaway bash loops (rawcell's metacognition does this reactively every 10 calls; this is proactive and configurable).

### Improvement 2: No-Progress Detection

Detect when the loop is stuck (same tool called with same/similar input repeatedly).

```rust
struct ProgressTracker {
    recent_calls: VecDeque<(String, u64)>,  // (tool_name, input_hash)
    max_window: usize,  // e.g., 5
}

impl ProgressTracker {
    fn record(&mut self, tool_name: &str, input: &str) {
        let hash = fnv1a_hash(input.as_bytes());
        self.recent_calls.push_back((tool_name.to_string(), hash));
        if self.recent_calls.len() > self.max_window {
            self.recent_calls.pop_front();
        }
    }

    fn is_stuck(&self) -> bool {
        if self.recent_calls.len() < self.max_window { return false; }
        // All recent calls have same tool + same input hash
        let first = &self.recent_calls[0];
        self.recent_calls.iter().all(|c| c.0 == first.0 && c.1 == first.1)
    }

    fn is_looping(&self) -> bool {
        if self.recent_calls.len() < 3 { return false; }
        // Same tool called 3+ times with no Write/Edit in between
        let tool = &self.recent_calls.back().unwrap().0;
        self.recent_calls.iter().all(|c| c.0 == *tool)
            && !["write_file", "edit_file"].iter().any(|t| self.recent_calls.iter().any(|c| c.0 == *t))
    }
}
```

**Value:** Catches tool loops BEFORE burning 10+ iterations. rawcell's metacognition-check fires every 10th call — this detects loops in 3-5 calls.

### Improvement 3: Parallel Tool Execution

When the LLM returns multiple tool_use blocks in one response, execute independent tools concurrently.

```rust
// Instead of:
for (id, name, input) in pending_tool_uses {
    // sequential execution
}

// Do:
let results: Vec<_> = pending_tool_uses
    .into_iter()
    .map(|(id, name, input)| {
        let executor = self.tool_executor.clone();
        tokio::spawn(async move {
            let result = executor.execute(&name, &input);
            (id, name, result)
        })
    })
    .collect();

// Await all:
for handle in results {
    let (id, name, result) = handle.await?;
    // process result...
}
```

**Caveats:**
- Only safe for read-only tools (Read, Grep, Glob, WebSearch). Write tools must stay sequential.
- Need to classify tools as parallelizable or sequential.
- Hooks still run per-tool (can't parallelize permission checks).

**Value:** Anthropic's research system sees 3-5× speedup on multi-source research tasks.

### Improvement 4: Conditional Branching (Session Fork Integration)

At decision points, fork the session and explore alternatives.

```rust
// When tool returns ambiguous result:
if tool_result.contains("ambiguous") || tool_result.contains("multiple options") {
    let fork_a = self.session.fork(Some("option-a"));
    let fork_b = self.session.fork(Some("option-b"));
    // Run both branches (could be parallel)
    // Score results, pick winner, merge back
}
```

This is the most experimental improvement. It requires:
- Session forking (already in claw-code-parity)
- A scoring function to pick the winning branch
- A merge strategy for combining results

**Value:** Enables "try both approaches" instead of committing to the first one. Combined with rawcell's biological system, different branches could run with different hormone states.

### Improvement 5: Cost-Aware Loop Control

Track cumulative token cost and adjust behavior.

```rust
pub struct CostAwareConfig {
    pub max_cost_usd: f64,
    pub warn_at_pct: f64,  // e.g., 0.8 = warn at 80% of budget
    pub downgrade_model_at_pct: f64,  // switch to cheaper model at N%
}

// In the loop:
let current_cost = self.usage_tracker.estimated_cost_usd();
if current_cost > config.max_cost_usd {
    return Err(RuntimeError::new(format!(
        "cost budget exceeded: ${current_cost:.4} > ${max:.4}", max = config.max_cost_usd
    )));
}
if current_cost > config.max_cost_usd * config.downgrade_model_at_pct {
    // Switch to cheaper model for remaining iterations
    self.system_prompt.push("NOTE: Switched to efficient mode due to cost budget.".into());
}
```

**Value:** Prevents $50 bills from runaway agent sessions. rawcell runs 24/7 — cost awareness is critical.

## Comparison: claw-code-parity vs Industry

| Feature | claw-code | OpenHands | LangGraph | Anthropic Multi-Agent |
|---------|-----------|-----------|-----------|----------------------|
| Iteration limit | ✅ max_iterations | ✅ configurable | ✅ per-node | ✅ per-agent |
| Tool budgets | ❌ | ❌ | ❌ | ✅ token budgets |
| Parallel tools | ❌ sequential | ❌ sequential | ✅ Send() API | ✅ 3-5 subagents |
| Progress detection | ❌ | ❌ | ❌ | ❌ |
| Branching | ❌ | ❌ | ✅ conditional edges | ❌ |
| Cost awareness | ❌ | ❌ | ❌ | ✅ per-run budgets |
| Session forking | ✅ (exists, unused) | ❌ | ✅ checkpoints | ❌ |
| Event sourcing | ❌ | ✅ | ✅ | ❌ |
| Hook system | ✅ pre/post | ❌ | ❌ | ❌ |

## Recommended Implementation Order

1. **Tool Budgets** (small, high value, no architectural change)
2. **No-Progress Detection** (small, uses existing FNV hash from cache tracker)
3. **Cost-Aware Loop Control** (medium, requires pricing data)
4. **Parallel Tool Execution** (medium, requires tool classification + async refactor)
5. **Conditional Branching** (large, experimental, requires session forking + scoring)

## Sources

- [Oracle: What Is the AI Agent Loop?](https://blogs.oracle.com/developers/what-is-the-ai-agent-loop-the-core-architecture-behind-autonomous-ai-systems)
- [StackAI: The 2026 Guide to Agentic Workflow Architectures](https://www.stackai.com/blog/the-2026-guide-to-agentic-workflow-architectures)
- [PromptEngineering.org: Agents At Work — The 2026 Playbook](https://promptengineering.org/agents-at-work-the-2026-playbook-for-building-reliable-agentic-workflows/)
- [OpenHands Software Agent SDK Paper](https://arxiv.org/html/2511.03690v1)
- [OpenHands Agent Architecture Docs](https://docs.openhands.dev/sdk/arch/agent)
- [Anthropic: How We Built Our Multi-Agent Research System](https://www.anthropic.com/engineering/multi-agent-research-system)
- [Anthropic: Building Effective Agents](https://www.anthropic.com/research/building-effective-agents)
- [Anthropic: Advanced Tool Use](https://www.anthropic.com/engineering/advanced-tool-use)
- [LangGraph State Machine Branching Logic](https://markaicode.com/langgraph-state-machine-branching-logic/)
- [LangGraph 2026 Edition](https://medium.com/@dewasheesh.rana/langgraph-explained-2026-edition-ea8f725abff3)
- [LangGraph Workflow Orchestrator (GitHub)](https://github.com/josephsenior/langgraph-workflow-orchestrator)
- [Fordel Studios: State of AI Agent Frameworks 2026](https://fordelstudios.com/research/state-of-ai-agent-frameworks-2026)
- [Claude Tool Use Docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling)
