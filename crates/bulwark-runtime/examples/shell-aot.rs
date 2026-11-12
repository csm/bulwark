//! Compile the portable shell assertions with the product's pinned compiler.
use anyhow::{Result, anyhow, ensure};
use cljrs_compiler::extensions::{CompileSession, Extension, ExtensionSet};
use std::{path::Path, process::Command};

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/shell-model-aot"));
    let extensions = ExtensionSet::new().with(Extension::new(
        "cljrs-async",
        cljrs_async::init,
        "cljrs_async::init",
    ));
    let session = CompileSession::new(vec![root.join("clj")], extensions);
    cljrs_compiler::aot::compile_file(&root.join("clj/tests/shell-model.cljrs"), &output, &session)
        .map_err(|error| anyhow!("{error}"))?;
    let result = Command::new(&output).output()?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    ensure!(
        result.status.success(),
        "AOT shell assertions failed: {}",
        result.status
    );
    ensure!(
        stdout.contains("Shell model assertions passed"),
        "AOT assertions did not finish"
    );
    Ok(())
}
