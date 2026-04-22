use assert_cmd::Command;
use bytes::Bytes;
use http::header::AUTHORIZATION;
use http::{Request, Response, StatusCode};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Frame as HyperFrame, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use prost::Message;
use quill_core::Frame;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::{Command as StdCommand, Output};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::sleep;

type TestBody = UnsyncBoxBody<Bytes, Infallible>;
type TestResponseFuture = Pin<Box<dyn Future<Output = Response<TestBody>> + Send>>;
type TestHandler = Arc<dyn Fn(Request<Incoming>) -> TestResponseFuture + Send + Sync>;

#[derive(Clone, PartialEq, Message)]
struct HelloRequest {
    #[prost(string, tag = "1")]
    name: String,
}

#[derive(Clone, PartialEq, Message)]
struct HelloReply {
    #[prost(string, tag = "1")]
    message: String,
}

struct TestServer {
    addr: SocketAddr,
    handle: JoinHandle<()>,
}

impl TestServer {
    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quill_call_round_trips_raw_bytes() -> anyhow::Result<()> {
    let server = spawn_server(|req| async move {
        if req.uri().path() != "/echo.v1.EchoService/Echo" {
            return proto_response(StatusCode::NOT_FOUND, Bytes::from_static(b"missing"));
        }

        let body =
            req.into_body().collect().await.expect("request body should be readable").to_bytes();
        proto_response(StatusCode::OK, body)
    })
    .await?;

    let output = Command::cargo_bin("quill")?
        .arg("call")
        .arg(server.url("/echo.v1.EchoService/Echo"))
        .arg("--input")
        .arg("hello")
        .output()?;

    let output = assert_success(output)?;
    assert_eq!(output.stdout, b"hello");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quill_call_encodes_and_decodes_json_with_descriptor_set() -> anyhow::Result<()> {
    let (_descriptor_dir, descriptor_set) = generate_descriptor_set(
        "examples/greeter/proto/greeter.proto",
        "examples/greeter/proto",
        "greeter.pb",
    )?;

    let server = spawn_server(|req| async move {
        if req.uri().path() != "/greeter.v1.Greeter/SayHello" {
            return proto_response(StatusCode::NOT_FOUND, Bytes::from_static(b"missing"));
        }

        let body =
            req.into_body().collect().await.expect("request body should be readable").to_bytes();
        let request = HelloRequest::decode(body).expect("request should decode");
        let reply = HelloReply { message: format!("Hello, {}!", request.name) };

        proto_response(StatusCode::OK, Bytes::from(reply.encode_to_vec()))
    })
    .await?;

    let output = Command::cargo_bin("quill")?
        .arg("call")
        .arg(server.url("/greeter.v1.Greeter/SayHello"))
        .arg("--descriptor-set")
        .arg(&descriptor_set)
        .arg("--input")
        .arg(r#"{"name":"World"}"#)
        .arg("--output-format")
        .arg("json")
        .output()?;

    let output = assert_success(output)?;
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout.trim(), r#"{"message":"Hello, World!"}"#);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quill_call_streams_descriptor_decoded_messages() -> anyhow::Result<()> {
    let (_descriptor_dir, descriptor_set) = generate_descriptor_set(
        "examples/greeter/proto/greeter.proto",
        "examples/greeter/proto",
        "greeter.pb",
    )?;

    let server = spawn_server(|req| async move {
        if req.uri().path() != "/greeter.v1.Greeter/SayHelloStream" {
            return proto_response(StatusCode::NOT_FOUND, Bytes::from_static(b"missing"));
        }

        let body =
            req.into_body().collect().await.expect("request body should be readable").to_bytes();
        let request = HelloRequest::decode(body).expect("request should decode");

        let first = HelloReply { message: format!("Hello, {} #1!", request.name) };
        let second = HelloReply { message: format!("Hello, {} #2!", request.name) };

        let chunks = vec![
            Frame::data(Bytes::from(first.encode_to_vec())).encode(),
            Frame::data(Bytes::from(second.encode_to_vec())).encode(),
            Frame::end_stream().encode(),
        ];

        streaming_proto_response(chunks)
    })
    .await?;

    let output = Command::cargo_bin("quill")?
        .arg("call")
        .arg(server.url("/greeter.v1.Greeter/SayHelloStream"))
        .arg("--descriptor-set")
        .arg(&descriptor_set)
        .arg("--input")
        .arg(r#"{"name":"Streamer"}"#)
        .arg("--stream")
        .arg("--output-format")
        .arg("json")
        .output()?;

    let output = assert_success(output)?;
    let stdout = String::from_utf8(output.stdout)?;
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![r#"{"message":"Hello, Streamer #1!"}"#, r#"{"message":"Hello, Streamer #2!"}"#,]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quill_call_supports_relative_urls_and_env_auth() -> anyhow::Result<()> {
    let server = spawn_server(|req| async move {
        let authorized = req.uri().path() == "/api/auth.v1.AuthService/Check"
            && req.headers().get(AUTHORIZATION).and_then(|value| value.to_str().ok())
                == Some("Bearer topsecret");

        let body =
            req.into_body().collect().await.expect("request body should be readable").to_bytes();

        if authorized && body == Bytes::from_static(b"ping") {
            proto_response(StatusCode::OK, Bytes::from_static(b"authorized"))
        } else {
            proto_response(StatusCode::UNAUTHORIZED, Bytes::from_static(b"unauthorized"))
        }
    })
    .await?;

    let output = Command::cargo_bin("quill")?
        .env("QUILL_URL", format!("http://{}/api", server.addr))
        .env("QUILL_TOKEN", "topsecret")
        .arg("call")
        .arg("/auth.v1.AuthService/Check")
        .arg("--input")
        .arg("ping")
        .output()?;

    let output = assert_success(output)?;
    assert_eq!(output.stdout, b"authorized");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quill_call_returns_network_exit_code_for_timeouts() -> anyhow::Result<()> {
    let server = spawn_server(|_req| async move {
        sleep(Duration::from_millis(1500)).await;
        proto_response(StatusCode::OK, Bytes::from_static(b"late"))
    })
    .await?;

    let output = Command::cargo_bin("quill")?
        .arg("call")
        .arg(server.url("/slow.v1.SlowService/Wait"))
        .arg("--input")
        .arg("slow")
        .arg("--timeout")
        .arg("1")
        .output()?;

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("timed out"));
    Ok(())
}

async fn spawn_server<F, Fut>(handler: F) -> anyhow::Result<TestServer>
where
    F: Fn(Request<Incoming>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response<TestBody>> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let handler: TestHandler = Arc::new(move |req| Box::pin(handler(req)));

    let handle = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(value) => value,
                Err(_) => break,
            };

            let handler = Arc::clone(&handler);
            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let handler = Arc::clone(&handler);
                    async move { Ok::<_, Infallible>(handler(req).await) }
                });

                let _ = http1::Builder::new()
                    .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                    .await;
            });
        }
    });

    Ok(TestServer { addr, handle })
}

fn proto_response(status: StatusCode, body: impl Into<Bytes>) -> Response<TestBody> {
    Response::builder()
        .status(status)
        .header("content-type", "application/proto")
        .body(Full::new(body.into()).boxed_unsync())
        .expect("response should be valid")
}

fn streaming_proto_response(chunks: Vec<Bytes>) -> Response<TestBody> {
    let frames = futures::stream::iter(
        chunks.into_iter().map(|chunk| Ok::<_, Infallible>(HyperFrame::data(chunk))),
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/proto")
        .body(StreamBody::new(frames).boxed_unsync())
        .expect("streaming response should be valid")
}

fn generate_descriptor_set(
    proto: &str,
    include_dir: &str,
    output_name: &str,
) -> anyhow::Result<(TempDir, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let proto = root.join(proto);
    let include_dir = root.join(include_dir);
    let tempdir = tempfile::tempdir()?;
    let descriptor_set = tempdir.path().join(output_name);

    let status = StdCommand::new(protoc_bin_vendored::protoc_bin_path()?)
        .arg(format!("--proto_path={}", include_dir.display()))
        .arg(format!("--descriptor_set_out={}", descriptor_set.display()))
        .arg("--include_imports")
        .arg(&proto)
        .status()?;

    if !status.success() {
        anyhow::bail!("failed to generate descriptor set for {}", proto.display());
    }

    Ok((tempdir, descriptor_set))
}

fn assert_success(output: Output) -> anyhow::Result<Output> {
    if output.status.success() {
        return Ok(output);
    }

    anyhow::bail!(
        "command failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
