# Streaming Guide

Quill supports four streaming patterns for real-time data exchange.

## Streaming Patterns

| Pattern | Requests | Responses | Use Case |
|---------|----------|-----------|----------|
| **Unary** | 1 | 1 | Simple request/response |
| **Server Streaming** | 1 | N | Feeds, lists, logs |
| **Client Streaming** | N | 1 | File uploads, aggregation |
| **Bidirectional** | N | N | Chat, real-time sync |

## Server Streaming

Server sends multiple responses to a single request.

### Proto Definition

```protobuf
service LogService {
  rpc TailLogs (TailRequest) returns (stream LogEntry);
}

message TailRequest {
  string filter = 1;
  int32 limit = 2;
}

message LogEntry {
  string timestamp = 1;
  string level = 2;
  string message = 3;
}
```

### Server Implementation

```rust
use quill_server::ServerStreamSender;

async fn tail_logs(
    request: Bytes,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    let req = TailRequest::decode(request)?;

    let mut count = 0;
    let mut log_stream = open_log_stream(&req.filter).await?;

    while let Some(entry) = log_stream.next().await {
        // Check if client cancelled
        if sender.is_cancelled() {
            break;
        }

        sender.send(entry.encode_to_vec().into()).await?;

        count += 1;
        if req.limit > 0 && count >= req.limit {
            break;
        }
    }

    Ok(())
}
```

### Client Usage

```rust
let request = TailRequest {
    filter: "level=ERROR".into(),
    limit: 100,
};

let mut stream = client
    .call_server_streaming(
        "logs.v1.LogService/TailLogs",
        request.encode_to_vec().into(),
    )
    .await?;

while let Some(response) = stream.next().await {
    let entry = LogEntry::decode(&response?[..])?;
    println!("[{}] {}: {}", entry.timestamp, entry.level, entry.message);
}
```

## Client Streaming

Client sends multiple requests, server responds once.

### Proto Definition

```protobuf
service FileService {
  rpc Upload (stream UploadChunk) returns (UploadResponse);
}

message UploadChunk {
  bytes data = 1;
  string filename = 2;  // First chunk only
}

message UploadResponse {
  string file_id = 1;
  int64 bytes_written = 2;
  string checksum = 3;
}
```

### Server Implementation

```rust
use quill_server::RequestStream;

async fn upload(mut stream: RequestStream) -> Result<Bytes, QuillError> {
    let mut total_bytes = 0;
    let mut hasher = Sha256::new();
    let mut filename = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = UploadChunk::decode(chunk?)?;

        if filename.is_empty() && !chunk.filename.is_empty() {
            filename = chunk.filename;
        }

        hasher.update(&chunk.data);
        storage.write(&chunk.data).await?;
        total_bytes += chunk.data.len() as i64;
    }

    let response = UploadResponse {
        file_id: generate_file_id(),
        bytes_written: total_bytes,
        checksum: hex::encode(hasher.finalize()),
    };

    Ok(response.encode_to_vec().into())
}
```

### Client Usage

```rust
use tokio::fs::File;
use tokio::io::AsyncReadExt;

let sender = client
    .call_client_streaming("files.v1.FileService/Upload")
    .await?;

let mut file = File::open("large_file.dat").await?;
let mut buffer = vec![0u8; 64 * 1024];  // 64KB chunks
let mut first = true;

loop {
    let n = file.read(&mut buffer).await?;
    if n == 0 {
        break;
    }

    let chunk = UploadChunk {
        data: buffer[..n].to_vec(),
        filename: if first { "large_file.dat".into() } else { String::new() },
    };
    first = false;

    sender.send(chunk.encode_to_vec().into()).await?;
}

let response = sender.finish().await?;
let result = UploadResponse::decode(&response[..])?;
println!("Uploaded: {} ({} bytes)", result.file_id, result.bytes_written);
```

## Bidirectional Streaming

Both sides send streams concurrently.

### Proto Definition

```protobuf
service ChatService {
  rpc Chat (stream ChatMessage) returns (stream ChatMessage);
}

message ChatMessage {
  string user = 1;
  string text = 2;
  int64 timestamp = 3;
}
```

### Server Implementation

```rust
async fn chat(
    mut stream: RequestStream,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    // Get or create chat room
    let room = get_chat_room().await;

    // Subscribe to room messages
    let mut receiver = room.subscribe();

    // Spawn task to receive from room and send to client
    let sender_clone = sender.clone();
    let receive_task = tokio::spawn(async move {
        while let Ok(msg) = receiver.recv().await {
            if sender_clone.send(msg.encode_to_vec().into()).await.is_err() {
                break;
            }
        }
    });

    // Process incoming messages from client
    while let Some(message) = stream.next().await {
        let msg = ChatMessage::decode(message?)?;
        room.broadcast(msg).await;
    }

    receive_task.abort();
    Ok(())
}
```

### Client Usage

```rust
let (sender, mut receiver) = client
    .call_bidi_streaming("chat.v1.ChatService/Chat")
    .await?;

// Spawn task to receive messages
let receive_handle = tokio::spawn(async move {
    while let Some(response) = receiver.next().await {
        match response {
            Ok(data) => {
                let msg = ChatMessage::decode(&data[..])?;
                println!("{}: {}", msg.user, msg.text);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }
    Ok::<_, QuillError>(())
});

// Send messages from stdin
let stdin = tokio::io::stdin();
let reader = tokio::io::BufReader::new(stdin);
let mut lines = reader.lines();

while let Some(line) = lines.next_line().await? {
    let msg = ChatMessage {
        user: "me".into(),
        text: line,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };
    sender.send(msg.encode_to_vec().into()).await?;
}

// Close sender and wait for receiver
sender.finish().await?;
receive_handle.await??;
```

## Flow Control

Quill uses credit-based flow control for backpressure.

### How It Works

1. Receiver starts with initial credits
2. Sender decrements credits per frame
3. Receiver sends CREDIT frames to grant more
4. Sender blocks when credits exhausted

### Configuration

```rust
// Server-side flow control
let server = QuillServer::builder()
    .initial_stream_credits(1000)
    .credit_grant_size(500)
    .build();

// Client-side flow control
let client = QuillClient::builder()
    .initial_stream_credits(1000)
    .build()?;
```

### Manual Credit Management

```rust
use quill_server::ServerStreamSender;

async fn controlled_stream(
    request: Bytes,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    // Wait for credits before sending
    sender.wait_for_credits(1).await?;

    // Send with backpressure
    for item in items {
        sender.send(item.encode_to_vec().into()).await?;
        // Automatically waits if credits exhausted
    }

    Ok(())
}
```

## Cancellation

### Server-Side Cancellation Detection

```rust
async fn long_stream(
    request: Bytes,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    loop {
        // Check for cancellation
        if sender.is_cancelled() {
            tracing::info!("Client cancelled stream");
            break;
        }

        // Do work...
        let data = expensive_computation().await?;
        sender.send(data).await?;
    }

    Ok(())
}
```

### Client-Side Cancellation

```rust
let mut stream = client
    .call_server_streaming("service/method", request)
    .await?;

// Process first 10 items then cancel
let mut count = 0;
while let Some(response) = stream.next().await {
    process(response?);
    count += 1;
    if count >= 10 {
        stream.cancel().await?;  // Send CANCEL frame
        break;
    }
}
```

## Error Handling in Streams

### Server Errors

```rust
async fn stream_with_errors(
    request: Bytes,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    for item in items {
        match process_item(item).await {
            Ok(data) => sender.send(data).await?,
            Err(e) => {
                // Send error and close stream
                return Err(QuillError::internal(format!("Processing failed: {}", e)));
            }
        }
    }
    Ok(())
}
```

### Client Error Recovery

```rust
let mut stream = client
    .call_server_streaming("service/method", request)
    .await?;

let mut items = Vec::new();
while let Some(response) = stream.next().await {
    match response {
        Ok(data) => items.push(data),
        Err(QuillError::Unavailable(_)) => {
            // Server temporarily unavailable, retry
            tokio::time::sleep(Duration::from_secs(1)).await;
            stream = client
                .call_server_streaming("service/method", request.clone())
                .await?;
        }
        Err(e) => return Err(e),
    }
}
```

## Best Practices

1. **Use flow control** - Always respect backpressure signals
2. **Check cancellation** - Periodically check `is_cancelled()` in long streams
3. **Handle partial failures** - Design for items that may fail mid-stream
4. **Set timeouts** - Configure appropriate stream timeouts
5. **Chunk large data** - Use reasonable chunk sizes (16-64KB)
6. **Log stream lifecycle** - Track stream open/close for debugging

## Next Steps

- [Flow Control](../flow-control.md) - Detailed flow control documentation
- [Server Guide](server.md) - Server streaming patterns
- [Client Guide](client.md) - Client streaming usage
- [Performance](../performance.md) - Streaming performance tuning
