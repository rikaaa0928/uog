fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/udp.proto")?;
    uniffi::generate_scaffolding("./src/uog.udl").unwrap();
    Ok(())
}