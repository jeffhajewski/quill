//! RPC call command (curl-for-proto)

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Args;
use quill_client::QuillClient;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Args, Debug)]
pub struct CallArgs {
    /// URL of the RPC endpoint (format: http://host:port/package.Service/Method)
    pub url: String,

    /// Input JSON string or @file path
    #[arg(short, long)]
    pub input: Option<String>,

    /// Additional headers in key:value format
    #[arg(short = 'H', long)]
    pub headers: Vec<String>,

    /// Enable server streaming mode
    #[arg(long)]
    pub stream: bool,

    /// Accept header value
    #[arg(long, default_value = "application/proto")]
    pub accept: String,

    /// Prism transport profile preference (hyper,turbo,classic)
    #[arg(long)]
    pub prism: Option<String>,

    /// Enable compression
    #[arg(long)]
    pub compress: bool,

    /// Timeout in seconds
    #[arg(long, default_value = "30")]
    pub timeout: u64,

    /// Pretty-print JSON output
    #[arg(long)]
    pub pretty: bool,
}

pub async fn run(args: CallArgs) -> Result<()> {
    println!("Making RPC call to: {}", args.url);

    // Parse URL to extract base URL and path
    let url = url::Url::parse(&args.url).context("Invalid URL")?;
    let base_url = format!("{}://{}", url.scheme(), url.host_str().unwrap_or("localhost"));
    let path = url.path().trim_start_matches('/');

    // Parse service and method from path
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid path format. Expected: /package.Service/Method");
    }
    let service = parts[0];
    let method = parts[1];

    println!("  Service: {}", service);
    println!("  Method: {}", method);

    // Read input
    let input_data = if let Some(input) = &args.input {
        if input.starts_with('@') {
            // Read from file
            let file_path = input.trim_start_matches('@');
            tokio::fs::read_to_string(file_path)
                .await
                .context(format!("Failed to read input file: {}", file_path))?
        } else {
            input.clone()
        }
    } else {
        // Read from stdin
        let mut buffer = String::new();
        tokio::io::stdin()
            .read_to_string(&mut buffer)
            .await
            .context("Failed to read from stdin")?;
        buffer
    };

    // For now, treat input as raw bytes (in a real implementation, we'd parse JSON and encode to protobuf)
    let request_bytes = Bytes::from(input_data);

    // Build client
    let mut client_builder = QuillClient::builder().base_url(&base_url);

    if args.compress {
        client_builder = client_builder.enable_compression(true);
    }

    let client = client_builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build client: {}", e))?;

    // Make the call
    if args.stream {
        println!("\n=== Streaming response ===\n");
        let mut stream = client
            .call_server_streaming(service, method, request_bytes)
            .await
            .context("RPC call failed")?;

        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            let result: Result<Bytes, quill_core::QuillError> = result;
            match result {
                Ok(bytes) => {
                    // Output the response
                    if args.pretty {
                        // Try to parse as JSON for pretty printing
                        if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                            println!("{}", serde_json::to_string_pretty(&json)?);
                        } else {
                            println!("{}", String::from_utf8_lossy(&bytes));
                        }
                    } else {
                        tokio::io::stdout().write_all(&bytes).await?;
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("Stream error: {}", e);
                    std::process::exit(3);
                }
            }
        }
    } else {
        println!("\n=== Response ===\n");
        let response = client
            .call(service, method, request_bytes)
            .await
            .context("RPC call failed")?;

        // Output the response
        if args.pretty {
            // Try to parse as JSON for pretty printing
            if let Ok(json) = serde_json::from_slice::<Value>(&response) {
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("{}", String::from_utf8_lossy(&response));
            }
        } else {
            tokio::io::stdout().write_all(&response).await?;
            println!();
        }
    }

    println!("\n✓ RPC call complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_args_url_parsing() {
        let url = url::Url::parse("http://localhost:8080/greeter.v1.Greeter/SayHello").unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("localhost"));
        assert_eq!(url.port(), Some(8080));
        assert_eq!(url.path(), "/greeter.v1.Greeter/SayHello");
    }

    #[test]
    fn test_path_parsing() {
        let path = "/greeter.v1.Greeter/SayHello";
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "greeter.v1.Greeter");
        assert_eq!(parts[1], "SayHello");
    }
}
