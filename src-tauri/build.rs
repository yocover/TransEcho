fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf definitions
    prost_build::compile_protos(
        &["proto/products/understanding/ast/ast_service.proto"],
        &["proto/"],
    )?;

    // Tauri build
    tauri_build::build();

    Ok(())
}
