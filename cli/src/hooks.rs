//! Hook execution for backup jobs.
//!
//! Hooks are shell commands that run before and after backup operations.
//! They are useful for:
//! - Pre-backup: Database dumps, application quiesce, staging data
//! - Post-backup: Cleanup staging directories, notifications
//!
//! ## Timeout Behavior
//!
//! When a hook times out, the entire process group is killed to ensure
//! all child processes spawned by the hook are terminated. On non-Unix
//! systems, only the immediate shell process is killed.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, info, warn};

#[cfg(unix)]
#[allow(unused_imports)]
use std::os::unix::process::CommandExt;

/// Configuration for a hook execution.
#[derive(Debug, Clone)]
pub struct HookConfig {
    /// The shell command to execute.
    pub command: String,

    /// Maximum time the hook can run.
    pub timeout: Duration,

    /// Shell to use (e.g., /bin/sh, /bin/bash).
    pub shell: String,

    /// Working directory for the command.
    pub working_dir: Option<PathBuf>,
}

/// Result of a hook execution.
#[derive(Debug)]
pub struct HookResult {
    /// Whether the hook succeeded (exit code 0).
    pub success: bool,

    /// Exit code of the command.
    pub exit_code: Option<i32>,

    /// Standard output.
    pub stdout: String,

    /// Standard error.
    pub stderr: String,

    /// How long the hook took to execute.
    pub duration: Duration,

    /// Whether the hook was killed due to timeout.
    pub timed_out: bool,
}

impl HookResult {
    /// Create a result for a successful hook.
    pub fn success(stdout: String, stderr: String, duration: Duration) -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout,
            stderr,
            duration,
            timed_out: false,
        }
    }

    /// Create a result for a failed hook.
    pub fn failure(exit_code: Option<i32>, stdout: String, stderr: String, duration: Duration) -> Self {
        Self {
            success: false,
            exit_code,
            stdout,
            stderr,
            duration,
            timed_out: false,
        }
    }

    /// Create a result for a timed-out hook.
    pub fn timeout(stdout: String, stderr: String, duration: Duration) -> Self {
        Self {
            success: false,
            exit_code: None,
            stdout,
            stderr,
            duration,
            timed_out: true,
        }
    }
}

/// Execute a hook command.
///
/// On Unix systems, the hook runs in its own process group so that timeouts
/// can kill the entire command tree (not just the shell).
pub async fn execute_hook(config: &HookConfig) -> Result<HookResult> {
    let start = Instant::now();

    info!("Executing hook: {}", truncate_command(&config.command, 60));
    debug!("Full command: {}", config.command);
    debug!("Shell: {}", config.shell);
    debug!("Timeout: {:?}", config.timeout);

    let mut cmd = Command::new(&config.shell);
    cmd.arg("-c").arg(&config.command);

    // Set working directory if specified
    if let Some(ref dir) = config.working_dir {
        cmd.current_dir(dir);
        debug!("Working directory: {}", dir.display());
    }

    // Capture stdout and stderr
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // On Unix, create a new process group so we can kill all children on timeout
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            // Create a new process group with this process as the leader
            libc::setsid();
            Ok(())
        });
    }

    // Spawn the process
    let mut child = cmd.spawn().with_context(|| {
        format!(
            "Failed to spawn hook process with shell '{}'",
            config.shell
        )
    })?;

    // Store the process ID for potential process group kill
    #[cfg(unix)]
    let child_pid = child.id();

    // Get handles to stdout/stderr
    let mut stdout_handle = child.stdout.take().expect("stdout was piped");
    let mut stderr_handle = child.stderr.take().expect("stderr was piped");

    // Execute with timeout
    let result = timeout(config.timeout, async {
        // Read stdout and stderr concurrently
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();

        let stdout_read = async {
            stdout_handle.read_to_end(&mut stdout_buf).await
        };
        let stderr_read = async {
            stderr_handle.read_to_end(&mut stderr_buf).await
        };
        let wait = child.wait();

        let (stdout_result, stderr_result, wait_result) =
            tokio::join!(stdout_read, stderr_read, wait);

        stdout_result?;
        stderr_result?;
        let status = wait_result?;

        Ok::<_, anyhow::Error>((status, stdout_buf, stderr_buf))
    })
    .await;

    let duration = start.elapsed();

    match result {
        Ok(Ok((status, stdout_buf, stderr_buf))) => {
            let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
            let stderr = String::from_utf8_lossy(&stderr_buf).to_string();

            if status.success() {
                info!("Hook completed successfully in {:?}", duration);
                if !stdout.is_empty() {
                    debug!("Hook stdout:\n{}", stdout);
                }
                Ok(HookResult::success(stdout, stderr, duration))
            } else {
                let code = status.code();
                warn!(
                    "Hook failed with exit code {:?} in {:?}",
                    code, duration
                );
                if !stderr.is_empty() {
                    warn!("Hook stderr:\n{}", stderr);
                }
                Ok(HookResult::failure(code, stdout, stderr, duration))
            }
        }
        Ok(Err(e)) => {
            // Process error
            Err(e.context("Hook process failed"))
        }
        Err(_) => {
            // Timeout - kill the entire process group
            warn!("Hook timed out after {:?}, killing process group", config.timeout);

            // On Unix, kill the entire process group
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                // Kill the process group (negative PID means process group)
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
                debug!("Sent SIGKILL to process group {}", pid);
            }

            // Also try the standard kill as fallback
            if let Err(e) = child.kill().await {
                debug!("Standard kill also failed (expected): {}", e);
            }

            // Wait for the process to clean up
            let _ = child.wait().await;

            Ok(HookResult::timeout(String::new(), String::new(), duration))
        }
    }
}

/// Execute a hook and print its output.
pub async fn execute_hook_with_output(
    name: &str,
    config: &HookConfig,
    verbose: bool,
) -> Result<HookResult> {
    println!("  {}: running...", name);

    let result = execute_hook(config).await?;

    if result.success {
        println!("  {}: OK ({:.1}s)", name, result.duration.as_secs_f64());
    } else if result.timed_out {
        println!(
            "  {}: TIMEOUT after {:.1}s",
            name,
            result.duration.as_secs_f64()
        );
    } else {
        println!(
            "  {}: FAILED (exit code {:?}, {:.1}s)",
            name,
            result.exit_code,
            result.duration.as_secs_f64()
        );
    }

    // Print output if verbose or on failure
    if verbose || !result.success {
        if !result.stdout.is_empty() {
            println!("    stdout:");
            for line in result.stdout.lines() {
                println!("      {}", line);
            }
        }
        if !result.stderr.is_empty() {
            println!("    stderr:");
            for line in result.stderr.lines() {
                println!("      {}", line);
            }
        }
    }

    Ok(result)
}

/// Truncate a command string for display.
fn truncate_command(cmd: &str, max_len: usize) -> String {
    let cmd = cmd.trim();
    let first_line = cmd.lines().next().unwrap_or(cmd);

    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_successful_hook() {
        let config = HookConfig {
            command: "echo 'hello world'".to_string(),
            timeout: Duration::from_secs(10),
            shell: "/bin/sh".to_string(),
            working_dir: None,
        };

        let result = execute_hook(&config).await.unwrap();
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello world"));
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn test_failed_hook() {
        let config = HookConfig {
            command: "exit 1".to_string(),
            timeout: Duration::from_secs(10),
            shell: "/bin/sh".to_string(),
            working_dir: None,
        };

        let result = execute_hook(&config).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn test_hook_timeout() {
        let config = HookConfig {
            command: "sleep 10".to_string(),
            timeout: Duration::from_millis(100),
            shell: "/bin/sh".to_string(),
            working_dir: None,
        };

        let result = execute_hook(&config).await.unwrap();
        assert!(!result.success);
        assert!(result.timed_out);
    }

    #[tokio::test]
    async fn test_hook_with_working_dir() {
        let config = HookConfig {
            command: "pwd".to_string(),
            timeout: Duration::from_secs(10),
            shell: "/bin/sh".to_string(),
            working_dir: Some(PathBuf::from("/tmp")),
        };

        let result = execute_hook(&config).await.unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("/tmp"));
    }

    #[tokio::test]
    async fn test_hook_captures_stderr() {
        let config = HookConfig {
            command: "echo 'error message' >&2".to_string(),
            timeout: Duration::from_secs(10),
            shell: "/bin/sh".to_string(),
            working_dir: None,
        };

        let result = execute_hook(&config).await.unwrap();
        assert!(result.success);
        assert!(result.stderr.contains("error message"));
    }
}
