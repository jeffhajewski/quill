fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    let mut config = prost_build::Config::new();

    // Include the log proto and quill annotations
    config.compile_protos(&["../../proto/log/v1/log.proto"], &["../../proto"])?;

    Ok(())
}
