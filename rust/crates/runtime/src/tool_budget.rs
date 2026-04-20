//! Tool budget enforcement — proactive per-tool and global call limits.
//!
//! Prevents runaway tool loops by capping how many times tools can be called
//! per turn. When a budget is exceeded, returns an error message to the model
//! (not a hard crash), allowing it to finish gracefully.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ToolBudget {
    max_total_calls: usize,
    per_tool_limits: BTreeMap<String, usize>,
    calls_executed: usize,
    per_tool_counts: BTreeMap<String, usize>,
}

impl ToolBudget {
    #[must_use]
    pub fn new(max_total: usize) -> Self {
        Self {
            max_total_calls: max_total,
            per_tool_limits: BTreeMap::new(),
            calls_executed: 0,
            per_tool_counts: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_tool_limit(mut self, tool: impl Into<String>, limit: usize) -> Self {
        self.per_tool_limits.insert(tool.into(), limit);
        self
    }

    /// Check if a tool call is within budget. Returns Err with details if exceeded.
    pub fn can_execute(&self, tool_name: &str) -> Result<(), BudgetExceeded> {
        if self.calls_executed >= self.max_total_calls {
            return Err(BudgetExceeded {
                tool_name: tool_name.to_string(),
                executed: self.calls_executed,
                limit: self.max_total_calls,
                is_global: true,
            });
        }
        if let Some(&limit) = self.per_tool_limits.get(tool_name) {
            let count = self.per_tool_counts.get(tool_name).copied().unwrap_or(0);
            if count >= limit {
                return Err(BudgetExceeded {
                    tool_name: tool_name.to_string(),
                    executed: count,
                    limit,
                    is_global: false,
                });
            }
        }
        Ok(())
    }

    /// Record a tool call against the budget.
    pub fn record_call(&mut self, tool_name: &str) {
        self.calls_executed += 1;
        *self
            .per_tool_counts
            .entry(tool_name.to_string())
            .or_default() += 1;
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.max_total_calls.saturating_sub(self.calls_executed)
    }

    #[must_use]
    pub fn remaining_for(&self, tool_name: &str) -> Option<usize> {
        self.per_tool_limits.get(tool_name).map(|limit| {
            limit.saturating_sub(self.per_tool_counts.get(tool_name).copied().unwrap_or(0))
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> BudgetSnapshot {
        BudgetSnapshot {
            total_remaining: self.remaining(),
            total_executed: self.calls_executed,
            per_tool: self
                .per_tool_limits
                .iter()
                .map(|(name, limit)| {
                    let executed = self.per_tool_counts.get(name).copied().unwrap_or(0);
                    (name.clone(), (executed, *limit))
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetSnapshot {
    pub total_remaining: usize,
    pub total_executed: usize,
    /// (executed, limit) per tool name.
    pub per_tool: BTreeMap<String, (usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct BudgetExceeded {
    pub tool_name: String,
    pub executed: usize,
    pub limit: usize,
    pub is_global: bool,
}

impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_global {
            write!(
                f,
                "Tool budget exhausted ({} total calls, limit {}). Finish without further tool use.",
                self.executed, self.limit
            )
        } else {
            write!(
                f,
                "Budget for '{}' exhausted ({}/{} calls).",
                self.tool_name, self.executed, self.limit
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_allows_within_limit() {
        let mut budget = ToolBudget::new(10);
        for _ in 0..5 {
            assert!(budget.can_execute("read_file").is_ok());
            budget.record_call("read_file");
        }
        assert_eq!(budget.remaining(), 5);
    }

    #[test]
    fn budget_blocks_at_global_limit() {
        let mut budget = ToolBudget::new(3);
        for _ in 0..3 {
            budget.record_call("read_file");
        }
        let err = budget.can_execute("read_file").unwrap_err();
        assert!(err.is_global);
        assert_eq!(err.executed, 3);
        assert_eq!(err.limit, 3);
    }

    #[test]
    fn per_tool_limit_blocks_specific_tool() {
        let mut budget = ToolBudget::new(100).with_tool_limit("bash", 2);
        budget.record_call("bash");
        budget.record_call("bash");
        let err = budget.can_execute("bash").unwrap_err();
        assert!(!err.is_global);
        assert_eq!(err.tool_name, "bash");
        // Other tools still allowed
        assert!(budget.can_execute("read_file").is_ok());
    }

    #[test]
    fn snapshot_tracks_counts() {
        let mut budget = ToolBudget::new(10)
            .with_tool_limit("bash", 5)
            .with_tool_limit("write_file", 3);
        budget.record_call("bash");
        budget.record_call("bash");
        budget.record_call("read_file");

        let snap = budget.snapshot();
        assert_eq!(snap.total_executed, 3);
        assert_eq!(snap.total_remaining, 7);
        assert_eq!(snap.per_tool["bash"], (2, 5));
        assert_eq!(snap.per_tool["write_file"], (0, 3));
    }
}
