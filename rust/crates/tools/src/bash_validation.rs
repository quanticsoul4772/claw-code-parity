//! Bash command classification and validation.
//!
//! Implements three upstream validation submodules:
//! - `commandSemantics` — classify command intent (read vs write vs destructive)
//! - `readOnlyValidation` — block write/destructive commands in read-only mode
//! - `destructiveCommandWarning` — detect dangerous patterns and return warnings

/// The classified intent of a bash command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandIntent {
    /// Command only reads state (ls, cat, grep, git status, etc.)
    ReadOnly,
    /// Command modifies files or state (cp, mv, tee, git commit, etc.)
    Write,
    /// Command is potentially destructive and hard to reverse (rm -rf, mkfs, etc.)
    Destructive,
    /// Intent could not be determined from the command string.
    Unknown,
}

/// Patterns that indicate destructive commands.
static DESTRUCTIVE_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -fr",
    "rm -Rf",
    "rmdir",
    "git reset --hard",
    "git clean -fd",
    "git clean -f",
    "git checkout -- .",
    "git restore .",
    "dd if=",
    "mkfs",
    "format ",
    "chmod -R 777",
    ":(){ :|:& };:", // fork bomb
    "> /dev/sd",
    "shred ",
    "wipefs",
];

/// Patterns that indicate write operations.
static WRITE_PATTERNS: &[&str] = &[
    "cp ",
    "mv ",
    "tee ",
    "touch ",
    "mkdir ",
    "chmod ",
    "chown ",
    "chgrp ",
    "ln ",
    "git add",
    "git commit",
    "git push",
    "git merge",
    "git rebase",
    "git stash",
    "npm install",
    "yarn add",
    "pip install",
    "cargo install",
    "apt install",
    "brew install",
];

/// Classify the intent of a bash command.
///
/// Checks destructive patterns first (highest severity), then write patterns,
/// then falls back to `Unknown`. Simple read-like commands (starting with
/// common read-only verbs) are classified as `ReadOnly`.
#[must_use]
pub fn classify_command(command: &str) -> CommandIntent {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return CommandIntent::Unknown;
    }

    // Check destructive patterns first.
    if check_destructive_patterns(trimmed).is_some() {
        return CommandIntent::Destructive;
    }

    // Check write patterns.
    for pattern in WRITE_PATTERNS {
        if trimmed.contains(pattern) {
            return CommandIntent::Write;
        }
    }

    // Check for output redirection (>, >>).
    if contains_output_redirect(trimmed) {
        return CommandIntent::Write;
    }

    // Check common read-only command prefixes.
    let read_only_commands = [
        "ls",
        "cat",
        "head",
        "tail",
        "less",
        "more",
        "grep",
        "rg",
        "find",
        "which",
        "where",
        "echo",
        "printf",
        "wc",
        "sort",
        "uniq",
        "diff",
        "file",
        "stat",
        "du",
        "df",
        "whoami",
        "hostname",
        "uname",
        "date",
        "pwd",
        "env",
        "printenv",
        "id",
        "ps",
        "top",
        "htop",
        "git status",
        "git log",
        "git diff",
        "git show",
        "git branch",
        "git tag",
        "cargo check",
        "cargo clippy",
        "cargo test",
        "cargo build",
        "cargo fmt",
        "npm test",
        "npm run",
        "yarn test",
        "python -m pytest",
        "go test",
    ];
    for cmd in &read_only_commands {
        if trimmed.starts_with(cmd) {
            return CommandIntent::ReadOnly;
        }
    }

    CommandIntent::Unknown
}

/// Check if a command matches known destructive patterns.
///
/// Returns `Some(warning_message)` if the command is destructive, `None` otherwise.
#[must_use]
pub fn check_destructive_patterns(command: &str) -> Option<String> {
    let trimmed = command.trim();
    for pattern in DESTRUCTIVE_PATTERNS {
        if trimmed.contains(pattern) {
            return Some(format!(
                "Warning: potentially destructive command detected ({pattern}). \
                 This operation may be difficult or impossible to reverse."
            ));
        }
    }

    // Special case: rm with force flags on broad paths.
    if trimmed.starts_with("rm ")
        && (trimmed.contains(" -f") || trimmed.contains(" --force"))
        && (trimmed.contains(" /") || trimmed.contains(" ~") || trimmed.contains(" *"))
    {
        return Some(
            "Warning: forced removal on a broad path detected. \
             This operation may be difficult or impossible to reverse."
                .to_string(),
        );
    }

    None
}

/// Validate a command against the given permission mode name.
///
/// In read-only mode, write and destructive commands are blocked.
/// Returns `Ok(())` if the command is allowed, or `Err(reason)` if blocked.
///
/// `mode_name` should be one of: `"read-only"`, `"workspace-write"`, `"danger-full-access"`.
pub fn validate_for_mode(command: &str, mode_name: &str) -> Result<Option<String>, String> {
    let intent = classify_command(command);

    match mode_name {
        "read-only" => match intent {
            CommandIntent::Destructive => Err(format!(
                "Command blocked: destructive commands are not allowed in read-only mode. \
                 Command: {command}"
            )),
            CommandIntent::Write => Err(format!(
                "Command blocked: write commands are not allowed in read-only mode. \
                 Command: {command}"
            )),
            _ => Ok(None),
        },
        "workspace-write" => {
            // In workspace-write, allow writes but warn on destructive.
            Ok(check_destructive_patterns(command))
        }
        "danger-full-access" => {
            // All commands allowed, but still return warnings for destructive ones.
            Ok(check_destructive_patterns(command))
        }
        _ => Ok(None),
    }
}

/// Check if a command contains output redirection operators (>, >>).
///
/// Skips `>` characters inside single or double quotes to avoid false positives
/// from strings like `echo "value > 100"` or comparison operators in `[[ ]]`.
fn contains_output_redirect(command: &str) -> bool {
    let bytes = command.as_bytes();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\'' if !in_double_quote => in_single_quote = !in_single_quote,
            b'"' if !in_single_quote => in_double_quote = !in_double_quote,
            b'\\' if in_double_quote && i + 1 < bytes.len() => {
                i += 1; // skip escaped character inside double quotes
            }
            b'>' if !in_single_quote && !in_double_quote => {
                // Found unquoted `>` — this is a redirect.
                return true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_only_commands() {
        assert_eq!(classify_command("ls -la"), CommandIntent::ReadOnly);
        assert_eq!(classify_command("cat foo.txt"), CommandIntent::ReadOnly);
        assert_eq!(
            classify_command("grep pattern file"),
            CommandIntent::ReadOnly
        );
        assert_eq!(classify_command("git status"), CommandIntent::ReadOnly);
        assert_eq!(classify_command("cargo test"), CommandIntent::ReadOnly);
        assert_eq!(classify_command("pwd"), CommandIntent::ReadOnly);
    }

    #[test]
    fn classifies_write_commands() {
        assert_eq!(classify_command("cp a b"), CommandIntent::Write);
        assert_eq!(classify_command("mv a b"), CommandIntent::Write);
        assert_eq!(classify_command("touch newfile"), CommandIntent::Write);
        assert_eq!(
            classify_command("git commit -m 'msg'"),
            CommandIntent::Write
        );
        assert_eq!(
            classify_command("echo hello > file.txt"),
            CommandIntent::Write
        );
    }

    #[test]
    fn classifies_destructive_commands() {
        assert_eq!(
            classify_command("rm -rf /tmp/stuff"),
            CommandIntent::Destructive
        );
        assert_eq!(
            classify_command("git reset --hard HEAD~1"),
            CommandIntent::Destructive
        );
        assert_eq!(
            classify_command("git clean -fd"),
            CommandIntent::Destructive
        );
        assert_eq!(
            classify_command("dd if=/dev/zero of=/dev/sda"),
            CommandIntent::Destructive
        );
        assert_eq!(
            classify_command("mkfs.ext4 /dev/sda1"),
            CommandIntent::Destructive
        );
    }

    #[test]
    fn classifies_unknown_commands() {
        assert_eq!(
            classify_command("my-custom-tool --flag"),
            CommandIntent::Unknown
        );
        assert_eq!(classify_command(""), CommandIntent::Unknown);
    }

    #[test]
    fn check_destructive_returns_warning() {
        let warning = check_destructive_patterns("rm -rf /");
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("destructive"));

        assert!(check_destructive_patterns("ls -la").is_none());
    }

    #[test]
    fn validate_blocks_writes_in_read_only() {
        let result = validate_for_mode("cp a b", "read-only");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("read-only mode"));
    }

    #[test]
    fn validate_blocks_destructive_in_read_only() {
        let result = validate_for_mode("rm -rf /tmp", "read-only");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("read-only mode"));
    }

    #[test]
    fn validate_allows_reads_in_read_only() {
        let result = validate_for_mode("ls -la", "read-only");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn validate_warns_destructive_in_workspace_write() {
        let result = validate_for_mode("rm -rf /tmp", "workspace-write");
        assert!(result.is_ok());
        let warning = result.unwrap();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("destructive"));
    }

    #[test]
    fn validate_allows_writes_in_workspace_write() {
        let result = validate_for_mode("cp a b", "workspace-write");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn validate_allows_everything_in_danger_mode() {
        let result = validate_for_mode("cp a b", "danger-full-access");
        assert!(result.is_ok());
    }

    #[test]
    fn output_redirect_detected_as_write() {
        assert_eq!(classify_command("echo hello > file"), CommandIntent::Write);
        assert_eq!(classify_command("cat a >> b"), CommandIntent::Write);
        assert_eq!(classify_command("ls 2> /dev/null"), CommandIntent::Write);
    }

    #[test]
    fn quoted_redirect_not_detected_as_write() {
        // > inside quotes should not be treated as a redirect
        assert_ne!(
            classify_command(r#"echo "value > 100""#),
            CommandIntent::Write
        );
        assert_ne!(
            classify_command("echo 'redirect > file'"),
            CommandIntent::Write
        );
    }
}
