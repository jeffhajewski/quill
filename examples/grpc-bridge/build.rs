fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    // Compile the echo proto for both gRPC (tonic) and Quill
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["../../proto/echo/v1/echo.proto"], &["../../proto"])?;

    Ok(())
}
