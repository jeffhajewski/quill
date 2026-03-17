//! RPC call command (curl-for-proto)

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use bytes::Bytes;
use clap::{Args, ValueEnum};
use http::header::{HeaderName, HeaderValue, AUTHORIZATION};
use prost::Message;
use prost_reflect::{DescriptorPool, DeserializeOptions, DynamicMessage, MessageDescriptor};
use quill_client::{QuillClient, RequestOptions};
use quill_core::{PrismProfile, ProfilePreference};
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, ValueEnum)]
pub enum InputFormat {
    /// Auto-detect JSON when a descriptor set is available; otherwise send raw bytes.
    Auto,
    /// Parse the input as JSON and encode it to protobuf using a descriptor set.
    Json,
    /// Send the input as UTF-8 text bytes.
    Text,
    /// Decode the input as hexadecimal.
    Hex,
    /// Decode the input as base64.
    Base64,
    /// Read the input from a file path.
    File,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    /// Auto-detect JSON output when possible; otherwise write raw bytes.
    Auto,
    /// Write raw response bytes to stdout.
    Raw,
    /// Decode the response to JSON.
    Json,
    /// Decode the response to pretty-printed JSON.
    JsonPretty,
    /// Print the response bytes as hexadecimal.
    Hex,
    /// Print the response bytes as base64.
    Base64,
}

#[derive(Args, Debug)]
pub struct CallArgs {
    /// URL of the RPC endpoint, or a relative /package.Service/Method path when QUILL_URL is set.
    pub url: String,

    /// Input data or @file path.
    #[arg(short, long, visible_alias = "in")]
    pub input: Option<String>,

    /// Additional headers in key:value format.
    #[arg(short = 'H', long = "headers", visible_alias = "header")]
    pub headers: Vec<String>,

    /// Enable server streaming mode.
    #[arg(long)]
    pub stream: bool,

    /// Accept header value.
    #[arg(long, default_value = "application/proto")]
    pub accept: String,

    /// Prism transport profile preference, for example turbo or hyper,turbo,classic.
    #[arg(long)]
    pub prism: Option<String>,

    /// Enable compression for the request.
    #[arg(long)]
    pub compress: bool,

    /// Timeout in seconds.
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Pretty-print JSON output.
    #[arg(long)]
    pub pretty: bool,

    /// Descriptor set used for JSON <-> protobuf conversion.
    #[arg(long)]
    pub descriptor_set: Option<PathBuf>,

    /// Format for request input.
    #[arg(long, value_enum, default_value = "auto")]
    pub input_format: InputFormat,

    /// Format for response output.
    #[arg(long, value_enum, default_value = "auto")]
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone)]
struct Endpoint {
    base_url: String,
    service: String,
    method: String,
}

#[derive(Debug, Clone)]
struct MethodDescriptors {
    input: MessageDescriptor,
    output: MessageDescriptor,
}

#[derive(Debug, Clone)]
struct InputData {
    bytes: Vec<u8>,
    text: Option<String>,
}

enum RenderedOutput {
    Binary(Vec<u8>),
    Text(String),
}

pub async fn run(args: CallArgs) -> Result<()> {
    let endpoint = resolve_endpoint(&args.url)?;
    let descriptors = load_method_descriptors(args.descriptor_set.as_deref(), &endpoint)?;
    let input = read_input_data(args.input.as_deref(), &args.input_format).await?;
    let request_bytes = encode_request_payload(&input, &args.input_format, descriptors.as_ref())?;

    let timeout = resolve_timeout(args.timeout)?;
    let request_options = build_request_options(&args, timeout)?;

    let mut client_builder = QuillClient::builder().base_url(&endpoint.base_url);
    if args.compress {
        client_builder = client_builder.enable_compression(true);
    }
    let client =
        client_builder.build().map_err(|e| anyhow::anyhow!("Failed to build client: {}", e))?;

    if args.stream {
        let mut stream = client
            .call_server_streaming_with_options(
                &endpoint.service,
                &endpoint.method,
                request_bytes,
                request_options,
            )
            .await
            .context("RPC call failed")?;

        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            let bytes = result.context("Stream error")?;
            let rendered = render_output(
                &bytes,
                &args.output_format,
                args.pretty,
                descriptors.as_ref().map(|d| &d.output),
            )?;
            write_rendered_output(rendered, true).await?;
        }
    } else {
        let response = client
            .call_with_options(&endpoint.service, &endpoint.method, request_bytes, request_options)
            .await
            .context("RPC call failed")?;

        let rendered = render_output(
            &response,
            &args.output_format,
            args.pretty,
            descriptors.as_ref().map(|d| &d.output),
        )?;
        let append_newline = !matches!(rendered, RenderedOutput::Binary(_));
        write_rendered_output(rendered, append_newline).await?;
    }

    Ok(())
}

fn resolve_endpoint(endpoint: &str) -> Result<Endpoint> {
    if let Ok(url) = url::Url::parse(endpoint) {
        return parse_absolute_endpoint(&url);
    }

    let base = env::var("QUILL_URL")
        .or_else(|_| env::var("QUILL_BASE_URL"))
        .context("Relative endpoints require QUILL_URL (or QUILL_BASE_URL) to be set")?;

    let mut base = url::Url::parse(&base)
        .with_context(|| format!("Invalid QUILL_URL/QUILL_BASE_URL value: {}", base))?;
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path().trim_end_matches('/'));
        base.set_path(&path);
    }

    let joined = base
        .join(endpoint.trim_start_matches('/'))
        .with_context(|| format!("Failed to resolve endpoint '{}'", endpoint))?;
    parse_absolute_endpoint(&joined)
}

fn parse_absolute_endpoint(url: &url::Url) -> Result<Endpoint> {
    let mut segments: Vec<_> = url
        .path_segments()
        .map(|segments| segments.filter(|segment| !segment.is_empty()).collect::<Vec<_>>())
        .ok_or_else(|| anyhow::anyhow!("Endpoint URL must include a service and method path"))?;

    if segments.len() < 2 {
        anyhow::bail!("Endpoint path must end with /package.Service/Method");
    }

    let method = segments.pop().unwrap().to_string();
    let service = segments.pop().unwrap().to_string();

    let mut base_url = url.origin().ascii_serialization();
    if !segments.is_empty() {
        base_url.push('/');
        base_url.push_str(&segments.join("/"));
    }

    Ok(Endpoint { base_url, service, method })
}

fn load_method_descriptors(
    descriptor_path: Option<&Path>,
    endpoint: &Endpoint,
) -> Result<Option<MethodDescriptors>> {
    let Some(path) = descriptor_path else {
        return Ok(None);
    };

    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read descriptor set: {}", path.display()))?;
    let pool = DescriptorPool::decode(bytes.as_slice())
        .with_context(|| format!("Failed to parse descriptor set: {}", path.display()))?;

    let service = pool
        .services()
        .find(|descriptor| {
            descriptor.full_name() == endpoint.service || descriptor.name() == endpoint.service
        })
        .with_context(|| {
            format!(
                "Service '{}' was not found in descriptor set {}",
                endpoint.service,
                path.display()
            )
        })?;

    let method = service
        .methods()
        .find(|descriptor| descriptor.name() == endpoint.method)
        .with_context(|| {
            format!(
                "Method '{}.{}' was not found in descriptor set {}",
                endpoint.service,
                endpoint.method,
                path.display()
            )
        })?;

    Ok(Some(MethodDescriptors { input: method.input(), output: method.output() }))
}

async fn read_input_data(input: Option<&str>, format: &InputFormat) -> Result<InputData> {
    let should_read_file = matches!(format, InputFormat::File)
        || input.map(|value| value.starts_with('@')).unwrap_or(false);

    let bytes = if should_read_file {
        let path = input
            .context("A file path is required when using --input-format file or @file syntax")?;
        let path = path.trim_start_matches('@');
        tokio::fs::read(path)
            .await
            .with_context(|| format!("Failed to read input file: {}", path))?
    } else if let Some(input) = input {
        input.as_bytes().to_vec()
    } else {
        let mut buffer = Vec::new();
        tokio::io::stdin()
            .read_to_end(&mut buffer)
            .await
            .context("Failed to read request body from stdin")?;
        buffer
    };

    let text = String::from_utf8(bytes.clone()).ok();
    Ok(InputData { bytes, text })
}

fn encode_request_payload(
    input: &InputData,
    format: &InputFormat,
    descriptors: Option<&MethodDescriptors>,
) -> Result<Bytes> {
    match format {
        InputFormat::Auto => {
            if let Some(descriptors) = descriptors {
                if let Some(text) = input.text.as_deref() {
                    if serde_json::from_str::<Value>(text.trim()).is_ok() {
                        return encode_json_payload(text, &descriptors.input);
                    }
                }
            }
            Ok(Bytes::from(input.bytes.clone()))
        }
        InputFormat::Json => {
            let descriptors = descriptors
                .context("--descriptor-set is required when using --input-format json")?;
            let text = input.text.as_deref().context("JSON input must be valid UTF-8")?;
            encode_json_payload(text, &descriptors.input)
        }
        InputFormat::Text | InputFormat::File => Ok(Bytes::from(input.bytes.clone())),
        InputFormat::Hex => {
            let text = input.text.as_deref().context("Hex input must be valid UTF-8")?;
            let decoded = hex::decode(text.trim().trim_start_matches("0x").replace(' ', ""))
                .context("Failed to decode hex input")?;
            Ok(Bytes::from(decoded))
        }
        InputFormat::Base64 => {
            let text = input.text.as_deref().context("Base64 input must be valid UTF-8")?;
            let decoded = BASE64.decode(text.trim()).context("Failed to decode base64 input")?;
            Ok(Bytes::from(decoded))
        }
    }
}

fn encode_json_payload(text: &str, descriptor: &MessageDescriptor) -> Result<Bytes> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let message = DynamicMessage::deserialize_with_options(
        descriptor.clone(),
        &mut deserializer,
        &DeserializeOptions::default(),
    )
    .with_context(|| format!("Failed to encode JSON request as '{}'", descriptor.full_name()))?;

    Ok(Bytes::from(message.encode_to_vec()))
}

fn build_request_options(args: &CallArgs, timeout: Duration) -> Result<RequestOptions> {
    let mut options = RequestOptions::new().timeout(timeout);

    let accept = HeaderValue::from_str(&args.accept)
        .with_context(|| format!("Invalid Accept header value: {}", args.accept))?;
    options = options.accept(accept);

    if let Some(preference) = resolve_profile_preference(args.prism.as_deref())? {
        options = options.profile_preference(preference);
    }

    let headers = parse_headers(&args.headers)?;
    let has_auth_header = headers.iter().any(|(name, _)| *name == AUTHORIZATION);

    for (name, value) in headers {
        options = options.header(name, value);
    }

    if !has_auth_header {
        if let Ok(token) = env::var("QUILL_TOKEN") {
            let value = HeaderValue::from_str(&format!("Bearer {}", token))
                .context("Invalid QUILL_TOKEN value")?;
            options = options.header(AUTHORIZATION, value);
        }
    }

    Ok(options)
}

fn parse_headers(headers: &[String]) -> Result<Vec<(HeaderName, HeaderValue)>> {
    headers.iter().map(|header| parse_header(header)).collect()
}

fn parse_header(header: &str) -> Result<(HeaderName, HeaderValue)> {
    let (name, value) = header
        .split_once(':')
        .with_context(|| format!("Invalid header '{}'. Expected key:value", header))?;

    let name = HeaderName::from_bytes(name.trim().as_bytes())
        .with_context(|| format!("Invalid header name '{}'", name.trim()))?;
    let value = HeaderValue::from_str(value.trim())
        .with_context(|| format!("Invalid header value for '{}'", name))?;

    Ok((name, value))
}

fn resolve_timeout(timeout: Option<u64>) -> Result<Duration> {
    if let Some(timeout) = timeout {
        return Ok(Duration::from_secs(timeout));
    }

    match env::var("QUILL_TIMEOUT") {
        Ok(value) => {
            let seconds = value
                .parse::<u64>()
                .with_context(|| format!("Invalid QUILL_TIMEOUT value: {}", value))?;
            Ok(Duration::from_secs(seconds))
        }
        Err(_) => Ok(Duration::from_secs(30)),
    }
}

fn resolve_profile_preference(value: Option<&str>) -> Result<Option<ProfilePreference>> {
    let value = match value {
        Some(value) => Some(value.to_string()),
        None => env::var("QUILL_PRISM").ok(),
    };

    value.as_deref().map(parse_profile_preference).transpose()
}

fn parse_profile_preference(value: &str) -> Result<ProfilePreference> {
    let profiles: Result<Vec<_>> = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<PrismProfile>()
                .with_context(|| format!("Invalid Prism profile '{}'", part))
        })
        .collect();

    let profiles = profiles?;
    if profiles.is_empty() {
        anyhow::bail!("At least one Prism profile must be provided");
    }

    Ok(ProfilePreference::new(profiles))
}

fn render_output(
    response: &[u8],
    format: &OutputFormat,
    pretty: bool,
    descriptor: Option<&MessageDescriptor>,
) -> Result<RenderedOutput> {
    match format {
        OutputFormat::Raw => Ok(RenderedOutput::Binary(response.to_vec())),
        OutputFormat::Hex => Ok(RenderedOutput::Text(hex::encode(response))),
        OutputFormat::Base64 => Ok(RenderedOutput::Text(BASE64.encode(response))),
        OutputFormat::Json => render_json_output(response, descriptor, pretty),
        OutputFormat::JsonPretty => render_json_output(response, descriptor, true),
        OutputFormat::Auto => {
            if let Some(descriptor) = descriptor {
                if let Some(value) = try_decode_json_value(response, Some(descriptor)) {
                    return Ok(RenderedOutput::Text(if pretty {
                        serde_json::to_string_pretty(&value)?
                    } else {
                        serde_json::to_string(&value)?
                    }));
                }
            }

            if pretty {
                if let Ok(value) = serde_json::from_slice::<Value>(response) {
                    return Ok(RenderedOutput::Text(serde_json::to_string_pretty(&value)?));
                }
            }

            Ok(RenderedOutput::Binary(response.to_vec()))
        }
    }
}

fn render_json_output(
    response: &[u8],
    descriptor: Option<&MessageDescriptor>,
    pretty: bool,
) -> Result<RenderedOutput> {
    let value = if let Some(descriptor) = descriptor {
        decode_descriptor_json(response, descriptor)?
    } else {
        serde_json::from_slice::<Value>(response).context(
            "JSON output requested but the response is not JSON. Provide --descriptor-set to decode protobuf responses.",
        )?
    };

    let rendered =
        if pretty { serde_json::to_string_pretty(&value)? } else { serde_json::to_string(&value)? };

    Ok(RenderedOutput::Text(rendered))
}

fn try_decode_json_value(response: &[u8], descriptor: Option<&MessageDescriptor>) -> Option<Value> {
    if let Some(descriptor) = descriptor {
        return decode_descriptor_json(response, descriptor).ok();
    }

    serde_json::from_slice::<Value>(response).ok()
}

fn decode_descriptor_json(response: &[u8], descriptor: &MessageDescriptor) -> Result<Value> {
    let message = DynamicMessage::decode(descriptor.clone(), response)
        .with_context(|| format!("Failed to decode response as '{}'", descriptor.full_name()))?;

    serde_json::to_value(&message).context("Failed to convert protobuf response to JSON")
}

async fn write_rendered_output(output: RenderedOutput, append_newline: bool) -> Result<()> {
    let mut stdout = tokio::io::stdout();
    match output {
        RenderedOutput::Binary(bytes) => {
            stdout.write_all(&bytes).await?;
            if append_newline {
                stdout.write_all(b"\n").await?;
            }
        }
        RenderedOutput::Text(text) => {
            stdout.write_all(text.as_bytes()).await?;
            if append_newline {
                stdout.write_all(b"\n").await?;
            }
        }
    }
    stdout.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_absolute_endpoint_with_port() {
        let url = url::Url::parse("http://localhost:8080/api/greeter.v1.Greeter/SayHello").unwrap();
        let endpoint = parse_absolute_endpoint(&url).unwrap();

        assert_eq!(endpoint.base_url, "http://localhost:8080/api");
        assert_eq!(endpoint.service, "greeter.v1.Greeter");
        assert_eq!(endpoint.method, "SayHello");
    }

    #[test]
    fn test_parse_header_with_spaces() {
        let (name, value) = parse_header("Authorization: Bearer token").unwrap();

        assert_eq!(name, AUTHORIZATION);
        assert_eq!(value, HeaderValue::from_static("Bearer token"));
    }

    #[test]
    fn test_parse_profile_preference_list() {
        let preference = parse_profile_preference("hyper,turbo,classic").unwrap();
        assert_eq!(preference.to_header_value(), "prism=hyper,turbo,classic");
    }

    #[test]
    fn test_encode_raw_text_request() {
        let input = InputData { bytes: b"hello".to_vec(), text: Some("hello".to_string()) };
        let encoded = encode_request_payload(&input, &InputFormat::Text, None).unwrap();
        assert_eq!(encoded, Bytes::from_static(b"hello"));
    }

    #[test]
    fn test_render_auto_hex_output() {
        let rendered = render_output(b"hi", &OutputFormat::Hex, false, None).unwrap();
        match rendered {
            RenderedOutput::Text(text) => assert_eq!(text, "6869"),
            RenderedOutput::Binary(_) => panic!("expected text output"),
        }
    }
}
