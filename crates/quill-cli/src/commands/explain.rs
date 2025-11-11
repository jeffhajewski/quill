//! Payload decoding command

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// Path to file descriptor set (.pb file)
    #[arg(short, long)]
    pub descriptor_set: PathBuf,

    /// Payload to decode (hex, base64, or file path)
    #[arg(short, long)]
    pub payload: String,

    /// Message type to decode as (e.g., greeter.v1.HelloRequest)
    #[arg(short, long)]
    pub message_type: Option<String>,

    /// Input format (hex, base64, file)
    #[arg(short, long, default_value = "hex")]
    pub format: String,

    /// Output format (json, text)
    #[arg(short, long, default_value = "json")]
    pub output: String,
}

pub fn run(args: ExplainArgs) -> Result<()> {
    println!("Decoding payload...");
    println!("  Descriptor set: {}", args.descriptor_set.display());
    println!("  Format: {}", args.format);

    if !args.descriptor_set.exists() {
        anyhow::bail!("Descriptor set file not found: {}", args.descriptor_set.display());
    }

    // TODO: Implement payload decoding
    // This would:
    // 1. Load file descriptor set
    // 2. Decode payload based on format (hex/base64/file)
    // 3. If message_type specified, decode as that type
    // 4. Otherwise, try to infer message type
    // 5. Output decoded message in specified format

    anyhow::bail!("Payload decoding not yet implemented");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_args() {
        let args = ExplainArgs {
            descriptor_set: PathBuf::from("test.pb"),
            payload: "0a05776f726c64".to_string(),
            message_type: Some("test.Message".to_string()),
            format: "hex".to_string(),
            output: "json".to_string(),
        };

        assert_eq!(args.format, "hex");
        assert_eq!(args.output, "json");
    }
}
