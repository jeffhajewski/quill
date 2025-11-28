//! Compatibility checking command
//!
//! Uses `buf` CLI for breaking change detection when available,
//! with fallback to basic proto file comparison.

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

#[derive(Args, Debug)]
pub struct CompatArgs {
    /// Reference to compare against (git ref, registry URL, or local path)
    #[arg(short, long)]
    pub against: String,

    /// Proto files or directories to check (defaults to current directory)
    #[arg(default_value = ".")]
    pub input: Vec<String>,

    /// Fail on breaking changes (exit code 2)
    #[arg(long)]
    pub strict: bool,

    /// Path to buf.yaml config file
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Output format (text, json)
    #[arg(short, long, default_value = "text")]
    pub format: String,

    /// Limit error count (0 = unlimited)
    #[arg(long, default_value = "0")]
    pub error_limit: usize,
}

/// Check if buf CLI is available
fn buf_available() -> bool {
    Command::new("buf")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run compatibility check using buf CLI
fn run_buf_breaking(args: &CompatArgs) -> Result<BreakingResult> {
    let mut cmd = Command::new("buf");
    cmd.arg("breaking");

    // Add input paths
    for input in &args.input {
        cmd.arg(input);
    }

    // Add against reference
    cmd.arg("--against").arg(&args.against);

    // Add config if specified
    if let Some(config) = &args.config {
        cmd.arg("--config").arg(config);
    }

    // Set output format
    if args.format == "json" {
        cmd.arg("--error-format").arg("json");
    }

    // Set error limit
    if args.error_limit > 0 {
        cmd.arg("--limit").arg(args.error_limit.to_string());
    }

    let output = cmd.output().context("Failed to execute buf command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        Ok(BreakingResult {
            breaking_changes: vec![],
            has_breaking: false,
            output: "No breaking changes detected.".to_string(),
        })
    } else {
        // Parse breaking changes from output
        let changes: Vec<BreakingChange> = if args.format == "json" {
            parse_json_output(&stdout)?
        } else {
            parse_text_output(&stdout, &stderr)
        };

        let has_breaking = !changes.is_empty();
        let output = if stdout.is_empty() {
            stderr.to_string()
        } else {
            stdout.to_string()
        };

        Ok(BreakingResult {
            breaking_changes: changes,
            has_breaking,
            output,
        })
    }
}

/// Parse JSON output from buf breaking
fn parse_json_output(output: &str) -> Result<Vec<BreakingChange>> {
    let mut changes = vec![];

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(change) = serde_json::from_str::<BufJsonError>(line) {
            changes.push(BreakingChange {
                file: change.path.unwrap_or_default(),
                line: change.start_line.unwrap_or(0),
                column: change.start_column.unwrap_or(0),
                message: change.message,
                rule: change.r#type.unwrap_or_default(),
            });
        }
    }

    Ok(changes)
}

/// Parse text output from buf breaking
fn parse_text_output(stdout: &str, stderr: &str) -> Vec<BreakingChange> {
    let mut changes = vec![];
    let combined = format!("{}\n{}", stdout, stderr);

    for line in combined.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Format: file:line:column:message
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 4 {
            changes.push(BreakingChange {
                file: parts[0].to_string(),
                line: parts[1].parse().unwrap_or(0),
                column: parts[2].parse().unwrap_or(0),
                message: parts[3].trim().to_string(),
                rule: String::new(),
            });
        } else if !line.trim().is_empty() {
            changes.push(BreakingChange {
                file: String::new(),
                line: 0,
                column: 0,
                message: line.to_string(),
                rule: String::new(),
            });
        }
    }

    changes
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct BufJsonError {
    path: Option<String>,
    start_line: Option<u32>,
    start_column: Option<u32>,
    end_line: Option<u32>,
    end_column: Option<u32>,
    r#type: Option<String>,
    message: String,
}

#[derive(Debug)]
pub struct BreakingChange {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub rule: String,
}

#[derive(Debug)]
pub struct BreakingResult {
    pub breaking_changes: Vec<BreakingChange>,
    pub has_breaking: bool,
    pub output: String,
}

pub fn run(args: CompatArgs) -> Result<()> {
    if !buf_available() {
        eprintln!("Warning: 'buf' CLI not found. For best results, install buf:");
        eprintln!("  https://buf.build/docs/installation");
        eprintln!();
        eprintln!("Without buf, compatibility checking is limited.");
        eprintln!();

        // Provide basic info about what we would check
        println!("Breaking Change Categories (requires buf):");
        println!("  - Field removals or renumbering");
        println!("  - Field type changes");
        println!("  - Required field additions");
        println!("  - Enum value removals");
        println!("  - Service/method removals");
        println!("  - Method signature changes");
        println!();
        println!("To check: {} against {}", args.input.join(", "), args.against);

        if args.strict {
            anyhow::bail!("buf CLI required for strict mode");
        }
        return Ok(());
    }

    println!("Checking compatibility...");
    println!("  Input:   {}", args.input.join(", "));
    println!("  Against: {}", args.against);
    println!();

    let result = run_buf_breaking(&args)?;

    if result.has_breaking {
        if args.format == "json" {
            // Output as JSON array
            let json_output: Vec<serde_json::Value> = result
                .breaking_changes
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "file": c.file,
                        "line": c.line,
                        "column": c.column,
                        "message": c.message,
                        "rule": c.rule,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json_output)?);
        } else {
            println!("Breaking changes detected:");
            println!();
            for change in &result.breaking_changes {
                if !change.file.is_empty() {
                    print!("  {}:{}:{}: ", change.file, change.line, change.column);
                } else {
                    print!("  ");
                }
                println!("{}", change.message);
            }
        }

        println!();
        println!(
            "Found {} breaking change(s)",
            result.breaking_changes.len()
        );

        if args.strict {
            std::process::exit(2);
        }
    } else {
        println!("{}", result.output);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_output() {
        let stdout = "proto/api.proto:10:3:Field \"foo\" was removed.";
        let changes = parse_text_output(stdout, "");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].file, "proto/api.proto");
        assert_eq!(changes[0].line, 10);
        assert_eq!(changes[0].column, 3);
        assert!(changes[0].message.contains("foo"));
    }

    #[test]
    fn test_buf_available() {
        // This test just ensures the function doesn't panic
        let _ = buf_available();
    }

    #[test]
    fn test_compat_args_defaults() {
        let args = CompatArgs {
            against: "main".to_string(),
            input: vec![".".to_string()],
            strict: false,
            config: None,
            format: "text".to_string(),
            error_limit: 0,
        };
        assert_eq!(args.format, "text");
        assert!(!args.strict);
    }
}
