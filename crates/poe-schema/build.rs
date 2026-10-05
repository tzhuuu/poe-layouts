use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("poe-schema should live under crates/poe-schema");
    let schema = workspace_root.join("schema").join("poe_layouts.fbs");
    let rust_out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ts_out = workspace_root.join("app").join("src").join("generated");

    println!("cargo:rerun-if-changed={}", schema.display());
    fs::create_dir_all(&ts_out).expect("create TypeScript generated output directory");

    run_flatc(&["--rust", "-o"], &rust_out, &schema);
    run_flatc(&["--ts", "-o"], &ts_out, &schema);
}

fn run_flatc(prefix_args: &[&str], out_dir: &Path, schema: &Path) {
    let mut command = Command::new("flatc");
    for arg in prefix_args {
        command.arg(arg);
    }
    command.arg(out_dir).arg(schema);

    let output = command.output().expect("failed to invoke flatc");
    if !output.status.success() {
        panic!(
            "flatc failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
