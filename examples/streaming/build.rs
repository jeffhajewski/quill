fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();

    // Include both the log proto and quill annotations
    config.compile_protos(
        &["../../proto/log/v1/log.proto"],
        &["../../proto"],
    )?;

    Ok(())
}
