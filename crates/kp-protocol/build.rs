fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "proto/protocol.proto";
    println!("cargo:rerun-if-changed={proto}");

    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.skip_debug(["."]);
    config.compile_protos(&[proto], &["proto"])?;
    Ok(())
}
