use quill_codegen::{compile_protos, QuillConfig};

fn main() -> std::io::Result<()> {
    // Configure code generation
    let config = QuillConfig::new()
        .with_package_prefix("example");

    // Compile protobuf definitions and generate Quill code
    compile_protos(&["proto/greeter.proto"], &["proto"], config)?;

    Ok(())
}
