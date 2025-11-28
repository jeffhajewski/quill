//! Payload decoding command
//!
//! Decodes protobuf payloads using file descriptor sets for dynamic message introspection.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use clap::{Args, ValueEnum};
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
pub enum InputFormat {
    /// Hexadecimal encoded (e.g., 0a05776f726c64)
    Hex,
    /// Base64 encoded
    Base64,
    /// Raw binary file
    File,
    /// Auto-detect based on content
    Auto,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    /// JSON output (default)
    Json,
    /// Pretty-printed JSON
    JsonPretty,
    /// Text proto format
    Text,
    /// Debug format
    Debug,
}

#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// Path to file descriptor set (.pb or .binpb file)
    #[arg(short, long)]
    pub descriptor_set: PathBuf,

    /// Payload to decode (hex string, base64 string, or file path depending on format)
    #[arg(short, long)]
    pub payload: String,

    /// Message type to decode as (e.g., greeter.v1.HelloRequest)
    /// If not specified, will list available message types
    #[arg(short, long)]
    pub message_type: Option<String>,

    /// Input format for the payload
    #[arg(short = 'f', long, default_value = "auto")]
    pub input_format: InputFormat,

    /// Output format
    #[arg(short, long, default_value = "json-pretty")]
    pub output_format: OutputFormat,

    /// List all available message types in the descriptor set
    #[arg(long)]
    pub list_types: bool,

    /// Show field numbers in output
    #[arg(long)]
    pub show_field_numbers: bool,
}

/// Load a file descriptor set from a .pb file
fn load_descriptor_pool(path: &PathBuf) -> Result<DescriptorPool> {
    let bytes = fs::read(path)
        .with_context(|| format!("Failed to read descriptor set: {}", path.display()))?;

    DescriptorPool::decode(bytes.as_slice())
        .with_context(|| format!("Failed to parse descriptor set: {}", path.display()))
}

/// Decode payload bytes based on input format
fn decode_payload(payload: &str, format: &InputFormat) -> Result<Vec<u8>> {
    match format {
        InputFormat::Hex => hex::decode(payload.trim_start_matches("0x").replace(' ', ""))
            .context("Failed to decode hex payload"),
        InputFormat::Base64 => {
            BASE64.decode(payload).context("Failed to decode base64 payload")
        }
        InputFormat::File => {
            fs::read(payload).with_context(|| format!("Failed to read payload file: {}", payload))
        }
        InputFormat::Auto => {
            // Try to auto-detect format
            let trimmed = payload.trim();

            // Check if it's a file path that exists
            if PathBuf::from(trimmed).exists() {
                return fs::read(trimmed)
                    .with_context(|| format!("Failed to read payload file: {}", trimmed));
            }

            // Try hex (common for protobuf debugging)
            if trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == ' ') {
                if let Ok(bytes) = hex::decode(trimmed.replace(' ', "")) {
                    return Ok(bytes);
                }
            }

            // Try base64
            if let Ok(bytes) = BASE64.decode(trimmed) {
                return Ok(bytes);
            }

            anyhow::bail!(
                "Could not auto-detect payload format. Please specify --input-format explicitly."
            )
        }
    }
}

/// Find a message descriptor by name
fn find_message(pool: &DescriptorPool, name: &str) -> Option<MessageDescriptor> {
    // Try exact match first
    if let Some(msg) = pool.get_message_by_name(name) {
        return Some(msg);
    }

    // Try with leading dot
    if let Some(msg) = pool.get_message_by_name(&format!(".{}", name)) {
        return Some(msg);
    }

    // Search all messages for partial match
    for msg in pool.all_messages() {
        if msg.full_name().ends_with(name) || msg.name() == name {
            return Some(msg);
        }
    }

    None
}

/// Format a decoded message for output
fn format_message(
    msg: &DynamicMessage,
    format: &OutputFormat,
    show_field_numbers: bool,
) -> Result<String> {
    match format {
        OutputFormat::Json => {
            serde_json::to_string(msg).context("Failed to serialize to JSON")
        }
        OutputFormat::JsonPretty => {
            serde_json::to_string_pretty(msg).context("Failed to serialize to JSON")
        }
        OutputFormat::Text => {
            // Use debug format with field info
            let mut output = String::new();
            format_message_text(msg, &mut output, 0, show_field_numbers);
            Ok(output)
        }
        OutputFormat::Debug => Ok(format!("{:#?}", msg)),
    }
}

/// Format message as text proto
fn format_message_text(msg: &DynamicMessage, output: &mut String, indent: usize, show_numbers: bool) {
    let indent_str = "  ".repeat(indent);

    for field in msg.descriptor().fields() {
        if !msg.has_field(&field) {
            continue;
        }

        let value = msg.get_field(&field);
        let field_name = field.name();
        let field_num = if show_numbers {
            format!(" [{}]", field.number())
        } else {
            String::new()
        };

        match value.as_ref() {
            prost_reflect::Value::Message(nested) => {
                output.push_str(&format!("{}{}{}: {{\n", indent_str, field_name, field_num));
                format_message_text(nested, output, indent + 1, show_numbers);
                output.push_str(&format!("{}}}\n", indent_str));
            }
            prost_reflect::Value::List(list) => {
                for item in list.iter() {
                    if let prost_reflect::Value::Message(nested) = item {
                        output.push_str(&format!("{}{}{}: {{\n", indent_str, field_name, field_num));
                        format_message_text(nested, output, indent + 1, show_numbers);
                        output.push_str(&format!("{}}}\n", indent_str));
                    } else {
                        output.push_str(&format!("{}{}{}: {:?}\n", indent_str, field_name, field_num, item));
                    }
                }
            }
            prost_reflect::Value::String(s) => {
                output.push_str(&format!("{}{}{}: \"{}\"\n", indent_str, field_name, field_num, s));
            }
            prost_reflect::Value::Bytes(b) => {
                output.push_str(&format!("{}{}{}: <{} bytes>\n", indent_str, field_name, field_num, b.len()));
            }
            other => {
                output.push_str(&format!("{}{}{}: {:?}\n", indent_str, field_name, field_num, other));
            }
        }
    }
}

/// List all message types in a descriptor pool
fn list_message_types(pool: &DescriptorPool) {
    println!("Available message types:");
    println!();

    let mut messages: Vec<_> = pool.all_messages().collect();
    messages.sort_by_key(|m| m.full_name().to_string());

    let mut current_package = String::new();
    for msg in messages {
        let full_name = msg.full_name();
        let package = full_name.rsplit_once('.').map(|(p, _)| p).unwrap_or("");

        if package != current_package {
            if !current_package.is_empty() {
                println!();
            }
            println!("  {}", if package.is_empty() { "(default)" } else { package });
            current_package = package.to_string();
        }

        println!("    - {}", msg.name());
    }

    println!();
    println!("Services:");
    for service in pool.services() {
        println!("  {}", service.full_name());
        for method in service.methods() {
            println!(
                "    - {} ({} -> {})",
                method.name(),
                method.input().name(),
                method.output().name()
            );
        }
    }
}

pub fn run(args: ExplainArgs) -> Result<()> {
    // Load descriptor set
    if !args.descriptor_set.exists() {
        anyhow::bail!(
            "Descriptor set not found: {}\n\n\
            To generate a descriptor set, use:\n\
            protoc --descriptor_set_out=output.pb --include_imports your.proto",
            args.descriptor_set.display()
        );
    }

    let pool = load_descriptor_pool(&args.descriptor_set)?;

    // If listing types, just show them and exit
    if args.list_types {
        list_message_types(&pool);
        return Ok(());
    }

    // Get message type
    let message_type = match &args.message_type {
        Some(t) => t.clone(),
        None => {
            println!("No message type specified. Use --message-type or --list-types\n");
            list_message_types(&pool);
            return Ok(());
        }
    };

    // Find the message descriptor
    let descriptor = find_message(&pool, &message_type).with_context(|| {
        format!(
            "Message type '{}' not found in descriptor set.\n\
            Use --list-types to see available types.",
            message_type
        )
    })?;

    // Decode the payload
    let payload_bytes = decode_payload(&args.payload, &args.input_format)?;

    // Parse the message
    let message = DynamicMessage::decode(descriptor, payload_bytes.as_slice())
        .context("Failed to decode protobuf message")?;

    // Format and output
    let output = format_message(&message, &args.output_format, args.show_field_numbers)?;
    println!("{}", output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_hex_payload() {
        let result = decode_payload("0a05776f726c64", &InputFormat::Hex);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        // This is "world" with a varint tag
        assert_eq!(bytes.len(), 7);
    }

    #[test]
    fn test_decode_hex_with_prefix() {
        let result = decode_payload("0x0a05776f726c64", &InputFormat::Hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_decode_hex_with_spaces() {
        let result = decode_payload("0a 05 77 6f 72 6c 64", &InputFormat::Hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_decode_base64_payload() {
        let result = decode_payload("CgV3b3JsZA==", &InputFormat::Base64);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert_eq!(bytes.len(), 7);
    }

    #[test]
    fn test_auto_detect_hex() {
        let result = decode_payload("0a05776f726c64", &InputFormat::Auto);
        assert!(result.is_ok());
    }

    #[test]
    fn test_auto_detect_base64() {
        let result = decode_payload("CgV3b3JsZA==", &InputFormat::Auto);
        assert!(result.is_ok());
    }

    #[test]
    fn test_explain_args() {
        let args = ExplainArgs {
            descriptor_set: PathBuf::from("test.pb"),
            payload: "0a05776f726c64".to_string(),
            message_type: Some("test.Message".to_string()),
            input_format: InputFormat::Hex,
            output_format: OutputFormat::JsonPretty,
            list_types: false,
            show_field_numbers: false,
        };

        assert!(matches!(args.input_format, InputFormat::Hex));
        assert!(matches!(args.output_format, OutputFormat::JsonPretty));
    }
}
