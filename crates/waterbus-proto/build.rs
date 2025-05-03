use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let proto_root = "proto";
    let protos = collect_proto_files(proto_root)?;

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&protos, &[proto_root])?;

    Ok(())
}

fn collect_proto_files(dir: &str) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = vec![];
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            files.extend(collect_proto_files(path.to_str().unwrap())?);
        } else if path.extension().map_or(false, |ext| ext == "proto") {
            files.push(path);
        }
    }

    Ok(files)
}
