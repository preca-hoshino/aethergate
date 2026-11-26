//! Build script compiling `v2rayapi.proto` using pure-Rust `protox`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = std::path::Path::new("../aethergate-protocol/proto/v2rayapi.proto");
    println!("cargo:rerun-if-changed={}", proto_path.display());

    let fds = protox::compile([proto_path], ["../aethergate-protocol/proto"])?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(fds)?;
    Ok(())
}
