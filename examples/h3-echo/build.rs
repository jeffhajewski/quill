fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    let mut config = prost_build::Config::new();

    // Include both the echo proto and quill annotations
    config.compile_protos(&["../../proto/echo/v1/echo.proto"], &["../../proto"])?;

    Ok(())
}
