//! Code generation command

use anyhow::{Context, Result};
use clap::Args;
use quill_codegen::{compile_protos, QuillConfig};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct GenArgs {
    /// Proto files to compile
    #[arg(required = true)]
    pub protos: Vec<PathBuf>,

    /// Include directories for proto imports
    #[arg(short = 'I', long, default_value = ".")]
    pub includes: Vec<PathBuf>,

    /// Output directory (defaults to OUT_DIR or current directory)
    #[arg(short, long)]
    pub out: Option<PathBuf>,

    /// Generate client code only
    #[arg(long)]
    pub client_only: bool,

    /// Generate server code only
    #[arg(long)]
    pub server_only: bool,

    /// Package prefix for service paths
    #[arg(long)]
    pub package_prefix: Option<String>,
}

pub fn run(args: GenArgs) -> Result<()> {
    println!("Generating code from proto files...");
    println!("  Protos: {:?}", args.protos);
    println!("  Includes: {:?}", args.includes);

    // Validate that proto files exist
    for proto in &args.protos {
        if !proto.exists() {
            anyhow::bail!("Proto file not found: {}", proto.display());
        }
    }

    // Validate include directories
    for include in &args.includes {
        if !include.exists() {
            anyhow::bail!("Include directory not found: {}", include.display());
        }
    }

    // Configure code generation
    let mut config = if args.client_only {
        QuillConfig::client_only()
    } else if args.server_only {
        QuillConfig::server_only()
    } else {
        QuillConfig::new()
    };

    if let Some(prefix) = args.package_prefix {
        config = config.with_package_prefix(prefix);
    }

    // Set output directory if specified
    if let Some(out_dir) = args.out {
        std::env::set_var("OUT_DIR", out_dir.as_os_str());
    }

    // Compile protos
    compile_protos(&args.protos, &args.includes, config)
        .context("Failed to compile proto files")?;

    println!("✓ Code generation complete!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_args_parsing() {
        let args = GenArgs {
            protos: vec![PathBuf::from("test.proto")],
            includes: vec![PathBuf::from(".")],
            out: None,
            client_only: false,
            server_only: false,
            package_prefix: None,
        };

        assert_eq!(args.protos.len(), 1);
        assert_eq!(args.includes.len(), 1);
    }

    #[test]
    fn test_config_selection() {
        // Client only
        let config = QuillConfig::client_only();
        assert!(config.generate_client);
        assert!(!config.generate_server);

        // Server only
        let config = QuillConfig::server_only();
        assert!(!config.generate_client);
        assert!(config.generate_server);

        // Both
        let config = QuillConfig::new();
        assert!(config.generate_client);
        assert!(config.generate_server);
    }
}
