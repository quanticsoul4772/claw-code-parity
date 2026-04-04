//! Parallel tool execution — classify, partition, and execute safe tools concurrently.
//!
//! When Claude returns multiple tool_use blocks in one response, read-only tools
//! can be executed in parallel via tokio::JoinSet. Write tools must stay sequential.

use serde::Serialize;

/// Default maximum concurrent tool executions per batch.
pub const DEFAULT_MAX_PARALLEL: usize = 5;

/// Tools safe for concurrent execution (read-only, no side effects).
const PARALLEL_SAFE: &[&str] = &[
    "read_file",
    "glob_search",
    "grep_search",
    "WebSearch",
    "WebFetch",
    "ToolSearch",
    "Sleep",
    "SendUserMessage",
    "AskUserQuestion",
    "TaskGet",
    "TaskList",
    "TaskOutput",
    "ListMcpResources",
    "ReadMcpResource",
    "CronList",
    "StructuredOutput",
];

/// Check if a tool can safely execute in parallel with other tools.
#[must_use]
pub fn is_parallel_safe(tool_name: &str) -> bool {
    PARALLEL_SAFE.contains(&tool_name)
}

/// Partition tool calls into (parallel_safe, must_be_sequential).
/// Order within each group is preserved.
pub fn partition<T>(tools: Vec<(String, T)>) -> (Vec<(String, T)>, Vec<(String, T)>) {
    tools
        .into_iter()
        .partition(|(name, _)| is_parallel_safe(name))
}

/// Summary of how tools were partitioned for execution.
#[derive(Debug, Clone, Serialize)]
pub struct ParallelExecutionSummary {
    pub parallel_count: usize,
    pub sequential_count: usize,
    pub parallel_tools: Vec<String>,
    pub sequential_tools: Vec<String>,
}

/// Create an execution summary from a set of pending tool names.
#[must_use]
pub fn execution_summary(tool_names: &[&str]) -> ParallelExecutionSummary {
    let mut parallel = Vec::new();
    let mut sequential = Vec::new();
    for &name in tool_names {
        if is_parallel_safe(name) {
            parallel.push(name.to_string());
        } else {
            sequential.push(name.to_string());
        }
    }
    ParallelExecutionSummary {
        parallel_count: parallel.len(),
        sequential_count: sequential.len(),
        parallel_tools: parallel,
        sequential_tools: sequential,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_only_as_parallel_safe() {
        assert!(is_parallel_safe("read_file"));
        assert!(is_parallel_safe("grep_search"));
        assert!(is_parallel_safe("glob_search"));
        assert!(is_parallel_safe("WebSearch"));
        assert!(is_parallel_safe("WebFetch"));
        assert!(is_parallel_safe("ToolSearch"));
    }

    #[test]
    fn classifies_write_tools_as_sequential() {
        assert!(!is_parallel_safe("write_file"));
        assert!(!is_parallel_safe("edit_file"));
        assert!(!is_parallel_safe("bash"));
        assert!(!is_parallel_safe("NotebookEdit"));
        assert!(!is_parallel_safe("Agent"));
        assert!(!is_parallel_safe("REPL"));
        assert!(!is_parallel_safe("PowerShell"));
    }

    #[test]
    fn unknown_tools_default_to_sequential() {
        assert!(!is_parallel_safe("some_custom_tool"));
        assert!(!is_parallel_safe(""));
    }

    #[test]
    fn partition_splits_mixed_tools() {
        let tools = vec![
            ("read_file".to_string(), "input1"),
            ("bash".to_string(), "input2"),
            ("grep_search".to_string(), "input3"),
            ("write_file".to_string(), "input4"),
            ("WebSearch".to_string(), "input5"),
        ];
        let (parallel, sequential) = partition(tools);
        assert_eq!(parallel.len(), 3);
        assert_eq!(sequential.len(), 2);
        assert_eq!(parallel[0].0, "read_file");
        assert_eq!(parallel[1].0, "grep_search");
        assert_eq!(parallel[2].0, "WebSearch");
        assert_eq!(sequential[0].0, "bash");
        assert_eq!(sequential[1].0, "write_file");
    }

    #[test]
    fn partition_preserves_order_within_groups() {
        let tools = vec![
            ("read_file".to_string(), 1),
            ("glob_search".to_string(), 2),
            ("grep_search".to_string(), 3),
        ];
        let (parallel, sequential) = partition(tools);
        assert_eq!(parallel.len(), 3);
        assert!(sequential.is_empty());
        assert_eq!(parallel[0].1, 1);
        assert_eq!(parallel[1].1, 2);
        assert_eq!(parallel[2].1, 3);
    }

    #[test]
    fn execution_summary_counts_correctly() {
        let names = &["read_file", "bash", "grep_search", "write_file", "WebFetch"];
        let summary = execution_summary(names);
        assert_eq!(summary.parallel_count, 3);
        assert_eq!(summary.sequential_count, 2);
        assert!(summary.parallel_tools.contains(&"read_file".to_string()));
        assert!(summary.sequential_tools.contains(&"bash".to_string()));
    }

    #[test]
    fn all_tools_sequential_when_none_safe() {
        let tools = vec![("bash".to_string(), ()), ("write_file".to_string(), ())];
        let (parallel, sequential) = partition(tools);
        assert!(parallel.is_empty());
        assert_eq!(sequential.len(), 2);
    }

    #[test]
    fn all_tools_parallel_when_all_safe() {
        let tools = vec![
            ("read_file".to_string(), ()),
            ("grep_search".to_string(), ()),
            ("WebSearch".to_string(), ()),
        ];
        let (parallel, sequential) = partition(tools);
        assert_eq!(parallel.len(), 3);
        assert!(sequential.is_empty());
    }
}
