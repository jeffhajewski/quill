fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();

    // Include the proto directory
    config.compile_protos(&["../../proto/quill/annotations.proto"], &["../../proto"])?;

    Ok(())
}
