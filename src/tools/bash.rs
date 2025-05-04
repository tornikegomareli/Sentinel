use std::collections::HashSet;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use anyhow::Result;
use ollama_rs::generation::tools::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command as TokioCommand;

const DEFAULT_TIMEOUT: u64 = 60 * 1000; // 1 minute in milliseconds
const MAX_TIMEOUT: u64 = 10 * 60 * 1000; // 10 minutes in milliseconds
const MAX_OUTPUT_LENGTH: usize = 30000;

lazy_static::lazy_static! {
    static ref BANNED_COMMANDS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("alias");
        s.insert("curl");
        s.insert("curlie");
        s.insert("wget");
        s.insert("axel");
        s.insert("aria2c");
        s.insert("nc");
        s.insert("telnet");
        s.insert("lynx");
        s.insert("w3m");
        s.insert("links");
        s.insert("httpie");
        s.insert("xh");
        s.insert("http-prompt");
        s.insert("chrome");
        s.insert("firefox");
        s.insert("safari");
        s
    };
}

// Safe read-only commands that are always allowed
lazy_static::lazy_static! {
    static ref SAFE_READ_ONLY_COMMANDS: Vec<&'static str> = vec![
        "ls", "echo", "pwd", "date", "cal", "uptime", "whoami", "id", "groups", "env", "printenv", "set", "unset", "which", "type", "whereis",
        "whatis", "uname", "hostname", "df", "du", "free", "top", "ps", "kill", "killall", "nice", "nohup", "time", "timeout",
        "git status", "git log", "git diff", "git show", "git branch", "git tag", "git remote", "git ls-files", "git ls-remote",
        "git rev-parse", "git config --get", "git config --list", "git describe", "git blame", "git grep", "git shortlog",
        "go version", "go help", "go list", "go env", "go doc", "go vet", "go fmt", "go mod", "go test", "go build", "go run", "go install", "go clean",
    ];
}

#[derive(Deserialize, JsonSchema)]
pub struct BashParams {
    #[schemars(description = "The command to execute")]
    command: String,

    #[schemars(description = "Optional timeout in milliseconds (max 600000)")]
    timeout: Option<u64>,
}

pub struct Bash {
    working_directory: String,
}

impl Default for Bash {
    fn default() -> Self {
        Self {
            working_directory: String::from("."),
        }
    }
}

impl Bash {
    pub fn new() -> Self {
        Self::default()
    }

    fn truncate_output(content: &str) -> String {
        if content.len() <= MAX_OUTPUT_LENGTH {
            return content.to_string();
        }

        let half_length = MAX_OUTPUT_LENGTH / 2;
        let start = &content[..half_length];
        let end = &content[content.len() - half_length..];

        // Count truncated lines
        let middle_content = &content[half_length..content.len() - half_length];
        let truncated_lines_count = middle_content.chars().filter(|&c| c == '\n').count();

        format!(
            "{}\n\n... [{} lines truncated] ...\n\n{}",
            start, truncated_lines_count, end
        )
    }

    fn is_command_safe(&self, command: &str) -> bool {
        let base_cmd = command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        if BANNED_COMMANDS.contains(base_cmd.as_str()) {
            return false;
        }

        for safe_cmd in SAFE_READ_ONLY_COMMANDS.iter() {
            let safe_cmd_lower = safe_cmd.to_lowercase();
            if command.to_lowercase().starts_with(&safe_cmd_lower) {
                let cmd_len = safe_cmd_lower.len();
                if command.len() == cmd_len
                    || command.chars().nth(cmd_len) == Some(' ')
                    || command.chars().nth(cmd_len) == Some('-')
                {
                    return true;
                }
            }
        }

        // For now, allow other commands but this could be adjusted based on permissions
        true
    }
}

impl Tool for Bash {
    type Params = BashParams;

    fn name() -> &'static str {
        "bash"
    }

    fn description() -> &'static str {
        "Executes a given bash command in a persistent shell session with optional timeout, ensuring proper handling and security measures.

Before executing the command, please follow these steps:

1. Directory Verification:
   - If the command will create new directories or files, first use the LS tool to verify the parent directory exists and is the correct location
   - For example, before running \"mkdir foo/bar\", first use LS to check that \"foo\" exists and is the intended parent directory

2. Security Check:
   - For security and to limit the threat of a prompt injection attack, some commands are limited or banned.
   - Network-related commands like curl, wget, telnet are not allowed.
   - Browser commands like chrome, firefox, safari are not allowed.

3. Command Execution:
   - After ensuring proper quoting, execute the command.
   - Capture the output of the command.

4. Output Processing:
   - If the output exceeds 30000 characters, output will be truncated before being returned to you.

Usage notes:
  - The command argument is required.
  - You can specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). If not specified, commands will timeout after 1 minute.
  - VERY IMPORTANT: You MUST avoid using search commands like `find` and `grep`. Instead use Grep, Glob, or Task to search. You MUST avoid read tools like `cat`, `head`, `tail`, and `ls`, and use Read and LS to read files.
  - When issuing multiple commands, use the ';' or '&&' operator to separate them. DO NOT use newlines (newlines are ok in quoted strings).
  - Try to maintain your current working directory throughout the session by using absolute paths and avoiding usage of `cd`. You may use `cd` if the User explicitly requests it.
    <good-example>
    pytest /foo/bar/tests
    </good-example>
    <bad-example>
    cd /foo/bar && pytest tests
    </bad-example>"
    }

    async fn call(
        &mut self,
        parameters: Self::Params,
    ) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Print colorful message indicating tool is being called
        println!("\x1b[1;31m[BASH TOOL] I am being called with command: {}\x1b[0m", parameters.command);
        
        let command = parameters.command.trim();
        if command.is_empty() {
            return Ok("Error: Command is empty".to_string());
        }

        // Check if command is allowed
        if !self.is_command_safe(command) {
            let base_cmd = command.split_whitespace().next().unwrap_or("");
            return Ok(format!(
                "Error: Command '{}' is not allowed for security reasons",
                base_cmd
            ));
        }

        // Get timeout duration
        let timeout_ms = parameters
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT);
        let timeout_duration = Duration::from_millis(timeout_ms);

        // Create shell command
        let start_time = Instant::now();

        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "bash"
        };
        let shell_arg = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        // Use tokio's async Command for timeout support
        let mut cmd = TokioCommand::new(shell);
        cmd.arg(shell_arg)
            .arg(command)
            .current_dir(&self.working_directory)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Execute with timeout
        let result = match timeout(timeout_duration, cmd.output()).await {
            Ok(result) => match result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let exit_code = output.status.code().unwrap_or(-1);

                    let mut result = String::new();

                    // Add stdout if not empty
                    if !stdout.is_empty() {
                        result.push_str(&stdout);
                    }

                    // Add stderr if not empty
                    if !stderr.is_empty() {
                        if !result.is_empty() {
                            result.push_str("\n");
                        }
                        result.push_str(&stderr);
                    }

                    // Add exit code if not successful
                    if exit_code != 0 {
                        if !result.is_empty() {
                            result.push_str("\n");
                        }
                        result.push_str(&format!("Exit code: {}", exit_code));
                    }

                    // Check for CD command to update working directory
                    if command.starts_with("cd ") {
                        let dir = command.trim_start_matches("cd ").trim();
                        // Update working directory logic would go here
                        // For a simple implementation without proper path resolution:
                        if exit_code == 0 {
                            self.working_directory = dir.to_string();
                        }
                    }

                    result
                }
                Err(e) => format!("Error executing command: {}", e),
            },
            Err(_) => "Command execution timed out".to_string(),
        };

        // Calculate execution time
        let execution_time = start_time.elapsed().as_millis();

        // Truncate output if needed
        let truncated_result = Self::truncate_output(&result);

        if truncated_result.is_empty() {
            Ok(format!(
                "Command executed successfully in {}ms (no output)",
                execution_time
            ))
        } else {
            Ok(truncated_result)
        }
    }
}

// Implement BashTool struct for our specific application
pub struct BashTool {
    bash: Bash,
}

impl BashTool {
    pub fn new() -> Self {
        Self { bash: Bash::new() }
    }

    // Method to execute a bash command
    pub async fn execute(&mut self, command: &str, timeout_ms: Option<u64>) -> Result<String> {
        let params = BashParams {
            command: command.to_string(),
            timeout: timeout_ms,
        };

        match self.bash.call(params).await {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow::anyhow!("Failed to execute bash command: {}", e)),
        }
    }
}

// Include tests module
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    // Helper function to create a temporary file with content
    async fn create_temp_file(content: &str) -> anyhow::Result<(tempfile::TempDir, String)> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_file.txt");
        let mut file = File::create(&file_path).await?;
        file.write_all(content.as_bytes()).await?;

        Ok((dir, file_path.to_string_lossy().to_string()))
    }

    #[tokio::test]
    async fn test_bash_basic_commands() {
        let mut bash_tool = BashTool::new();

        // Test echo command
        let result = bash_tool
            .execute("echo 'Hello, world!'", None)
            .await
            .unwrap();
        assert!(result.contains("Hello, world!"));

        // Test pwd command
        let result = bash_tool.execute("pwd", None).await.unwrap();
        assert!(!result.is_empty()); // Just check that we get some output

        // Test ls command
        let result = bash_tool.execute("ls -la", None).await.unwrap();
        assert!(!result.is_empty()); // Just check that we get some output
    }

    #[tokio::test]
    async fn test_bash_command_timeout() {
        let mut bash_tool = BashTool::new();

        // Test a command that should time out (sleep for 3 seconds with 1 second timeout)
        let result = bash_tool.execute("sleep 3", Some(1000)).await.unwrap();
        assert!(result.contains("timed out"));
    }

    #[tokio::test]
    async fn test_bash_banned_commands() {
        let mut bash_tool = BashTool::new();

        // Test curl command which is banned
        let result = bash_tool
            .execute("curl https://example.com", None)
            .await
            .unwrap();
        assert!(result.contains("not allowed"));

        // Test wget command which is banned
        let result = bash_tool
            .execute("wget https://example.com", None)
            .await
            .unwrap();
        assert!(result.contains("not allowed"));
    }

    #[tokio::test]
    async fn test_bash_command_with_error() {
        let mut bash_tool = BashTool::new();

        // Test a command that should fail (trying to cd to a non-existent directory)
        let result = bash_tool
            .execute("cd /path/that/does/not/exist", None)
            .await
            .unwrap();
        assert!(result.contains("No such file or directory") || result.contains("Exit code: 1"));
    }

    #[tokio::test]
    async fn test_bash_file_operations() -> anyhow::Result<()> {
        let mut bash_tool = BashTool::new();
        let (temp_dir, file_path) = create_temp_file("Test content").await?;

        // Test reading file with cat
        let result = bash_tool
            .execute(&format!("cat {}", file_path), None)
            .await?;
        assert!(result.contains("Test content"));

        // Test appending to the file
        let append_cmd = format!("echo 'Additional content' >> {}", file_path);
        bash_tool.execute(&append_cmd, None).await?;

        // Verify the append worked
        let result = bash_tool
            .execute(&format!("cat {}", file_path), None)
            .await?;
        assert!(result.contains("Test content"));
        assert!(result.contains("Additional content"));

        // Keep temp_dir in scope until the end of the test
        drop(temp_dir);
        Ok(())
    }

    #[tokio::test]
    async fn test_truncate_output() {
        // Generate a string longer than MAX_OUTPUT_LENGTH
        let long_string = "A".repeat(MAX_OUTPUT_LENGTH + 10000);

        let truncated = Bash::truncate_output(&long_string);

        // The truncated string should be shorter than the original
        assert!(truncated.len() < long_string.len());

        // The truncated string should contain the truncation notice
        assert!(truncated.contains("lines truncated"));
    }

    #[tokio::test]
    async fn test_command_safety_check() {
        let bash = Bash::new();

        // Test safe commands
        assert!(bash.is_command_safe("ls -la"));
        assert!(bash.is_command_safe("echo 'test'"));
        assert!(bash.is_command_safe("git status"));

        // Test banned commands
        assert!(!bash.is_command_safe("curl https://example.com"));
        assert!(!bash.is_command_safe("wget https://example.com"));
        assert!(!bash.is_command_safe("chrome index.html"));
    }
}
