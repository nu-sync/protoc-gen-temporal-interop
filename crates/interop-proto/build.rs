use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../proto/interop/v1/interop.proto");
    println!("cargo:rerun-if-changed=../../buf.yaml");
    println!("cargo:rerun-if-changed=../../buf.gen.yaml");

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let marker = manifest_dir.join("src/gen/interop/v1/interop_temporal.rs");
    if marker.exists() {
        return;
    }

    if which("buf").is_none()
        || which("protoc-gen-prost").is_none()
        || which("protoc-gen-rust-temporal").is_none()
    {
        println!(
            "cargo:warning=generated sources are missing; run `cargo run -p interop-harness -- gen`"
        );
        return;
    }

    let root = manifest_dir.join("../..");
    match Command::new("buf")
        .arg("generate")
        .current_dir(&root)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!("cargo:warning=`buf generate` exited with {status}");
        }
        Err(err) => {
            println!("cargo:warning=could not run `buf generate`: {err}");
        }
    }
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
