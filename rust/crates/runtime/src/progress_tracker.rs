//! No-progress detection — detects stuck tool loops in 3-5 calls.
//!
//! Tracks recent tool calls in a sliding window and identifies two stuck patterns:
//! - Same tool + same input hash repeated (exact loop)
//! - Same tool repeatedly with no write/edit output (spinning without progress)

use serde::Serialize;
use std::collections::VecDeque;

const DEFAULT_WINDOW: usize = 5;
const PRODUCTIVE_TOOLS: &[&str] = &["write_file", "edit_file", "NotebookEdit", "TodoWrite"];

// FNV-1a constants (same as cache_tracker / sandbox-types)
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[derive(Debug, Clone)]
struct ToolCall {
    tool_name: String,
    input_hash: u64,
}

#[derive(Debug, Clone)]
pub struct ProgressTracker {
    window: usize,
    recent_calls: VecDeque<ToolCall>,
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            window: DEFAULT_WINDOW,
            recent_calls: VecDeque::with_capacity(DEFAULT_WINDOW + 1),
        }
    }

    #[must_use]
    pub fn with_window(window: usize) -> Self {
        Self {
            window,
            recent_calls: VecDeque::with_capacity(window + 1),
        }
    }

    /// Record a tool call for progress tracking.
    pub fn record(&mut self, tool_name: &str, input: &str) {
        self.recent_calls.push_back(ToolCall {
            tool_name: tool_name.to_string(),
            input_hash: fnv1a_hash(input.as_bytes()),
        });
        if self.recent_calls.len() > self.window {
            self.recent_calls.pop_front();
        }
    }

    /// Check if the loop appears stuck. Returns the reason if so.
    #[must_use]
    pub fn is_stuck(&self) -> Option<StuckReason> {
        if self.recent_calls.len() < self.window {
            return None;
        }

        let first = &self.recent_calls[0];

        // Pattern 1: All calls are same tool + same input (exact loop)
        let all_identical = self
            .recent_calls
            .iter()
            .all(|c| c.tool_name == first.tool_name && c.input_hash == first.input_hash);
        if all_identical {
            return Some(StuckReason::SameToolSameInput {
                tool: first.tool_name.clone(),
                count: self.recent_calls.len(),
            });
        }

        // Pattern 2: All calls are same tool, none are productive (write/edit)
        let all_same_tool = self
            .recent_calls
            .iter()
            .all(|c| c.tool_name == first.tool_name);
        let any_productive = self
            .recent_calls
            .iter()
            .any(|c| PRODUCTIVE_TOOLS.contains(&c.tool_name.as_str()));
        if all_same_tool && !any_productive {
            return Some(StuckReason::SameToolNoProgress {
                tool: first.tool_name.clone(),
                count: self.recent_calls.len(),
            });
        }

        None
    }

    #[must_use]
    pub fn snapshot(&self) -> ProgressSnapshot {
        let mut unique = std::collections::BTreeSet::new();
        for call in &self.recent_calls {
            unique.insert(call.tool_name.clone());
        }
        ProgressSnapshot {
            recent_tools: self
                .recent_calls
                .iter()
                .map(|c| c.tool_name.clone())
                .collect(),
            stuck: self.is_stuck(),
            unique_tools_in_window: unique.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum StuckReason {
    /// Same tool called with identical input N times.
    SameToolSameInput { tool: String, count: usize },
    /// Same tool called N times with no productive (write/edit) output.
    SameToolNoProgress { tool: String, count: usize },
}

impl std::fmt::Display for StuckReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SameToolSameInput { tool, count } => write!(
                f,
                "[LOOP DETECTED] Tool '{tool}' called {count} times with identical input. Try a different approach."
            ),
            Self::SameToolNoProgress { tool, count } => write!(
                f,
                "[NO PROGRESS] Tool '{tool}' called {count} times with no write/edit output. Consider what you're trying to achieve."
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressSnapshot {
    pub recent_tools: Vec<String>,
    pub stuck: Option<StuckReason>,
    pub unique_tools_in_window: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_same_tool_same_input() {
        let mut tracker = ProgressTracker::with_window(3);
        for _ in 0..3 {
            tracker.record("read_file", r#"{"path":"foo.txt"}"#);
        }
        let reason = tracker.is_stuck().expect("should detect stuck");
        assert!(matches!(
            reason,
            StuckReason::SameToolSameInput { count: 3, .. }
        ));
    }

    #[test]
    fn detects_same_tool_no_progress() {
        let mut tracker = ProgressTracker::with_window(3);
        tracker.record("grep_search", r#"{"pattern":"foo"}"#);
        tracker.record("grep_search", r#"{"pattern":"bar"}"#);
        tracker.record("grep_search", r#"{"pattern":"baz"}"#);
        let reason = tracker.is_stuck().expect("should detect no progress");
        assert!(matches!(reason, StuckReason::SameToolNoProgress { .. }));
    }

    #[test]
    fn no_stuck_with_mixed_tools() {
        let mut tracker = ProgressTracker::with_window(3);
        tracker.record("read_file", "a");
        tracker.record("grep_search", "b");
        tracker.record("write_file", "c");
        assert!(tracker.is_stuck().is_none());
    }

    #[test]
    fn window_not_full_returns_none() {
        let mut tracker = ProgressTracker::with_window(5);
        tracker.record("read_file", "a");
        tracker.record("read_file", "a");
        assert!(tracker.is_stuck().is_none());
    }

    #[test]
    fn productive_tool_breaks_no_progress() {
        let mut tracker = ProgressTracker::with_window(3);
        tracker.record("read_file", "a");
        tracker.record("write_file", "b");
        tracker.record("read_file", "c");
        assert!(tracker.is_stuck().is_none());
    }

    #[test]
    fn snapshot_shows_unique_tools() {
        let mut tracker = ProgressTracker::with_window(5);
        tracker.record("read_file", "a");
        tracker.record("grep_search", "b");
        tracker.record("read_file", "c");
        let snap = tracker.snapshot();
        assert_eq!(snap.unique_tools_in_window, 2);
        assert_eq!(snap.recent_tools.len(), 3);
    }
}
