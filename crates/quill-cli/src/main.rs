//! CLI tool for the Quill RPC framework.
//!
//! Provides commands for:
//! - gen: Code generation
//! - call: Making RPC calls (curl-for-proto)
//! - bench: Benchmarking
//! - compat: Breaking change detection
//! - explain: Payload decoding

mod commands;

use clap::{Parser, Subcommand};
use commands::{bench, call, compat, explain, gen};

#[derive(Parser)]
#[command(name = "quill")]
#[command(about = "CLI tool for the Quill RPC framework", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate code from .proto files
    Gen(gen::GenArgs),
    /// Make RPC calls (curl-for-proto)
    Call(call::CallArgs),
    /// Run benchmarks
    Bench(bench::BenchArgs),
    /// Check for breaking changes
    Compat(compat::CompatArgs),
    /// Decode payloads
    Explain(explain::ExplainArgs),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Gen(args) => gen::run(args),
        Commands::Call(args) => call::run(args).await,
        Commands::Bench(args) => bench::run(args).await,
        Commands::Compat(args) => compat::run(args),
        Commands::Explain(args) => explain::run(args),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(match e.to_string().as_str() {
            s if s.contains("Invalid input") => 2,
            s if s.contains("Network") || s.contains("Connection") => 3,
            s if s.contains("Server") || s.contains("RPC") => 4,
            _ => 1,
        });
    }
}
