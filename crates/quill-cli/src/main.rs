//! CLI tool for the Quill RPC framework.
//!
//! Provides commands for:
//! - gen: Code generation
//! - call: Making RPC calls (curl-for-proto)
//! - bench: Benchmarking
//! - compat: Breaking change detection
//! - explain: Payload decoding

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "quill")]
#[command(about = "CLI tool for the Quill RPC framework", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate code from .proto files
    Gen,
    /// Make RPC calls (curl-for-proto)
    Call,
    /// Run benchmarks
    Bench,
    /// Check for breaking changes
    Compat,
    /// Decode payloads
    Explain,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Gen => println!("gen command (not yet implemented)"),
        Commands::Call => println!("call command (not yet implemented)"),
        Commands::Bench => println!("bench command (not yet implemented)"),
        Commands::Compat => println!("compat command (not yet implemented)"),
        Commands::Explain => println!("explain command (not yet implemented)"),
    }
}
