fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();

    // Include both the echo proto and quill annotations
    config.compile_protos(
        &["../../proto/echo/v1/echo.proto"],
        &["../../proto"],
    )?;

    Ok(())
}
